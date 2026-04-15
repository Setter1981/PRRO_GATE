"""
Gate 2i — recovery attempts ceiling / retry loop semantics.

Documents the CURRENT behavior: there is NO ceiling on recovery_attempts.
ReconciliationService.reconcile_pending() has no max_recovery_attempts check.
Documents with DocumentState.ERROR_RETRYABLE remain reconciliation candidates
indefinitely across startup cycles.

Current behavior (Variant B — ceiling not implemented):
  - recovery_attempts increments unboundedly on each RETRYABLE reconciliation pass
  - doc.state stays ERROR_RETRYABLE after every pass
  - doc is selected as a reconciliation candidate again on the next startup
  - ReconciliationRunResult.retryable == 1 on every pass (not still_pending)
  - REQUIRES_MANUAL_RECONCILIATION state exists in enums and in candidate WHERE clause
    but NO code path in ReconciliationService transitions ERROR_RETRYABLE → REQUIRES_MANUAL_RECONCILIATION

Why there is no minimal local fix here:
  - Adding a ceiling requires: (1) config schema change (max_recovery_attempts in RuntimeConfig),
    (2) wiring through RuntimeContainer, (3) branching logic in ReconciliationService.
  - The target state semantics (REQUIRES_MANUAL_RECONCILIATION vs other) require
    a policy/design decision — not a one-liner.
  - This is documented as a known gap, not a bug to fix silently.

Two tests:
  A — unbounded growth: 3 consecutive passes, recovery_attempts grows 1→2→3, state stays ERROR_RETRYABLE
  B — still a candidate: after ERROR_RETRYABLE, the next reconciliation pass still picks up the doc
      (reconciliation_retryable == 1, not still_pending == 1)
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
RECEIPT_ID = 'gate2i-receipt-001'

_RETRYABLE_PASSES = 3  # number of consecutive retryable reconciliation passes to simulate


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2i'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2i-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2I-001',
            'updated_at': '2026-03-30T20:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2I-001',
            'updated_at': '2026-03-30T20:01:00+00:00',
        })
    raise AssertionError(f'gate2i phase1: unexpected {request.method} {request.url}')


def _strict_no_http_mock(request: httpx.Request) -> httpx.Response:
    """Raises if any HTTP call is made — proves no send occurs during RETRYABLE reconciliation."""
    raise AssertionError(
        f'gate2i: unexpected HTTP call {request.method} {request.url}. '
        f'RETRYABLE path (missing transport_request_id) must not make HTTP calls.'
    )


def _config(tmp_path: Path, db_name: str) -> AppConfig:
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
            'channel_owner': 'gate2i',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2I-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T20:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2h',
        'business_ts': ts,
    }


def _setup_sent_no_transport_id(tmp_path: Path, db_name: str) -> tuple[RuntimeContainer, str]:
    """Phase 1: SHIFT_OPEN + SELL → ACK. Crash-sim: SENT + transport_request_id=NULL."""
    cfg = _config(tmp_path, db_name)
    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2i-open-' + db_name), 'business_ts': '2026-03-30T20:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-' + db_name, 'cashier_id': 'cashier-gate2i'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2i-sell-' + db_name),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-' + db_name,
                'cashier_id': 'cashier-gate2i',
                'goods': [{'name': 'Coffee', 'price': 2000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2000}],
            },
        })
        assert r_sell.status_code == 200 and r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

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
# Test A — unbounded growth: 3 passes, recovery_attempts grows, state unchanged
# ---------------------------------------------------------------------------

def test_gate2i_recovery_attempts_grows_without_ceiling(tmp_path: Path) -> None:
    """
    Documents current model: NO ceiling on recovery_attempts.

    After 3 consecutive RETRYABLE reconciliation passes:
    - doc.state == ERROR_RETRYABLE (unchanged — no terminal transition)
    - doc.recovery_attempts == 3 (grows linearly)
    - transport_trace_log contains 3 records: reconcile-poll-1, -2, -3
    - No REQUIRES_MANUAL_RECONCILIATION transition occurs

    This test should FAIL if a ceiling is ever implemented — at that point
    it documents a behavior change and this test must be updated.
    """
    db_name = 'gate2i_a.sqlite3'
    c1, sell_doc_id = _setup_sent_no_transport_id(tmp_path, db_name)
    cfg = _config(tmp_path, db_name)

    # Run _RETRYABLE_PASSES consecutive reconciliation cycles
    for pass_n in range(1, _RETRYABLE_PASSES + 1):
        c = RuntimeContainer(
            cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
        )
        with TestClient(create_app(c)) as _:
            pass
        assert c.last_startup_report.reconciliation_retryable == 1, (
            f'Pass {pass_n}: expected retryable==1, got {c.last_startup_report.reconciliation_retryable}'
        )

    with cfg and c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)

        poll_traces = conn.execute(
            "SELECT trace_id FROM transport_trace_log "
            "WHERE trace_id LIKE ? ORDER BY trace_id",
            (f'{sell_doc_id}-reconcile-poll-%',),
        ).fetchall()

    assert doc.state == DocumentState.ERROR_RETRYABLE, (
        f'After {_RETRYABLE_PASSES} retryable passes, doc must still be ERROR_RETRYABLE '
        f'(no ceiling implemented): got {doc.state}. '
        f'If REQUIRES_MANUAL_RECONCILIATION appears here, a ceiling was added — update this test.'
    )
    assert doc.recovery_attempts == _RETRYABLE_PASSES, (
        f'recovery_attempts must equal pass count ({_RETRYABLE_PASSES}) — no ceiling: '
        f'got {doc.recovery_attempts}'
    )

    trace_ids = [r[0] for r in poll_traces]
    expected_traces = [f'{sell_doc_id}-reconcile-poll-{n}' for n in range(1, _RETRYABLE_PASSES + 1)]
    assert trace_ids == expected_traces, (
        f'transport_trace_log must contain one trace per pass with incrementing suffix. '
        f'Expected: {expected_traces}. Got: {trace_ids}'
    )


# ---------------------------------------------------------------------------
# Test B — still a candidate: ERROR_RETRYABLE doc is picked again on next pass
# ---------------------------------------------------------------------------

def test_gate2i_error_retryable_doc_remains_reconciliation_candidate(tmp_path: Path) -> None:
    """
    Documents that ERROR_RETRYABLE is in get_pending_for_reconciliation() candidate states.

    After a doc reaches ERROR_RETRYABLE via RETRYABLE reconciliation:
    - The next startup's reconciliation picks it up again (retryable==1, not still_pending==1)
    - This confirms the unbounded retry loop: without a ceiling, every startup
      attempts to reconcile every ERROR_RETRYABLE doc.

    SQL evidence: get_pending_for_reconciliation() WHERE clause includes 'ERROR_RETRYABLE'.
    """
    db_name = 'gate2i_b.sqlite3'
    c1, sell_doc_id = _setup_sent_no_transport_id(tmp_path, db_name)
    cfg = _config(tmp_path, db_name)

    # Pass 1: SENT → ERROR_RETRYABLE
    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c2)) as _:
        pass
    assert c2.last_startup_report.reconciliation_retryable == 1

    with c2.connect() as conn:
        doc_after_pass1 = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc_after_pass1.state == DocumentState.ERROR_RETRYABLE

    # Pass 2: ERROR_RETRYABLE → ERROR_RETRYABLE again (candidate is picked up again)
    c3 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c3)) as _:
        pass

    assert c3.last_startup_report.reconciliation_retryable == 1, (
        f'ERROR_RETRYABLE doc must be selected as reconciliation candidate again. '
        f'retryable={c3.last_startup_report.reconciliation_retryable}, '
        f'still_pending={c3.last_startup_report.reconciliation_still_pending}. '
        f'If still_pending==1 here, get_pending_for_reconciliation() excludes ERROR_RETRYABLE '
        f'— update this test.'
    )
    assert c3.last_startup_report.reconciliation_still_pending == 0, (
        f'Doc must be processed (retryable), not skipped (still_pending)'
    )

    with c3.connect() as conn:
        doc_after_pass2 = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc_after_pass2.state == DocumentState.ERROR_RETRYABLE, (
        f'Doc must remain ERROR_RETRYABLE after second pass (no ceiling): got {doc_after_pass2.state}'
    )
    assert doc_after_pass2.recovery_attempts == 2, (
        f'recovery_attempts must be 2 after second pass: got {doc_after_pass2.recovery_attempts}'
    )
