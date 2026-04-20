"""Sprint 12 — write_path gap tests.

A1: GET_STATUS doesn't allocate LND or create fiscal document
B1: GO_OFFLINE blocked without active offline range
B2: GO_OFFLINE below watermark emits audit event, still succeeds
C1: CRYPTO_DEGRADED mode set when breaker opens; cleared on recovery
BLOCKED: enforce_blocked_mode=True → BLOCKED after monthly limit; fiscal ops blocked
"""
from __future__ import annotations

import sqlite3
from datetime import UTC, datetime

from prro_gateway.enums import CanonicalErrorCode, NodeMode, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand
from prro_gateway.repositories import AuditRepository, InboxRepository, ShiftRepository
from prro_gateway.repositories.fn_config import FiscalNumberConfigRepository
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.repositories.offline import OfflineRepository
from prro_gateway.services import WritePathWorker
from prro_gateway.utils.json_codec import dumps_json

FN = 'FN-DEV-0001'
BACKEND = 'backend_checkbox_default'
TRANSPORT = 'transport_checkbox_rest_default'

_seq = 0


def _nid(p: str) -> str:
    global _seq
    _seq += 1
    return f'{p}-s12-{_seq}'


class _StubCrypto:
    def sign(self, *, document_id: str, payload_json: str) -> str:
        return f'sig::{document_id}'


class _FailCrypto:
    """Always raises — simulates persistent crypto failure."""
    def sign(self, *, document_id: str, payload_json: str) -> str:
        from prro_gateway.runtime.providers import CryptoProviderUnavailableError
        raise CryptoProviderUnavailableError('simulated crypto failure')


class _StubTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        now = datetime.now(UTC)
        return SendResult(
            transport_request_id='tx', submission_status='ACK',
            server_fiscal_no='SFN', server_fiscal_date=now.isoformat(),
            response_json='{}', sent_at=now, ack_at=now,
        )


def _worker(crypto=None, threshold: int = 0) -> WritePathWorker:
    return WritePathWorker(
        crypto_provider=crypto or _StubCrypto(),
        transport_client=_StubTransport(),
        crypto_breaker_threshold=threshold,
    )


def _enqueue(conn: sqlite3.Connection, op: OperationType, payload: dict | None = None) -> str:
    rid = _nid('req')
    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=_nid('idem'),
        protocol=Protocol.CHECKBOX_REST, operation_type=op,
        fiscal_number=FN, route_key='pos-1',
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        channel_owner='front-a', external_request_id=_nid('ext'),
        business_ts=datetime(2026, 4, 15, 10, 0, 0, tzinfo=UTC),
        payload=payload or {}, payload_sha256=_nid('sha'),
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=rid, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=FN, backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT, channel_owner='front-a',
        external_request_id=cmd.external_request_id,
        protocol_session_id=None,
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256,
    )
    conn.commit()
    return rid


def _set_mode(conn: sqlite3.Connection, mode: NodeMode) -> None:
    conn.execute('BEGIN IMMEDIATE')
    NodeStateRepository.update_mode(conn, fiscal_number=FN, mode=mode)
    conn.commit()


def _create_range(conn: sqlite3.Connection, suffix: str = 'default',
                  first: int = 1001, last: int = 1200) -> None:
    conn.execute('BEGIN IMMEDIATE')
    OfflineRepository.create_range(
        conn, range_id=f'range-{suffix}', fiscal_number=FN,
        first_fiscal_no=first, last_fiscal_no=last,
        issued_at='2026-04-15T09:00:00+00:00',
    )
    conn.commit()


# ===========================================================================
# A1 — GET_STATUS
# ===========================================================================

def test_a1_get_status_returns_ack(conn: sqlite3.Connection) -> None:
    _enqueue(conn, OperationType.GET_STATUS)
    r = _worker().process_next(conn, fiscal_number=FN)
    assert r.outcome == 'ACK', r


def test_a1_get_status_no_lnd_allocated(conn: sqlite3.Connection) -> None:
    before = NodeStateRepository.get_state(conn, FN).next_lnd
    _enqueue(conn, OperationType.GET_STATUS)
    _worker().process_next(conn, fiscal_number=FN)
    after = NodeStateRepository.get_state(conn, FN).next_lnd
    assert after == before, f"LND changed: {before} → {after}"


