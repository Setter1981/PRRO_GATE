"""Pure-Python receipt formatter.

Input: `RenderContext` (typed, fully-populated by caller).
Output: `ReceiptLines` with header/body/footer slices plus optional
`qr_payload` for emitters to render a QR image beneath the footer.

Layout mirrors the WebCheck thermal approach:
* Header — merchant identity + point + FN + doc no.
* Body  — operation-specific content (items, totals, tax breakdown,
          payment summary).
* Footer — fiscal ID, date/time, "ФІСКАЛЬНИЙ ЧЕК" marker.

Line width defaults to 42 (80мм thermal, Consolas 8pt).  Pass
`width=32` for narrower 58мм printers.
"""
from __future__ import annotations

from datetime import timezone
from typing import Callable
from urllib.parse import urlencode

from ..enums import OperationType
from .format_utils import (
    center,
    kopecks_to_uah,
    right_align,
    sanitize_line,
    separator,
    two_column,
    wrap_long,
)
from .models import (
    ReceiptItem,
    ReceiptLines,
    ReceiptPayment,
    ReceiptTax,
    RenderContext,
)

_KYIV_OFFSET_SECONDS = 3 * 3600  # UTC+3 summer; formatter only displays, not MAC

_DPS_QR_BASE = "https://cabinet.tax.gov.ua/cabinet/faces/checkxml"


# ─── Public entry point ──────────────────────────────────────────────


def format_receipt(ctx: RenderContext, *, width: int = 42) -> ReceiptLines:
    header = _format_header(ctx, width)
    body = _BODY_BUILDERS[ctx.operation_type](ctx, width)
    footer = _format_footer(ctx, width)
    qr_payload = _qr_payload(ctx) if _needs_qr(ctx) else None
    return ReceiptLines(header=header, body=body, footer=footer,
                        qr_payload=qr_payload, width=width)


# ─── Header / footer ─────────────────────────────────────────────────


def _format_header(ctx: RenderContext, width: int) -> list[str]:
    # All user-sourced fields sanitized (strips control chars that
    # could otherwise inject forged lines into the fiscal receipt).
    org = sanitize_line(ctx.org_name)
    point_nm = sanitize_line(ctx.point_name)
    point_ad = sanitize_line(ctx.point_addr)
    cashier = sanitize_line(ctx.cashier)

    lines: list[str] = []
    if org:
        lines.extend(center(line, width) for line in wrap_long(org, width))
    if ctx.tin:
        lines.append(center(f"ТІН: {ctx.tin}", width))
    if point_nm:
        lines.extend(center(line, width) for line in wrap_long(point_nm, width))
    if point_ad:
        lines.extend(center(line, width) for line in wrap_long(point_ad, width))
    if ctx.fiscal_number:
        lines.append(center(f"ПРРО ФН: {ctx.fiscal_number}", width))
    lines.append(separator("=", width))
    lines.append(_op_title_line(ctx, width))
    lines.append(separator("=", width))
    lines.append(two_column(f"ЛНД № {ctx.local_doc_no}",
                             _kyiv_datetime_str(ctx), width))
    if ctx.shift_no is not None:
        lines.append(two_column("Зміна №:", str(ctx.shift_no), width))
    if cashier:
        lines.append(two_column("Касир:", cashier, width))
    return lines


def _format_footer(ctx: RenderContext, width: int) -> list[str]:
    lines: list[str] = [separator("-", width)]
    if ctx.fiscal_doc_no:
        lines.append(two_column("Фіск. номер:", ctx.fiscal_doc_no, width))
    lines.append(two_column("Дата:", _kyiv_datetime_str(ctx), width))
    # "ФІСКАЛЬНИЙ ЧЕК" is required by UA fiscal law on SELL/RETURN;
    # also printed on all other ops for consistency with reference.
    lines.append(center(_op_fiscal_marker(ctx), width))
    return lines


def _op_title_line(ctx: RenderContext, width: int) -> str:
    return center(_OPERATION_TITLES.get(ctx.operation_type, ""), width)


def _op_fiscal_marker(ctx: RenderContext) -> str:
    return _OPERATION_MARKERS.get(ctx.operation_type, "ФІСКАЛЬНИЙ ЧЕК")


# ─── Body builders per operation ─────────────────────────────────────


