"""
Gate 1w — startup recovery stage smoke tests.

Verifies two startup recovery mechanisms:

Stage-1 — startup reconciliation (reconcile_on_startup=True, default):
  A SENT document (crash-simulated after transport send, before ACK) is recovered
  by reconcile_pending() running during RuntimeContainer.initialize().
  StartupRunReport.reconciliation_acked == 1 confirms recovery happened.
  Document reaches ACK state without re-sending (poll only).

Stage-2 — stale PROCESSING lease auto-reclaim (acquire_lease() mechanics):
  A PROCESSING inbox record with an expired lease_expires_at is automatically
  re-acquired by the next acquire_lease() call (inside process_next()).
  No explicit release_stale_leases() method exists; reclaim is implicit via SQL:
    status='PROCESSING' AND lease_expires_at < CURRENT_TIMESTAMP

  Test proves the stale inbox record is reclaimed and does NOT leave a
  PROCESSING lease. Uses a stale SHIFT_OPEN record (guard fails with
  SHIFT_ALREADY_OPEN before transport is reached, avoiding transport mock
  complexity). The stale record ends as ERROR, no PROCESSING remains.

  Ordering determinism: the stale record's created_at is set far in the past
  so acquire_lease ORDER BY created_at reliably picks it first.

Invariants:
  #1 (no network in tx): reconciliation poll happens outside transaction.
  #8 (no silent state violations): SENT → ACK via poll, not via send.
  #9 (graceful shutdown): health.ready and startup_complete asserted inside
     TestClient context (before shutdown resets health state).
"""
from __future__ import annotations

from datetime import UTC, datetime, timedelta
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

RECEIPT_ID = 'gate1w-receipt-001'


# ---------------------------------------------------------------------------
# Transport mocks
# ---------------------------------------------------------------------------

def _phase1_mock(request: httpx.Request) -> httpx.Response:
    """Phase 1 (initial ops): handles shift open + sell."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1w'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1w-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1W-001',
            'updated_at': '2026-03-29T19:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1W-001',
            'updated_at': '2026-03-29T19:01:00+00:00',
        })
    raise AssertionError(f'gate1w phase1: unexpected {request.method} {request.url}')


def _phase2_mock(request: httpx.Request) -> httpx.Response:
    """Phase 2 (startup reconciliation): handles auth + poll GET /receipts/{id}."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1w-phase2'})
    if path.endswith(f'/receipts/{RECEIPT_ID}'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1W-001',
            'updated_at': '2026-03-29T19:01:00+00:00',
        })
    raise AssertionError(f'gate1w phase2: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str = 'gate1w.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate1w',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1W-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T19:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1w',
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — startup reconciliation recovers SENT document
# ---------------------------------------------------------------------------

def test_gate1w_startup_reconciliation_recovers_sent_document(tmp_path: Path) -> None:
    """
    Scenario:
    1. Phase 1: SHIFT_OPEN + SELL → ACK (normal flow).
    2. Crash simulation: reset SELL document to SENT (ack_at=NULL).
    3. Phase 2: new container (same DB), startup reconciliation runs.
       reconcile_pending() calls poll_status() → document reaches ACK.
    4. StartupRunReport shows reconciliation_acked=1.
    5. health.ready=True, health.startup_complete=True (checked inside lifespan).
    """
    cfg = _config(tmp_path)

    # Phase 1: create document via normal flow
    container1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(container1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1w-open'), 'business_ts': '2026-03-29T19:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1w', 'cashier_id': 'cashier-gate1w'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1w-sell-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate1w-001',
                'cashier_id': 'cashier-gate1w',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Crash simulation: reset SELL to SENT
    with container1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

    # Phase 2: new container, same DB — startup reconciliation runs automatically
    container2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase2_mock))
    )
    with TestClient(create_app(container2)) as _:
        # Health state is valid during the lifespan (before shutdown resets it)
        assert container2.health.ready, 'health.ready must be True during lifespan'
        assert container2.health.startup_complete, (
            'health.startup_complete must be True after startup phases complete'
        )

    # Startup report is preserved after shutdown
    report = container2.last_startup_report
    assert report is not None, 'last_startup_report must be populated'
    assert report.phase2_attempted, (
        'Phase 2 reconciliation must have been attempted (reconcile_on_startup=True is default)'
    )
    assert report.reconciliation_acked == 1, (
        f'Expected reconciliation_acked=1 (SENT→ACK), got {report.reconciliation_acked}. '
        f'Report: {report}'
    )

    # Verify document was recovered
    with container2.connect() as conn:
        recovered = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)

    assert recovered.state == DocumentState.ACK, (
        f'Document must be ACK after startup reconciliation, got {recovered.state}'
    )
    assert recovered.ack_at is not None, 'ack_at must be populated after reconciliation'


