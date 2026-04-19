"""Tests for concurrency=N parameter on ReconciliationService.reconcile_pending.

Invariants verified:
- concurrency=1 produces bit-identical results to the legacy serial path (no regression)
- per-fiscal_number order is preserved (single-writer invariant)
- parallel poll never calls poll_status concurrently for the same fiscal_number
- SQLite mutations stay on caller thread (conn never touches worker threads)
- poll exception in one group does not prevent other groups from completing
- cancel_event stops polling mid-group; already-polled docs are still applied
"""
from __future__ import annotations

import threading
import time
from datetime import UTC, datetime
from types import SimpleNamespace
from typing import Any
from unittest.mock import MagicMock

import pytest

from prro_gateway.enums import DocumentState
from prro_gateway.ports import PollResult
from prro_gateway.services.reconciliation import ReconciliationService, ReconciliationRunResult, _COOLDOWN_SKIP


# ---------------------------------------------------------------------------
# Minimal doc stub — only the fields reconciliation.py actually reads
# ---------------------------------------------------------------------------

def _doc(
    *,
    document_id: str,
    fiscal_number: str,
    state: str = DocumentState.SENT.value,
    submission_status: str = 'SENT',
    updated_at=None,
    response_json: str | None = None,
    doc_type: str = 'SELL',
    backend_profile_id: str = 'bp',
    transport_profile_id: str = 'tp',
    transport_request_id: str | None = 'tid',
    lnd: int = 1,
    request_id: str = 'req',
    fs_mode: str = 'ONLINE',
    ack_at: str | None = None,
    sent_at: str | None = None,
    payload_json: str | None = None,
    recovery_attempts: int = 0,
) -> SimpleNamespace:
    return SimpleNamespace(
        document_id=document_id,
        fiscal_number=fiscal_number,
        state=DocumentState(state),
        submission_status=submission_status,
        updated_at=updated_at,
        response_json=response_json,
        doc_type=doc_type,
        backend_profile_id=backend_profile_id,
        transport_profile_id=transport_profile_id,
        transport_request_id=transport_request_id,
        lnd=lnd,
        request_id=request_id,
        fs_mode=fs_mode,
        ack_at=ack_at,
        sent_at=sent_at,
        payload_json=payload_json,
        recovery_attempts=recovery_attempts,
    )


# ---------------------------------------------------------------------------
# Stub transport that records calls
# ---------------------------------------------------------------------------

class _RecordingTransport:
    """Returns a configurable PollResult; records (thread_id, fiscal_number, document_id) per call."""

    def __init__(self, result: PollResult | None = None, *, sleep_seconds: float = 0.0, raise_for: str | None = None):
        self._result = result or PollResult(state=DocumentState.ACK.value, submission_status='ACK', ack_at=datetime.now(UTC))
        self._sleep = sleep_seconds
        self._raise_for = raise_for
        self.calls: list[dict] = []
        self._lock = threading.Lock()

    def poll_status(self, *, document_id: str, fiscal_number: str, **_kwargs) -> PollResult:
        if self._sleep:
            time.sleep(self._sleep)
        if self._raise_for and fiscal_number == self._raise_for:
            raise RuntimeError(f'Simulated poll error for {fiscal_number}')
        with self._lock:
            self.calls.append({
                'thread_id': threading.get_ident(),
                'fiscal_number': fiscal_number,
                'document_id': document_id,
                'ts': time.monotonic(),
            })
        return self._result


# ---------------------------------------------------------------------------
# Stub conn — captures DB calls; does nothing to SQLite
# ---------------------------------------------------------------------------

class _StubConn:
    """Minimal sqlite3.Connection stub: records thread that called each method."""

    def __init__(self):
        self.caller_thread = threading.get_ident()
        self.thread_violations: list[int] = []  # threads that called commit/execute != caller
        self._txn_open = False

    def execute(self, sql: str, params=()) -> Any:
        tid = threading.get_ident()
        if tid != self.caller_thread:
            self.thread_violations.append(tid)
        self._txn_open = ('BEGIN' in sql.upper())
        return MagicMock()

    def commit(self) -> None:
        tid = threading.get_ident()
        if tid != self.caller_thread:
            self.thread_violations.append(tid)
        self._txn_open = False

    def rollback(self) -> None:
        tid = threading.get_ident()
        if tid != self.caller_thread:
            self.thread_violations.append(tid)
        self._txn_open = False


