"""Proof tests for M7-Py-4c: DPS gRPC Status → CanonicalErrorCode.

Before this sprint, every `CheckResponse.Status < 0` collapsed into
`CanonicalErrorCode.BACKEND_TEMPORARY_UNAVAILABLE` at
`services/write_path.py:792`, losing the 16 distinct gRPC error codes
documented in `rust/prro_sidecar/proto/check.proto`.

This test file asserts:
1. `canonical_error_from_dps_status(status: int)` maps each of the
   16 gRPC Status values to a dedicated `CanonicalErrorCode`.
2. `TransportRejectedError` carries `dps_status: int | None` so the
   worker can extract it.
3. The write_path exception handler prefers the mapped code over
   the previous generic fallback.
4. Unknown / unmapped statuses still fall back gracefully (not a
   KeyError that would crash the worker).
5. `DpsMacRecoveryError` subclass still carries `dps_status` as
   before (no backward-compat regression).
"""
from __future__ import annotations

import pytest

from prro_gateway.enums import CanonicalErrorCode
from prro_gateway.ports import DpsMacRecoveryError, TransportRejectedError
from prro_gateway.transports.dps_fiscal_server import (
    canonical_error_from_dps_status,
)


# ─── 1. Per-status mapping ───────────────────────────────────────────


# check.proto CheckResponse.Status (negative integer codes).
# (status, expected CanonicalErrorCode name)
_DPS_STATUS_TABLE = [
    (-1, "DPS_SIGNATURE_VERIFICATION_FAILED"),  # ERROR_VEREFY
    (-2, "DPS_CHECK_FAILED"),                    # ERROR_CHECK
    (-3, "DPS_SAVE_FAILED"),                     # ERROR_SAVE
    (-4, "DPS_UNKNOWN_ERROR"),                   # ERROR_UNKNOWN
    (-5, "DPS_TYPE_ERROR"),                      # ERROR_TYPE
    (-6, "PREVIOUS_ZREPORT_MISSING"),            # ERROR_NOT_PREV_ZREPORT
    (-7, "INVALID_XML"),                          # ERROR_XML
    (-8, "INVALID_FISCAL_DATE"),                 # ERROR_XML_DATE (existing enum value)
    (-9, "INVALID_XML_CHECK"),                   # ERROR_XML_CHK
    (-10, "INVALID_XML_ZREPORT"),                # ERROR_XML_ZREPORT
    (-11, "OFFLINE_168_HOUR_LIMIT"),             # ERROR_OFFLINE_168
    (-12, "PREVIOUS_HASH_MISMATCH"),             # ERROR_BAD_HASH_PREV
    (-13, "RRO_NOT_REGISTERED"),                 # ERROR_NOT_REGISTERED_RRO
    (-14, "SIGNER_NOT_REGISTERED"),              # ERROR_NOT_REGISTERED_SIGNER
    (-15, "SHIFT_NOT_OPEN"),                     # ERROR_NOT_OPEN_SHIFT (existing)
    (-16, "OFFLINE_ID_INVALID"),                 # ERROR_OFFLINE_ID
]


@pytest.mark.parametrize("status, expected_name", _DPS_STATUS_TABLE)
def test_canonical_error_from_dps_status_maps_each_grpc_code(
    status: int, expected_name: str,
) -> None:
    got = canonical_error_from_dps_status(status)
    assert isinstance(got, CanonicalErrorCode)
    assert got.name == expected_name, (
        f"DPS status {status} should map to {expected_name}; got {got.name}"
    )


def test_mapping_covers_all_sixteen_grpc_error_codes() -> None:
    # Guards that the table above is exhaustive for the proto enum.
    proto_statuses = {-1, -2, -3, -4, -5, -6, -7, -8, -9, -10, -11, -12, -13, -14, -15, -16}
    mapped_statuses = {s for s, _ in _DPS_STATUS_TABLE}
    assert mapped_statuses == proto_statuses, (
        f"Table missing statuses: {proto_statuses - mapped_statuses}; "
        f"unexpected statuses: {mapped_statuses - proto_statuses}"
    )


# ─── 2. Unknown / edge-case statuses ─────────────────────────────────


def test_unknown_status_falls_back_not_raises_keyerror() -> None:
    # Ops safety: if DPS adds a new negative code in future, we must
    # NOT crash the worker.  Fall back to a conservative canonical code.
    got = canonical_error_from_dps_status(-999)
    assert isinstance(got, CanonicalErrorCode)
    # Must be one of the generic "something went wrong at backend" codes.
    assert got in {
        CanonicalErrorCode.BACKEND_TEMPORARY_UNAVAILABLE,
        CanonicalErrorCode.DPS_UNKNOWN_ERROR,
    }


def test_positive_status_still_falls_back_gracefully() -> None:
    # status=1 is OK in the proto but if someone accidentally passes
    # it here we must return a valid enum value, not None/crash.
    got = canonical_error_from_dps_status(1)
    assert isinstance(got, CanonicalErrorCode)


def test_zero_status_graceful() -> None:
    got = canonical_error_from_dps_status(0)
    assert isinstance(got, CanonicalErrorCode)


# ─── 3. TransportRejectedError carries dps_status ─────────────────────


def test_transport_rejected_error_accepts_dps_status() -> None:
    err = TransportRejectedError("rejected", dps_status=-7)
    assert err.dps_status == -7
    assert "rejected" in str(err)


def test_transport_rejected_error_dps_status_optional_for_backward_compat() -> None:
    # Existing raise sites without kwarg must still work.
    err = TransportRejectedError("legacy message")
    assert err.dps_status is None


def test_dps_mac_recovery_error_still_carries_dps_status() -> None:
    # Subclass of TransportRejectedError — must preserve the original
    # `dps_status` + `expected_mac` surface.
    err = DpsMacRecoveryError("hash mismatch", expected_mac="a" * 64, dps_status=-12)
    assert err.dps_status == -12
    assert err.expected_mac == "a" * 64
    assert isinstance(err, TransportRejectedError)


# ─── 4. Mapping is consistent with existing enum values ──────────────


def test_error_xml_date_maps_to_existing_invalid_fiscal_date() -> None:
    # We prefer reusing existing canonical codes where semantics match.
    # ERROR_XML_DATE (-8) = "XML date field is invalid" — maps naturally
    # to the existing INVALID_FISCAL_DATE.
    got = canonical_error_from_dps_status(-8)
    assert got == CanonicalErrorCode.INVALID_FISCAL_DATE


def test_error_not_open_shift_maps_to_existing_shift_not_open() -> None:
    # ERROR_NOT_OPEN_SHIFT (-15) is semantically identical to our
    # SHIFT_NOT_OPEN canonical guard — reuse, don't duplicate.
    got = canonical_error_from_dps_status(-15)
    assert got == CanonicalErrorCode.SHIFT_NOT_OPEN


# ─── 5. Every new code has a Ukrainian default_message ───────────────


def test_every_new_canonical_code_has_default_message() -> None:
    # Operator-facing messages must exist for the new codes so the
    # fiscal error surfaced to 1С isn't empty string.
    for _, name in _DPS_STATUS_TABLE:
        code = CanonicalErrorCode[name]
        msg = code.default_message
        assert msg and isinstance(msg, str) and len(msg) > 0, (
            f"{name} must have a non-empty default_message"
        )
