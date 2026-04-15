"""
Gate 1q — offline reconciliation exclusion smoke tests.

Verifies that ReconciliationService.reconcile_pending() does NOT process
offline documents and does NOT call send() or poll_status() on them.

Exclusion mechanism:
  FiscalDocumentRepository.get_pending_for_reconciliation() filters by:
    state IN ('SENT','KVT1','KVT2','ERROR_RETRYABLE','REQUIRES_MANUAL_RECONCILIATION')

  An offline SELL completes with state='OFFLINE_LOCAL_ACK' (set by _stage_finalize_ack
  when fs_mode='OFFLINE').  OFFLINE_LOCAL_ACK is not in that set → the document is
  never picked up.

  The submission_status='OFFLINE_LOCAL' marker provides an additional semantic
  anchor but is NOT relied on by the query — the state filter is sufficient.

Two vectors:
  A — pure offline: only offline OFFLINE_LOCAL_ACK document exists.
      reconcile_pending() must find nothing; send_calls=0, poll_calls=0.
  B — mixed: one offline OFFLINE_LOCAL_ACK + one online SENT (crash-simulated).
      reconcile_pending() must process only the online SENT document;
      poll_calls=1, send_calls=0, offline document untouched.

Invariants:
  #1 (no network in tx): reconcile_pending opens its own BEGIN IMMEDIATE;
      transport is never reached for offline docs.
  #8 (recovery must not violate state transitions): offline OFFLINE_LOCAL_ACK must not
      transition to any other state during reconciliation.
"""
from __future__ import annotations

from datetime import UTC, datetime, timedelta
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState, NodeMode, TransportKind
from prro_gateway.ports import PollResult
from prro_gateway.repositories import FiscalDocumentRepository
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.repositories.offline import OfflineRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app
from prro_gateway.services.reconciliation import ReconciliationService
from prro_gateway.transports import ProfileAwareTransportRouter

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


# ---------------------------------------------------------------------------
# Transport mocks
# ---------------------------------------------------------------------------

def _online_mock(request: httpx.Request) -> httpx.Response:
    """Handles auth + shift open + sell; raises on anything unexpected."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1q'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1q-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1Q-001',
            'updated_at': '2026-03-29T14:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate1q-receipt-001',
            'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1Q-001',
            'updated_at': '2026-03-29T14:01:00+00:00',
        })
    raise AssertionError(f'gate1q: unexpected transport {request.method} {request.url}')


def _offline_only_mock(request: httpx.Request) -> httpx.Response:
    """Only shift open allowed; receipt call → AssertionError."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1q'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1q-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1Q-001',
            'updated_at': '2026-03-29T14:00:00+00:00',
        })
    raise AssertionError(
        f'gate1q: transport must NOT be called in OFFLINE mode — '
        f'unexpected {request.method} {request.url}'
    )


class _StrictNoCallTracker:
    """Tracker for Vector A: raises if send() or poll_status() is ever called."""

    def __init__(self):
        self.send_calls: int = 0
        self.poll_calls: int = 0

    def send(self, **kwargs):
        self.send_calls += 1
        raise AssertionError(
            'gate1q: send() called during reconciliation of offline-only set'
        )

    def poll_status(self, **kwargs):
        self.poll_calls += 1
        raise AssertionError(
            'gate1q: poll_status() called during reconciliation of offline-only set — '
            'offline ACK documents must not enter the reconciliation queue'
        )


class _OnlineOnlyPollTracker:
    """Tracker for Vector B: poll_status returns ACK; send raises."""

    def __init__(self):
        self.send_calls: int = 0
        self.poll_calls: int = 0

    def send(self, **kwargs):
        self.send_calls += 1
        raise AssertionError(
            'gate1q: send() called during reconciliation — re-fiscalisation bug'
        )

    def poll_status(self, **kwargs):
        self.poll_calls += 1
        return PollResult(
            state=DocumentState.ACK.value,
            submission_status='ACK',
            ack_at=datetime.now(UTC),
            response_json='{"status": "ACK", "fiscal_code": "RCPT-GATE1Q-001"}',
        )


# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------

def _config(tmp_path: Path, db_name: str = 'gate1q.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate1q',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1Q-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T14:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1q',
        'business_ts': ts,
    }


def _open_shift(client: TestClient, req_id: str = 'gate1q-open') -> None:
    resp = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx(req_id), 'business_ts': '2026-03-29T14:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': f'ext-open-{req_id}',
            'cashier_id': 'cashier-gate1q',
        },
    })
    assert resp.status_code == 200, f'SHIFT_OPEN failed: {resp.text}'
    assert resp.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {resp.json()}'