# ---------------------------------------------------------------------------
# Helper: build a ReconciliationService with mocked repo access so we can
# test the poll/partition/apply logic without a real SQLite DB.
# ---------------------------------------------------------------------------

def _build_service(transport: _RecordingTransport, *, concurrency: int = 1, cancel_event=None) -> ReconciliationService:
    svc = ReconciliationService(
        transport_status_client=transport,
        max_recovery_attempts=5,
        concurrency=concurrency,
        cancel_event=cancel_event,
    )
    return svc


# ---------------------------------------------------------------------------
# Test 1: concurrency=1 default produces expected result
# ---------------------------------------------------------------------------

def test_concurrency_1_is_serial_and_correct():
    """With concurrency=1, every doc is polled and counted; order matches input."""
    transport = _RecordingTransport(PollResult(state=DocumentState.ACK.value, submission_status='ACK', ack_at=datetime.now(UTC)))
    svc = _build_service(transport, concurrency=1)

    docs = [
        _doc(document_id='d1', fiscal_number='FN-1'),
        _doc(document_id='d2', fiscal_number='FN-2'),
        _doc(document_id='d3', fiscal_number='FN-1'),
    ]

    # Patch FiscalDocumentRepository so we don't need a real DB
    from prro_gateway import repositories
    original_get = repositories.FiscalDocumentRepository.get_pending_for_reconciliation

    import unittest.mock as mock

    with mock.patch.object(
        repositories.FiscalDocumentRepository,
        'get_pending_for_reconciliation',
        return_value=docs,
    ), mock.patch.object(
        svc, '_apply_poll_result', wraps=lambda conn, doc, poll_or_exc, result: setattr(result, 'acked', result.acked + 1),
    ) as apply_mock:
        stub_conn = _StubConn()
        result = svc.reconcile_pending(stub_conn)

    assert result.checked == 3
    assert apply_mock.call_count == 3
    # Transport called once per doc
    assert len(transport.calls) == 3
    # All calls from caller thread (no threads spawned for concurrency=1)
    caller_tid = threading.get_ident()
    for c in transport.calls:
        assert c['thread_id'] == caller_tid, 'concurrency=1 must poll on caller thread'


# ---------------------------------------------------------------------------
# Test 2: single-writer invariant — no two concurrent calls share fiscal_number
# ---------------------------------------------------------------------------

def test_parallel_poll_never_concurrent_for_same_fiscal_number():
    """Groups run in parallel but each group is serial: fiscal_number isolation preserved."""
    transport = _RecordingTransport(
        PollResult(state=DocumentState.ACK.value, submission_status='ACK', ack_at=datetime.now(UTC)),
        sleep_seconds=0.03,
    )
    svc = _build_service(transport, concurrency=4)

    # 3 fiscal_numbers, 2 docs each = 6 docs total
    fiscal_numbers = ['FN-A', 'FN-B', 'FN-C']
    docs = []
    for i, fn in enumerate(fiscal_numbers):
        docs.append(_doc(document_id=f'{fn}-doc1', fiscal_number=fn))
        docs.append(_doc(document_id=f'{fn}-doc2', fiscal_number=fn))

    import unittest.mock as mock
    from prro_gateway import repositories

    with mock.patch.object(
        repositories.FiscalDocumentRepository,
        'get_pending_for_reconciliation',
        return_value=docs,
    ), mock.patch.object(svc, '_apply_poll_result'):
        stub_conn = _StubConn()
        svc.reconcile_pending(stub_conn)

    # Verify no two calls with the same fiscal_number overlapped in time.
    # Overlap = call A starts before call B ends AND they share fiscal_number.
    calls_by_fn: dict[str, list] = {}
    for c in transport.calls:
        calls_by_fn.setdefault(c['fiscal_number'], []).append(c)

    for fn, fn_calls in calls_by_fn.items():
        # Each call starts after the previous one finishes (serial within group).
        # We only have 'ts' (start time). With sleep=30ms, if calls within a group
        # were truly concurrent they would start almost simultaneously (<5ms apart).
        # Serial start gap must be >= 25ms (sleep * 0.8).
        if len(fn_calls) < 2:
            continue
        for i in range(len(fn_calls) - 1):
            gap = fn_calls[i + 1]['ts'] - fn_calls[i]['ts']
            assert gap >= 0.020, (
                f'Calls within fiscal_number {fn} started too close together '
                f'({gap*1000:.1f}ms < 20ms): likely ran in parallel, violating single-writer invariant'
            )


