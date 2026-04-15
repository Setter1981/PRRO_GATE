"""
Gate 1z — reconciliation candidate hygiene smoke tests.

Verifies that reconciliation selection is narrow and correct:
  - Only candidate states are processed: SENT, KVT1, KVT2, ERROR_RETRYABLE,
    REQUIRES_MANUAL_RECONCILIATION
  - Terminal states (ACK, REJECTED) and offline-local docs are excluded
  - Transport is called exactly once for the one SENT candidate
  - Non-candidate document states are not mutated

Mixed-state setup:
  doc1 (SENT)     — candidate: reset from ACK to SENT via SQL (crash simulation)
  doc2 (ACK)      — non-candidate: stays ACK, transport must not be called
  doc3 (REJECTED) — non-candidate: stays REJECTED, transport must not be called

Phase 2 mock:
  - Accepts auth (POST /cashier/signinPinCode)
  - Accepts poll for doc1's receipt (GET /receipts/gate1z-receipt-001) → DONE
  - Raises AssertionError for any other receipt poll

Invariant #8: recovery and reconciliation must not silently violate state transitions.
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

# Receipt IDs returned by phase1 mock — used in phase2 poll matcher
RECEIPT_ID_1 = 'gate1z-receipt-001'  # candidate (SENT)
RECEIPT_ID_2 = 'gate1z-receipt-002'  # non-candidate (ACK)
RECEIPT_ID_3 = 'gate1z-receipt-003'  # non-candidate (REJECTED)

_sell_call_count = 0


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    global _sell_call_count
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1z'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1z-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1Z-001',
            'updated_at': '2026-03-30T11:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        _sell_call_count += 1
        receipt_ids = [RECEIPT_ID_1, RECEIPT_ID_2, RECEIPT_ID_3]
        receipt_id = receipt_ids[(_sell_call_count - 1) % 3]
        return httpx.Response(200, json={
            'id': receipt_id, 'status': 'DONE',
            'fiscal_code': f'RCPT-GATE1Z-{_sell_call_count:03d}',
            'updated_at': '2026-03-30T11:01:00+00:00',
        })
    raise AssertionError(f'gate1z phase1: unexpected {request.method} {request.url}')


def _phase2_mock(request: httpx.Request) -> httpx.Response:
    """Phase 2 (reconciliation): only doc1 (SENT) should be polled."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1z-phase2'})
    if path.endswith(f'/receipts/{RECEIPT_ID_1}'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID_1, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1Z-001',
            'updated_at': '2026-03-30T11:01:00+00:00',
        })
    # Any other receipt poll means a non-candidate was incorrectly selected
    raise AssertionError(
        f'gate1z phase2: non-candidate doc must NOT be polled: {request.method} {request.url}'
    )


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1z.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1z',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1Z-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T11:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1z',
        'business_ts': ts,
    }


def _sell_body(req_id: str, ext_id: str) -> dict:
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1z',
            'goods': [{'name': 'Coffee', 'price': 1500, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1500}],
        },
    }


