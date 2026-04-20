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

from ..enums import AcquiringSource, OperationType, PaymentType, Protocol
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
    "CASH_WITHDRAWAL": OperationType.CASH_WITHDRAWAL,
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

# CSHG wire format (per Maria 304 protocol §54):
# `<sum:9><fee:9><L:2 ACQ><L:2 TERM><L:2 PAN><L:2 PS><L:2 AUTH><L:2 RRN>
#  <cashier_sig:1><cardholder_sig:1><qr_mode:1><qr_scale:2><qr_level:1>`
# All numeric fields are ASCII digits; length prefixes are 2 digits.
# The Rust driver decodes CP866 to UTF-8 at the wire layer
# (rust/maria304_driver/src/wire/cp866.rs) before shipping the body to
# us, so Python str codepoint indexing matches the length prefix count.
_CSHG_SUM_CHARS = 9
_CSHG_FEE_CHARS = 9
_CSHG_LENGTH_FIELD_CHARS = 2
_CSHG_TAIL_CHARS = 1 + 1 + 1 + 2 + 1  # flags + QR mode/scale/level

# Per-field length caps per Maria 304 protocol §54 (CSHG command).
# acquirer_id allows 1..64 chars; the rest 1..32.  Tighter caps catch
# malformed frames + prevent downstream printer/buffer overruns.
# Review finding R1.
_CSHG_FIELD_LIMITS: dict[str, tuple[int, int]] = {
    "merchant_id":    (1, 64),
    "terminal_id":    (1, 32),
    "pan":            (1, 32),
    "payment_system": (1, 32),
    "auth_code":      (1, 32),
    "rrn":            (1, 32),
}

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
        elif operation_type == OperationType.CASH_WITHDRAWAL:
            self._enrich_cashwithdrawal_payload(payload)
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
    def _enrich_cashwithdrawal_payload(payload: dict[str, Any]) -> None:
        """Parse CSHG 19-parameter body and synthesise the canonical
        CASH_WITHDRAWAL payload.

        The Rust driver does NOT parse CSHG — it captures the body
        verbatim into `raw_frames`.  Here we walk the 19 fields in
        order, extract the acquirer slip details, and build a single
        CASHLESS payment entry that matches `cash_withdrawal_sum` so
        `validate_cash_withdrawal_receipt` passes downstream.
        """
        frames = payload.get("receipt", {}).get("raw_frames") or []
        cshg_body: str | None = None
        for frame in frames:
            if isinstance(frame, dict) and frame.get("opcode") == "CSHG":
                candidate = frame.get("body")
                if isinstance(candidate, str):
                    cshg_body = candidate
                    break
        if cshg_body is None:
            raise AdapterMappingError(
                "CASH_WITHDRAWAL requires a CSHG frame in raw_frames",
                code="PAYLOAD_VALIDATION_FAILED",
            )
        parsed = _parse_cshg_body(cshg_body)
        payment = {
            "payment_id": "maria304-cshg-1",
            "payment_type": PaymentType.CASHLESS.value,
            "amount_kopecks": parsed["sum_kopecks"],
            "amount": parsed["sum_kopecks"],
            "commission": parsed["fee"],
            "acquirer_and_seller": parsed["merchant_id"],
            "terminal": parsed["terminal_id"],
            "card_mask": parsed["pan"],
            "payment_system": parsed["payment_system"],
            "auth_code": parsed["auth_code"],
            "rrn": parsed["rrn"],
            "signature_required": parsed["cashier_sig"] or parsed["cardholder_sig"],
            "acquiring_source": AcquiringSource.POS_INTEGRATION.value,
        }
        payload["cash_withdrawal_sum"] = parsed["sum_kopecks"]
        payload["receipt"]["payments"] = [payment]
        # Keep totals.total_sum consistent with the CSHG sum so the
        # validator's cross-check passes.
        totals = payload["receipt"].get("totals") or {}
        totals["total_sum"] = parsed["sum_kopecks"]
        payload["receipt"]["totals"] = totals
        # Surface CSHG-specific metadata at receipt level for audit /
        # reconciliation with acquirer statements (review finding B2).
        payload["receipt"]["cshg_fee_kopecks"] = parsed["fee"]
        payload["receipt"]["cshg_qr"] = {
            "mode": parsed["qr_mode"],
            "scale": parsed["qr_scale"],
            "level": parsed["qr_level"],
        }

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


