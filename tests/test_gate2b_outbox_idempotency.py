"""
Gate 2b — outbox idempotency smoke test.

Verifies that repeated reconciliation startup does not create duplicate
outbox records for the same document.

Idempotency mechanism:
  enqueue_document() uses plain INSERT (no INSERT OR IGNORE).
  Protection comes from the state machine: once a document reaches ACK,
  get_pending_for_reconciliation() no longer selects it (ACK not in candidate
  states), so enqueue_document() is never called a second time for the same doc.

Two outbox record formats coexist for the same document:
  Write-path ACK:    '{document_id}-outbox'           (payload_sha256=actual hash)
  Reconciliation ACK: '{document_id}-reconcile-outbox' (payload_sha256='reconcile')

These have different outbox_ids so they do not conflict. A crash-simulated
document (ACK → SENT reset → reconciled) has both:
  1. '{document_id}-outbox' from the original write-path ACK (before crash sim)
  2. '{document_id}-reconcile-outbox' from reconciliation

Running a third container (second reconciliation pass) must not add any more records:
  - doc is already ACK
  - get_pending_for_reconciliation() returns []
  - enqueue_document() is never called
  - total sync_outbox count for this doc stays at 2

Invariant #8: reconciliation must not silently violate state transitions.
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
RECEIPT_ID = 'gate2b-receipt-001'


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2b'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2b-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2B-001',
            'updated_at': '2026-03-30T13:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2B-001',
            'updated_at': '2026-03-30T13:01:00+00:00',
        })
    raise AssertionError(f'gate2b phase1: unexpected {request.method} {request.url}')


def _phase2_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2b-phase2'})
    if path.endswith(f'/receipts/{RECEIPT_ID}'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2B-001',
            'updated_at': '2026-03-30T13:01:00+00:00',
        })
    raise AssertionError(f'gate2b phase2: unexpected {request.method} {request.url}')


def _strict_no_call(request: httpx.Request) -> httpx.Response:
    """Phase 3: transport must not be called — doc is already ACK."""
    raise AssertionError(
        f'gate2b phase3: transport must NOT be called when doc is already ACK: '
        f'{request.method} {request.url}'
    )


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate2b.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate2b',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2B-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T13:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2b',
        'business_ts': ts,
    }


def test_gate2b_repeated_reconciliation_does_not_duplicate_outbox(tmp_path: Path) -> None:
    """
    After reconciliation-ACK creates '{doc_id}-reconcile-outbox', a second
    reconciliation pass (doc already ACK) must not create a duplicate record.

    State machine protection: ACK is not in get_pending_for_reconciliation()
    candidate states, so enqueue_document() is never called a second time.
    enqueue_document() uses plain INSERT, so a duplicate call would raise
    IntegrityError — the state machine is the only guard.

    Timeline:
    1. Phase 1: SHIFT_OPEN + SELL → ACK (write-path outbox: '{doc_id}-outbox')
    2. Crash sim: reset SELL to SENT
    3. Phase 2: reconciliation → ACK (reconciliation outbox: '{doc_id}-reconcile-outbox')
    4. Phase 3: second reconciliation pass — doc is ACK, nothing enqueued
    5. Assert: total outbox count for doc_id == 2 (write-path + reconciliation, no more)
    """
    cfg = _config(tmp_path)

    # Phase 1: normal flow
    container1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(container1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2b-open'), 'business_ts': '2026-03-30T13:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2b', 'cashier_id': 'cashier-gate2b'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2b-sell-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2b-001',
                'cashier_id': 'cashier-gate2b',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Write-path outbox record must exist at this point
    with container1.connect() as conn:
        wp_count = conn.execute(
            "SELECT COUNT(*) FROM sync_outbox WHERE outbox_id=?",
            (f'{sell_doc_id}-outbox',),
        ).fetchone()[0]
    assert wp_count == 1, 'Write-path outbox record must exist after phase1 ACK'

    # Crash simulation: reset SELL to SENT
    with container1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

    # Phase 2: first reconciliation — transitions SENT → ACK, enqueues reconcile outbox
    container2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_mock))
    )
    with TestClient(create_app(container2)) as _:
        pass

    report2 = container2.last_startup_report
    assert report2.reconciliation_acked == 1, (
        f'Phase 2 must reconcile 1 doc: got {report2.reconciliation_acked}'
    )

    with container2.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        assert doc.state == DocumentState.ACK

        count_after_phase2 = conn.execute(
            "SELECT COUNT(*) FROM sync_outbox WHERE aggregate_id=?",
            (sell_doc_id,),
        ).fetchone()[0]

    # After phase 2: write-path + reconcile = 2 outbox records
    assert count_after_phase2 == 2, (
        f'Expected 2 outbox records after phase2 (write-path + reconcile): '
        f'got {count_after_phase2}'
    )

    # Phase 3: second reconciliation pass — doc already ACK, must be no-op
    container3 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_call))
    )
    with TestClient(create_app(container3)) as _:
        pass

    report3 = container3.last_startup_report
    assert report3.reconciliation_checked == 0, (
        f'Phase 3 must find no candidates (doc is ACK): got {report3.reconciliation_checked}'
    )
    assert report3.reconciliation_acked == 0

    with container3.connect() as conn:
        count_after_phase3 = conn.execute(
            "SELECT COUNT(*) FROM sync_outbox WHERE aggregate_id=?",
            (sell_doc_id,),
        ).fetchone()[0]

        # Verify both records individually
        wp_row = conn.execute(
            "SELECT status FROM sync_outbox WHERE outbox_id=?",
            (f'{sell_doc_id}-outbox',),
        ).fetchone()
        recon_row = conn.execute(
            "SELECT status FROM sync_outbox WHERE outbox_id=?",
            (f'{sell_doc_id}-reconcile-outbox',),
        ).fetchone()

    assert count_after_phase3 == 2, (
        f'Second reconciliation must not create duplicate outbox records: '
        f'expected 2, got {count_after_phase3}. '
        f'enqueue_document() uses plain INSERT — state machine is the only guard.'
    )
    assert wp_row is not None and wp_row[0] == 'PENDING', (
        f'Write-path outbox must remain PENDING: {wp_row}'
    )
    assert recon_row is not None and recon_row[0] == 'PENDING', (
        f'Reconcile outbox must remain PENDING: {recon_row}'
    )
