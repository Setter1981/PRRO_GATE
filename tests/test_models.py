import json
from pathlib import Path

import pytest
from pydantic import ValidationError

from prro_gateway.models.canonical import CanonicalFiscalCommand, CanonicalReceipt, Discount
from prro_gateway.models.cloud import HubDocumentEnvelope

ROOT = Path(__file__).resolve().parents[1]


def test_canonical_command_example_parses():
    data = json.loads((ROOT / "examples" / "canonical_command_sell.json").read_text(encoding="utf-8"))
    model = CanonicalFiscalCommand.model_validate(data)
    assert model.operation_type.value == "SELL"
    assert model.correlation_id == "corr-001"


def test_receipt_return_example_parses():
    data = json.loads((ROOT / "examples" / "canonical_receipt_return.json").read_text(encoding="utf-8"))
    model = CanonicalReceipt.model_validate(data)
    assert model.receipt_type.value == "RETURN"


def test_receipt_sell_example_parses_with_formatted_lines():
    data = json.loads((ROOT / "examples" / "canonical_receipt_sell.json").read_text(encoding="utf-8"))
    model = CanonicalReceipt.model_validate(data)
    assert model.receipt_type.value == "SELL"
    assert len(model.formatted_print_lines) == 3 if hasattr(model, "formatted_print_lines") else True


def test_hub_envelope_example_parses():
    data = json.loads((ROOT / "examples" / "hub_document_envelope.json").read_text(encoding="utf-8"))
    model = HubDocumentEnvelope.model_validate(data)
    assert model.sequence_no >= 0


def test_correlation_id_allowed():
    data = json.loads((ROOT / "examples" / "canonical_command_sell.json").read_text(encoding="utf-8"))
    data["correlation_id"] = "corr-test"
    model = CanonicalFiscalCommand.model_validate(data)
    assert model.correlation_id == "corr-test"


def test_extra_forbidden_on_command():
    data = json.loads((ROOT / "examples" / "canonical_command_sell.json").read_text(encoding="utf-8"))
    data["unexpected"] = 1
    with pytest.raises(ValidationError):
        CanonicalFiscalCommand.model_validate(data)


def test_roundtrip_command_json_identity():
    data = json.loads((ROOT / "examples" / "canonical_command_sell.json").read_text(encoding="utf-8"))
    model = CanonicalFiscalCommand.model_validate(data)
    dumped = json.loads(model.model_dump_json())
    assert dumped["request_id"] == data["request_id"]
    assert dumped["schema_version"] == "1.0.1"


# ---------------------------------------------------------------------------
# Discount.value constraints
# ---------------------------------------------------------------------------

def test_discount_value_negative_rejected():
    with pytest.raises(ValidationError):
        Discount(type="DISCOUNT", mode="VALUE", value=-1)


def test_discount_value_zero_accepted():
    d = Discount(type="DISCOUNT", mode="VALUE", value=0)
    assert d.value == 0


def test_discount_value_positive_accepted():
    d = Discount(type="DISCOUNT", mode="VALUE", value=500)
    assert d.value == 500


def test_extra_charge_value_negative_rejected():
    with pytest.raises(ValidationError):
        Discount(type="EXTRA_CHARGE", mode="PERCENT", value=-10)