def _body_sale_or_return(ctx: RenderContext, width: int) -> list[str]:
    lines: list[str] = []
    # Items (goods)
    for item in ctx.items:
        lines.extend(_format_item_lines(item, width))
    if ctx.items:
        lines.append(separator("-", width))
    # Total
    lines.append(two_column("РАЗОМ:",
                             f"{kopecks_to_uah(ctx.total_sum_kopecks)} {ctx.currency}",
                             width))
    lines.append(separator("-", width))
    # Payments
    for pay in ctx.payments:
        lines.append(two_column(sanitize_line(pay.kind),
                                 f"{kopecks_to_uah(pay.amount_kopecks)} {ctx.currency}",
                                 width))
    # Tax breakdown
    if ctx.taxes:
        lines.append(separator("-", width))
        for tax in ctx.taxes:
            lines.extend(_format_tax_lines(tax, width))
    return lines


def _body_shift_open(ctx: RenderContext, width: int) -> list[str]:
    lines: list[str] = []
    if ctx.shift_no is not None:
        lines.append(two_column("Зміна №:", str(ctx.shift_no), width))
    lines.append(center("— зміну відкрито —", width))
    return lines


def _body_shift_close(ctx: RenderContext, width: int) -> list[str]:
    lines: list[str] = []
    if ctx.shift_no is not None:
        lines.append(two_column("Зміна №:", str(ctx.shift_no), width))
    lines.append(center("— зміну закрито —", width))
    return lines


def _body_z_report(ctx: RenderContext, width: int) -> list[str]:
    lines: list[str] = []
    if ctx.z_report_no is not None:
        lines.append(two_column("Z-звіт №:", str(ctx.z_report_no), width))
    lines.append(two_column("Оборот:",
                             f"{kopecks_to_uah(ctx.total_sum_kopecks)} {ctx.currency}",
                             width))
    if ctx.taxes:
        lines.append(separator("-", width))
        for tax in ctx.taxes:
            lines.extend(_format_tax_lines(tax, width))
    return lines


def _body_service_in(ctx: RenderContext, width: int) -> list[str]:
    return [
        two_column("Сума внесення:",
                   f"{kopecks_to_uah(ctx.total_sum_kopecks)} {ctx.currency}",
                   width),
    ]


def _body_service_out(ctx: RenderContext, width: int) -> list[str]:
    return [
        two_column("Сума видачі:",
                   f"{kopecks_to_uah(ctx.total_sum_kopecks)} {ctx.currency}",
                   width),
    ]


def _body_cash_withdrawal(ctx: RenderContext, width: int) -> list[str]:
    lines: list[str] = [
        two_column("Сума видачі:",
                   f"{kopecks_to_uah(ctx.total_sum_kopecks)} {ctx.currency}",
                   width),
    ]
    for pay in ctx.payments:
        lines.append(two_column(sanitize_line(pay.kind),
                                 f"{kopecks_to_uah(pay.amount_kopecks)} {ctx.currency}",
                                 width))
    return lines


_BODY_BUILDERS: dict[OperationType, Callable[[RenderContext, int], list[str]]] = {
    OperationType.SELL:            _body_sale_or_return,
    OperationType.RETURN:          _body_sale_or_return,
    OperationType.SHIFT_OPEN:      _body_shift_open,
    OperationType.SHIFT_CLOSE:     _body_shift_close,
    OperationType.Z_REPORT:        _body_z_report,
    OperationType.SERVICE_IN:      _body_service_in,
    OperationType.SERVICE_OUT:     _body_service_out,
    OperationType.CASH_WITHDRAWAL: _body_cash_withdrawal,
}


# ─── Item / tax helpers ──────────────────────────────────────────────


def _format_item_lines(item: ReceiptItem, width: int) -> list[str]:
    """Render one item over 1–3 lines:
        <name>                          (wrapped if long)
        <qty> x <price> [<tax_letter>]       <sum>
    """
    out: list[str] = []
    out.extend(wrap_long(sanitize_line(item.name), width))
    qty = item.quantity_thousandths / 1000.0
    qty_str = f"{qty:g}"
    left = f"{qty_str} x {kopecks_to_uah(item.price_kopecks)}"
    if item.tax_letter:
        left += f" [{item.tax_letter}]"
    right = kopecks_to_uah(item.sum_kopecks)
    out.append(two_column(left, right, width))
    if item.discount_kopecks:
        out.append(two_column("  Знижка:",
                               kopecks_to_uah(-item.discount_kopecks), width))
    return out


def _format_tax_lines(tax: ReceiptTax, width: int) -> list[str]:
    rate_str = f"{tax.rate_pct:g}%"
    left = f"{tax.letter} {rate_str}"
    return [
        two_column(f"{left}  оборот:", kopecks_to_uah(tax.sum_kopecks), width),
        two_column(f"{left}  податок:", kopecks_to_uah(tax.tax_kopecks), width),
    ]


# ─── QR payload ──────────────────────────────────────────────────────