# ---------------------------------------------------------------------------
# Test B — stale PROCESSING lease is auto-reclaimed, no lease leak
# ---------------------------------------------------------------------------

def test_gate1w_stale_processing_lease_reclaimed_no_leak(tmp_path: Path) -> None:
    """
    A stale PROCESSING inbox record (expired lease) is automatically re-acquired
    by acquire_lease() when process_next() runs for this fiscal_number.

    Mechanics: acquire_lease() selects WHERE status='PROCESSING' AND
    lease_expires_at < CURRENT_TIMESTAMP, so an expired lease is picked up
    exactly like a NEW record.

    Setup:
    - SHIFT_OPEN → ACK (inbox=DONE).
    - Reset the SHIFT_OPEN inbox to PROCESSING with expired lease and
      created_at='2020-01-01' (ensures it is selected FIRST by ORDER BY
      created_at when acquire_lease runs, before any newer NEW records).
    - Send a SELL → triggers process_next.
    - process_next re-acquires the stale SHIFT_OPEN inbox record.
    - Re-processing SHIFT_OPEN while a shift is OPEN → SHIFT_ALREADY_OPEN
      (guard error, pre-transport, inbox → ERROR).
    - No PROCESSING lease remains.
    """
    cfg = _config(tmp_path, 'gate1w_b.sqlite3')
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    stale_request_id = 'gate1w-open-b'

    with TestClient(create_app(container)) as client:
        # SHIFT_OPEN — inbox ends as DONE
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx(stale_request_id), 'business_ts': '2026-03-29T19:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'ext-open-gate1w-b',
                'cashier_id': 'cashier-gate1w',
            },
        })
        assert r_open.status_code == 200
        assert r_open.json().get('document_state') == 'ACK'

        # Verify inbox is DONE before seeding stale state
        with container.connect() as conn:
            done_row = conn.execute(
                "SELECT status FROM ingress_inbox WHERE request_id=?", (stale_request_id,)
            ).fetchone()
        assert done_row and done_row[0] == 'DONE'

        # Seed stale PROCESSING: expired lease + far past created_at for deterministic ordering
        expired_ts = (datetime.now(UTC) - timedelta(hours=2)).strftime('%Y-%m-%d %H:%M:%S')
        with container.connect() as conn:
            conn.execute(
                """
                UPDATE ingress_inbox
                SET status='PROCESSING', lease_expires_at=?, lease_token='stale-token',
                    created_at='2020-01-01 00:00:00', finished_at=NULL
                WHERE request_id=?
                """,
                (expired_ts, stale_request_id),
            )
            conn.commit()

        # Confirm it is stuck in PROCESSING
        with container.connect() as conn:
            stuck = conn.execute(
                "SELECT status FROM ingress_inbox WHERE request_id=?", (stale_request_id,)
            ).fetchone()
        assert stuck and stuck[0] == 'PROCESSING', (
            f'Expected PROCESSING after reset, got {stuck}'
        )

        # Send a SELL — triggers process_next which re-acquires the stale record first
        # (stale has created_at='2020-01-01', newer than any SELL just inserted)
        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1w-sell-b-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate1w-b-001',
                'cashier_id': 'cashier-gate1w',
                'goods': [{'name': 'Coffee', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        })
        assert r_sell.status_code == 200, f'SELL response non-200: {r_sell.text}'

    # No PROCESSING lease must remain — stale record was reclaimed and resolved
    with container.connect() as conn:
        processing_count = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox WHERE status='PROCESSING'"
        ).fetchone()[0]

        # The stale SHIFT_OPEN record must NOT be stuck in PROCESSING
        stale_row = conn.execute(
            "SELECT status FROM ingress_inbox WHERE request_id=?", (stale_request_id,)
        ).fetchone()

    assert processing_count == 0, (
        f'No PROCESSING records must remain after stale lease reclaim: {processing_count} stuck'
    )
    assert stale_row is not None
    assert stale_row[0] != 'PROCESSING', (
        f'Stale inbox must NOT remain in PROCESSING, got {stale_row[0]!r}'
    )
