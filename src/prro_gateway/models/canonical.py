from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import Field, field_validator

from ..constants import SCHEMA_VERSION
from ..enums import AcquiringSource, DocumentState, OperationType, PaymentType, Protocol, ReceiptType
from .common import StrictModel


class Discount(StrictModel):
    type: str
    mode: str
    value: int
    name: str | None = None
    tax_code: str | None = None
    tax_codes: list[str] = Field(default_factory=list)
    privilege: str | None = None


class CanonicalReceiptItem(StrictModel):
    item_id: str
    item_no: int = Field(ge=1)
    code: str | None = None
    good_id: str | None = None
    name: str
    uktzed: str | None = None
    barcode: str | None = None
    excise_barcodes: list[str] = Field(default_factory=list)
    price: int = Field(ge=0)
    quantity: int = Field(ge=1, description="Quantity in thousandths")
    sum: int = Field(ge=0)
    header: str | None = None
    footer: str | None = None
    discounts: list[Discount] = Field(default_factory=list)
    item_attributes: dict[str, Any] | None = None
    is_return: bool | None = None


class DeliveryBlock(StrictModel):
    email: str | None = None
    emails: list[str] = Field(default_factory=list)
    phone: str | None = None


class ReceiptTotals(StrictModel):
    total_sum: int | None = Field(default=None, ge=0)
    round_sum: int | None = None
    discounts_sum: int | None = None
    extra_charge_sum: int | None = None


class CanonicalPayment(StrictModel):
    payment_id: str
    payment_type: PaymentType
    provider_type: str | None = None
    label: str | None = None
    payment_code: str | None = None
    amount: int = Field(ge=0, description="Always non-negative in canonical flow. Direction is defined by receipt_type.")
    commission: int | None = Field(default=None, ge=0)
    card_mask: str | None = None
    bank_name: str | None = None
    auth_code: str | None = None
    rrn: str | None = None
    payment_system: str | None = None
    owner_name: str | None = None
    terminal: str | None = None
    acquirer_and_seller: str | None = None
    receipt_no: str | None = None
    signature_required: bool | None = None
    tapxphone_terminal: str | None = None
    acquiring_source: AcquiringSource | None = None
    acquiring_payload_json: str | None = None


class TraceContext(StrictModel):
    source_ip: str | None = None
    source_port: int | None = None
    session_id: str | None = None
    correlation_id: str | None = None


class CanonicalFiscalCommand(StrictModel):
    schema_version: str = Field(default=SCHEMA_VERSION)
    request_id: str
    idempotency_key: str
    protocol: Protocol
    operation_type: OperationType
    fiscal_number: str
    route_key: str | None = None
    backend_profile_id: str | None = None
    transport_profile_id: str | None = None
    channel_owner: str | None = None
    external_request_id: str | None = None
    business_ts: datetime
    payload: dict[str, Any]
    payload_json: str | None = Field(default=None, description="Deprecated redundant serialized representation.")
    payload_sha256: str
    trace_context: TraceContext | dict[str, Any] | None = None
    requires_signature: bool | None = None
    requires_shift: bool | None = None
    requires_fiscal_number: bool | None = None
    requires_offline_code: bool | None = None
    correlation_id: str | None = None


class CanonicalReceipt(StrictModel):
    schema_version: str = Field(default=SCHEMA_VERSION)
    receipt_id: str
    receipt_type: ReceiptType
    fiscal_number: str
    shift_id: str | None = None
    status: DocumentState
    business_ts: datetime
    created_offline: bool = False
    offline_fiscal_code: int | None = None
    offline_fiscal_date: datetime | None = None
    fiscal_code: str | None = None
    fiscal_date: datetime | None = None
    serial: str | None = None
    control_number: str | None = None
    related_receipt_id: str | None = None
    previous_receipt_id: str | None = None
    technical_return: bool | None = None
    header: str | None = None
    footer: str | None = None
    barcode: str | None = None
    previous_hash: str | None = None
    delivery: DeliveryBlock | None = None
    rounding_enabled: bool | None = None
    channel_lock_ref: str | None = None
    goods: list[CanonicalReceiptItem]
    payments: list[CanonicalPayment]
    totals: ReceiptTotals | None = None
    formatted_print_lines: list[FormattedPrintLine] = Field(default_factory=list)

    @field_validator("goods")
    @classmethod
    def validate_goods_nonempty(cls, value: list[CanonicalReceiptItem]) -> list[CanonicalReceiptItem]:
        if not value:
            raise ValueError("goods must not be empty")
        return value

    @field_validator("payments")
    @classmethod
    def validate_payments_nonempty(cls, value: list[CanonicalPayment]) -> list[CanonicalPayment]:
        if not value:
            raise ValueError("payments must not be empty")
        return value


class FormattedPrintLine(StrictModel):
    line_type: str
    content: str
    alignment: str
    font_style: str
    max_width: int = Field(ge=20, le=80)


__all__ = [
    'Discount', 'CanonicalReceiptItem', 'DeliveryBlock', 'ReceiptTotals',
    'CanonicalPayment', 'TraceContext', 'CanonicalFiscalCommand', 'CanonicalReceipt', 'FormattedPrintLine'
]
