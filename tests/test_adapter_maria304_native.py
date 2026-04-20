"""Proof-level tests for Maria304NativeAdapter.

Written before implementation (TDD, M7-Py-1).  Fail as a block until
`src/prro_gateway/adapters/maria304_native.py` exists and the enum +
constants are wired.

The adapter consumes exactly the JSON shape that
`rust/maria304_driver/src/bridge/dto.rs::CanonicalCommand` emits:
top-level {schema_version, fiscal_number, command_type, idempotency_key,
cashier_id, department, return_check_number, payload}.  No separate
`context` block — the adapter builds `AdapterContext` from the Rust
fields directly.

All assertions are contract-level: they assert on shape / invariants /
enum round-trips, not on internals like private helpers.
"""
from __future__ import annotations

from typing import Any

import pytest

from prro_gateway.adapters.base import AdapterMappingError
from prro_gateway.constants import SCHEMA_VERSION
from prro_gateway.enums import OperationType, Protocol

# The adapter / enum value are introduced by the M7-Py-1 impl.  Importing
# at top-level guarantees a clean ImportError while the implementation
# is absent, so the entire file fails fast.
from prro_gateway.adapters.maria304_native import Maria304NativeAdapter


ADAPTER = Maria304NativeAdapter()


def _payload(**overrides: Any) -> dict[str, Any]:
    base: dict[str, Any] = {
        "direction": "SALE",
        "goods": [],
        "payments": [],
        "dual_tax_mode": None,
        "totals": {"sale_kopecks": 0, "return_kopecks": 0},
        "raw_frames": [],
    }
    base.update(overrides)
    return base


def _cmd(**overrides: Any) -> dict[str, Any]:
    base: dict[str, Any] = {
        "schema_version": "1.0",
        "fiscal_number": "3001234567",
        "command_type": "SELL",
        "idempotency_key": "maria304:3001234567:sess-uuid:1",
        "cashier_id": "csh1",
        "department": "1",
        "return_check_number": None,
        "payload": _payload(),
    }
    base.update(overrides)
    return base


# Representative FiscalLine with every optional field populated — the
# Rust DTO carries all of these (see dto.rs::FiscalLine lines 98-115).
def _rich_good() -> dict[str, Any]:
    return {
        "name": "Паляниця",
        "uktzed": "1905901000",
        "quantity_milli": 2000,
        "price_kopecks": 2500,
        "tax_group_1": 1,
        "tax_group_2": 2,
        "article_code": 42,
        "discount": {
            "direction": "DISCOUNT",
            "name": "Акція",
            "amount_kopecks": 300,
        },
        "excise_stamps": ["UA-STAMP-001", "UA-STAMP-002"],
        "barcode": "4820001234567",
    }


def _payment_with_slip() -> dict[str, Any]:
    return {
        "type": "CASHLESS_1",
        "amount_kopecks": 4700,
        "acquirer_slip": {
            "payment_form_index": 1,
            "merchant_id": "MRCH001",
            "terminal_id": "T42",
            "operation_type": "SALE",
            "pan": "************1234",
            "approval_code": "APR123",
            "payment_system": "MASTERCARD",
            "transaction_code": "TXN9999",
            "fee_kopecks": 50,
            "cashier_signature_placeholder": False,
            "cardholder_signature_placeholder": False,
        },
    }


# ─── 1. Protocol + OperationType mapping ─────────────────────────────


def test_protocol_is_maria_304_native() -> None:
    cmd = ADAPTER.map_command(_cmd())
    assert cmd.protocol == Protocol.MARIA_304_NATIVE


@pytest.mark.parametrize(
    "rust_cmd_type, expected_op",
    [
        ("SELL", OperationType.SELL),
        ("RETURN", OperationType.RETURN),
        ("SHIFT_OPEN", OperationType.SHIFT_OPEN),
        ("SHIFT_CLOSE", OperationType.SHIFT_CLOSE),
        ("X_REPORT", OperationType.X_REPORT),
        ("Z_REPORT", OperationType.Z_REPORT),
        ("SERVICE_IN", OperationType.SERVICE_IN),
        ("SERVICE_OUT", OperationType.SERVICE_OUT),
    ],
)
def test_command_type_maps_to_operation_type(
    rust_cmd_type: str, expected_op: OperationType,
) -> None:
    raw = _cmd(command_type=rust_cmd_type)
    # SERVICE_IN/SERVICE_OUT require a matching CAIO raw_frame for the
    # sum parser — inject one.
    if rust_cmd_type in {"SERVICE_IN", "SERVICE_OUT"}:
        opcode = "CAIOI" if rust_cmd_type == "SERVICE_IN" else "CAIOO"
        raw["payload"] = _payload(raw_frames=[{"opcode": opcode, "body": "0000000001"}])
    cmd = ADAPTER.map_command(raw)
    assert cmd.operation_type == expected_op


def test_periodic_report_is_rejected_as_unsupported() -> None:
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(_cmd(command_type="PERIODIC_REPORT"))
    assert exc.value.code == "UNSUPPORTED_METHOD"


def test_unknown_command_type_is_rejected() -> None:
    with pytest.raises(AdapterMappingError):
        ADAPTER.map_command(_cmd(command_type="ZZZZZZ"))