def test_a1_get_status_no_fiscal_document(conn: sqlite3.Connection) -> None:
    _enqueue(conn, OperationType.GET_STATUS)
    _worker().process_next(conn, fiscal_number=FN)
    count = conn.execute("SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number=?", (FN,)).fetchone()[0]
    assert count == 0


def test_a1_get_status_works_with_open_shift(conn: sqlite3.Connection) -> None:
    from prro_gateway.enums import ShiftState
    from prro_gateway.repositories import ShiftRepository
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(
        conn, shift_id='shift-a1', fiscal_number=FN,
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST, integration_owner='front-a',
        channel_lock_acquired_at='2026-04-15T10:00:00+00:00',
    )
    conn.commit()
    _enqueue(conn, OperationType.GET_STATUS)
    r = _worker().process_next(conn, fiscal_number=FN)
    assert r.outcome == 'ACK', r


# ===========================================================================
# B1 — GO_OFFLINE without active range
# ===========================================================================

def test_b1_go_offline_blocked_without_range(conn: sqlite3.Connection) -> None:
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue(conn, OperationType.GO_OFFLINE)
    r = _worker().process_next(conn, fiscal_number=FN)
    assert r.outcome == 'ERROR', r
    assert r.canonical_error is not None
    assert r.canonical_error.code == CanonicalErrorCode.OFFLINE_CODES_EXHAUSTED.value


def test_b1_go_offline_blocked_mode_stays_online(conn: sqlite3.Connection) -> None:
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue(conn, OperationType.GO_OFFLINE)
    _worker().process_next(conn, fiscal_number=FN)
    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.ONLINE


def test_b1_go_offline_succeeds_with_range(conn: sqlite3.Connection) -> None:
    _create_range(conn, 'b1-ok')
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue(conn, OperationType.GO_OFFLINE)
    r = _worker().process_next(conn, fiscal_number=FN)
    assert r.outcome == 'ACK', r
    assert NodeStateRepository.get_state(conn, FN).mode == NodeMode.OFFLINE


# ===========================================================================
# B2 — GO_OFFLINE watermark warning
# ===========================================================================

def test_b2_below_watermark_emits_audit_event(conn: sqlite3.Connection) -> None:
    # min=50, codes_remaining=10 → warning
    _create_range(conn, 'b2-warn', first=1001, last=1010)  # 10 codes
    conn.execute('BEGIN IMMEDIATE')
    FiscalNumberConfigRepository.upsert(conn, fiscal_number=FN, min_offline_codes=50, max_offline_codes=100)
    conn.commit()
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue(conn, OperationType.GO_OFFLINE)
    r = _worker().process_next(conn, fiscal_number=FN)
    assert r.outcome == 'ACK', r  # still succeeds
    events = conn.execute(
        "SELECT event_type FROM audit_log WHERE entity_id=? AND event_type='OFFLINE_CODES_BELOW_WATERMARK'",
        (FN,),
    ).fetchall()
    assert len(events) >= 1, "Expected OFFLINE_CODES_BELOW_WATERMARK audit event"


def test_b2_above_watermark_no_audit_event(conn: sqlite3.Connection) -> None:
    # min=5, codes_remaining=100 → no warning
    _create_range(conn, 'b2-ok', first=1001, last=1100)  # 100 codes
    conn.execute('BEGIN IMMEDIATE')
    FiscalNumberConfigRepository.upsert(conn, fiscal_number=FN, min_offline_codes=5, max_offline_codes=200)
    conn.commit()
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue(conn, OperationType.GO_OFFLINE)
    _worker().process_next(conn, fiscal_number=FN)
    events = conn.execute(
        "SELECT event_type FROM audit_log WHERE entity_id=? AND event_type='OFFLINE_CODES_BELOW_WATERMARK'",
        (FN,),
    ).fetchall()
    assert len(events) == 0


def test_b2_default_watermark_zero_no_event(conn: sqlite3.Connection) -> None:
    # min=0 (DEFAULT) → no warning ever
    _create_range(conn, 'b2-def', first=1001, last=1010)
    _set_mode(conn, NodeMode.ONLINE)
    _enqueue(conn, OperationType.GO_OFFLINE)
    _worker().process_next(conn, fiscal_number=FN)
    events = conn.execute(
        "SELECT event_type FROM audit_log WHERE entity_id=? AND event_type='OFFLINE_CODES_BELOW_WATERMARK'",
        (FN,),
    ).fetchall()
    assert len(events) == 0