def _seed_offline(container: RuntimeContainer, session_id: str, range_id: str) -> None:
    _session_start = (datetime.now(UTC) - timedelta(minutes=5)).strftime('%Y-%m-%dT%H:%M:%S')
    with container.connect() as conn:
        conn.execute('BEGIN IMMEDIATE')
        NodeStateRepository.update_mode(conn, fiscal_number=FISCAL_NUMBER, mode=NodeMode.OFFLINE)
        OfflineRepository.create_open_session(
            conn,
            offline_session_id=session_id,
            fiscal_number=FISCAL_NUMBER,
            started_at=_session_start,
            status='OPEN',
            accumulated_month_seconds=0,
        )
        conn.execute(
            """
            INSERT INTO offline_ranges
                (range_id, fiscal_number, first_fiscal_no, last_fiscal_no,
                 next_fiscal_no, issued_at, status)
            VALUES (?, ?, 1001, 1010, 1001, ?, 'ACTIVE')
            """,
            (range_id, FISCAL_NUMBER, _session_start),
        )
        conn.commit()


def _sell_payload(req_id: str, ext_id: str) -> dict:
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1q',
            'goods': [{'name': 'Coffee', 'price': 3000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 3000}],
        },
    }


def _make_reconciliation_service(
    container: RuntimeContainer,
    conn,
    tracker,
) -> ReconciliationService:
    router = ProfileAwareTransportRouter.from_connection(
        conn,
        handlers={TransportKind.CHECKBOX_REST_TRANSPORT: tracker},
    )
    return ReconciliationService(transport_status_client=router)


# ---------------------------------------------------------------------------
# Vector A — pure offline: reconciliation finds nothing to process
# ---------------------------------------------------------------------------

def test_gate1q_offline_document_excluded_from_reconciliation(tmp_path: Path) -> None:
    """
    Single offline ACK document must not enter the reconciliation queue.

    reconcile_pending() must return 0 acked / 0 still_pending.
    The strict tracker raises if poll_status() or send() is called.
    """
    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_offline_only_mock)),
    )

    with TestClient(create_app(container)) as client:
        _open_shift(client)
        _seed_offline(container, 'gate1q-session-vec-a', 'gate1q-range-vec-a')

        r_sell = client.post('/v1/ingress/checkbox',
                             json=_sell_payload('gate1q-sell-vec-a', 'ext-sell-gate1q-vec-a'))
        assert r_sell.status_code == 200, f'Offline SELL failed: {r_sell.text}'
        assert r_sell.json().get('document_state') == 'OFFLINE_LOCAL_ACK', (
            f'Offline SELL must be OFFLINE_LOCAL_ACK: {r_sell.json()}'
        )
        offline_doc_id = r_sell.json()['document_id']

    tracker = _StrictNoCallTracker()

    with container.connect() as conn:
        # Verify document is in OFFLINE_LOCAL_ACK state with OFFLINE markers before reconciliation
        doc_before = FiscalDocumentRepository.get_by_id(conn, offline_doc_id)
        assert doc_before is not None
        assert doc_before.state == DocumentState.OFFLINE_LOCAL_ACK
        assert doc_before.fs_mode == 'OFFLINE'
        assert doc_before.submission_status == 'OFFLINE_LOCAL'

        # Run reconciliation — must touch nothing
        svc = _make_reconciliation_service(container, conn, tracker)
        result = svc.reconcile_pending(conn)

    # Tracker must never have been called
    assert tracker.send_calls == 0, (
        f'send() called {tracker.send_calls} time(s) during reconciliation of offline-only DB'
    )
    assert tracker.poll_calls == 0, (
        f'poll_status() called {tracker.poll_calls} time(s) — offline ACK must not be reconciled'
    )

    # Result must reflect nothing processed
    assert result.acked == 0, (
        f'reconcile_pending must ACK 0 documents (offline doc is already OFFLINE_LOCAL_ACK), got {result.acked}'
    )
    assert result.still_pending == 0, (
        f'reconcile_pending must report 0 pending, got {result.still_pending}'
    )

    # Offline document must remain exactly as-is
    with container.connect() as conn:
        doc_after = FiscalDocumentRepository.get_by_id(conn, offline_doc_id)
    assert doc_after is not None
    assert doc_after.state == DocumentState.OFFLINE_LOCAL_ACK, (
        f'Offline document state changed unexpectedly: {doc_after.state}'
    )
    assert doc_after.submission_status == 'OFFLINE_LOCAL', (
        f'submission_status changed: {doc_after.submission_status!r}'
    )
    assert doc_after.offline_fiscal_no is not None, (
        'offline_fiscal_no must remain set'
    )