# ─── 2. Idempotency / external_request_id contract ───────────────────


def test_rust_idempotency_key_flows_into_external_request_id() -> None:
    cmd = ADAPTER.map_command(
        _cmd(idempotency_key="maria304:3001234567:abcd-ef:5"),
    )
    assert cmd.external_request_id == "maria304:3001234567:abcd-ef:5"


def test_canonical_idempotency_key_uses_rust_key_not_payload_hash() -> None:
    # Two identical POSTs replaying the same idempotency_key must build
    # the same canonical idempotency_key regardless of payload entropy.
    base = _cmd(idempotency_key="maria304:3001234567:sess-X:42")
    a = ADAPTER.map_command(base)
    b = ADAPTER.map_command(base)
    assert a.idempotency_key == b.idempotency_key
    assert "sess-X:42" in a.idempotency_key

    # Different payload, same key → still same canonical idempotency_key
    # (idempotency is keyed on Rust-supplied string, not sha256 of body).
    other = _cmd(
        idempotency_key="maria304:3001234567:sess-X:42",
        payload=_payload(totals={"sale_kopecks": 999, "return_kopecks": 0}),
    )
    c = ADAPTER.map_command(other)
    assert c.idempotency_key == a.idempotency_key


def test_distinct_idempotency_keys_produce_distinct_canonical_keys() -> None:
    a = ADAPTER.map_command(_cmd(idempotency_key="maria304:F:s:1"))
    b = ADAPTER.map_command(_cmd(idempotency_key="maria304:F:s:2"))
    assert a.idempotency_key != b.idempotency_key


# ─── 3. Schema version + fiscal_number ───────────────────────────────


def test_envelope_schema_version_is_canonical_not_rust_protocol_version() -> None:
    # Rust sends schema_version="1.0" (Maria protocol version).  The
    # canonical envelope must be stamped with SCHEMA_VERSION (the
    # gateway's canonical schema).  These are intentionally different.
    cmd = ADAPTER.map_command(_cmd(schema_version="1.0"))
    assert cmd.schema_version == SCHEMA_VERSION


def test_fiscal_number_is_propagated() -> None:
    cmd = ADAPTER.map_command(_cmd(fiscal_number="4000000077"))
    assert cmd.fiscal_number == "4000000077"


# ─── 4. Payload preservation ─────────────────────────────────────────


def test_raw_frames_are_preserved_verbatim_under_receipt() -> None:
    frames = [
        {"opcode": "PREP", "body": "1"},
        {"opcode": "FISC", "body": "Paliannica 1 10000 A"},
        {"opcode": "PSDt", "body": "c 10000"},
        {"opcode": "COMP", "body": "csh1 sum 10000"},
    ]
    cmd = ADAPTER.map_command(
        _cmd(payload=_payload(raw_frames=frames)),
    )
    assert cmd.payload["receipt"]["raw_frames"] == frames


def test_totals_are_preserved() -> None:
    cmd = ADAPTER.map_command(
        _cmd(payload=_payload(totals={"sale_kopecks": 12345, "return_kopecks": 0})),
    )
    assert cmd.payload["receipt"]["totals"] == {
        "sale_kopecks": 12345,
        "return_kopecks": 0,
    }


def test_direction_is_preserved() -> None:
    cmd_sale = ADAPTER.map_command(
        _cmd(payload=_payload(direction="SALE")),
    )
    assert cmd_sale.payload["receipt"]["direction"] == "SALE"

    cmd_return = ADAPTER.map_command(
        _cmd(
            command_type="RETURN",
            payload=_payload(direction="RETURN"),
        ),
    )
    assert cmd_return.payload["receipt"]["direction"] == "RETURN"


def test_dual_tax_mode_none_passes_through() -> None:
    cmd = ADAPTER.map_command(_cmd(payload=_payload(dual_tax_mode=None)))
    assert cmd.payload["receipt"]["dual_tax_mode"] is None


def test_dual_tax_mode_populated_passes_through() -> None:
    cmd = ADAPTER.map_command(
        _cmd(
            payload=_payload(dual_tax_mode={"tax_group_1": 1, "tax_group_2": 2}),
        ),
    )
    assert cmd.payload["receipt"]["dual_tax_mode"] == {
        "tax_group_1": 1,
        "tax_group_2": 2,
    }


def test_goods_and_payments_empty_is_accepted() -> None:
    # M4 Rust envelope has empty goods/payments; raw_frames carry the
    # real data.  The adapter must not fail on empty lists.
    cmd = ADAPTER.map_command(_cmd(payload=_payload(goods=[], payments=[])))
    assert cmd.payload["receipt"]["goods"] == []
    assert cmd.payload["receipt"]["payments"] == []


def test_goods_and_payments_populated_passes_through() -> None:
    goods = [{"name": "X", "quantity_milli": 1000, "price_kopecks": 100, "tax_group_1": 1, "tax_group_2": 0}]
    payments = [{"type": "CASH", "amount_kopecks": 100}]
    cmd = ADAPTER.map_command(
        _cmd(payload=_payload(goods=goods, payments=payments)),
    )
    assert cmd.payload["receipt"]["goods"] == goods
    assert cmd.payload["receipt"]["payments"] == payments


