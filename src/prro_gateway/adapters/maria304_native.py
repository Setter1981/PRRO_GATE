"""Adapter for the `maria304_driver` Rust ingress.

The Rust driver posts canonical-shaped envelopes to
`POST /v1/ingress/maria304`.  This adapter normalises that wire shape
(`rust/maria304_driver/src/bridge/dto.rs::CanonicalCommand`) into the
gateway's internal `CanonicalFiscalCommand` without re-parsing the
accumulated `raw_frames` — rich fiscal-line parsing is out of scope for
M7-Py-1 (see plan 2026-04-21-maria304-python-adapter §1 Не входить).

Invariants touched (per CLAUDE.md §Frozen invariants):
* #4 idempotency — `idempotency_key` from the driver is used verbatim
  as `external_request_id` so the canonical key is keyed on the
  driver's session/receipt tuple, not on a payload hash.  `request_id`
  is freshly generated per HTTP attempt (avoids collision with the
  inbox unique constraint on replay — review finding B4).
* #6 full payload — `goods`, `payments`, and `raw_frames` are
  preserved as-is; missing top-level optionals (`cashier_id`,
  `department`) are allowed but surface in the canonical payload.
* #7 schema_version — the canonical envelope is stamped with the
  gateway's `SCHEMA_VERSION`, not the Rust-side protocol version.

Security posture:
* All fields are type-guarded; hostile / malformed input raises
  `AdapterMappingError(PAYLOAD_VALIDATION_FAILED)` rather than bare
  TypeError (review finding B1).
* `raw_frames`, `goods`, `payments` are bounded to prevent memory
  exhaustion (review finding B2).  Caps are generous (enough for a
  full shift of receipts in one burst) but bounded.
* `idempotency_key` is regex-validated so only the Rust-side key
  shape reaches the inbox (review finding R6).
* `business_ts` is currently set to ingest-time UTC — the Rust DTO
  does not carry a device timestamp field.  This bypasses backdating
  guards in the write-path (review finding B3); documented here and
  deferred to a Rust-side DTO extension in a later sprint.
"""
from __future__ import annotations

import re
import uuid
from datetime import UTC, datetime
from typing import Any

from ..enums import OperationType, Protocol
from .base import AdapterContext, AdapterMappingError, CanonicalEnvelopeBuilder

_COMMAND_TO_OPERATION: dict[str, OperationType] = {
    "SELL": OperationType.SELL,
    "RETURN": OperationType.RETURN,
    "SHIFT_OPEN": OperationType.SHIFT_OPEN,
    "SHIFT_CLOSE": OperationType.SHIFT_CLOSE,
    "X_REPORT": OperationType.X_REPORT,
    "Z_REPORT": OperationType.Z_REPORT,
    "SERVICE_IN": OperationType.SERVICE_IN,
    "SERVICE_OUT": OperationType.SERVICE_OUT,
}

_UNSUPPORTED_COMMANDS: frozenset[str] = frozenset({"PERIODIC_REPORT"})

_REQUIRES_SHIFT_OPS: frozenset[OperationType] = frozenset({
    OperationType.SELL,
    OperationType.RETURN,
    OperationType.SERVICE_IN,
    OperationType.SERVICE_OUT,
    OperationType.X_REPORT,
    OperationType.Z_REPORT,
    OperationType.SHIFT_CLOSE,
})

_IDEMPOTENCY_KEY_PATTERN = re.compile(r"^maria304:[A-Za-z0-9_\-:]{3,256}$")

_CAIO_OPCODE_FOR_OPERATION: dict[OperationType, str] = {
    OperationType.SERVICE_IN: "CAIOI",
    OperationType.SERVICE_OUT: "CAIOO",
}

# Wire format: `<D10 sum_kopecks><description>`.  D10 = 10 zero-padded
# ASCII digits.  `description` is CP866-encoded by the driver before
# framing; at this layer we see it as a UTF-8 str already.
_CAIO_SUM_PREFIX_CHARS = 10
_CAIO_DESCRIPTION_MAX_CHARS = 60

# Safety caps — generous but bounded.  A 2000-frame receipt is three
# orders of magnitude above a realistic grocery-basket ceiling; anything
# above this is either abuse or a driver bug.
_MAX_RAW_FRAMES = 2_000
_MAX_FRAME_BODY_CHARS = 4_096
_MAX_GOODS = 500
_MAX_PAYMENTS = 16


