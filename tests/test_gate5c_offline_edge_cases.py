"""Gate 5C — Cross-month offline rollover + OFFLINE_BACKLOG_NOT_SYNCED tests."""
from __future__ import annotations

from datetime import UTC, datetime

import pytest

from prro_gateway.enums import (
    CanonicalErrorCode,
    DocumentState,
    OperationType,
    Protocol,
    ShiftState,
)
from prro_gateway.models.canonical import CanonicalFiscalCommand
from prro_gateway.repositories import (
    FiscalDocumentRepository,
    InboxRepository,
    NodeStateRepository,
    ShiftRepository,
)
from prro_gateway.services import WritePathWorker
from prro_gateway.utils.json_codec import dumps_json

FN = "FN-DEV-0001"
BACKEND = "backend_checkbox_default"
TRANSPORT = "transport_checkbox_rest_default"

_counter = 0


def _uid(prefix: str) -> str:
    global _counter
    _counter += 1
    return f"{prefix}-5c-{_counter}"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _ok_transport():
    class _T:
        def send(self, **kw):
            from prro_gateway.ports import SendResult

            now = datetime.now(UTC)
            return SendResult(
                transport_request_id="tx",
                submission_status="ACK",
                server_fiscal_no="SFN",
                server_fiscal_date=now.isoformat(),
                response_json="{}",
                sent_at=now,
                ack_at=now,
            )

    return _T()


def _ok_crypto():
    class _C:
        def sign(self, *, document_id, payload_json):
            return f"sig::{document_id}"

    return _C()


def _insert_offline_local_ack_doc(conn, *, shift_id: str) -> str:
    """Insert a minimal OFFLINE_LOCAL_ACK document (simulates a doc pending DPS sync)."""
    doc_id = _uid("doc")
    req_id = _uid("req")
    # Insert a matching inbox row first (FK constraint)
    conn.execute(
        """
        INSERT INTO ingress_inbox (
            request_id, idempotency_key, protocol, operation_type, fiscal_number,
            payload_json, payload_sha256, status
        ) VALUES (?, ?, 'CHECKBOX_REST', 'SELL', ?, '{}', ?, 'DONE')
        """,
        (req_id, _uid("idem"), FN, _uid("sha")),
    )
    FiscalDocumentRepository.create_prepared(
        conn,
        document_id=doc_id,
        request_id=req_id,
        fiscal_number=FN,
        lnd=1,
        doc_type="SELL",
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        fs_mode="OFFLINE",
        business_ts="2026-04-17T09:00:00+00:00",
        payload_json="{}",
        payload_sha256=_uid("sha"),
        shift_id=shift_id,
        offline_fiscal_no=1,
    )
    conn.execute(
        "UPDATE fiscal_documents SET state='OFFLINE_LOCAL_ACK', submission_status='OFFLINE_LOCAL'"
        " WHERE document_id=?",
        (doc_id,),
    )
    return doc_id


def _create_open_shift(conn) -> str:
    shift_id = _uid("shift")
    conn.execute("BEGIN IMMEDIATE")
    ShiftRepository.create_shift(
        conn,
        shift_id=shift_id,
        fiscal_number=FN,
        state=ShiftState.OPENED,
        open_mode="ONLINE",
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST,
        integration_owner="test",
        channel_lock_acquired_at="2026-04-17T09:00:00+00:00",
    )
    conn.commit()
    return shift_id


def _enqueue(conn, operation_type: OperationType) -> str:
    rid = _uid("req")
    cmd = CanonicalFiscalCommand(
        request_id=rid,
        idempotency_key=_uid("idem"),
        protocol=Protocol.CHECKBOX_REST,
        operation_type=operation_type,
        fiscal_number=FN,
        route_key="pos-1",
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner="test",
        external_request_id=_uid("ext"),
        business_ts=datetime(2026, 4, 17, 10, 0, 0, tzinfo=UTC),
        payload={"receipt": {"type": operation_type.value}},
        payload_sha256=_uid("sha"),
    )
    conn.execute("BEGIN IMMEDIATE")
    InboxRepository.accept_command(
        conn,
        request_id=rid,
        idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol,
        operation_type=cmd.operation_type,
        fiscal_number=FN,
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner="test",
        external_request_id=cmd.external_request_id,
        payload_json=dumps_json(cmd.model_dump(mode="json")),
        payload_sha256=cmd.payload_sha256,
    )
    conn.commit()
    return rid


# ---------------------------------------------------------------------------
# Test 1: Cross-month rollover resets offline seconds
# ---------------------------------------------------------------------------


def test_cross_month_resets_offline_seconds(conn):
    """update_offline_seconds with a new month_bucket resets seconds to delta."""
    # Seed state with a large accumulated count in the previous month
    conn.execute("BEGIN IMMEDIATE")
    conn.execute(
        """UPDATE node_state
           SET current_month_bucket='2026-03',
               current_month_offline_seconds=500000
           WHERE fiscal_number=?""",
        (FN,),
    )
    conn.commit()

    # Call with a new month bucket — should reset, not accumulate
    state = NodeStateRepository.update_offline_seconds(
        conn,
        fiscal_number=FN,
        month_bucket="2026-04",
        seconds_delta=3600,
    )
    conn.commit()

    assert state is not None
    assert state.current_month_bucket == "2026-04", (
        f"Expected bucket 2026-04, got {state.current_month_bucket}"
    )
    assert state.current_month_offline_seconds == 3600, (
        f"Expected 3600 after cross-month reset, got {state.current_month_offline_seconds}"
    )


# ---------------------------------------------------------------------------
# Test 2: SHIFT_CLOSE blocked when offline backlog is non-empty
# ---------------------------------------------------------------------------


def test_shift_close_blocked_by_offline_backlog(conn):
    """SHIFT_CLOSE must be rejected with OFFLINE_BACKLOG_NOT_SYNCED when
    there are OFFLINE_LOCAL_ACK documents pending DPS submission."""
    shift_id = _create_open_shift(conn)

    # Insert a document in OFFLINE_LOCAL_ACK state — backlog not empty
    conn.execute("BEGIN IMMEDIATE")
    doc_id = _insert_offline_local_ack_doc(conn, shift_id=shift_id)
    conn.commit()

    # Verify backlog count
    pending = FiscalDocumentRepository.count_pending_for_offline_sync(conn, fiscal_number=FN)
    assert pending >= 1, "Precondition: must have at least one pending doc"

    # Enqueue SHIFT_CLOSE
    _enqueue(conn, OperationType.SHIFT_CLOSE)

    worker = WritePathWorker(
        crypto_provider=_ok_crypto(),
        transport_client=_ok_transport(),
        crypto_breaker_threshold=0,
    )
    result = worker.process_next(conn, fiscal_number=FN)

    # Must be blocked — shift must NOT become CLOSED
    assert result.outcome != "ACK", (
        f"SHIFT_CLOSE must not ACK while offline backlog is pending, got: {result}"
    )
    assert result.canonical_error is not None, "Expected canonical_error on rejection"
    assert CanonicalErrorCode.OFFLINE_BACKLOG_NOT_SYNCED.value in result.canonical_error.code, (
        f"Expected OFFLINE_BACKLOG_NOT_SYNCED error, got: {result.canonical_error.code}"
    )

    # Shift remains open
    active = ShiftRepository.get_active_shift(conn, FN)
    assert active is not None, "Shift must still be active after blocked SHIFT_CLOSE"
    assert active.state != ShiftState.CLOSED, (
        f"Shift must not be CLOSED after blocked SHIFT_CLOSE, got: {active.state}"
    )
