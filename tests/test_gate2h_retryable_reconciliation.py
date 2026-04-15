"""
Gate 2h — RETRYABLE reconciliation branch smoke tests.

Documents and verifies the RETRYABLE outcome path in ReconciliationService.reconcile_pending()
when poll_status() returns retryable=True.

Current implementation trigger:
  transport_request_id is None → CheckboxRestTransportClient.poll_status() returns
  PollResult(state=ERROR_RETRYABLE, submission_status='MISSING_TRANSPORT_REQUEST_ID', retryable=True)
  without making any HTTP call.

RETRYABLE branch effects:
  - fiscal_documents.state   → ERROR_RETRYABLE
  - recovery_attempts        → incremented by 1 (via set_recovery_attempts)
  - audit_log                → DOCUMENT_RECONCILED_RETRYABLE, severity=WARNING
                               payload={'state': 'ERROR_RETRYABLE', 'recovery_attempts': N}
  - transport_trace_log      → '{doc_id}-reconcile-poll-{N}', direction=EVENT,
                               technical_status=RECONCILE_POLL
                               (N-suffix prevents PRIMARY KEY collision on repeated retries)
  - ReconciliationRunResult  → retryable += 1
  - No outbox record enqueued (only ACK branch enqueues)
  - No HTTP send triggered (poll is read-only; RETRYABLE does not re-send)

Three tests:
  A — basic RETRYABLE: state=ERROR_RETRYABLE, recovery_attempts=1, trace created
  B — second pass: recovery_attempts increments to 2, second trace_id is {doc_id}-reconcile-poll-2
  C — no send: retryable path creates no outbox record and calls no send HTTP endpoint
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
RECEIPT_ID = 'gate2h-receipt-001'


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2h'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2h-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2H-001',
            'updated_at': '2026-03-30T19:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2H-001',
            'updated_at': '2026-03-30T19:01:00+00:00',
        })
    raise AssertionError(f'gate2h phase1: unexpected {request.method} {request.url}')


def _strict_no_http_mock(request: httpx.Request) -> httpx.Response:
    """Phase 2 transport mock: raises if any HTTP call is attempted.

    When transport_request_id is None, poll_status() returns early before any HTTP call.
    If this mock is ever called, the test fails — proving no send occurred.
    """
    raise AssertionError(
        f'gate2h retryable phase: unexpected HTTP call {request.method} {request.url}. '
        f'RETRYABLE path (missing transport_request_id) must not make HTTP calls.'
    )


def _config(tmp_path: Path, db_name: str = 'gate2h.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate2h',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2H-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T19:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2h',
        'business_ts': ts,
    }


def _setup_sent_no_transport_id(tmp_path: Path, db_name: str) -> tuple[RuntimeContainer, str]:
    """Phase 1: SHIFT_OPEN + SELL → ACK.

    Crash-sim: reset SELL to SENT and clear transport_request_id.
    Missing transport_request_id causes poll_status() to return retryable=True
    without any HTTP calls.
    """
    cfg = _config(tmp_path, db_name)
    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2h-open-' + db_name), 'business_ts': '2026-03-30T19:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-' + db_name, 'cashier_id': 'cashier-gate2h'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2h-sell-' + db_name),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-' + db_name,
                'cashier_id': 'cashier-gate2h',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    # Crash-sim: SENT + no transport_request_id → triggers MISSING_TRANSPORT_REQUEST_ID retryable path
    with c1.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents "
            "SET state='SENT', ack_at=NULL, response_json=NULL, transport_request_id=NULL "
            "WHERE document_id=?",
            (sell_doc_id,),
        )
        conn.commit()

    return c1, sell_doc_id


# ---------------------------------------------------------------------------
# Test A — basic RETRYABLE: state, recovery_attempts, trace
# ---------------------------------------------------------------------------

def test_gate2h_retryable_sets_error_retryable_and_writes_trace(tmp_path: Path) -> None:
    """
    When reconciliation poll_status() returns retryable=True (missing transport_request_id):
    - doc.state == ERROR_RETRYABLE
    - doc.recovery_attempts == 1 (was 0, incremented by RETRYABLE branch)
    - transport_trace_log: '{doc_id}-reconcile-poll-1' with direction=EVENT,
      technical_status=RECONCILE_POLL, business_status='MISSING_TRANSPORT_REQUEST_ID'
    - audit_log: DOCUMENT_RECONCILED_RETRYABLE, severity=WARNING,
      payload={'state': 'ERROR_RETRYABLE', 'recovery_attempts': 1}
    - ReconciliationRunResult.retryable == 1
    """
    c1, sell_doc_id = _setup_sent_no_transport_id(tmp_path, 'gate2h_a.sqlite3')
    cfg = _config(tmp_path, 'gate2h_a.sqlite3')

    # Phase 2: retryable reconciliation — strict mock ensures no HTTP calls
    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c2)) as _:
        pass

    assert c2.last_startup_report.reconciliation_retryable == 1, (
        f'ReconciliationRunResult.retryable must be 1: '
        f'got {c2.last_startup_report.reconciliation_retryable}'
    )

    with c2.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)

    assert doc.state == DocumentState.ERROR_RETRYABLE, (
        f'doc.state must be ERROR_RETRYABLE after retryable poll: got {doc.state}'
    )
    assert doc.recovery_attempts == 1, (
        f'recovery_attempts must be incremented to 1: got {doc.recovery_attempts}'
    )

    with c2.connect() as conn:
        poll_row = conn.execute(
            "SELECT direction, technical_status, business_status "
            "FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-reconcile-poll-1',),
        ).fetchone()

        audit_rows = conn.execute(
            "SELECT severity, event_payload_json "
            "FROM audit_log WHERE entity_id=? AND event_type=?",
            (sell_doc_id, 'DOCUMENT_RECONCILED_RETRYABLE'),
        ).fetchall()

    assert poll_row is not None, (
        f'transport_trace_log must contain {{doc_id}}-reconcile-poll-1 after RETRYABLE reconciliation. '
        f'trace_id={sell_doc_id}-reconcile-poll-1 not found.'
    )
    direction, technical_status, business_status = poll_row
    assert direction == 'EVENT', f'direction must be EVENT: got {direction!r}'
    assert technical_status == 'RECONCILE_POLL', f'technical_status must be RECONCILE_POLL: got {technical_status!r}'
    assert business_status == 'MISSING_TRANSPORT_REQUEST_ID', (
        f'business_status must reflect submission_status from PollResult: got {business_status!r}'
    )

    assert len(audit_rows) == 1, (
        f'Exactly 1 DOCUMENT_RECONCILED_RETRYABLE audit record expected: got {len(audit_rows)}'
    )
    severity, payload_json = audit_rows[0]
    assert severity == 'WARNING', f'audit severity must be WARNING: got {severity!r}'
    payload = json.loads(payload_json)
    assert payload == {'state': 'ERROR_RETRYABLE', 'recovery_attempts': 1}, (
        f'audit payload mismatch: got {payload}'
    )


# ---------------------------------------------------------------------------
# Test B — second pass: recovery_attempts increments, trace_id uses new suffix
# ---------------------------------------------------------------------------

def test_gate2h_retryable_second_pass_increments_attempts(tmp_path: Path) -> None:
    """
    After a second reconciliation run on a doc already in ERROR_RETRYABLE
    (still with transport_request_id=NULL):
    - recovery_attempts increments from 1 → 2
    - transport_trace_log gains a NEW record: '{doc_id}-reconcile-poll-2'
    - First trace '{doc_id}-reconcile-poll-1' is unchanged (no overwrite)
    - doc.state stays ERROR_RETRYABLE
    """
    c1, sell_doc_id = _setup_sent_no_transport_id(tmp_path, 'gate2h_b.sqlite3')
    cfg = _config(tmp_path, 'gate2h_b.sqlite3')

    # Pass 1
    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c2)) as _:
        pass
    assert c2.last_startup_report.reconciliation_retryable == 1

    # Pass 2
    c3 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c3)) as _:
        pass
    assert c3.last_startup_report.reconciliation_retryable == 1

    with c3.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
        assert doc.state == DocumentState.ERROR_RETRYABLE
        assert doc.recovery_attempts == 2, (
            f'recovery_attempts must be 2 after second retryable pass: got {doc.recovery_attempts}'
        )

        trace1 = conn.execute(
            "SELECT direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-reconcile-poll-1',),
        ).fetchone()
        trace2 = conn.execute(
            "SELECT direction FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-reconcile-poll-2',),
        ).fetchone()

    assert trace1 is not None and trace1[0] == 'EVENT', (
        f'First reconcile-poll-1 trace must still exist after second pass'
    )
    assert trace2 is not None and trace2[0] == 'EVENT', (
        f'Second reconcile-poll-2 trace must exist after second pass. '
        f'Suffix-based trace_id prevents PRIMARY KEY collision.'
    )


# ---------------------------------------------------------------------------
# Test C — no send: retryable path creates no outbox record
# ---------------------------------------------------------------------------

def test_gate2h_retryable_does_not_enqueue_outbox(tmp_path: Path) -> None:
    """
    RETRYABLE reconciliation branch does NOT enqueue an outbox record.
    Only the ACK branch calls OutboxRepository.enqueue_document().

    After retryable reconciliation:
    - No outbox record with aggregate_id=sell_doc_id exists in sync_outbox
    - (The write-path outbox record {doc_id}-outbox may or may not exist,
       but no reconcile-outbox record is created by RETRYABLE branch)
    """
    c1, sell_doc_id = _setup_sent_no_transport_id(tmp_path, 'gate2h_c.sqlite3')

    # Also remove any write-path outbox record so we can assert cleanly
    with c1.connect() as conn:
        conn.execute("DELETE FROM sync_outbox WHERE aggregate_id=?", (sell_doc_id,))
        conn.commit()

    cfg = _config(tmp_path, 'gate2h_c.sqlite3')
    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c2)) as _:
        pass
    assert c2.last_startup_report.reconciliation_retryable == 1

    with c2.connect() as conn:
        outbox_rows = conn.execute(
            "SELECT outbox_id FROM sync_outbox WHERE aggregate_id=?",
            (sell_doc_id,),
        ).fetchall()

    assert outbox_rows == [], (
        f'RETRYABLE reconciliation must NOT create an outbox record. '
        f'Found: {[r[0] for r in outbox_rows]}'
    )