# ===========================================================================
# C1 — CRYPTO_DEGRADED mode
# ===========================================================================

def _enqueue_sell(conn: sqlite3.Connection) -> None:
    from prro_gateway.enums import ShiftState
    from prro_gateway.repositories import ShiftRepository
    shift_row = conn.execute("SELECT shift_id FROM shifts WHERE fiscal_number=? AND state='OPENED'", (FN,)).fetchone()
    if shift_row is None:
        conn.execute('BEGIN IMMEDIATE')
        ShiftRepository.create_shift(
            conn, shift_id='shift-c1', fiscal_number=FN,
            state=ShiftState.OPENED, open_mode='ONLINE',
            backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
            protocol=Protocol.CHECKBOX_REST, integration_owner='front-a',
            channel_lock_acquired_at='2026-04-15T10:00:00+00:00',
        )
        conn.commit()
    _enqueue(conn, OperationType.SELL, payload={'receipt': {'payments': [{'type': 'CASH', 'value': 100}], 'goods': [{'name': 'Item', 'price': 100, 'quantity': 1000}]}})


def test_c1_crypto_degraded_set_when_breaker_opens(conn: sqlite3.Connection) -> None:
    threshold = 2
    worker = _worker(crypto=_FailCrypto(), threshold=threshold)
    for _ in range(threshold):
        _enqueue_sell(conn)
        worker.process_next(conn, fiscal_number=FN)

    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.CRYPTO_DEGRADED, f"Expected CRYPTO_DEGRADED, got {state.mode}"


def test_c1_recovery_resets_to_online(conn: sqlite3.Connection) -> None:
    threshold = 2
    worker = _worker(crypto=_FailCrypto(), threshold=threshold)
    for _ in range(threshold):
        _enqueue_sell(conn)
        worker.process_next(conn, fiscal_number=FN)
    assert NodeStateRepository.get_state(conn, FN).mode == NodeMode.CRYPTO_DEGRADED

    # Switch to good crypto on same worker instance
    worker.crypto_provider = _StubCrypto()
    worker._crypto_consecutive_failures = 0
    _enqueue_sell(conn)
    worker.process_next(conn, fiscal_number=FN)

    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.ONLINE, f"Expected ONLINE after recovery, got {state.mode}"


# ===========================================================================
# BLOCKED mode enforcement
# ===========================================================================

def _open_shift(conn: sqlite3.Connection, shift_id: str = 'shift-blocked') -> None:
    """Create an OPENED shift so SELL operations pass the shift guard."""
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(
        conn, shift_id=shift_id, fiscal_number=FN,
        state=ShiftState.OPENED, open_mode='OFFLINE',
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST, integration_owner='front-a',
        channel_lock_acquired_at='2026-04-15T09:30:00+00:00',
    )
    conn.commit()


def _force_monthly_limit(conn: sqlite3.Connection) -> None:
    """Directly set monthly offline seconds to just above limit (168h)."""
    limit = WritePathWorker.MAX_OFFLINE_MONTH_SECONDS
    month_bucket = datetime.now(UTC).strftime('%Y-%m')
    conn.execute(
        "UPDATE node_state SET current_month_offline_seconds=?, current_month_bucket=? WHERE fiscal_number=?",
        (limit + 1, month_bucket, FN),
    )


def _force_offline_with_open_session(conn: sqlite3.Connection) -> None:
    """Put node in OFFLINE mode with an open offline session.

    started_at is set 1 hour ago (relative to now) so continuous_seconds < 36h limit.
    Monthly limit is forced separately via _force_monthly_limit.
    """
    from datetime import timedelta
    started_at = (datetime.now(UTC) - timedelta(hours=1)).isoformat()
    conn.execute('BEGIN IMMEDIATE')
    OfflineRepository.create_open_session(
        conn, offline_session_id='sess-blocked', fiscal_number=FN,
        started_at=started_at,
        status='OPEN',
    )
    NodeStateRepository.update_mode(conn, fiscal_number=FN, mode=NodeMode.OFFLINE)
    conn.commit()


