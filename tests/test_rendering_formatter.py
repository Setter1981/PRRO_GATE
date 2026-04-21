"""Proof tests for the ASCII receipt formatter (Rendering Phase 1).

Strategy: one canonical golden output per operation type plus structural
assertions (width, QR payload, Cyrillic handling).  Golden outputs
mirror the WebCheck thermal-layout pattern (Consolas 42-char lines for
80мм paper) but are pure Python — identical on Windows/Linux/macOS.

Formatter is pure: input = RenderContext (typed); output =
ReceiptLines.  Downstream HTML/PDF/ESC-POS emitters just iterate the
lines.
"""
from __future__ import annotations

from datetime import datetime, timezone

import pytest

from prro_gateway.enums import OperationType
from prro_gateway.rendering.formatter import format_receipt
from prro_gateway.rendering.models import (
    ReceiptItem,
    ReceiptLines,
    ReceiptPayment,
    ReceiptTax,
    RenderContext,
)


_BASE_MERCHANT = {
    "fiscal_number": "3001234567",
    "fiscal_doc_no": "0000012345",
    "fiscal_date": datetime(2026, 4, 21, 10, 15, 30, tzinfo=timezone.utc),
    "local_doc_no": 42,
    "org_name": "ТОВ Приклад",
    "point_name": "Магазин №1",
    "point_addr": "м. Київ, вул. Хрещатик, 1",
    "tin": "1234567890",
    "cashier": "Іванова І.І.",
}


def _ctx(op: OperationType, **overrides) -> RenderContext:
    kwargs: dict = dict(_BASE_MERCHANT)
    kwargs["operation_type"] = op
    kwargs["total_sum_kopecks"] = overrides.pop("total_sum_kopecks", 0)
    kwargs.update(overrides)
    return RenderContext(**kwargs)


# ─── 1. Structural guarantees ────────────────────────────────────────


def test_output_is_receipt_lines_dataclass() -> None:
    out = format_receipt(_ctx(OperationType.SHIFT_OPEN))
    assert isinstance(out, ReceiptLines)
    assert out.width == 42


def test_all_lines_fit_configured_width() -> None:
    out = format_receipt(_ctx(OperationType.SHIFT_OPEN))
    for line in out.header + out.body + out.footer:
        assert len(line) <= out.width, (
            f"Line too wide: {line!r} ({len(line)} > {out.width})"
        )


def test_width_parameter_respected() -> None:
    out = format_receipt(_ctx(OperationType.SHIFT_OPEN), width=32)
    assert out.width == 32
    for line in out.header:
        assert len(line) <= 32


# ─── 2. SELL (SALE) golden output ────────────────────────────────────


def _sell_ctx() -> RenderContext:
    return _ctx(
        OperationType.SELL,
        total_sum_kopecks=12345,  # 123.45 UAH
        items=[
            ReceiptItem(
                name="Хліб пшеничний",
                quantity_thousandths=1000,  # 1 unit
                price_kopecks=2500,
                sum_kopecks=2500,
                tax_letter="А",
            ),
            ReceiptItem(
                name="Молоко 1л",
                quantity_thousandths=2000,  # 2 units
                price_kopecks=4922,
                sum_kopecks=9844,  # 2 × 49.22
                tax_letter="А",
            ),
        ],
        payments=[
            ReceiptPayment(kind="Готівка", amount_kopecks=12345),
        ],
        taxes=[
            ReceiptTax(letter="А", rate_pct=20.0, sum_kopecks=12345, tax_kopecks=2058),
        ],
    )


def test_sell_receipt_contains_merchant_header() -> None:
    out = format_receipt(_sell_ctx())
    header_text = "\n".join(out.header)
    assert "ТОВ Приклад" in header_text
    assert "Магазин №1" in header_text
    assert "1234567890" in header_text  # TIN
    assert "3001234567" in header_text  # FN


def test_sell_receipt_contains_all_items() -> None:
    out = format_receipt(_sell_ctx())
    body_text = "\n".join(out.body)
    assert "Хліб пшеничний" in body_text
    assert "Молоко 1л" in body_text


def test_sell_receipt_shows_item_quantity_price_sum() -> None:
    out = format_receipt(_sell_ctx())
    body_text = "\n".join(out.body)
    # 2 × 49.22 = 98.44 — sum must appear
    assert "98.44" in body_text
    assert "25.00" in body_text  # 2500 kopecks of bread


def test_sell_receipt_shows_total_in_uah() -> None:
    out = format_receipt(_sell_ctx())
    full = "\n".join(out.header + out.body + out.footer)
    assert "123.45" in full


