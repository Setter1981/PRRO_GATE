"""
Gate 2f — document_files / archive consistency smoke tests.

Documents and verifies the asymmetry between write-path ACK and reconciliation ACK
in terms of document_files persistence.

Write-path document_files records (created by WritePathService):
  '{doc_id}-request'  → file_kind=REQUEST_JSON,    path='.../request.json'  (at PREPARE stage)
  '{doc_id}-response' → file_kind=PROTO_RESPONSE,  path='.../response.json' (if response_json present)
  '{doc_id}-signed'   → file_kind=SIGNED_XML,      path='.../signed.xml'    (signing path only)

Reconciliation document_files records: NONE.
  ReconciliationService updates fiscal_documents.response_json via update_state(),
  but does NOT call DocumentFilesRepository.add_file() at any point.

Consequence (asymmetry):
  Direct ACK (write-path):    request.json + response.json files in document_files
  Reconciliation ACK:         request.json exists (from write-path PREPARE),
                              response.json NOT recreated if it was absent/deleted

Test A — write-path ACK: both request and response files exist in document_files.
Test B — reconciliation ACK: request file exists, reconciliation does NOT create
         a new response file even when the existing one is removed (crash-sim).
         Only fiscal_documents.response_json is updated.
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState
from prro_gateway.repositories import FiscalDocumentRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'
RECEIPT_ID = 'gate2f-receipt-001'


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2f'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2f-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2F-001',
            'updated_at': '2026-03-30T17:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2F-001',
            'updated_at': '2026-03-30T17:01:00+00:00',
        })
    raise AssertionError(f'gate2f phase1: unexpected {request.method} {request.url}')


def _phase2_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2f-p2'})
    if path.endswith(f'/receipts/{RECEIPT_ID}'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2F-001',
            'updated_at': '2026-03-30T17:01:00+00:00',
        })
    raise AssertionError(f'gate2f phase2: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str = 'gate2f.sqlite3') -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / db_name),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate2f',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2F-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T17:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2f',
        'business_ts': ts,
    }


def _get_doc_files(conn, document_id: str) -> dict[str, str]:
    """Returns {file_id: file_kind} for a document."""
    rows = conn.execute(
        "SELECT file_id, file_kind FROM document_files WHERE document_id=? ORDER BY file_id",
        (document_id,),
    ).fetchall()
    return {r[0]: r[1] for r in rows}


# ---------------------------------------------------------------------------
# Test A — write-path ACK creates both request and response document_files
# ---------------------------------------------------------------------------

def test_gate2f_write_path_ack_creates_request_and_response_files(tmp_path: Path) -> None:
    """
    After a direct write-path ACK (SELL → transport returns DONE):
    - document_files must contain '{doc_id}-request' (REQUEST_JSON)
    - document_files must contain '{doc_id}-response' (PROTO_RESPONSE)

    Both files are created by WritePathService:
    - request: at _record_prepared_locked() (PREPARE stage, always)
    - response: at stage_send_complete() only when response_json is non-null

    This is the baseline: full document_files coverage for write-path ACK.
    """
    cfg = _config(tmp_path, 'gate2f_a.sqlite3')
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )

    with TestClient(create_app(container)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2f-open-a'), 'business_ts': '2026-03-30T17:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2f-a', 'cashier_id': 'cashier-gate2f'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2f-sell-a'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2f-a',
                'cashier_id': 'cashier-gate2f',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    with container.connect() as conn:
        files = _get_doc_files(conn, sell_doc_id)

    assert f'{sell_doc_id}-request' in files, (
        f'document_files must contain {{doc_id}}-request (REQUEST_JSON) after write-path ACK. '
        f'Files found: {files}'
    )
    assert files[f'{sell_doc_id}-request'] == 'REQUEST_JSON', (
        f'Expected file_kind=REQUEST_JSON: got {files[f"{sell_doc_id}-request"]!r}'
    )

    assert f'{sell_doc_id}-response' in files, (
        f'document_files must contain {{doc_id}}-response (PROTO_RESPONSE) after write-path ACK '
        f'(transport returned response_json). Files found: {files}'
    )
    assert files[f'{sell_doc_id}-response'] == 'PROTO_RESPONSE', (
        f'Expected file_kind=PROTO_RESPONSE: got {files[f"{sell_doc_id}-response"]!r}'
    )


# ---------------------------------------------------------------------------
# Test B — reconciliation ACK does NOT create new document_files records
# ---------------------------------------------------------------------------

def test_gate2f_reconciliation_ack_does_not_create_document_files(tmp_path: Path) -> None:
    """
    Reconciliation ACK updates fiscal_documents.response_json via update_state()
    but does NOT call DocumentFilesRepository.add_file().

    Proof: remove the write-path response file before crash-sim, then reconcile.
    After reconciliation:
    - '{doc_id}-request' exists (created at PREPARE, not touched by crash/reconcile)
    - '{doc_id}-response' does NOT exist (deleted in crash-sim, not recreated by reconcile)
    - fiscal_documents.response_json IS populated (updated by reconciliation)

    This documents the asymmetry: response_json in the doc row is updated,
    but the document_files table is not touched by reconciliation.
    """
    cfg = _config(tmp_path, 'gate2f_b.sqlite3')

    # Phase 1: SHIFT_OPEN + SELL → ACK (creates request + response files)
    container1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(container1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2f-open-b'), 'business_ts': '2026-03-30T17:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2f-b', 'cashier_id': 'cashier-gate2f'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2f-sell-b'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2f-b',
                'cashier_id': 'cashier-gate2f',
                'goods': [{'name': 'Tea', 'price': 1500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1500}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Verify both files exist after phase1
    with container1.connect() as conn:
        files_after_phase1 = _get_doc_files(conn, sell_doc_id)
    assert f'{sell_doc_id}-request' in files_after_phase1
    assert f'{sell_doc_id}-response' in files_after_phase1

    # Crash simulation: reset doc to SENT + remove response file (simulating partial crash)
    with container1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.execute(
            "DELETE FROM document_files WHERE file_id=?",
            (f'{sell_doc_id}-response',),
        )
        conn.commit()

    # Confirm crash state: request file remains, response file gone
    with container1.connect() as conn:
        files_after_crash = _get_doc_files(conn, sell_doc_id)
    assert f'{sell_doc_id}-request' in files_after_crash, 'request file must survive crash'
    assert f'{sell_doc_id}-response' not in files_after_crash, 'response file must be deleted'

    # Phase 2: reconciliation runs — transitions SENT → ACK
    container2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_mock))
    )
    with TestClient(create_app(container2)) as _:
        pass

    assert container2.last_startup_report.reconciliation_acked == 1

    with container2.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        assert doc.state == DocumentState.ACK

        files_after_reconcile = _get_doc_files(conn, sell_doc_id)

    # request file: still exists (unaffected by reconciliation)
    assert f'{sell_doc_id}-request' in files_after_reconcile, (
        'request file must be unchanged after reconciliation'
    )

    # response file: NOT recreated by reconciliation
    assert f'{sell_doc_id}-response' not in files_after_reconcile, (
        f'reconciliation must NOT recreate document_files response record. '
        f'ReconciliationService does not call DocumentFilesRepository.add_file(). '
        f'Files after reconcile: {files_after_reconcile}'
    )

    # But fiscal_documents.response_json IS populated by reconciliation's update_state()
    assert doc.response_json is not None, (
        'fiscal_documents.response_json must be populated by reconciliation update_state() '
        'even though document_files has no response record'
    )