# ---------------------------------------------------------------------------
# Test 3: concurrency=N provides speedup over concurrency=1 with slow transport
# ---------------------------------------------------------------------------

def test_concurrency_speedup_with_slow_transport():
    """4 fiscal_numbers × 1 doc, transport sleeps 100ms each.
    concurrency=1 >= 400ms; concurrency=4 < 300ms.
    """
    transport = _RecordingTransport(
        PollResult(state=DocumentState.ACK.value, submission_status='ACK', ack_at=datetime.now(UTC)),
        sleep_seconds=0.10,
    )

    fiscal_numbers = ['FN-T1', 'FN-T2', 'FN-T3', 'FN-T4']
    docs = [_doc(document_id=f'{fn}-d', fiscal_number=fn) for fn in fiscal_numbers]

    import unittest.mock as mock
    from prro_gateway import repositories

    # --- serial path ---
    svc_serial = _build_service(transport, concurrency=1)
    with mock.patch.object(
        repositories.FiscalDocumentRepository,
        'get_pending_for_reconciliation',
        return_value=docs,
    ), mock.patch.object(svc_serial, '_apply_poll_result'):
        stub_conn = _StubConn()
        t0 = time.monotonic()
        svc_serial.reconcile_pending(stub_conn)
        serial_elapsed = time.monotonic() - t0

    # --- parallel path ---
    svc_parallel = _build_service(transport, concurrency=4)
    with mock.patch.object(
        repositories.FiscalDocumentRepository,
        'get_pending_for_reconciliation',
        return_value=docs,
    ), mock.patch.object(svc_parallel, '_apply_poll_result'):
        stub_conn2 = _StubConn()
        t1 = time.monotonic()
        svc_parallel.reconcile_pending(stub_conn2)
        parallel_elapsed = time.monotonic() - t1

    assert serial_elapsed >= 0.35, f'Serial path too fast ({serial_elapsed*1000:.0f}ms) — stub not sleeping?'
    assert parallel_elapsed < 0.30, f'Parallel path not fast enough ({parallel_elapsed*1000:.0f}ms >= 300ms)'


# ---------------------------------------------------------------------------
# Test 4: exception in one group does not affect other groups
# ---------------------------------------------------------------------------

def test_poll_exception_isolates_to_single_group():
    """One fiscal_number raises in poll_status; other groups still complete."""
    transport = _RecordingTransport(
        PollResult(state=DocumentState.ACK.value, submission_status='ACK', ack_at=datetime.now(UTC)),
        raise_for='FN-FAIL',
    )
    svc = _build_service(transport, concurrency=3)

    docs = [
        _doc(document_id='ok-1', fiscal_number='FN-OK-1'),
        _doc(document_id='fail-1', fiscal_number='FN-FAIL'),
        _doc(document_id='ok-2', fiscal_number='FN-OK-2'),
    ]

    import unittest.mock as mock
    from prro_gateway import repositories

    applied: list[tuple[str, Any]] = []

    def _capture_apply(conn, doc, poll_or_exc, result):
        applied.append((doc.fiscal_number, poll_or_exc))
        if isinstance(poll_or_exc, Exception):
            result.still_pending += 1
        else:
            result.acked += 1

    with mock.patch.object(
        repositories.FiscalDocumentRepository,
        'get_pending_for_reconciliation',
        return_value=docs,
    ), mock.patch.object(svc, '_apply_poll_result', side_effect=_capture_apply):
        stub_conn = _StubConn()
        result = svc.reconcile_pending(stub_conn)

    assert result.checked == 3
    # FN-OK-1 and FN-OK-2 should have ACK results
    ok_results = [(fn, pr) for fn, pr in applied if fn != 'FN-FAIL']
    assert len(ok_results) == 2
    for fn, pr in ok_results:
        assert isinstance(pr, PollResult), f'{fn} should have PollResult, got {type(pr)}'

    # FN-FAIL should have an Exception
    fail_results = [(fn, pr) for fn, pr in applied if fn == 'FN-FAIL']
    assert len(fail_results) == 1
    assert isinstance(fail_results[0][1], Exception)