# ─── 5. Top-level Rust fields surface in payload ─────────────────────


def test_cashier_id_lands_under_payload() -> None:
    cmd = ADAPTER.map_command(_cmd(cashier_id="csh-42"))
    assert cmd.payload["cashier_id"] == "csh-42"


def test_missing_cashier_id_does_not_fail() -> None:
    cmd = ADAPTER.map_command(_cmd(cashier_id=None))
    assert cmd.payload["cashier_id"] is None


def test_department_lands_under_payload() -> None:
    cmd = ADAPTER.map_command(_cmd(department="Bar"))
    assert cmd.payload["department"] == "Bar"


def test_return_check_number_lands_under_payload() -> None:
    cmd = ADAPTER.map_command(
        _cmd(command_type="RETURN", return_check_number="orig-42"),
    )
    assert cmd.payload["return_check_number"] == "orig-42"


# ─── 6. Requires-* flags + channel_owner ─────────────────────────────


def test_channel_owner_marks_maria304_driver() -> None:
    cmd = ADAPTER.map_command(_cmd())
    assert cmd.channel_owner == "maria304-driver"


@pytest.mark.parametrize(
    "rust_cmd_type, requires_shift_expected",
    [
        ("SHIFT_OPEN", False),
        ("SHIFT_CLOSE", True),
        ("SELL", True),
        ("RETURN", True),
        ("SERVICE_IN", True),
        ("SERVICE_OUT", True),
        ("X_REPORT", True),
        ("Z_REPORT", True),
    ],
)
def test_requires_shift_flag_matches_operation_semantics(
    rust_cmd_type: str, requires_shift_expected: bool,
) -> None:
    raw = _cmd(command_type=rust_cmd_type)
    if rust_cmd_type in {"SERVICE_IN", "SERVICE_OUT"}:
        opcode = "CAIOI" if rust_cmd_type == "SERVICE_IN" else "CAIOO"
        raw["payload"] = _payload(raw_frames=[{"opcode": opcode, "body": "0000000001"}])
    cmd = ADAPTER.map_command(raw)
    assert cmd.requires_shift is requires_shift_expected


# ─── 7. Payload hash is stable and non-empty ─────────────────────────


def test_payload_sha256_is_64_hex_chars() -> None:
    cmd = ADAPTER.map_command(_cmd())
    assert len(cmd.payload_sha256) == 64
    int(cmd.payload_sha256, 16)  # must be valid hex


def test_payload_sha256_changes_when_receipt_body_changes() -> None:
    a = ADAPTER.map_command(
        _cmd(payload=_payload(totals={"sale_kopecks": 100, "return_kopecks": 0})),
    )
    b = ADAPTER.map_command(
        _cmd(payload=_payload(totals={"sale_kopecks": 200, "return_kopecks": 0})),
    )
    assert a.payload_sha256 != b.payload_sha256


# ─── 8. Business timestamp ───────────────────────────────────────────


def test_business_ts_is_tz_aware_utc() -> None:
    cmd = ADAPTER.map_command(_cmd())
    assert cmd.business_ts.tzinfo is not None
    assert cmd.business_ts.utcoffset().total_seconds() == 0


# ─── 9. Malformed input defensive paths ──────────────────────────────


@pytest.mark.parametrize("field", ["fiscal_number", "idempotency_key", "command_type", "payload"])
def test_missing_required_field_raises_adapter_mapping_error(field: str) -> None:
    raw = _cmd()
    del raw[field]
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


@pytest.mark.parametrize(
    "field, bad_value",
    [
        ("fiscal_number", ""),
        ("fiscal_number", None),
        ("fiscal_number", 12345),
        ("idempotency_key", ""),
        ("idempotency_key", None),
        ("command_type", ""),
        ("command_type", None),
    ],
)
def test_malformed_required_field_type_raises_validation_error(
    field: str, bad_value: Any,
) -> None:
    raw = _cmd()
    raw[field] = bad_value
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_non_dict_raw_request_is_rejected() -> None:
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(["not", "an", "object"])  # type: ignore[arg-type]
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_payload_as_list_is_rejected() -> None:
    raw = _cmd(payload=["not a dict"])
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


# ─── 10. Rich Rust DTO field preservation ────────────────────────────


def test_fiscal_line_full_optional_set_preserved_verbatim() -> None:
    good = _rich_good()
    cmd = ADAPTER.map_command(_cmd(payload=_payload(goods=[good])))
    assert cmd.payload["receipt"]["goods"] == [good]


def test_acquirer_slip_is_preserved_on_payment() -> None:
    pay = _payment_with_slip()
    cmd = ADAPTER.map_command(_cmd(payload=_payload(payments=[pay])))
    assert cmd.payload["receipt"]["payments"] == [pay]


def test_cyrillic_names_survive_utf8_roundtrip() -> None:
    good = _rich_good()  # "Паляниця", "Акція"
    cmd = ADAPTER.map_command(
        _cmd(
            cashier_id="касир-Іван",
            department="Відділ№1",
            payload=_payload(goods=[good]),
        ),
    )
    assert cmd.payload["cashier_id"] == "касир-Іван"
    assert cmd.payload["department"] == "Відділ№1"
    assert cmd.payload["receipt"]["goods"][0]["name"] == "Паляниця"
    assert cmd.payload["receipt"]["goods"][0]["discount"]["name"] == "Акція"