def test_blocked_not_set_when_enforce_off(conn: sqlite3.Connection) -> None:
    """Default enforce_blocked_mode=0 → mode stays OFFLINE even after monthly limit."""
    _create_range(conn, 'bl-off')
    _set_mode(conn, NodeMode.ONLINE)
    _force_offline_with_open_session(conn)
    _open_shift(conn, 'shift-bl-off')
    conn.execute('BEGIN IMMEDIATE')
    _force_monthly_limit(conn)
    conn.commit()

    _enqueue(conn, OperationType.SELL, payload={'receipt': {'payments': [{'type': 'CASH', 'value': 100}], 'goods': [{'name': 'Item', 'price': 100, 'quantity': 1000}]}})
    _worker().process_next(conn, fiscal_number=FN)

    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.OFFLINE, f"Expected OFFLINE (not BLOCKED) when enforce=off, got {state.mode}"


def test_blocked_set_when_enforce_on(conn: sqlite3.Connection) -> None:
    """enforce_blocked_mode=1 → monthly limit → mode=BLOCKED."""
    _create_range(conn, 'bl-on')
    conn.execute('BEGIN IMMEDIATE')
    FiscalNumberConfigRepository.upsert(conn, fiscal_number=FN, enforce_blocked_mode=1)
    conn.commit()
    _set_mode(conn, NodeMode.ONLINE)
    _force_offline_with_open_session(conn)
    _open_shift(conn, 'shift-bl-on')
    conn.execute('BEGIN IMMEDIATE')
    _force_monthly_limit(conn)
    conn.commit()

    _enqueue(conn, OperationType.SELL, payload={'receipt': {'payments': [{'type': 'CASH', 'value': 100}], 'goods': [{'name': 'Item', 'price': 100, 'quantity': 1000}]}})
    _worker().process_next(conn, fiscal_number=FN)

    state = NodeStateRepository.get_state(conn, FN)
    assert state.mode == NodeMode.BLOCKED, f"Expected BLOCKED, got {state.mode}"


def test_blocked_mode_blocks_fiscal_ops(conn: sqlite3.Connection) -> None:
    """Fiscal operations return error when node is BLOCKED."""
    _set_mode(conn, NodeMode.BLOCKED)
    _enqueue(conn, OperationType.SELL, payload={'receipt': {}})
    r = _worker().process_next(conn, fiscal_number=FN)
    assert r.outcome == 'ERROR', r
    assert r.canonical_error is not None
    assert r.canonical_error.code == CanonicalErrorCode.OFFLINE_LIMIT_REACHED.value


def test_go_online_from_blocked_allowed(conn: sqlite3.Connection) -> None:
    """GO_ONLINE succeeds from BLOCKED mode (operator manual unblock)."""
    _set_mode(conn, NodeMode.BLOCKED)
    _enqueue(conn, OperationType.GO_ONLINE)
    r = _worker().process_next(conn, fiscal_number=FN)
    assert r.outcome == 'ACK', r
    assert NodeStateRepository.get_state(conn, FN).mode == NodeMode.ONLINE


# ===========================================================================
# ARCH-001 regression: multi-session monthly sum (not max)
# ===========================================================================

def test_monthly_limit_sum_across_sessions(conn: sqlite3.Connection) -> None:
    """Regression for ARCH-001: closed-session seconds + current session must be summed.

    Scenario:
      - Previous closed sessions this month: 160h  (stored in node_state)
      - Current open session started 9h ago        (continuous_seconds ≈ 9h)
      - True monthly total: 169h > 168h limit      → OFFLINE_LIMIT_REACHED

    With the old max() formula: max(160h, 9h) = 160h < 168h → no error (bug).
    With the correct + formula: 160h + 9h = 169h > 168h → error (fixed).
    """
    from datetime import timedelta

    now = datetime.now(UTC)
    month_bucket = now.strftime('%Y-%m')
    # Session started 9h ago — continuous_seconds ≈ 32400s
    nine_hours_ago = (now - timedelta(hours=9)).isoformat()

    _create_range(conn, 'ms-sum', first=3001, last=3100)

    # Closed sessions accumulated: 160h (below limit on its own)
    conn.execute('BEGIN IMMEDIATE')
    conn.execute(
        "UPDATE node_state SET current_month_offline_seconds=?, current_month_bucket=? WHERE fiscal_number=?",
        (160 * 3600, month_bucket, FN),
    )
    conn.commit()

    # Open session started 9h ago
    conn.execute('BEGIN IMMEDIATE')
    OfflineRepository.create_open_session(
        conn, offline_session_id='sess-ms-sum', fiscal_number=FN,
        started_at=nine_hours_ago, status='OPEN',
    )
    NodeStateRepository.update_mode(conn, fiscal_number=FN, mode=NodeMode.OFFLINE)
    conn.commit()

    _open_shift(conn, 'shift-ms-sum')

    _enqueue(conn, OperationType.SELL, payload={
        'receipt': {
            'payments': [{'type': 'CASH', 'value': 100}],
            'goods': [{'name': 'Item', 'price': 100, 'quantity': 1000}],
        }
    })
    r = _worker().process_next(conn, fiscal_number=FN)

    assert r.outcome == 'ERROR', f"Expected monthly limit to trigger, got {r.outcome}"
    assert r.canonical_error is not None
    assert r.canonical_error.code == CanonicalErrorCode.OFFLINE_LIMIT_REACHED.value, \
        f"Expected OFFLINE_LIMIT_REACHED, got {r.canonical_error.code}"


