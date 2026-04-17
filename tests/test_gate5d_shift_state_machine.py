"""Gate 5D — ShiftState transitions for SHIFT_CLOSE error paths.

Two scenarios:
  1. Transport raises TransportRetryableError during SHIFT_CLOSE
     → shift must NOT become CLOSED (stays OPENED or moves to CLOSING at most)
  2. Transport returns ACK for SHIFT_CLOSE
     → shift must become CLOSED
"""
from __future__ import annotations

from datetime import UTC, datetime

from prro_gateway.enums import OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.services import WritePathWorker
from prro_gateway.utils.json_codec import dumps_json

FN = "FN-DEV-0001"
BACKEND = "backend_checkbox_default"
TRANSPORT = "transport_checkbox_rest_default"

_counter = 0


def _uid(prefix: str) -> str:
    global _counter
    _counter += 1
    return f"{prefix}-sfm-{_counter}"


# ---------------------------------------------------------------------------
# Fakes
# ---------------------------------------------------------------------------


class _OkCrypto:
    def sign(self, *, document_id, payload_json):
        return f"sig::{document_id}"

    def sign_raw(self, *, data, document_id=None):
        return b"signed"


class _FailTransport:
    def send(self, **kw):
        from prro_gateway.ports import TransportRetryableError

        raise TransportRetryableError("simulated transport failure")


class _OkTransport:
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


# ---------------------------------------------------------------------------
# Setup helpers
# ---------------------------------------------------------------------------


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


def _enqueue_shift_close(conn) -> None:
    rid = _uid("req")
    cmd = CanonicalFiscalCommand(
        request_id=rid,
        idempotency_key=_uid("idem"),
        protocol=Protocol.CHECKBOX_REST,
        operation_type=OperationType.SHIFT_CLOSE,
        fiscal_number=FN,
        route_key="pos-1",
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner="test",
        external_request_id=_uid("ext"),
        business_ts=datetime(2026, 4, 17, 10, 0, 0, tzinfo=UTC),
        payload={"receipt": {"type": "SHIFT_CLOSE"}},
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


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


def test_shift_close_transport_error_shift_not_closed(conn):
    """A retryable transport error during SHIFT_CLOSE must not close the shift."""
    _create_open_shift(conn)
    _enqueue_shift_close(conn)

    worker = WritePathWorker(
        crypto_provider=_OkCrypto(),
        transport_client=_FailTransport(),
        crypto_breaker_threshold=0,
    )
    result = worker.process_next(conn, fiscal_number=FN)

    assert result.outcome != "ACK", (
        f"Transport error must not produce ACK outcome, got: {result.outcome}"
    )

    # Shift must not be CLOSED — check the most-recent shift row directly
    row = conn.execute(
        "SELECT state FROM shifts WHERE fiscal_number=? ORDER BY rowid DESC LIMIT 1",
        (FN,),
    ).fetchone()
    assert row is not None, "Shift row must exist"
    assert row[0] != ShiftState.CLOSED.value, (
        f"Shift must not be CLOSED after transport error, got state={row[0]}"
    )


def test_shift_close_success_marks_closed(conn):
    """A successful SHIFT_CLOSE must transition the shift to CLOSED."""
    _create_open_shift(conn)
    _enqueue_shift_close(conn)

    worker = WritePathWorker(
        crypto_provider=_OkCrypto(),
        transport_client=_OkTransport(),
        crypto_breaker_threshold=0,
    )
    result = worker.process_next(conn, fiscal_number=FN)

    assert result.outcome == "ACK", (
        f"Expected ACK on successful SHIFT_CLOSE, got {result.outcome}: {result.canonical_error}"
    )

    row = conn.execute(
        "SELECT state FROM shifts WHERE fiscal_number=? ORDER BY rowid DESC LIMIT 1",
        (FN,),
    ).fetchone()
    assert row is not None, "Shift row must exist"
    assert row[0] == ShiftState.CLOSED.value, (
        f"Expected CLOSED after successful SHIFT_CLOSE, got state={row[0]}"
    )