# ---------------------------------------------------------------------------
# Vector B — mixed: offline ACK + online SENT
# ---------------------------------------------------------------------------

def test_gate1q_reconciliation_processes_only_online_sent_document(tmp_path: Path) -> None:
    """
    With one offline ACK document and one online SENT (crash-simulated) document:
    - reconcile_pending() must call poll_status() exactly once (for the SENT doc).
    - The offline doc must not be touched.
    - After reconciliation: online doc → ACK, offline doc → still ACK/OFFLINE_LOCAL.
    """
    cfg = _config(tmp_path, 'gate1q_vec_b.sqlite3')
    container = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_online_mock)),
    )

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1q-open-b')

        # --- Online SELL (normal, will be crash-simulated later) ---
        r_online = client.post('/v1/ingress/checkbox',
                               json=_sell_payload('gate1q-sell-online', 'ext-sell-online-gate1q'))
        assert r_online.status_code == 200, f'Online SELL failed: {r_online.text}'
        assert r_online.json().get('document_state') == 'ACK', (
            f'Online SELL must ACK: {r_online.json()}'
        )
        online_doc_id = r_online.json()['document_id']

        # Simulate crash-after-SENT: reset online doc to SENT
        with container.connect() as conn:
            conn.execute(
                "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
                "WHERE document_id=?",
                (online_doc_id,),
            )
            conn.commit()

        # --- Offline SELL ---
        _seed_offline(container, 'gate1q-session-vec-b', 'gate1q-range-vec-b')

        r_offline = client.post('/v1/ingress/checkbox',
                                json=_sell_payload('gate1q-sell-offline', 'ext-sell-offline-gate1q'))
        assert r_offline.status_code == 200, f'Offline SELL failed: {r_offline.text}'
        assert r_offline.json().get('document_state') == 'OFFLINE_LOCAL_ACK', (
            f'Offline SELL must be OFFLINE_LOCAL_ACK: {r_offline.json()}'
        )
        offline_doc_id = r_offline.json()['document_id']

    # Confirm pre-reconciliation states
    with container.connect() as conn:
        online_pre = FiscalDocumentRepository.get_by_id(conn, online_doc_id)
        offline_pre = FiscalDocumentRepository.get_by_id(conn, offline_doc_id)

    assert online_pre.state == DocumentState.SENT, (
        f'Online doc must be SENT before reconciliation, got {online_pre.state}'
    )
    assert offline_pre.state == DocumentState.OFFLINE_LOCAL_ACK, (
        f'Offline doc must be OFFLINE_LOCAL_ACK before reconciliation, got {offline_pre.state}'
    )
    assert offline_pre.fs_mode == 'OFFLINE'

    # Run reconciliation
    tracker = _OnlineOnlyPollTracker()
    with container.connect() as conn:
        svc = _make_reconciliation_service(container, conn, tracker)
        result = svc.reconcile_pending(conn)

    # Exactly 1 poll — only the online SENT doc
    assert tracker.send_calls == 0, (
        f'send() must not be called during reconciliation, got {tracker.send_calls}'
    )
    assert tracker.poll_calls == 1, (
        f'Expected 1 poll_status call (online SENT doc only), got {tracker.poll_calls}'
    )
    assert result.acked == 1, (
        f'Expected 1 acked document (online SENT → ACK), got {result.acked}'
    )

    # Post-reconciliation: online → ACK, offline → unchanged
    with container.connect() as conn:
        online_post = FiscalDocumentRepository.get_by_id(conn, online_doc_id)
        offline_post = FiscalDocumentRepository.get_by_id(conn, offline_doc_id)

    assert online_post.state == DocumentState.ACK, (
        f'Online document must be ACK after reconciliation, got {online_post.state}'
    )
    assert online_post.ack_at is not None, 'online doc.ack_at must be set after reconciliation'

    assert offline_post.state == DocumentState.OFFLINE_LOCAL_ACK, (
        f'Offline document must remain OFFLINE_LOCAL_ACK after reconciliation, got {offline_post.state}'
    )
    assert offline_post.submission_status == 'OFFLINE_LOCAL', (
        f'Offline submission_status must remain OFFLINE_LOCAL, got {offline_post.submission_status!r}'
    )
    assert offline_post.offline_fiscal_no is not None, (
        'Offline fiscal_no must remain set after reconciliation'
    )