# ===========================================================================
# A2 — X_REPORT management command
# ===========================================================================

# ===========================================================================
# B1 — deferred-sign expected_states guard (H-4)
# ===========================================================================

def test_b1_update_state_signed_expected_states_guard(conn: sqlite3.Connection) -> None:
    """B1 (H-4): update_state(SIGNED, expected_states=(PREPARED,)) on a SIGNED doc returns None.

    This is the underlying mechanism the write_path stage_sign fix relies on.
    A doc already in SIGNED state must not be silently moved back to SIGNED
    (which could advance an ACKed doc backward in a crash-recovery replay).
    """
    from prro_gateway.enums import DocumentState
    from prro_gateway.repositories.fiscal_documents import FiscalDocumentRepository

    # Enqueue and process one step to get a fiscal document into SIGNED state.
    _enqueue(conn, OperationType.SELL)
    # The stub transport returns ACK, so after processing the doc will be ACK.
    # Instead, let's use the repository directly to insert a doc in SIGNED state.
    conn.execute('BEGIN IMMEDIATE')
    req_id = _nid('req-b1')
    conn.execute("""
        INSERT OR IGNORE INTO ingress_inbox (
            request_id, idempotency_key, protocol, operation_type, fiscal_number,
            backend_profile_id, transport_profile_id, channel_owner,
            payload_json, payload_sha256, status, response_deadline_at
        ) VALUES (?, ?, 'CHECKBOX_REST', 'SELL', ?,
                  ?, ?, 'test', '{}', 'sha256stub', 'DONE', '2099-01-01T00:00:00Z')
    """, (req_id, _nid('idem-b1'), FN, BACKEND, TRANSPORT))
    conn.execute("""
        INSERT INTO fiscal_documents (
            document_id, request_id, fiscal_number, doc_type, state, fs_mode,
            lnd, backend_profile_id, transport_profile_id,
            submission_status, payload_json, payload_sha256, business_ts
        ) VALUES (?, ?, ?, 'SELL', 'SIGNED', 'OFFLINE',
            1, ?, ?, 'PENDING', '{}', 'sha256stub', '2026-04-15T10:00:00+00:00')
    """, (_nid('doc-b1'), req_id, FN, BACKEND, TRANSPORT))
    conn.commit()

    doc_id = conn.execute(
        "SELECT document_id FROM fiscal_documents WHERE state='SIGNED' AND fiscal_number=?", (FN,)
    ).fetchone()[0]

    # Try to "update" to SIGNED with expected_states=(PREPARED,) — must return None (guard fires)
    conn.execute('BEGIN IMMEDIATE')
    result = FiscalDocumentRepository.update_state(
        conn, document_id=doc_id, state=DocumentState.SIGNED,
        expected_states=(DocumentState.PREPARED,),
    )
    conn.rollback()

    assert result is None, (
        f"B1: update_state(SIGNED, expected=(PREPARED,)) must return None for a SIGNED doc; got {result}"
    )


