"""Data classes for the receipt rendering pipeline.

`RenderContext` is the formatter input: everything needed to render a
receipt regardless of upstream protocol.  Populated by the caller from
`FiscalDocumentRecord` + related tables (shifts, operators, fn_config).

`ReceiptLines` is the formatter output: pre-formatted fixed-width ASCII
lines ready for any emitter (HTML, PDF, ESC-POS).
"""
from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime

from ..enums import OperationType


@dataclass
class ReceiptItem:
    """One line of goods / services on the receipt.

    Quantities are in thousandths to preserve precision (1000 = 1
    unit).  Prices/sums are in kopecks (integer).
    """
    name: str
    quantity_thousandths: int
    price_kopecks: int
    sum_kopecks: int
    uktzed: str | None = None
    tax_letter: str = ""          # "А"/"Б"/"В"/... per Ukrainian TX groups
    discount_kopecks: int = 0


@dataclass
class ReceiptPayment:
    kind: str                     # "Готівка" / "Картка" / "Безготівковий"
    amount_kopecks: int


@dataclass
class ReceiptTax:
    letter: str                   # "А"/"Б"/"В"/...
    rate_pct: float               # 20.0 for ПДВ 20%
    sum_kopecks: int              # turnover subject to this tax
    tax_kopecks: int              # tax amount


@dataclass
class RenderContext:
    """Full set of fields the formatter needs.

    Populated by the caller.  Optional fields default to sensible
    empties so the formatter never crashes on sparse data (audit /
    replay scenarios with missing columns).
    """
    operation_type: OperationType
    fiscal_number: str                        # 10-digit FN
    fiscal_doc_no: str                        # DPS server_fiscal_no
    fiscal_date: datetime                     # UTC, formatter converts to Kyiv
    local_doc_no: int                         # LND
    org_name: str = ""
    point_name: str = ""
    point_addr: str = ""
    tin: str = ""                             # ЄДРПОУ / TIN
    cashier: str = ""
    total_sum_kopecks: int = 0
    items: list[ReceiptItem] = field(default_factory=list)
    payments: list[ReceiptPayment] = field(default_factory=list)
    taxes: list[ReceiptTax] = field(default_factory=list)
    shift_no: int | None = None
    z_report_no: int | None = None
    currency: str = "UAH"


@dataclass
class ReceiptLines:
    """Formatter output — ready to feed any emitter.

    `header`/`body`/`footer` split helps emitters place QR / logo in
    the right slot without re-parsing.  Lines are guaranteed to be
    ≤ `width` codepoints long.
    """
    header: list[str]
    body: list[str]
    footer: list[str]
    qr_payload: str | None         # URL for DPS QR (None for non-fiscal ops)
    width: int = 42                # 80мм thermal default


__all__ = [
    "ReceiptItem",
    "ReceiptPayment",
    "ReceiptTax",
    "RenderContext",
    "ReceiptLines",
]