def _read_ascii_digits(body: str, offset: int, length: int, field: str) -> int:
    end = offset + length
    if len(body) < end:
        raise AdapterMappingError(
            f"CSHG body truncated at {field}",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    chunk = body[offset:end]
    if not (chunk.isascii() and chunk.isdigit()):
        raise AdapterMappingError(
            f"CSHG {field} must be {length} ASCII digits; got {chunk!r}",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    return int(chunk)


def _read_length_prefixed(body: str, offset: int, field: str) -> tuple[str, int]:
    length = _read_ascii_digits(body, offset, _CSHG_LENGTH_FIELD_CHARS, f"{field}_len")
    low, high = _CSHG_FIELD_LIMITS[field]
    if length < low or length > high:
        raise AdapterMappingError(
            f"CSHG {field} length {length} out of [{low},{high}]",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    value_start = offset + _CSHG_LENGTH_FIELD_CHARS
    value_end = value_start + length
    if len(body) < value_end:
        raise AdapterMappingError(
            f"CSHG body truncated in {field} value (claimed length {length})",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    return body[value_start:value_end], value_end


def _parse_cshg_body(body: str) -> dict[str, Any]:
    if not isinstance(body, str):
        raise AdapterMappingError(
            "CSHG body must be a string",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    offset = 0
    sum_kopecks = _read_ascii_digits(body, offset, _CSHG_SUM_CHARS, "sum")
    offset += _CSHG_SUM_CHARS
    fee = _read_ascii_digits(body, offset, _CSHG_FEE_CHARS, "fee")
    offset += _CSHG_FEE_CHARS

    merchant_id, offset = _read_length_prefixed(body, offset, "merchant_id")
    terminal_id, offset = _read_length_prefixed(body, offset, "terminal_id")
    pan, offset = _read_length_prefixed(body, offset, "pan")
    payment_system, offset = _read_length_prefixed(body, offset, "payment_system")
    auth_code, offset = _read_length_prefixed(body, offset, "auth_code")
    rrn, offset = _read_length_prefixed(body, offset, "rrn")

    tail_end = offset + _CSHG_TAIL_CHARS
    if len(body) < tail_end:
        raise AdapterMappingError(
            "CSHG body truncated in trailing flags/QR params",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    # Strict: reject trailing garbage after the 19-param sequence.
    # Review finding R3 — §54 is a fixed-schema frame, not extensible.
    if len(body) > tail_end:
        raise AdapterMappingError(
            f"CSHG body has {len(body) - tail_end} trailing chars after params",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    cashier_sig_char = body[offset]
    cardholder_sig_char = body[offset + 1]
    if cashier_sig_char not in ("0", "1") or cardholder_sig_char not in ("0", "1"):
        raise AdapterMappingError(
            "CSHG signature flags must be '0' or '1'",
            code="PAYLOAD_VALIDATION_FAILED",
        )
    qr_mode = body[offset + 2]
    qr_scale = body[offset + 3:offset + 5]
    qr_level = body[offset + 5]

    # Zero-sum CSHG violates fiscal invariant (no real withdrawal).
    # Reject at adapter boundary — review finding B1.
    if sum_kopecks <= 0:
        raise AdapterMappingError(
            "CSHG sum must be > 0",
            code="PAYLOAD_VALIDATION_FAILED",
        )

    return {
        "sum_kopecks": sum_kopecks,
        "fee": fee,
        "merchant_id": merchant_id,
        "terminal_id": terminal_id,
        "pan": pan,
        "payment_system": payment_system,
        "auth_code": auth_code,
        "rrn": rrn,
        "cashier_sig": cashier_sig_char == "1",
        "cardholder_sig": cardholder_sig_char == "1",
        "qr_mode": qr_mode,
        "qr_scale": qr_scale,
        "qr_level": qr_level,
    }


__all__ = ["Maria304NativeAdapter"]