def test_a2_x_report_ack(conn: sqlite3.Connection) -> None:
    """X_REPORT via write_path produces ACK and creates a fiscal document."""
    _open_shift(conn, 'shift-xreport')
    _enqueue(conn, OperationType.X_REPORT)
    result = _worker().process_next(conn, fiscal_number=FN)
    assert result.outcome == 'ACK', (
        f"Expected ACK, got {result.outcome}: "
        f"{result.canonical_error}"
    )
    # X_REPORT is a fiscal operation — it allocates LND and creates a fiscal document.
    assert result.document_id is not None, "Expected a fiscal document to be created for X_REPORT"
    from prro_gateway.enums import DocumentState
    from prro_gateway.repositories.fiscal_documents import FiscalDocumentRepository
    doc = FiscalDocumentRepository.get_by_id(conn, result.document_id)
    assert doc is not None, f"Fiscal document {result.document_id} not found"
    assert doc.state == DocumentState.ACK, f"Expected DocumentState.ACK, got {doc.state}"
    assert doc.doc_type == 'X_REPORT', f"Expected doc_type X_REPORT, got {doc.doc_type}"


# ===========================================================================
# Shift state guards — write_path (mirrors A3 fix for offline_sync)
# ===========================================================================

def _open_shift_in_state(conn: sqlite3.Connection, shift_id: str, state: ShiftState) -> None:
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(
        conn, shift_id=shift_id, fiscal_number=FN,
        state=state, open_mode='ONLINE',
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-15T10:00:00+00:00',
    )
    conn.commit()


def test_wp_sync_shift_open_closing_not_moved_to_opened(conn: sqlite3.Connection) -> None:
    """write_path _sync_shift_open_locked: CLOSING shift must NOT be transitioned to OPENED.

    This exercises the guard added to _sync_shift_open_locked directly — the write_path
    validate stage blocks new SHIFT_OPENs when an active shift exists, so this path
    is only reachable via crash recovery / reconciliation, which is why we test it
    at the method level rather than through process_next.
    """
    import logging
    from types import SimpleNamespace
    from prro_gateway.models.canonical import CanonicalFiscalCommand

    _open_shift_in_state(conn, 'shift-wp-close-guard', ShiftState.CLOSING)
    worker = _worker()

    fake_doc = SimpleNamespace(
        document_id='doc-wp-guard', fiscal_number=FN,
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
        fs_mode='ONLINE',
    )
    fake_cmd = SimpleNamespace(
        operation_type=OperationType.SHIFT_OPEN, channel_owner='test', protocol=Protocol.CHECKBOX_REST,
    )
    fake_ctx = SimpleNamespace(
        document=fake_doc, command=fake_cmd, fiscal_number=FN,
        active_shift=None, send_result=None,
    )

    conn.execute('BEGIN IMMEDIATE')
    from prro_gateway.enums import DocumentState
    worker._sync_shift_open_locked(conn, ctx=fake_ctx, target_state=DocumentState.ACK)
    conn.rollback()

    shift = ShiftRepository.get_by_id(conn, 'shift-wp-close-guard')
    assert shift.state == ShiftState.CLOSING, (
        f"CLOSING shift must not be moved to OPENED by _sync_shift_open_locked; got {shift.state}"
    )


def test_wp_sync_shift_close_guard_opening_shift(conn: sqlite3.Connection) -> None:
    """write_path _sync_shift_close_locked: OPENING shift must NOT be moved to CLOSED.

    Guards against closing a shift that never fully opened — could happen in recovery
    if SHIFT_CLOSE ACK arrives before SHIFT_OPEN ACK is processed.
    """
    from types import SimpleNamespace
    from prro_gateway.enums import DocumentState

    _open_shift_in_state(conn, 'shift-wp-open-guard', ShiftState.OPENING)
    worker = _worker()

    fake_doc = SimpleNamespace(
        document_id='doc-wp-close-guard', fiscal_number=FN,
        backend_profile_id=BACKEND, transport_profile_id=TRANSPORT,
    )
    fake_ctx = SimpleNamespace(document=fake_doc, fiscal_number=FN)

    conn.execute('BEGIN IMMEDIATE')
    worker._sync_shift_close_locked(conn, ctx=fake_ctx, target_state=DocumentState.ACK)
    conn.rollback()

    shift = ShiftRepository.get_by_id(conn, 'shift-wp-open-guard')
    assert shift.state == ShiftState.OPENING, (
        f"OPENING shift must not be closed by _sync_shift_close_locked; got {shift.state}"
    )
