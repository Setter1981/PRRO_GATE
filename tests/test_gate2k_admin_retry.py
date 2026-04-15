"""
Gate 2k — admin retry endpoint for REQUIRES_MANUAL_RECONCILIATION documents.

POST /v1/admin/documents/{document_id}/retry

Resets a REQUIRES_MANUAL_RECONCILIATION document back to ERROR_RETRYABLE
with recovery_attempts=0 so it gets a fresh ceiling worth of reconciliation retries.

Semantics:
  - Only valid for documents in REQUIRES_MANUAL_RECONCILIATION state
  - Returns 404 for unknown document_id
  - Returns 409 for documents not in REQUIRES_MANUAL_RECONCILIATION
  - On success: state=ERROR_RETRYABLE, recovery_attempts=0
  - Audit: DOCUMENT_ADMIN_RETRY_REQUESTED, severity=INFO
  - After reset, next reconciliation startup picks up the document again

Four tests:
  A — basic reset: REQUIRES_MANUAL_RECONCILIATION → ERROR_RETRYABLE, recovery_attempts=0, audit written
  B — excluded state guard: 409 for documents in ACK (terminal non-manual state)
  C — not found: 404 for unknown document_id
  D — re-processed: after admin reset, next reconciliation startup picks up the document
"""
from __future__ import annotations

import json
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
RECEIPT_ID = 'gate2k-receipt-001'
MAX_RECOVERY = 2  # low ceiling for fast ceiling-hit setup


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2k'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2k-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2K-001',
            'updated_at': '2026-03-30T22:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2K-001',
            'updated_at': '2026-03-30T22:01:00+00:00',
        })
    raise AssertionError(f'gate2k phase1: unexpected {request.method} {request.url}')


def _strict_no_http_mock(request: httpx.Request) -> httpx.Response:
    raise AssertionError(
        f'gate2k: unexpected HTTP call {request.method} {request.url}. '
        f'RETRYABLE path (missing transport_request_id) must not make HTTP calls.'
    )


def _config(tmp_path: Path, db_name: str, max_recovery_attempts: int = MAX_RECOVERY) -> AppConfig:
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
            'channel_owner': 'gate2k',
        },
        'runtime': {
            'process_immediately': True,
            'max_recovery_attempts': max_recovery_attempts,
        },
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2K-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T22:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2k',
        'business_ts': ts,
    }


def _setup_manual_state(tmp_path: Path, db_name: str) -> tuple[RuntimeContainer, str]:
    """Sets up a document in REQUIRES_MANUAL_RECONCILIATION via ceiling-hit path."""
    cfg = _config(tmp_path, db_name)
    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2k-open-' + db_name), 'business_ts': '2026-03-30T22:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-' + db_name, 'cashier_id': 'cashier-gate2k'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2k-sell-' + db_name),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-' + db_name,
                'cashier_id': 'cashier-gate2k',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Crash-sim: SENT + no transport_request_id → retryable path
    with c1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents "
            "SET state='SENT', ack_at=NULL, response_json=NULL, transport_request_id=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

    # Run MAX_RECOVERY passes to hit ceiling
    for _ in range(MAX_RECOVERY):
        c = RuntimeContainer(
            cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
        )
        with TestClient(create_app(c)) as _:
            pass

    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc.state == DocumentState.REQUIRES_MANUAL_RECONCILIATION, (
        f'Setup failed: expected REQUIRES_MANUAL_RECONCILIATION, got {doc.state}'
    )
    return c, sell_doc_id


# ---------------------------------------------------------------------------
# Test A — basic reset: state, recovery_attempts, audit
# ---------------------------------------------------------------------------