def test_sell_receipt_shows_cash_payment() -> None:
    out = format_receipt(_sell_ctx())
    body_text = "\n".join(out.body)
    assert "Готівка" in body_text
    assert "123.45" in body_text


def test_sell_receipt_shows_tax_breakdown() -> None:
    out = format_receipt(_sell_ctx())
    body_text = "\n".join(out.body)
    # Tax letter, rate, and tax amount all appear.
    assert "А" in body_text
    assert "20" in body_text
    assert "20.58" in body_text  # 2058 kopecks


def test_sell_receipt_footer_has_fiscal_id_and_date() -> None:
    out = format_receipt(_sell_ctx())
    footer_text = "\n".join(out.footer)
    assert "0000012345" in footer_text
    assert "21.04.2026" in footer_text or "2026-04-21" in footer_text


def test_sell_receipt_has_qr_payload_with_dps_url() -> None:
    out = format_receipt(_sell_ctx())
    assert out.qr_payload is not None
    assert "cabinet.tax.gov.ua" in out.qr_payload
    assert "3001234567" in out.qr_payload  # FN
    assert "0000012345" in out.qr_payload  # fiscal doc no


def test_sell_receipt_has_fiscal_marker_line() -> None:
    # Per Ukrainian fiscal law, receipt MUST display "ФІСКАЛЬНИЙ ЧЕК"
    # (or equivalent wording) prominently.
    out = format_receipt(_sell_ctx())
    full = "\n".join(out.header + out.body + out.footer)
    assert "ФІСКАЛЬНИЙ" in full.upper()


# ─── 3. RETURN operation ─────────────────────────────────────────────


def test_return_receipt_marks_direction() -> None:
    out = format_receipt(_ctx(
        OperationType.RETURN,
        total_sum_kopecks=5000,
        items=[ReceiptItem(
            name="Повернення: Хліб",
            quantity_thousandths=1000,
            price_kopecks=2500,
            sum_kopecks=2500,
            tax_letter="А",
        )],
        payments=[ReceiptPayment(kind="Готівка", amount_kopecks=5000)],
    ))
    full = "\n".join(out.header + out.body + out.footer)
    # RETURN receipt MUST be clearly distinguishable from SELL —
    # Ukrainian compliance requires visible "ПОВЕРНЕННЯ" header.
    assert "ПОВЕРНЕННЯ" in full.upper()


def test_return_has_qr_payload_too() -> None:
    out = format_receipt(_ctx(
        OperationType.RETURN,
        total_sum_kopecks=5000,
        items=[ReceiptItem(
            name="Повернення",
            quantity_thousandths=1000,
            price_kopecks=2500,
            sum_kopecks=2500,
            tax_letter="А",
        )],
        payments=[ReceiptPayment(kind="Готівка", amount_kopecks=5000)],
    ))
    assert out.qr_payload is not None


# ─── 4. SHIFT_OPEN / SHIFT_CLOSE ─────────────────────────────────────


def test_shift_open_has_no_qr() -> None:
    out = format_receipt(_ctx(OperationType.SHIFT_OPEN, shift_no=5))
    # SHIFT_OPEN is not a consumer-facing fiscal sale → no QR needed.
    assert out.qr_payload is None


def test_shift_open_shows_shift_number() -> None:
    out = format_receipt(_ctx(OperationType.SHIFT_OPEN, shift_no=5))
    full = "\n".join(out.header + out.body + out.footer)
    assert "ВІДКРИТТЯ" in full.upper()
    assert "5" in full  # shift no


def test_shift_close_shows_shift_number() -> None:
    out = format_receipt(_ctx(OperationType.SHIFT_CLOSE, shift_no=5))
    full = "\n".join(out.header + out.body + out.footer)
    assert "ЗАКРИТТЯ" in full.upper()


# ─── 5. Z_REPORT ─────────────────────────────────────────────────────


def test_z_report_has_header_identifying_it() -> None:
    out = format_receipt(_ctx(
        OperationType.Z_REPORT,
        z_report_no=3,
        total_sum_kopecks=100_000,
    ))
    full = "\n".join(out.header + out.body + out.footer)
    assert "Z-ЗВІТ" in full.upper() or "ФІСКАЛЬНИЙ ЗВІТ" in full.upper()


def test_z_report_shows_z_number() -> None:
    out = format_receipt(_ctx(OperationType.Z_REPORT, z_report_no=3))
    full = "\n".join(out.header + out.body + out.footer)
    assert "3" in full