class Maria304NativeAdapter:
    def map_command(self, raw_request: dict[str, Any]) -> Any:
        if not isinstance(raw_request, dict):
            raise AdapterMappingError(
                "raw_request must be a JSON object",
                code="PAYLOAD_VALIDATION_FAILED",
            )

        fiscal_number = _require_str(raw_request, "fiscal_number")
        idempotency_key = _require_str(raw_request, "idempotency_key")
        command_type = _require_str(raw_request, "command_type")
        receipt_payload = _require_dict(raw_request, "payload")

        if not _IDEMPOTENCY_KEY_PATTERN.match(idempotency_key):
            raise AdapterMappingError(
                "idempotency_key does not match expected Maria 304 shape",
                code="PAYLOAD_VALIDATION_FAILED",
            )

        if command_type in _UNSUPPORTED_COMMANDS:
            raise AdapterMappingError(
                f"Maria 304 command not yet supported: {command_type}",
                code="UNSUPPORTED_METHOD",
            )
        if command_type not in _COMMAND_TO_OPERATION:
            raise AdapterMappingError(
                f"Unknown Maria 304 command_type: {command_type}",
                code="UNKNOWN_METHOD",
            )
        operation_type = _COMMAND_TO_OPERATION[command_type]

        payload = self._build_payload(raw_request, receipt_payload)
        if operation_type in _CAIO_OPCODE_FOR_OPERATION:
            self._enrich_service_payload(payload, operation_type)
        now_utc = datetime.now(UTC)
        context = AdapterContext(
            request_id=self._build_request_id(fiscal_number, now_utc),
            fiscal_number=fiscal_number,
            channel_owner="maria304-driver",
            business_ts=now_utc,
        )
        requires_shift = operation_type in _REQUIRES_SHIFT_OPS

        return CanonicalEnvelopeBuilder.build(
            context=context,
            protocol=Protocol.MARIA_304_NATIVE,
            operation_type=operation_type,
            payload=payload,
            external_request_id=idempotency_key,
            requires_shift=requires_shift,
            requires_offline_code=False,
        )

    @staticmethod
    def _enrich_service_payload(
        payload: dict[str, Any], operation_type: OperationType,
    ) -> None:
        """Parse SERVICE_IN/SERVICE_OUT sum from the CAIO raw_frame.

        The Rust driver captures `CAIOI<D10 sum><desc>` / `CAIOO<...>`
        verbatim into `raw_frames[0]`.  The canonical write-path
        validator expects `payload["service_sum"]` — this method
        extracts it, plus stashes the description under the receipt
        for audit.
        """
        frames = payload.get("receipt", {}).get("raw_frames") or []
        if not frames:
            raise AdapterMappingError(
                "SERVICE_IN/SERVICE_OUT requires at least one raw_frame",
                code="PAYLOAD_VALIDATION_FAILED",
            )
        first = frames[0]
        expected_opcode = _CAIO_OPCODE_FOR_OPERATION[operation_type]
        if not isinstance(first, dict) or first.get("opcode") != expected_opcode:
            raise AdapterMappingError(
                f"First raw_frame must be {expected_opcode} for {operation_type.value}",
                code="PAYLOAD_VALIDATION_FAILED",
            )
        body = first.get("body", "")
        if not isinstance(body, str) or len(body) < _CAIO_SUM_PREFIX_CHARS:
            raise AdapterMappingError(
                f"{expected_opcode} body must contain a 10-char sum prefix",
                code="PAYLOAD_VALIDATION_FAILED",
            )
        sum_chars = body[:_CAIO_SUM_PREFIX_CHARS]
        # Pin ASCII: int() also accepts Unicode digits (Arabic-Indic
        # ٠-٩, etc.) which would desync from the wire bytes the DPS
        # signer will hash.  Review finding R1.
        if not (sum_chars.isascii() and sum_chars.isdigit()):
            raise AdapterMappingError(
                f"{expected_opcode} sum prefix must be 10 ASCII digits",
                code="PAYLOAD_VALIDATION_FAILED",
            )
        description = body[_CAIO_SUM_PREFIX_CHARS:_CAIO_SUM_PREFIX_CHARS + _CAIO_DESCRIPTION_MAX_CHARS]
        payload["service_sum"] = int(sum_chars)
        payload["receipt"]["service_description"] = description

    @staticmethod
    def _build_request_id(fiscal_number: str, now: datetime) -> str:
        # Fresh per ingest attempt; replays match on idempotency_key.
        # A uuid4 suffix prevents collision when two requests land in
        # the same microsecond (or on platforms where time.time() has
        # sub-µs coarseness — WSL/Windows).  Review finding N1.
        return (
            f"req-maria304-{fiscal_number}-"
            f"{int(now.timestamp() * 1_000_000)}-{uuid.uuid4().hex[:8]}"
        )

    @staticmethod
    def _build_payload(
        raw_request: dict[str, Any], receipt: dict[str, Any],
    ) -> dict[str, Any]:
        raw_frames = _bounded_list(receipt.get("raw_frames"), _MAX_RAW_FRAMES, "raw_frames")
        _validate_raw_frames(raw_frames)
        goods = _bounded_list(receipt.get("goods"), _MAX_GOODS, "goods")
        payments = _bounded_list(receipt.get("payments"), _MAX_PAYMENTS, "payments")

        totals_raw = receipt.get("totals")
        if totals_raw is not None and not isinstance(totals_raw, dict):
            raise AdapterMappingError(
                "receipt.totals must be an object or null",
                code="PAYLOAD_VALIDATION_FAILED",
            )
        totals_payload = dict(totals_raw) if totals_raw is not None else {}
        dual_tax_raw = receipt.get("dual_tax_mode")
        if dual_tax_raw is not None and not isinstance(dual_tax_raw, dict):
            raise AdapterMappingError(
                "receipt.dual_tax_mode must be an object or null",
                code="PAYLOAD_VALIDATION_FAILED",
            )

        return {
            "cashier_id": _optional_str(raw_request.get("cashier_id"), "cashier_id"),
            "department": _optional_str(raw_request.get("department"), "department"),
            "return_check_number": _optional_str(
                raw_request.get("return_check_number"), "return_check_number",
            ),
            "receipt": {
                "direction": _optional_str(receipt.get("direction"), "direction") or "SALE",
                "goods": goods,
                "payments": payments,
                "dual_tax_mode": dual_tax_raw,
                "totals": totals_payload,
                "raw_frames": raw_frames,
            },
        }