def _needs_qr(ctx: RenderContext) -> bool:
    # QR opens the DPS cabinet page for this fiscal document.  If
    # `fiscal_doc_no` isn't populated yet (offline pre-ACK window),
    # the QR URL resolves to a DPS error page — skip emission and
    # let the receipt show a fallback "QR буде доступний після
    # синхронізації з ДПС" if needed.
    if not ctx.fiscal_doc_no:
        return False
    return ctx.operation_type in {
        OperationType.SELL,
        OperationType.RETURN,
        OperationType.SERVICE_IN,
        OperationType.SERVICE_OUT,
        OperationType.CASH_WITHDRAWAL,
        OperationType.Z_REPORT,
    }


def _qr_payload(ctx: RenderContext) -> str:
    """Build the DPS cabinet URL encoded into the QR image.

    Scanning reveals the verified fiscal document on
    `cabinet.tax.gov.ua` — part of Ukrainian fiscal law compliance.
    """
    local = _to_kyiv(ctx.fiscal_date)
    params = {
        "fn": ctx.fiscal_number,
        "dt": local.strftime("%Y%m%d"),
        "tm": local.strftime("%H%M%S"),
        "fd": ctx.fiscal_doc_no or "",
        "sm": str(ctx.total_sum_kopecks),
    }
    return f"{_DPS_QR_BASE}?{urlencode(params)}"


# ─── Timestamp helpers ──────────────────────────────────────────────


def _kyiv_datetime_str(ctx: RenderContext) -> str:
    local = _to_kyiv(ctx.fiscal_date)
    return local.strftime("%d.%m.%Y %H:%M:%S")


def _to_kyiv(utc_dt):
    """Display-only Kyiv local time via IANA tzdata.

    Matches Python `zoneinfo('Europe/Kyiv')` which DST-transitions
    correctly (UTC+2 winter / UTC+3 summer).  Previously fell back
    to fixed UTC+3 if zoneinfo was missing — that produced a
    wrong-hour timestamp in winter (Oct–Mar) on Windows hosts
    without tzdata, and the QR payload built from the same helper
    would then disagree with what DPS stored.  We now raise
    explicitly so operators install `tzdata` instead of silently
    shipping wrong receipts.
    """
    try:
        from zoneinfo import ZoneInfo
        return utc_dt.astimezone(ZoneInfo("Europe/Kyiv"))
    except Exception as exc:
        raise RuntimeError(
            "Europe/Kyiv zoneinfo is unavailable — install tzdata "
            "(`pip install tzdata` or ensure IANA zoneinfo in container)"
        ) from exc


# ─── Titles and markers ──────────────────────────────────────────────


_OPERATION_TITLES: dict[OperationType, str] = {
    OperationType.SELL:            "ФІСКАЛЬНИЙ ЧЕК",
    OperationType.RETURN:          "ЧЕК ПОВЕРНЕННЯ",
    OperationType.SHIFT_OPEN:      "ВІДКРИТТЯ ЗМІНИ",
    OperationType.SHIFT_CLOSE:     "ЗАКРИТТЯ ЗМІНИ",
    OperationType.Z_REPORT:        "Z-ЗВІТ",
    OperationType.SERVICE_IN:      "СЛУЖБОВЕ ВНЕСЕННЯ",
    OperationType.SERVICE_OUT:     "СЛУЖБОВА ВИДАЧА",
    OperationType.CASH_WITHDRAWAL: "ВИДАЧА ГОТІВКИ",
}

_OPERATION_MARKERS: dict[OperationType, str] = {
    OperationType.SELL:            "ФІСКАЛЬНИЙ ЧЕК",
    OperationType.RETURN:          "ФІСКАЛЬНИЙ ЧЕК ПОВЕРНЕННЯ",
    OperationType.SHIFT_OPEN:      "НЕФІСКАЛЬНИЙ ДОКУМЕНТ",
    OperationType.SHIFT_CLOSE:     "НЕФІСКАЛЬНИЙ ДОКУМЕНТ",
    OperationType.Z_REPORT:        "ФІСКАЛЬНИЙ ЗВІТ",
    # Per UA fiscal statute, SERVICE_IN/OUT and CASH_WITHDRAWAL are
    # NOT "sales" receipts — labeling them "ФІСКАЛЬНИЙ ЧЕК" misleads
    # auditors.  Use operation-specific markers.
    OperationType.SERVICE_IN:      "ФІСКАЛЬНИЙ СЛУЖБОВИЙ ЧЕК",
    OperationType.SERVICE_OUT:     "ФІСКАЛЬНИЙ СЛУЖБОВИЙ ЧЕК",
    OperationType.CASH_WITHDRAWAL: "ФІСКАЛЬНИЙ ЧЕК ВИДАЧІ ГОТІВКИ",
}


__all__ = ["format_receipt"]