# ─── 6. SERVICE_IN / SERVICE_OUT ─────────────────────────────────────


def test_service_in_header() -> None:
    out = format_receipt(_ctx(
        OperationType.SERVICE_IN,
        total_sum_kopecks=50_000,
    ))
    full = "\n".join(out.header + out.body + out.footer)
    assert "СЛУЖБОВЕ ВНЕСЕННЯ" in full.upper()


def test_service_out_header() -> None:
    out = format_receipt(_ctx(
        OperationType.SERVICE_OUT,
        total_sum_kopecks=25_000,
    ))
    full = "\n".join(out.header + out.body + out.footer)
    assert "СЛУЖБОВА ВИДАЧА" in full.upper()


def test_service_receipt_shows_amount() -> None:
    out = format_receipt(_ctx(
        OperationType.SERVICE_IN,
        total_sum_kopecks=50_000,
    ))
    body_text = "\n".join(out.body)
    assert "500.00" in body_text


# ─── 7. CASH_WITHDRAWAL ──────────────────────────────────────────────


def test_cash_withdrawal_header() -> None:
    out = format_receipt(_ctx(
        OperationType.CASH_WITHDRAWAL,
        total_sum_kopecks=100_000,
        payments=[ReceiptPayment(kind="Картка", amount_kopecks=100_000)],
    ))
    full = "\n".join(out.header + out.body + out.footer)
    assert "ВИДАЧА ГОТІВКИ" in full.upper()


# ─── 8. Cyrillic / Unicode handling ──────────────────────────────────


def test_long_merchant_name_wraps_or_truncates_gracefully() -> None:
    ctx = _ctx(
        OperationType.SHIFT_OPEN,
        org_name="ПП " + "А" * 60,  # 63 chars
    )
    out = format_receipt(ctx)
    for line in out.header:
        assert len(line) <= out.width


def test_cyrillic_center_alignment_is_codepoint_based() -> None:
    # Regression: if center() used byte length for Cyrillic (2 bytes
    # per char in UTF-8) the alignment would be wrong.  We use
    # codepoint length.
    ctx = _ctx(OperationType.SHIFT_OPEN, org_name="ТОВ")
    out = format_receipt(ctx)
    # Find the line containing "ТОВ" — should be centered with spaces.
    org_line = next(l for l in out.header if "ТОВ" in l)
    leading = len(org_line) - len(org_line.lstrip())
    trailing = len(org_line) - len(org_line.rstrip())
    # Centered ± 1 (odd width padding).
    assert abs(leading - trailing) <= 1


# ─── 9. Defensive — missing fields ───────────────────────────────────


def test_missing_cashier_does_not_crash() -> None:
    ctx = _ctx(OperationType.SHIFT_OPEN, cashier="")
    out = format_receipt(ctx)
    assert isinstance(out, ReceiptLines)


def test_missing_items_for_sell_still_renders() -> None:
    ctx = _ctx(OperationType.SELL, total_sum_kopecks=0, items=[], payments=[])
    out = format_receipt(ctx)
    assert isinstance(out, ReceiptLines)


# ─── 10. Currency formatting precision ───────────────────────────────


@pytest.mark.parametrize(
    "kopecks, expected_str",
    [
        (0, "0.00"),
        (1, "0.01"),
        (100, "1.00"),
        (12345, "123.45"),
        (999_999_999, "9999999.99"),  # 10M UAH near upper bound
    ],
)
def test_kopecks_to_uah_string_format(kopecks: int, expected_str: str) -> None:
    from prro_gateway.rendering.format_utils import kopecks_to_uah
    assert kopecks_to_uah(kopecks) == expected_str


# ─── 11. format_utils primitives ─────────────────────────────────────


def test_center_helper() -> None:
    from prro_gateway.rendering.format_utils import center
    assert center("X", 5) == "  X  "
    assert center("АБ", 6) == "  АБ  "


def test_right_align_helper() -> None:
    from prro_gateway.rendering.format_utils import right_align
    assert right_align("X", 5) == "    X"


def test_two_column_helper() -> None:
    from prro_gateway.rendering.format_utils import two_column
    # "left" left-aligned, "right" right-aligned, total = width.
    result = two_column("Сума:", "123.45", 20)
    assert len(result) == 20
    assert result.startswith("Сума:")
    assert result.endswith("123.45")


def test_separator_helper() -> None:
    from prro_gateway.rendering.format_utils import separator
    assert separator("-", 8) == "--------"
    assert separator("=", 5) == "====="