def test_multiple_raw_frames_preserve_ordering_and_content() -> None:
    frames = [{"opcode": f"OP{i:02d}", "body": f"body-{i}"} for i in range(25)]
    cmd = ADAPTER.map_command(_cmd(payload=_payload(raw_frames=frames)))
    assert cmd.payload["receipt"]["raw_frames"] == frames


# ─── 11. Idempotency / stability edge cases ──────────────────────────


def test_same_suffix_different_fiscal_number_distinct_canonical_keys() -> None:
    # Two receipts with the same "session:seq" suffix but different FNs
    # must not collide — the canonical key carries fiscal_number.
    a = ADAPTER.map_command(
        _cmd(
            fiscal_number="AAA1111111",
            idempotency_key="maria304:AAA1111111:sess:1",
        ),
    )
    b = ADAPTER.map_command(
        _cmd(
            fiscal_number="BBB2222222",
            idempotency_key="maria304:BBB2222222:sess:1",
        ),
    )
    assert a.idempotency_key != b.idempotency_key
    assert a.fiscal_number != b.fiscal_number


def test_payload_sha256_is_stable_across_identical_calls() -> None:
    # Regression: a refactor that seeds sha256 with a timestamp would
    # break idempotency at the payload level.
    raw = _cmd(payload=_payload(totals={"sale_kopecks": 555, "return_kopecks": 0}))
    a = ADAPTER.map_command(raw)
    b = ADAPTER.map_command(raw)
    assert a.payload_sha256 == b.payload_sha256


def test_request_id_is_fresh_per_call_not_reused_idempotency_key() -> None:
    # Review finding B4: request_id must be fresh per HTTP attempt so
    # the inbox UNIQUE constraint on request_id does not collide on
    # replay.  Two map_command calls return different request_ids
    # even when idempotency_key is identical.
    raw = _cmd(idempotency_key="maria304:F:sess:1")
    a = ADAPTER.map_command(raw)
    b = ADAPTER.map_command(raw)
    assert a.request_id != b.request_id
    # But the canonical idempotency key IS identical — the inbox uses
    # this to collapse replays.
    assert a.idempotency_key == b.idempotency_key


# ─── 12. Idempotency-key shape validation ────────────────────────────


@pytest.mark.parametrize(
    "bad_key",
    [
        "",
        "not-maria-prefix",
        "maria304:",                    # empty suffix
        "maria304:fn with space",
        "maria304:fn\nnewline",
        "maria304:fn\x00nul",
        "other304:fn:sess:1",           # wrong prefix
        "maria304:" + "x" * 300,        # too long
    ],
)
def test_idempotency_key_shape_is_enforced(bad_key: str) -> None:
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(_cmd(idempotency_key=bad_key))
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_idempotency_key_with_safe_chars_is_accepted() -> None:
    # Positive: the Rust-generated shape (prefix:fn:uuid-with-dashes:seq[:opcode])
    # matches.
    ADAPTER.map_command(_cmd(idempotency_key="maria304:FN_01:abcd-1234-ef56-7890:42"))
    ADAPTER.map_command(_cmd(idempotency_key="maria304:FN_01:sess:42:ZREP"))


# ─── 13. Memory-exhaustion caps ──────────────────────────────────────


def test_raw_frames_beyond_cap_raises_validation_error() -> None:
    oversize = [{"opcode": "OP", "body": "x"}] * 10_000
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(_cmd(payload=_payload(raw_frames=oversize)))
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_oversize_raw_frame_body_raises_validation_error() -> None:
    frames = [{"opcode": "OP", "body": "x" * 1_000_000}]
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(_cmd(payload=_payload(raw_frames=frames)))
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_goods_beyond_cap_raises_validation_error() -> None:
    goods = [{"name": "x"}] * 1_000
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(_cmd(payload=_payload(goods=goods)))
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_payments_beyond_cap_raises_validation_error() -> None:
    pays = [{"type": "CASH", "amount_kopecks": 1}] * 64
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(_cmd(payload=_payload(payments=pays)))
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_raw_frames_non_list_rejected() -> None:
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(_cmd(payload=_payload(raw_frames="not a list")))
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


# ─── 14. Second-review hardening ─────────────────────────────────────


def test_non_string_raw_frame_body_rejected() -> None:
    frames = [{"opcode": "OP", "body": 42}]
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(_cmd(payload=_payload(raw_frames=frames)))
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_non_string_raw_frame_opcode_rejected() -> None:
    frames = [{"opcode": None, "body": "x"}]
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(_cmd(payload=_payload(raw_frames=frames)))
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_request_id_is_unique_under_rapid_sequential_calls() -> None:
    # Review finding N1: guard against microsecond collision when two
    # map_command calls land in the same µs (possible on WSL/Windows
    # where time.time() has coarser resolution).
    raw = _cmd(idempotency_key="maria304:F:sess:burst")
    seen: set[str] = set()
    for _ in range(500):
        cmd = ADAPTER.map_command(raw)
        assert cmd.request_id not in seen
        seen.add(cmd.request_id)
    assert len(seen) == 500