def _require_str(source: dict[str, Any], field: str) -> str:
    if field not in source:
        raise AdapterMappingError(
            f"Missing required field: {field}",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    value = source[field]
    if not isinstance(value, str) or not value:
        raise AdapterMappingError(
            f"Field {field} must be a non-empty string",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    return value


def _require_dict(source: dict[str, Any], field: str) -> dict[str, Any]:
    if field not in source:
        raise AdapterMappingError(
            f"Missing required field: {field}",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    value = source[field]
    if not isinstance(value, dict):
        raise AdapterMappingError(
            f"Field {field} must be a JSON object",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    return value


def _optional_str(value: Any, field: str) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise AdapterMappingError(
            f"Field {field} must be a string or null",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    return value


def _bounded_list(value: Any, cap: int, field: str) -> list[Any]:
    if value is None:
        return []
    if not isinstance(value, list):
        raise AdapterMappingError(
            f"Field {field} must be a list",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    if len(value) > cap:
        raise AdapterMappingError(
            f"Field {field} exceeds cap of {cap} entries",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    return list(value)


def _validate_raw_frames(frames: list[Any]) -> None:
    for idx, frame in enumerate(frames):
        if not isinstance(frame, dict):
            raise AdapterMappingError(
                f"raw_frames[{idx}] must be an object",
                code="PAYLOAD_VALIDATION_FAILED",
            )
        body = frame.get("body", "")
        if not isinstance(body, str):
            raise AdapterMappingError(
                f"raw_frames[{idx}].body must be a string",
                code="PAYLOAD_VALIDATION_FAILED",
            )
        if len(body) > _MAX_FRAME_BODY_CHARS:
            raise AdapterMappingError(
                f"raw_frames[{idx}].body exceeds {_MAX_FRAME_BODY_CHARS} chars",
                code="PAYLOAD_VALIDATION_FAILED",
            )
        opcode = frame.get("opcode", "")
        if not isinstance(opcode, str):
            raise AdapterMappingError(
                f"raw_frames[{idx}].opcode must be a string",
                code="PAYLOAD_VALIDATION_FAILED",
            )


__all__ = ["Maria304NativeAdapter"]