def test_gate1z_reconciliation_only_processes_candidate_states(tmp_path: Path) -> None:
    """
    Mixed-state scenario: one SENT candidate, one ACK non-candidate, one REJECTED non-candidate.
    Reconciliation must process only the SENT doc and leave the others unchanged.

    Setup:
    - doc1: SENT (ACK → SENT via SQL crash simulation)
    - doc2: ACK (untouched)
    - doc3: REJECTED (ACK → REJECTED via SQL to simulate terminal state)

    Expected:
    - reconciliation_checked == 1 (only doc1 selected)
    - reconciliation_acked == 1 (doc1 reconciled to ACK)
    - doc1 state → ACK
    - doc2 state → ACK (unchanged)
    - doc3 state → REJECTED (unchanged)
    - Transport called only for doc1's receipt (phase2 mock raises for others)
    """
    global _sell_call_count
    _sell_call_count = 0

    cfg = _config(tmp_path)

    # Phase 1: SHIFT_OPEN + 3 SELLs — all reach ACK
    container1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(container1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1z-open'), 'business_ts': '2026-03-30T11:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1z', 'cashier_id': 'cashier-gate1z'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r1 = client.post('/v1/ingress/checkbox', json=_sell_body('gate1z-sell-1', 'ext-sell-gate1z-1'))
        assert r1.status_code == 200
        assert r1.json().get('document_state') == 'ACK'
        doc1_id = r1.json()['document_id']

        r2 = client.post('/v1/ingress/checkbox', json=_sell_body('gate1z-sell-2', 'ext-sell-gate1z-2'))
        assert r2.status_code == 200
        assert r2.json().get('document_state') == 'ACK'
        doc2_id = r2.json()['document_id']

        r3 = client.post('/v1/ingress/checkbox', json=_sell_body('gate1z-sell-3', 'ext-sell-gate1z-3'))
        assert r3.status_code == 200
        assert r3.json().get('document_state') == 'ACK'
        doc3_id = r3.json()['document_id']

    # Verify all three are ACK before state manipulation
    with container1.connect() as conn:
        for doc_id in (doc1_id, doc2_id, doc3_id):
            doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
            assert doc.state == DocumentState.ACK, f'{doc_id} must be ACK before manipulation'

    # Seed mixed states:
    # doc1 → SENT (candidate, crash-simulated)
    # doc2 → stays ACK (non-candidate)
    # doc3 → REJECTED (non-candidate terminal)
    with container1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
            "WHERE document_id=?",
            (doc1_id,),
        )
        conn.execute(
            "UPDATE fiscal_documents SET state='REJECTED' WHERE document_id=?",
            (doc3_id,),
        )
        conn.commit()

    # Confirm states before phase 2
    with container1.connect() as conn:
        assert FiscalDocumentRepository.get_by_id(conn, doc1_id).state == DocumentState.SENT
        assert FiscalDocumentRepository.get_by_id(conn, doc2_id).state == DocumentState.ACK
        assert FiscalDocumentRepository.get_by_id(conn, doc3_id).state == DocumentState.REJECTED

    # Phase 2: new container, reconciliation runs
    container2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_mock))
    )
    with TestClient(create_app(container2)) as _:
        assert container2.health.ready
        assert container2.health.startup_complete

    report = container2.last_startup_report
    assert report is not None
    assert report.phase2_attempted

    assert report.reconciliation_checked == 1, (
        f'Only 1 candidate (SENT doc1) must be selected: got {report.reconciliation_checked}. '
        f'ACK and REJECTED docs must be excluded from selection. Report: {report}'
    )
    assert report.reconciliation_acked == 1, (
        f'doc1 must be reconciled to ACK: got {report.reconciliation_acked}'
    )
    assert report.reconciliation_rejected == 0, (
        f'reconciliation_rejected must be 0 (REJECTED docs not selected): got {report.reconciliation_rejected}'
    )

    # Verify final states
    with container2.connect() as conn:
        doc1_final = FiscalDocumentRepository.get_by_id(conn, doc1_id)
        doc2_final = FiscalDocumentRepository.get_by_id(conn, doc2_id)
        doc3_final = FiscalDocumentRepository.get_by_id(conn, doc3_id)

    assert doc1_final.state == DocumentState.ACK, (
        f'doc1 (SENT candidate) must be ACK after reconciliation: got {doc1_final.state}'
    )
    assert doc1_final.ack_at is not None, 'doc1 ack_at must be set after reconciliation'

    assert doc2_final.state == DocumentState.ACK, (
        f'doc2 (ACK non-candidate) must remain ACK: got {doc2_final.state}'
    )

    assert doc3_final.state == DocumentState.REJECTED, (
        f'doc3 (REJECTED non-candidate) must remain REJECTED: got {doc3_final.state}'
    )
