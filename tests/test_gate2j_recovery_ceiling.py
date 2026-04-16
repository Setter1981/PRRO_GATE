"""
Gate 2j — recovery ceiling implementation smoke tests.

Verifies the ceiling semantics introduced in ReconciliationService:
  - When recovery_attempts reaches max_recovery_attempts, the document
    transitions to REQUIRES_MANUAL_RECONCILIATION (not ERROR_RETRYABLE)
  - REQUIRES_MANUAL_RECONCILIATION docs are excluded from the standard
    reconciliation candidate selection
  - Audit event: DOCUMENT_REQUIRES_MANUAL_RECONCILIATION, severity=ERROR
  - Transport trace: '{doc_id}-reconcile-poll-{N}', direction=EVENT (same as retryable)
  - ReconciliationRunResult.manual == 1 (distinct counter from retryable)

Config seam: max_recovery_attempts in RuntimeConfig (default=5)
  Tests use max_recovery_attempts=2 to minimise startup cycles.

Three tests:
  A — ceiling hit: after max_recovery_attempts passes, doc → REQUIRES_MANUAL_RECONCILIATION
      audit DOCUMENT_REQUIRES_MANUAL_RECONCILIATION severity=ERROR
      manual counter incremented
  B — excluded after ceiling: subsequent reconciliation does NOT pick up the doc
      (still_pending==0, retryable==0, manual==0 — checked==0 entirely)
  C — below-ceiling retryable path unaffected: normal RETRYABLE behavior intact
      (uses max_recovery_attempts=3, two passes stay ERROR_RETRYABLE)
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
RECEIPT_ID = 'gate2j-receipt-001'


def _phase1_mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2j'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2j-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2J-001',
            'updated_at': '2026-03-30T21:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': RECEIPT_ID, 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2J-001',
            'updated_at': '2026-03-30T21:01:00+00:00',
        })
    raise AssertionError(f'gate2j phase1: unexpected {request.method} {request.url}')


def _strict_no_http_mock(request: httpx.Request) -> httpx.Response:
    raise AssertionError(
        f'gate2j: unexpected HTTP call {request.method} {request.url}. '
        f'RETRYABLE path must not make HTTP calls.'
    )


def _config(tmp_path: Path, db_name: str, max_recovery_attempts: int = 2) -> AppConfig:
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
            'channel_owner': 'gate2j',
        },
        'runtime': {
            'process_immediately': True,
            'max_recovery_attempts': max_recovery_attempts,
        },
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2J-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T21:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2j',
        'business_ts': ts,
    }


def _setup_sent_no_transport_id(tmp_path: Path, db_name: str, max_recovery_attempts: int = 2) -> tuple[RuntimeContainer, str]:
    """Phase 1: SHIFT_OPEN + SELL → ACK. Crash-sim: SENT + transport_request_id=NULL."""
    cfg = _config(tmp_path, db_name, max_recovery_attempts)
    c1 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_phase1_mock))
    )
    with TestClient(create_app(c1)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2j-open-' + db_name), 'business_ts': '2026-03-30T21:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-' + db_name, 'cashier_id': 'cashier-gate2j'},
        })
        assert r_shift.status_code == 200 and r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2j-sell-' + db_name),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-' + db_name,
                'cashier_id': 'cashier-gate2j',
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
# Test A — ceiling hit: REQUIRES_MANUAL_RECONCILIATION transition
# ---------------------------------------------------------------------------

def test_gate2j_ceiling_hit_transitions_to_manual(tmp_path: Path) -> None:
    """
    With max_recovery_attempts=2:
    - Pass 1: attempts=1 < 2 → ERROR_RETRYABLE, retryable counter
    - Pass 2: attempts=2 >= 2 → REQUIRES_MANUAL_RECONCILIATION, manual counter
    - Audit: DOCUMENT_REQUIRES_MANUAL_RECONCILIATION severity=ERROR
    - Transport trace: '{doc_id}-reconcile-poll-2' with direction=EVENT
    """
    MAX = 2
    db_name = 'gate2j_a.sqlite3'
    c1, sell_doc_id = _setup_sent_no_transport_id(tmp_path, db_name, MAX)
    cfg = _config(tmp_path, db_name, MAX)

    # Pass 1: below ceiling → ERROR_RETRYABLE
    c2 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c2)) as _:
        pass
    assert c2.last_startup_report.reconciliation_retryable == 1, (
        f'Pass 1 must be retryable: got {c2.last_startup_report}'
    )
    assert c2.last_startup_report.reconciliation_manual == 0

    with c2.connect() as conn:
        doc_p1 = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc_p1.state == DocumentState.ERROR_RETRYABLE
    assert doc_p1.recovery_attempts == 1

    # Pass 2: ceiling hit → REQUIRES_MANUAL_RECONCILIATION
    c3 = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c3)) as _:
        pass
    assert c3.last_startup_report.reconciliation_manual == 1, (
        f'Pass 2 must hit ceiling (manual=1): got {c3.last_startup_report}'
    )
    assert c3.last_startup_report.reconciliation_retryable == 0

    with c3.connect() as conn:
        doc_p2 = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)

        poll_trace = conn.execute(
            "SELECT direction, technical_status FROM transport_trace_log WHERE trace_id=?",
            (f'{sell_doc_id}-reconcile-poll-2',),
        ).fetchone()

        audit_rows = conn.execute(
            "SELECT severity, event_payload_json FROM audit_log "
            "WHERE entity_id=? AND event_type=?",
            (sell_doc_id, 'DOCUMENT_REQUIRES_MANUAL_RECONCILIATION'),
        ).fetchall()

    assert doc_p2.state == DocumentState.REQUIRES_MANUAL_RECONCILIATION, (
        f'After ceiling hit, doc must be REQUIRES_MANUAL_RECONCILIATION: got {doc_p2.state}'
    )
    assert doc_p2.recovery_attempts == MAX, (
        f'recovery_attempts must equal max ({MAX}): got {doc_p2.recovery_attempts}'
    )

    assert poll_trace is not None, (
        f'Transport trace {sell_doc_id}-reconcile-poll-2 must exist after ceiling hit'
    )
    assert poll_trace[0] == 'EVENT' and poll_trace[1] == 'RECONCILE_POLL'

    assert len(audit_rows) == 1, (
        f'Exactly 1 DOCUMENT_REQUIRES_MANUAL_RECONCILIATION audit record expected: got {len(audit_rows)}'
    )
    severity, payload_json = audit_rows[0]
    assert severity == 'ERROR', f'Ceiling audit severity must be ERROR: got {severity!r}'
    payload = json.loads(payload_json)
    assert payload == {'state': 'REQUIRES_MANUAL_RECONCILIATION', 'recovery_attempts': MAX}, (
        f'audit payload mismatch: got {payload}'
    )


# ---------------------------------------------------------------------------
# Test B — excluded after ceiling: next reconciliation ignores the doc
# ---------------------------------------------------------------------------

def test_gate2j_manual_state_excluded_from_reconciliation(tmp_path: Path) -> None:
    """
    After doc reaches REQUIRES_MANUAL_RECONCILIATION:
    - Next reconciliation startup polls the doc (checked==1) — auto-close on DPS ACK/REJECTED is now supported
    - When DPS returns retryable (doc has no transport_request_id → no HTTP call), the doc is counted as still_pending
    - doc.state remains REQUIRES_MANUAL_RECONCILIATION (no accidental transition on retryable response)
    - recovery_attempts must not increase after state leaves standard retryable loop
    """
    MAX = 2
    db_name = 'gate2j_b.sqlite3'
    c1, sell_doc_id = _setup_sent_no_transport_id(tmp_path, db_name, MAX)
    cfg = _config(tmp_path, db_name, MAX)

    # Pass 1 and 2: reach ceiling
    for _ in range(MAX):
        c = RuntimeContainer(
            cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
        )
        with TestClient(create_app(c)) as _:
            pass

    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc.state == DocumentState.REQUIRES_MANUAL_RECONCILIATION

    # Pass 3: doc must not be picked as a candidate
    c_post = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
    )
    with TestClient(create_app(c_post)) as _:
        pass

    report = c_post.last_startup_report
    assert report.reconciliation_checked == 1, (
        f'REQUIRES_MANUAL_RECONCILIATION doc must be polled (auto-close on ACK/REJECTED): '
        f'checked={report.reconciliation_checked}'
    )
    assert report.reconciliation_still_pending == 1, (
        f'Retryable DPS response must count as still_pending: got {report.reconciliation_still_pending}'
    )
    assert report.reconciliation_retryable == 0
    assert report.reconciliation_manual == 0

    with c_post.connect() as conn:
        doc_after = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc_after.state == DocumentState.REQUIRES_MANUAL_RECONCILIATION, (
        'State must remain REQUIRES_MANUAL_RECONCILIATION after subsequent reconciliation pass'
    )
    assert doc_after.recovery_attempts == MAX, (
        'recovery_attempts must not change after doc leaves standard reconciliation loop'
    )


# ---------------------------------------------------------------------------
# Test C — below-ceiling retryable path unaffected
# ---------------------------------------------------------------------------

def test_gate2j_below_ceiling_retryable_unaffected(tmp_path: Path) -> None:
    """
    With max_recovery_attempts=3, two passes stay in ERROR_RETRYABLE.
    The ceiling does not prematurely fire.
    Normal retryable semantics (recovery_attempts increments, state=ERROR_RETRYABLE) intact.
    """
    MAX = 3
    db_name = 'gate2j_c.sqlite3'
    c1, sell_doc_id = _setup_sent_no_transport_id(tmp_path, db_name, MAX)
    cfg = _config(tmp_path, db_name, MAX)

    for pass_n in range(1, MAX):  # 2 passes: attempts 1 and 2, both < 3
        c = RuntimeContainer(
            cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_strict_no_http_mock))
        )
        with TestClient(create_app(c)) as _:
            pass
        assert c.last_startup_report.reconciliation_retryable == 1, (
            f'Pass {pass_n}: must be retryable (below ceiling={MAX}): got {c.last_startup_report}'
        )
        assert c.last_startup_report.reconciliation_manual == 0, (
            f'Pass {pass_n}: must not hit ceiling yet: got manual={c.last_startup_report.reconciliation_manual}'
        )

    with c.connect() as conn:
        doc = FiscalDocumentRepository.get_by_id(conn, sell_doc_id)
    assert doc.state == DocumentState.ERROR_RETRYABLE, (
        f'After {MAX - 1} passes below ceiling={MAX}, state must remain ERROR_RETRYABLE: got {doc.state}'
    )
    assert doc.recovery_attempts == MAX - 1, (
        f'recovery_attempts must be {MAX - 1}: got {doc.recovery_attempts}'
    )