def test_wrap_long_helper() -> None:
    from prro_gateway.rendering.format_utils import wrap_long
    wrapped = wrap_long("Дуже довга назва товару", 10)
    assert all(len(line) <= 10 for line in wrapped)
    # Reconstituted (joined with space) preserves original content.
    assert "Дуже" in wrapped[0]


# ─── 12. Injection / sanitization (review finding) ────────────────────


def test_sanitize_strips_newline_injection() -> None:
    from prro_gateway.rendering.format_utils import sanitize_line
    # Attacker tries to inject a fake receipt line via merchant name.
    out = sanitize_line("ACME\nFAKE ФН: 0000000001")
    assert "\n" not in out
    assert "FAKE" in out  # content preserved but collapsed to one line


def test_sanitize_strips_cr_tab_control_chars() -> None:
    from prro_gateway.rendering.format_utils import sanitize_line
    for bad in ("a\tb", "a\rb", "a\x00b", "a\x1fb", "a\x7fb", "a\u2028b"):
        out = sanitize_line(bad)
        assert "\t" not in out and "\r" not in out
        assert "\x00" not in out and "\u2028" not in out


def test_sell_receipt_with_malicious_merchant_name_collapses_to_one_logical_line() -> None:
    # Regression: org_name with embedded newline must NOT produce a
    # separate line break in the emitted `header`.  Every element of
    # `out.header` is one line (no '\n' inside) — so the injection
    # becomes inline text on the merchant row, not a standalone
    # forged fiscal-ID row.
    ctx = _ctx(
        OperationType.SHIFT_OPEN,
        org_name="ТОВ Приклад\nФН: 9999999999",
    )
    out = format_receipt(ctx)
    for line in out.header:
        assert "\n" not in line
    # Only ONE legit "ПРРО ФН:" line — injection does not duplicate it.
    prro_fn_lines = [l for l in out.header if "ПРРО ФН:" in l]
    assert len(prro_fn_lines) == 1
    assert "3001234567" in prro_fn_lines[0]


# ─── 13. QR payload gating on fiscal_doc_no ──────────────────────────


def test_sell_without_fiscal_doc_no_has_no_qr() -> None:
    # Offline pre-ACK window: document exists locally but no DPS
    # fiscal_doc_no yet.  QR would point to a broken URL.
    ctx = _ctx(
        OperationType.SELL,
        fiscal_doc_no="",
        total_sum_kopecks=100,
        items=[ReceiptItem(
            name="x", quantity_thousandths=1000,
            price_kopecks=100, sum_kopecks=100, tax_letter="А",
        )],
        payments=[ReceiptPayment(kind="Готівка", amount_kopecks=100)],
    )
    out = format_receipt(ctx)
    assert out.qr_payload is None


# ─── 14. Shift number on every fiscal document ───────────────────────


def test_sell_receipt_shows_shift_number() -> None:
    ctx = _sell_ctx()
    ctx.shift_no = 7
    out = format_receipt(ctx)
    full = "\n".join(out.header + out.body + out.footer)
    assert "Зміна №" in full
    assert "7" in full


def test_service_in_receipt_shows_shift_number() -> None:
    ctx = _ctx(
        OperationType.SERVICE_IN,
        total_sum_kopecks=50_000,
        shift_no=3,
    )
    out = format_receipt(ctx)
    full = "\n".join(out.header + out.body + out.footer)
    assert "Зміна №" in full


# ─── 15. Service/CashWithdrawal distinct fiscal markers ──────────────


def test_service_in_marker_not_confused_with_sale() -> None:
    out = format_receipt(_ctx(OperationType.SERVICE_IN, total_sum_kopecks=50_000))
    full = "\n".join(out.header + out.body + out.footer)
    assert "СЛУЖБОВИЙ" in full.upper()


def test_cash_withdrawal_marker_distinct() -> None:
    out = format_receipt(_ctx(
        OperationType.CASH_WITHDRAWAL,
        total_sum_kopecks=100_000,
        payments=[ReceiptPayment(kind="Картка", amount_kopecks=100_000)],
    ))
    full = "\n".join(out.header + out.body + out.footer)
    assert "ВИДАЧ" in full.upper()  # УКР: "ВИДАЧА" / "ВИДАЧІ"


# ─── 16. ЛНД label (Ukrainian audit convention) ──────────────────────


def test_local_doc_no_uses_lnd_label() -> None:
    out = format_receipt(_ctx(OperationType.SHIFT_OPEN))
    header_text = "\n".join(out.header)
    assert "ЛНД" in header_text
    assert "42" in header_text  # local_doc_no from _BASE_MERCHANT