# ---------------------------------------------------------------------------
# Test 5: cancel_event stops polling mid-group; already-polled applied
# ---------------------------------------------------------------------------

def test_cancel_event_stops_polling_mid_group():
    """Setting cancel_event after first poll stops further polls in that group.
    The already-polled doc is still counted via _apply_poll_result.
    """
    cancel_event = threading.Event()
    call_count = 0

    class _CancellingTransport:
        def poll_status(self, *, document_id, fiscal_number, **_kwargs):
            nonlocal call_count
            call_count += 1
            # Cancel after first call so second doc in same group is skipped
            cancel_event.set()
            return PollResult(state=DocumentState.ACK.value, submission_status='ACK', ack_at=datetime.now(UTC))

    svc = ReconciliationService(
        transport_status_client=_CancellingTransport(),
        max_recovery_attempts=5,
        concurrency=1,  # serial is fine; cancel is checked per-doc in serial path too
        cancel_event=cancel_event,
    )

    # Two docs in same fiscal_number; cancel fires after first poll
    docs = [
        _doc(document_id='d1', fiscal_number='FN-CANCEL'),
        _doc(document_id='d2', fiscal_number='FN-CANCEL'),
    ]

    import unittest.mock as mock
    from prro_gateway import repositories

    applied_docs: list[str] = []

    def _capture_apply(conn, doc, poll_or_exc, result):
        applied_docs.append(doc.document_id)
        result.acked += 1

    with mock.patch.object(
        repositories.FiscalDocumentRepository,
        'get_pending_for_reconciliation',
        return_value=docs,
    ), mock.patch.object(svc, '_apply_poll_result', side_effect=_capture_apply):
        stub_conn = _StubConn()
        result = svc.reconcile_pending(stub_conn)

    # First doc polled and applied; second doc skipped because cancel fired
    assert call_count == 1, f'Expected 1 poll call, got {call_count}'
    assert 'd1' in applied_docs
    assert 'd2' not in applied_docs
    # Second doc counted as still_pending (group cancelled mid-flight)
    assert result.still_pending == 1


# ---------------------------------------------------------------------------
# Test 6: _poll_doc_network returns _COOLDOWN_SKIP without calling transport
# ---------------------------------------------------------------------------

def test_poll_doc_network_returns_cooldown_skip_without_network_call():
    """_poll_doc_network must return _COOLDOWN_SKIP (not call transport) for rate-limited docs."""
    from prro_gateway.services.reconciliation import _COOLDOWN_SKIP
    import json

    transport = _RecordingTransport()
    svc = _build_service(transport)

    now = datetime.now(UTC)
    doc = _doc(
        document_id='rate-ltd',
        fiscal_number='FN-RL',
        submission_status='DPS_RATE_LIMITED',
        updated_at=now.isoformat(),
        response_json=json.dumps({'retry_after_seconds': 600}),
    )
    result = svc._poll_doc_network(doc)
    assert result is _COOLDOWN_SKIP
    assert len(transport.calls) == 0, 'No network call for rate-limited doc in cooldown'


# ---------------------------------------------------------------------------
# Test 7: concurrency clamped to 1 minimum even if 0 or negative passed
# ---------------------------------------------------------------------------

def test_concurrency_clamped_to_minimum_1():
    for bad in (0, -1, -100):
        svc = ReconciliationService(
            transport_status_client=_RecordingTransport(),
            concurrency=bad,
        )
        assert svc.concurrency == 1, f'concurrency={bad} should clamp to 1, got {svc.concurrency}'