def test_empty_totals_dict_is_preserved_as_empty_dict() -> None:
    # Review finding N3: {} must not be silently coerced with None.
    cmd = ADAPTER.map_command(_cmd(payload=_payload(totals={})))
    assert cmd.payload["receipt"]["totals"] == {}


# ─── 15. SERVICE_IN / SERVICE_OUT sum parsing from CAIO raw_frames ────
#
# The Rust driver's wire opcodes `CAIOI` and `CAIOO` carry the sum as a
# 10-char ASCII-digit prefix in the frame body: `D10` = zero-padded
# kopecks.  The adapter must parse this out and inject
# payload["service_sum"] so `validate_service_receipt` passes.


def _service_cmd(opcode: str, sum_kopecks: int, desc: str = "") -> dict[str, Any]:
    operation = "SERVICE_IN" if opcode == "CAIOI" else "SERVICE_OUT"
    body = f"{sum_kopecks:010d}{desc}"
    return _cmd(
        command_type=operation,
        idempotency_key=f"maria304:FN-DEV-0001:sess:{opcode}-{sum_kopecks}",
        payload=_payload(raw_frames=[{"opcode": opcode, "body": body}]),
    )


def test_service_in_parses_sum_from_caioi_body() -> None:
    cmd = ADAPTER.map_command(_service_cmd("CAIOI", 50_000, "Каса ранок"))
    assert cmd.operation_type == OperationType.SERVICE_IN
    assert cmd.payload["service_sum"] == 50_000


def test_service_out_parses_sum_from_caioo_body() -> None:
    cmd = ADAPTER.map_command(_service_cmd("CAIOO", 12_345, "Інкасація"))
    assert cmd.operation_type == OperationType.SERVICE_OUT
    assert cmd.payload["service_sum"] == 12_345


def test_service_sum_preserves_leading_zeros_from_wire() -> None:
    # Wire body "0000050000" must parse as 50000, not crash on leading
    # zeros.
    cmd = ADAPTER.map_command(_service_cmd("CAIOI", 1))
    assert cmd.payload["service_sum"] == 1


def test_service_with_cyrillic_description_parses_sum_correctly() -> None:
    # The 10-char sum prefix must be read by character index, not byte
    # index — Cyrillic multibyte descriptions follow without breaking
    # the parser.
    cmd = ADAPTER.map_command(_service_cmd("CAIOI", 999, "підкасовий залишок"))
    assert cmd.payload["service_sum"] == 999


def test_service_without_raw_frames_raises_validation_error() -> None:
    raw = _cmd(command_type="SERVICE_IN",
               payload=_payload(raw_frames=[]))
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_service_with_non_caio_opcode_raises() -> None:
    raw = _cmd(command_type="SERVICE_IN",
               payload=_payload(raw_frames=[{"opcode": "FOO", "body": "0000050000"}]))
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_service_with_short_body_raises() -> None:
    raw = _cmd(command_type="SERVICE_IN",
               payload=_payload(raw_frames=[{"opcode": "CAIOI", "body": "12345"}]))
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_service_with_non_numeric_sum_raises() -> None:
    raw = _cmd(command_type="SERVICE_IN",
               payload=_payload(raw_frames=[{"opcode": "CAIOI", "body": "XXXXXXXXXXdesc"}]))
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_service_in_out_must_not_be_treated_as_sell() -> None:
    # Negative control: a SELL command must not get service_sum.
    cmd = ADAPTER.map_command(_cmd(command_type="SELL"))
    assert "service_sum" not in cmd.payload


def test_z_report_does_not_get_service_sum_even_with_caioi_frame() -> None:
    # Regression guard: the enrichment must key off operation_type,
    # not on opcode presence.  A Z_REPORT that happens to have a stray
    # CAIOI frame in raw_frames must NOT be treated as SERVICE_IN.
    raw = _cmd(
        command_type="Z_REPORT",
        idempotency_key="maria304:FN-DEV-0001:sess:z:1",
        payload=_payload(raw_frames=[{"opcode": "CAIOI", "body": "0000010000stray"}]),
    )
    cmd = ADAPTER.map_command(raw)
    assert "service_sum" not in cmd.payload


def test_service_description_is_preserved_in_receipt() -> None:
    cmd = ADAPTER.map_command(_service_cmd("CAIOI", 100, "Ранкова зміна"))
    assert cmd.payload["receipt"]["service_description"] == "Ранкова зміна"


def test_service_description_is_empty_string_when_body_is_exactly_10_chars() -> None:
    # Edge: body has just the sum, no description — parser must not
    # crash and must store empty string.
    raw = _cmd(
        command_type="SERVICE_IN",
        idempotency_key="maria304:F:sess:caioi-no-desc",
        payload=_payload(raw_frames=[{"opcode": "CAIOI", "body": "0000000100"}]),
    )
    cmd = ADAPTER.map_command(raw)
    assert cmd.payload["service_sum"] == 100
    assert cmd.payload["receipt"]["service_description"] == ""


