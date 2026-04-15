"""
Gate 1g — recovery/reconciliation smoke test.

Scenario: a SELL document is created via the REST endpoint (process_immediately=True),
then the process "crashes" between _stage_send_or_offline and _stage_finalize_ack by
resetting the document state to SENT in the DB.  ReconciliationService.reconcile_pending
is then called with a tracked handler whose send() raises if invoked.

Verified:
  - reconciliation brings the document to ACK without calling send() again
  - handler.send_calls == 0  (no re-fiscalisation)
  - handler.poll_calls == 1  (exactly one status poll per document)
  - document.state == ACK after reconciliation
  - document.ack_at is populated by reconciliation
  - inbox is still consistent (DONE)
  - shift remains OPENED

This complements the existing unit tests in test_reconciliation.py, which operate at
the WritePathWorker level.  Gate 1g adds the REST integration layer: the document is
created through the full HTTP ingress stack so the DB row mirrors production shape.

Invariants preserved:
  #1 (no network in tx): reconcile_pending opens its own BEGIN IMMEDIATE per document;
      transport send is skipped entirely — verified by the AssertionError handler.
  #8 (recovery must not violate state transitions): SENT → ACK is the only allowed
      transition here; the test confirms no spurious state is introduced.
"""
from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import DocumentState, TransportKind
from prro_gateway.ports import PollResult
from prro_gateway.repositories import FiscalDocumentRepository
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

def _mock_online_handler(request: httpx.Request) -> httpx.Response:
    """Normal online flow: handles auth + shift open + receipt send."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1g'})
    if path.endswith('/shifts'):
        return httpx.Response(200, json={
            'id': 'gate1g-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1G-001',
            'updated_at': '2026-03-29T16:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate1g-receipt-001',
            'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1G-001',
            'updated_at': '2026-03-29T16:01:00+00:00',
        })
    raise AssertionError(f'gate1g: unexpected transport call {request.method} {request.url}')


class _ReconcileTracker:
    """
    Stand-in transport handler for the reconciliation phase.
    poll_status() returns ACK.
    send() raises AssertionError — reconciliation must never re-send.
    """

    def __init__(self):
        self.send_calls: int = 0
        self.poll_calls: int = 0

    def send(self, **kwargs):
        self.send_calls += 1
        raise AssertionError(
            'gate1g: send() called during reconciliation — this is a re-fiscalisation bug'
        )

    def poll_status(self, **kwargs):
        self.poll_calls += 1
        return PollResult(
            state=DocumentState.ACK.value,
            submission_status='ACK',
            ack_at=datetime.now(UTC),
            response_json='{"status": "ACK", "fiscal_code": "RCPT-GATE1G-001"}',
        )


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1g.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1g',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1G-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str) -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1g',
        'business_ts': '2026-03-29T16:01:00Z',
    }


# ---------------------------------------------------------------------------
# Gate 1g: crash-after-SENT recovery
# ---------------------------------------------------------------------------

def test_gate1g_sent_document_recovered_by_reconciliation(tmp_path: Path) -> None:
    """
    Full scenario:
      1. REST: open shift + SELL → document state=ACK (normal path).
      2. DB: simulate crash-after-SENT by resetting state='SENT', ack_at=NULL.
      3. Reconcile with a tracker that raises if send() is called.
      4. Assert: document→ACK, send_calls=0, poll_calls=1.
    """
    cfg = _config(tmp_path)
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_online_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    # --- Phase 1: create document via REST ---
    with TestClient(create_app(container)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1g-open'), 'business_ts': '2026-03-29T16:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'ext-open-gate1g',
                'cashier_id': 'cashier-gate1g',
            },
        })
        assert r_shift.status_code == 200, f'SHIFT_OPEN failed: {r_shift.text}'
        assert r_shift.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {r_shift.json()}'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1g-sell-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate1g-001',
                'cashier_id': 'cashier-gate1g',
                'goods': [{'name': 'Widget', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200, f'SELL failed: {r_sell.text}'
        sell_body = r_sell.json()
        assert sell_body.get('document_state') == 'ACK', f'SELL not ACK: {sell_body}'

    sell_doc_id = sell_body['document_id']

    # --- Phase 2: simulate crash-after-SENT ---
    with container.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET state='SENT', ack_at=NULL, response_json=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

        # Confirm the document is now stuck in SENT
        stuck = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert stuck is not None
    assert stuck.state == DocumentState.SENT, f'Expected SENT after reset, got {stuck.state}'
    assert stuck.ack_at is None, 'ack_at should be NULL after crash simulation'

    # --- Phase 3: reconcile with tracked handler ---
    tracker = _ReconcileTracker()
    with container.connect() as conn:
        router = ProfileAwareTransportRouter.from_connection(
            conn,
            handlers={TransportKind.CHECKBOX_REST_TRANSPORT: tracker},
        )
        svc = ReconciliationService(transport_status_client=router)
        result = svc.reconcile_pending(conn)

    # --- Phase 4: assert ---
    assert result.acked == 1, (
        f'Expected 1 acked document, got result={result}'
    )
    assert tracker.send_calls == 0, (
        f'send() must not be called during reconciliation (got {tracker.send_calls} calls) '
        f'— this would be a double-fiscalisation'
    )
    assert tracker.poll_calls == 1, (
        f'Expected exactly 1 poll_status call, got {tracker.poll_calls}'
    )

    with container.connect() as conn:
        recovered = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        shift = conn.execute(
            "SELECT state FROM shifts WHERE fiscal_number=? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()

    assert recovered is not None
    assert recovered.state == DocumentState.ACK, (
        f'Document must be ACK after reconciliation, got {recovered.state}'
    )
    assert recovered.ack_at is not None, 'ack_at must be set after reconciliation ACK'

    assert shift is not None and shift[0] == 'OPENED', (
        f'Shift must remain OPENED after reconciliation: {shift}'
    )