def test_gate2k_admin_retry_resets_to_error_retryable(tmp_path: Path) -> None:
    """
    POST /v1/admin/documents/{document_id}/retry on a REQUIRES_MANUAL_RECONCILIATION doc:
    - Returns 200 with previous_state, new_state, recovery_attempts=0
    - doc.state == ERROR_RETRYABLE
    - doc.recovery_attempts == 0 (reset, not ceiling value)
    - Audit: DOCUMENT_ADMIN_RETRY_REQUESTED, severity=INFO,
             payload contains previous_state and new_state
    """
    c_setup, sell_doc_id = _setup_manual_state(tmp_path, 'gate2k_a.sqlite3')
    cfg = _config(tmp_path, 'gate2k_a.sqlite3')

    c = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c)) as client:
        resp = client.post(f'/v1/admin/documents/{sell_doc_id}/retry')

    assert resp.status_code == 200, f'Expected 200: got {resp.status_code} {resp.text}'
    body = resp.json()
    assert body['previous_state'] == 'REQUIRES_MANUAL_RECONCILIATION'
    assert body['new_state'] == 'ERROR_RETRYABLE'
    assert body['recovery_attempts'] == 0
    assert body['document_id'] == sell_doc_id

    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        audit_rows = conn.execute(
            "SELECT severity, event_payload_json FROM audit_log "
            "WHERE entity_id=? AND event_type=? ORDER BY audit_id DESC LIMIT 1",
            (sell_doc_id, 'DOCUMENT_ADMIN_RETRY_REQUESTED'),
        ).fetchall()

    assert doc.state == DocumentState.ERROR_RETRYABLE, (
        f'doc.state must be ERROR_RETRYABLE after admin retry: got {doc.state}'
    )
    assert doc.recovery_attempts == 0, (
        f'recovery_attempts must be reset to 0: got {doc.recovery_attempts}'
    )

    assert len(audit_rows) == 1, (
        f'Exactly 1 DOCUMENT_ADMIN_RETRY_REQUESTED audit record expected: got {len(audit_rows)}'
    )
    severity, payload_json = audit_rows[0]
    assert severity == 'INFO', f'audit severity must be INFO: got {severity!r}'
    payload = json.loads(payload_json)
    assert payload['previous_state'] == 'REQUIRES_MANUAL_RECONCILIATION'
    assert payload['new_state'] == 'ERROR_RETRYABLE'
    assert payload['recovery_attempts_reset'] is True


# ---------------------------------------------------------------------------
# Test B — 409 guard: wrong state (ACK is terminal, not manual)
# ---------------------------------------------------------------------------

def test_gate2k_admin_retry_rejects_non_manual_state(tmp_path: Path) -> None:
    """
    POST /v1/admin/documents/{document_id}/retry on a doc NOT in REQUIRES_MANUAL_RECONCILIATION
    returns 409 with current_state in response.
    """
    cfg = _config(tmp_path, 'gate2k_b.sqlite3')
    c = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2k-open-b'), 'business_ts': '2026-03-30T22:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2k-b', 'cashier_id': 'cashier-gate2k'},
        })
        assert r_shift.status_code == 200

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2k-sell-b'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2k-b',
                'cashier_id': 'cashier-gate2k',
                'goods': [{'name': 'Tea', 'price': 1500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1500}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

        # Try admin retry on ACK doc
        resp = client.post(f'/v1/admin/documents/{sell_doc_id}/retry')

    assert resp.status_code == 409, f'Expected 409 for ACK doc: got {resp.status_code} {resp.text}'
    body = resp.json()
    assert body['current_state'] == 'ACK', f'Response must include current_state: got {body}'


# ---------------------------------------------------------------------------
# Test C — 404: unknown document_id
# ---------------------------------------------------------------------------

def test_gate2k_admin_retry_returns_404_for_unknown(tmp_path: Path) -> None:
    """
    POST /v1/admin/documents/{document_id}/retry with unknown document_id returns 404.
    """
    cfg = _config(tmp_path, 'gate2k_c.sqlite3')
    c = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c)) as client:
        resp = client.post('/v1/admin/documents/nonexistent-doc-id-gate2k/retry')

    assert resp.status_code == 404, f'Expected 404 for unknown doc: got {resp.status_code}'


# ---------------------------------------------------------------------------
# Test D — re-processed: next reconciliation startup picks up the reset doc
# ---------------------------------------------------------------------------

def test_gate2k_admin_reset_doc_is_picked_by_next_reconciliation(tmp_path: Path) -> None:
    """
    After admin retry reset:
    - doc is in ERROR_RETRYABLE with recovery_attempts=0
    - Next reconciliation startup selects it as a candidate (checked==1)
    - It is processed as retryable (reconciliation_retryable==1)
    - recovery_attempts increments to 1 (not stuck at 0)
    """
    c_setup, sell_doc_id = _setup_manual_state(tmp_path, 'gate2k_d.sqlite3')
    cfg = _config(tmp_path, 'gate2k_d.sqlite3')

    # Admin reset via running container
    c_reset = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c_reset)) as client:
        resp = client.post(f'/v1/admin/documents/{sell_doc_id}/retry')
    assert resp.status_code == 200

    # Next startup: reconciliation picks up the reset doc
    c_next = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c_next)) as _:
        pass

    report = c_next.last_startup_report
    assert report.reconciliation_checked == 1, (
        f'Reset doc must be picked as reconciliation candidate: checked={report.reconciliation_checked}'
    )
    assert report.reconciliation_retryable == 1, (
        f'Reset doc with transport_request_id=NULL must result in retryable: '
        f'retryable={report.reconciliation_retryable}'
    )

    with c_next.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc.state == DocumentState.ERROR_RETRYABLE
    assert doc.recovery_attempts == 1, (
        f'recovery_attempts must be 1 after first post-reset reconciliation pass: '
        f'got {doc.recovery_attempts}'
    )