def test_service_in_with_caioo_frame_is_rejected() -> None:
    # Operation/opcode mismatch guard: SERVICE_IN must have CAIOI, not
    # CAIOO.
    raw = _cmd(
        command_type="SERVICE_IN",
        idempotency_key="maria304:F:sess:mismatch",
        payload=_payload(raw_frames=[{"opcode": "CAIOO", "body": "0000000100desc"}]),
    )
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_service_with_nine_digits_and_letter_is_rejected() -> None:
    # Boundary: '000000000A' — 9 digits + 1 ASCII letter.  Must be
    # rejected (isdigit returns False for mixed).
    raw = _cmd(
        command_type="SERVICE_IN",
        idempotency_key="maria304:F:sess:mixed-digit",
        payload=_payload(raw_frames=[{"opcode": "CAIOI", "body": "000000000A"}]),
    )
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_service_with_unicode_digit_is_rejected() -> None:
    # Review finding R1: Arabic-Indic digits satisfy .isdigit() but
    # desync from the wire's ASCII representation.  Adapter must
    # reject these.
    raw = _cmd(
        command_type="SERVICE_IN",
        idempotency_key="maria304:F:sess:unicode-digit",
        payload=_payload(raw_frames=[{"opcode": "CAIOI", "body": "٠٠٠٠٠٠٠٠٠١"}]),
    )
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_service_max_sum_ten_nines_is_accepted() -> None:
    # Upper bound of the 10-digit space: 9_999_999_999 kopecks = 99.99M UAH.
    cmd = ADAPTER.map_command(_service_cmd("CAIOI", 9_999_999_999))
    assert cmd.payload["service_sum"] == 9_999_999_999


def test_service_parses_correctly_with_leading_zeros_preserving_decimal_value() -> None:
    # Guard the "octal interpretation" regression: "0000000010" must
    # parse as decimal 10, not octal 8.
    raw = _cmd(
        command_type="SERVICE_IN",
        idempotency_key="maria304:F:sess:octal-guard",
        payload=_payload(raw_frames=[{"opcode": "CAIOI", "body": "0000000010"}]),
    )
    cmd = ADAPTER.map_command(raw)
    assert cmd.payload["service_sum"] == 10  # NOT 8


def test_service_with_multiple_frames_takes_only_first() -> None:
    # Documented behaviour: only raw_frames[0] is parsed.  Any trailing
    # frames are preserved but do not influence the sum.
    raw = _cmd(
        command_type="SERVICE_IN",
        idempotency_key="maria304:F:sess:multi-frame",
        payload=_payload(raw_frames=[
            {"opcode": "CAIOI", "body": "0000000555desc"},
            {"opcode": "CAIOI", "body": "0000099999junk"},
        ]),
    )
    cmd = ADAPTER.map_command(raw)
    assert cmd.payload["service_sum"] == 555
    assert len(cmd.payload["receipt"]["raw_frames"]) == 2


# ─── 16. CASH_WITHDRAWAL — CSHG 19-parameter body parser ─────────────
#
# Wire format (per Maria 304 protocol §54):
#
#   CSHG<p1:9 sum><p2:9 fee>
#       <p3:2 len_acq><p4:len_acq merchant_id>
#       <p5:2 len_term><p6:len_term terminal_id>
#       <p7:2 len_pan><p8:len_pan pan>
#       <p9:2 len_ps><p10:len_ps payment_system>
#       <p11:2 len_auth><p12:len_auth auth_code>
#       <p13:2 len_rrn><p14:len_rrn rrn>
#       <p15:1 cashier_sig><p16:1 cardholder_sig>
#       <p17:1 qr_mode><p18:2 qr_scale><p19:1 qr_level>
#
# Example from real 1C trace:
# CSHG 000100000 000001500 07 5968236 09 789456123 16 XXXXXXXXXXXX1339
#      14 Плат.термiнал 06 569878 08 15963258 1 1 a 05 L


def _cshg_body(
    *, sum_kopecks: int = 100_000, fee: int = 1500,
    merchant_id: str = "5968236", terminal_id: str = "789456123",
    pan: str = "XXXXXXXXXXXX1339", payment_system: str = "Плат.термiнал",
    auth_code: str = "569878", rrn: str = "15963258",
    cashier_sig: bool = True, cardholder_sig: bool = True,
    qr_mode: str = "a", qr_scale: str = "05", qr_level: str = "L",
) -> str:
    def _len_prefix(s: str) -> str:
        return f"{len(s):02d}{s}"
    return (
        f"{sum_kopecks:09d}{fee:09d}"
        f"{_len_prefix(merchant_id)}"
        f"{_len_prefix(terminal_id)}"
        f"{_len_prefix(pan)}"
        f"{_len_prefix(payment_system)}"
        f"{_len_prefix(auth_code)}"
        f"{_len_prefix(rrn)}"
        f"{'1' if cashier_sig else '0'}"
        f"{'1' if cardholder_sig else '0'}"
        f"{qr_mode}{qr_scale}{qr_level}"
    )


def _cshg_cmd(body: str) -> dict[str, Any]:
    return _cmd(
        command_type="CASH_WITHDRAWAL",
        idempotency_key="maria304:FN-DEV-0001:sess:cshg-1",
        payload=_payload(raw_frames=[
            {"opcode": "PREP", "body": "1"},
            {"opcode": "CSHG", "body": body},
            {"opcode": "COMP", "body": ""},
        ]),
    )


def test_cshg_full_body_parses_all_19_params() -> None:
    body = _cshg_body()
    cmd = ADAPTER.map_command(_cshg_cmd(body))
    assert cmd.operation_type == OperationType.CASH_WITHDRAWAL
    assert cmd.payload["cash_withdrawal_sum"] == 100_000
    payments = cmd.payload["receipt"]["payments"]
    assert len(payments) == 1
    pay = payments[0]
    # The only payment is a single CASHLESS slip with acquirer details.
    assert pay["amount_kopecks"] == 100_000
    assert pay["commission"] == 1500
    assert pay["acquirer_and_seller"] == "5968236"
    assert pay["terminal"] == "789456123"
    assert pay["card_mask"] == "XXXXXXXXXXXX1339"
    assert pay["payment_system"] == "Плат.термiнал"
    assert pay["auth_code"] == "569878"
    assert pay["rrn"] == "15963258"


def test_cshg_cyrillic_payment_system_preserved() -> None:
    body = _cshg_body(payment_system="Приват Банк")
    cmd = ADAPTER.map_command(_cshg_cmd(body))
    assert cmd.payload["receipt"]["payments"][0]["payment_system"] == "Приват Банк"


def test_cshg_totals_equal_sum() -> None:
    # validate_cash_withdrawal_receipt requires totals.total_sum ==
    # cash_withdrawal_sum when totals present.
    body = _cshg_body(sum_kopecks=50_000, fee=0)
    cmd = ADAPTER.map_command(_cshg_cmd(body))
    assert cmd.payload["receipt"]["totals"]["total_sum"] == 50_000


def test_cshg_signature_flags_produce_payment_signature_required() -> None:
    # Cashier or cardholder signature → signature_required=True
    cmd_both = ADAPTER.map_command(
        _cshg_cmd(_cshg_body(cashier_sig=True, cardholder_sig=True)),
    )
    assert cmd_both.payload["receipt"]["payments"][0]["signature_required"] is True

    cmd_none = ADAPTER.map_command(
        _cshg_cmd(_cshg_body(cashier_sig=False, cardholder_sig=False)),
    )
    assert cmd_none.payload["receipt"]["payments"][0]["signature_required"] is False


def test_cshg_command_type_detection_by_frame_opcode() -> None:
    # Negative control: SELL with FISC frame (no CSHG) must NOT become
    # CASH_WITHDRAWAL.  The command_type on the wire is what the Rust
    # driver puts in.  Python adapter trusts that but must parse CSHG
    # frame if command_type=CASH_WITHDRAWAL.
    raw = _cmd(
        command_type="SELL",
        payload=_payload(raw_frames=[{"opcode": "FISC", "body": "Паляниця 1 100 А"}]),
    )
    cmd = ADAPTER.map_command(raw)
    assert cmd.operation_type == OperationType.SELL
    assert "cash_withdrawal_sum" not in cmd.payload


def test_cshg_missing_frame_raises() -> None:
    raw = _cmd(
        command_type="CASH_WITHDRAWAL",
        idempotency_key="maria304:F:sess:no-cshg",
        payload=_payload(raw_frames=[{"opcode": "PREP", "body": "1"}]),
    )
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_cshg_short_body_raises() -> None:
    # Body truncated mid-params
    raw = _cmd(
        command_type="CASH_WITHDRAWAL",
        idempotency_key="maria304:F:sess:short-cshg",
        payload=_payload(raw_frames=[{"opcode": "CSHG", "body": "00010000000000150007"}]),
    )
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_cshg_non_numeric_sum_raises() -> None:
    body = "XXXXXXXXX" + "000001500" + _cshg_body()[18:]
    raw = _cmd(
        command_type="CASH_WITHDRAWAL",
        idempotency_key="maria304:F:sess:cshg-bad-sum",
        payload=_payload(raw_frames=[{"opcode": "CSHG", "body": body}]),
    )
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_cshg_length_field_overflows_remaining_body_raises() -> None:
    # p3="99" would claim 99 chars for merchant_id; body cannot supply.
    body = "000100000000001500" + "99" + "too-short"
    raw = _cmd(
        command_type="CASH_WITHDRAWAL",
        idempotency_key="maria304:F:sess:cshg-overflow",
        payload=_payload(raw_frames=[{"opcode": "CSHG", "body": body}]),
    )
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_cshg_with_unicode_digit_in_sum_raises() -> None:
    # Same ASCII-digit hardening as CAIO parser.
    body = "٠٠٠١٠٠٠٠٠" + _cshg_body()[9:]
    raw = _cmd(
        command_type="CASH_WITHDRAWAL",
        idempotency_key="maria304:F:sess:cshg-unicode",
        payload=_payload(raw_frames=[{"opcode": "CSHG", "body": body}]),
    )
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_cshg_preserves_leading_zeros_decimal_not_octal() -> None:
    body = _cshg_body(sum_kopecks=10)  # "000000010" — must parse to 10, not octal 8
    cmd = ADAPTER.map_command(_cshg_cmd(body))
    assert cmd.payload["cash_withdrawal_sum"] == 10


def test_cshg_zero_sum_is_rejected() -> None:
    # B1: adapter must reject sum=0 before it bypasses the downstream
    # cash-balance guard.
    body = _cshg_body(sum_kopecks=0)
    raw = _cshg_cmd(body)
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_cshg_fee_surfaces_at_receipt_level_for_audit() -> None:
    # B2: acquirer fee must be auditable, not just as payment.commission.
    cmd = ADAPTER.map_command(_cshg_cmd(_cshg_body(sum_kopecks=10_000, fee=150)))
    assert cmd.payload["receipt"]["cshg_fee_kopecks"] == 150


def test_cshg_qr_params_preserved_round_trip() -> None:
    body = _cshg_body(qr_mode="b", qr_scale="08", qr_level="Q")
    cmd = ADAPTER.map_command(_cshg_cmd(body))
    qr = cmd.payload["receipt"]["cshg_qr"]
    assert qr == {"mode": "b", "scale": "08", "level": "Q"}


def test_cshg_trailing_bytes_rejected() -> None:
    # R3: §54 is fixed-schema; extra trailing chars after QR level are
    # a framer bug or malicious padding.
    body = _cshg_body() + "garbage"
    raw = _cshg_cmd(body)
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_cshg_9_digit_max_sum_accepted() -> None:
    # Upper bound of 9-digit field: 999_999_999 kopecks (~10M UAH).
    body = _cshg_body(sum_kopecks=999_999_999, fee=0)
    cmd = ADAPTER.map_command(_cshg_cmd(body))
    assert cmd.payload["cash_withdrawal_sum"] == 999_999_999


@pytest.mark.parametrize(
    "field, value, expected_pass",
    [
        ("merchant_id", "A", True),              # len=01, min
        ("merchant_id", "X" * 64, True),         # len=64, max per spec
        ("merchant_id", "X" * 65, False),        # exceeds max
        ("terminal_id", "T", True),              # len=01
        ("terminal_id", "T" * 32, True),         # len=32, max
        ("terminal_id", "T" * 33, False),        # exceeds 32
        ("pan", "X" * 32, True),                 # len=32 boundary
        ("pan", "X" * 33, False),
        ("auth_code", "A" * 32, True),
        ("auth_code", "A" * 33, False),
    ],
)
def test_cshg_field_length_caps(
    field: str, value: str, expected_pass: bool,
) -> None:
    body = _cshg_body(**{field: value})
    raw = _cshg_cmd(body)
    if expected_pass:
        cmd = ADAPTER.map_command(raw)
        payment = cmd.payload["receipt"]["payments"][0]
        field_to_key = {
            "merchant_id": "acquirer_and_seller",
            "terminal_id": "terminal",
            "pan": "card_mask",
            "payment_system": "payment_system",
            "auth_code": "auth_code",
            "rrn": "rrn",
        }
        assert payment[field_to_key[field]] == value
    else:
        with pytest.raises(AdapterMappingError) as exc:
            ADAPTER.map_command(raw)
        assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_cshg_invalid_signature_flag_rejected() -> None:
    body = _cshg_body()
    # Replace cashier_sig char ('1') at its known offset with '2'.
    # It's at offset = 18 + 2+7 + 2+9 + 2+16 + 2+14 + 2+6 + 2+8 = 90
    # for default params.  Use string substitution to be robust.
    corrupted = body.replace("11a", "21a", 1)
    raw = _cshg_cmd(corrupted)
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"


def test_cshg_multi_frame_takes_first_cshg() -> None:
    # Documented behaviour: if two CSHG frames appear, the first wins.
    # This is consistent with raw_frames ordering — the wire protocol
    # would never emit two anyway, but we pin the deterministic choice.
    first = _cshg_body(sum_kopecks=100)
    second = _cshg_body(sum_kopecks=99_999)
    raw = _cmd(
        command_type="CASH_WITHDRAWAL",
        idempotency_key="maria304:F:sess:two-cshg",
        payload=_payload(raw_frames=[
            {"opcode": "CSHG", "body": first},
            {"opcode": "CSHG", "body": second},
        ]),
    )
    cmd = ADAPTER.map_command(raw)
    assert cmd.payload["cash_withdrawal_sum"] == 100


def test_cshg_empty_pan_is_accepted() -> None:
    # Per spec п7 ≥ 1, but we need to handle len="00" defensively.
    # Expected: adapter rejects len==0 (spec says 1..32 range).
    body = "000100000" + "000001500" + "00" + "..." # len_pan=0
    # Adjust: construct a body where one field has len=00.
    # sum(9)+fee(9)+len_acq(2)+acq(1)+len_term(2)+term(1)+len_pan(2=00)...
    body = (
        "000100000" + "000001500"
        + "01" + "A"        # merchant "A"
        + "01" + "B"        # terminal "B"
        + "00"              # len_pan = 0 → spec violation
        + "01" + "X"        # payment_system
        + "01" + "a"        # auth_code
        + "01" + "r"        # rrn
        + "00aLa05L"
    )
    raw = _cmd(
        command_type="CASH_WITHDRAWAL",
        idempotency_key="maria304:F:sess:cshg-empty-pan",
        payload=_payload(raw_frames=[{"opcode": "CSHG", "body": body}]),
    )
    with pytest.raises(AdapterMappingError) as exc:
        ADAPTER.map_command(raw)
    assert exc.value.code == "PAYLOAD_VALIDATION_FAILED"
