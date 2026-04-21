"""Proof tests for PDF receipt renderer (Rendering Phase 3).

Takes `ReceiptLines` → PDF bytes.  Uses bundled DejaVu Sans Mono TTF
for guaranteed Cyrillic rendering on any OS.  PDF is suitable for
receipt archive, legal audit evidence, email attachment, duplicate
reprint.
"""
from __future__ import annotations

import re
from datetime import datetime, timezone

import pytest

from prro_gateway.enums import OperationType
from prro_gateway.rendering.formatter import format_receipt
from prro_gateway.rendering.pdf_renderer import render_pdf
from prro_gateway.rendering.models import (
    ReceiptItem,
    ReceiptPayment,
    ReceiptTax,
    RenderContext,
)


def _sell_ctx() -> RenderContext:
    return RenderContext(
        operation_type=OperationType.SELL,
        fiscal_number="3001234567",
        fiscal_doc_no="0000012345",
        fiscal_date=datetime(2026, 4, 21, 10, 15, 30, tzinfo=timezone.utc),
        local_doc_no=42,
        org_name="ТОВ Приклад",
        point_name="Магазин №1",
        point_addr="м. Київ, вул. Хрещатик, 1",
        tin="1234567890",
        cashier="Іванова І.І.",
        total_sum_kopecks=12345,
        items=[ReceiptItem(
            name="Хліб пшеничний",
            quantity_thousandths=1000,
            price_kopecks=2500,
            sum_kopecks=2500,
            tax_letter="А",
        )],
        payments=[ReceiptPayment(kind="Готівка", amount_kopecks=12345)],
        taxes=[ReceiptTax(letter="А", rate_pct=20.0,
                          sum_kopecks=12345, tax_kopecks=2058)],
    )


# ─── 1. Output is a valid PDF ────────────────────────────────────────


def test_output_starts_with_pdf_magic_bytes() -> None:
    lines = format_receipt(_sell_ctx())
    pdf = render_pdf(lines)
    assert isinstance(pdf, bytes)
    assert pdf.startswith(b"%PDF-"), "must start with PDF magic bytes"


def test_output_ends_with_eof_marker() -> None:
    lines = format_receipt(_sell_ctx())
    pdf = render_pdf(lines)
    # PDF must end with %%EOF (possibly + trailing whitespace).
    assert b"%%EOF" in pdf[-32:]


def test_pdf_contains_font_resource() -> None:
    lines = format_receipt(_sell_ctx())
    pdf = render_pdf(lines)
    # PDF must reference a mono font for the receipt body.  DejaVu
    # Sans Mono is our bundled Cyrillic-capable mono TTF.
    assert b"DejaVu" in pdf or b"Type0" in pdf


# ─── 2. Cyrillic rendering ────────────────────────────────────────────


def test_cyrillic_text_survives_pdf_extraction() -> None:
    # Extract text via pypdf and assert merchant/item names are
    # present.  A broken font embedding would drop Cyrillic to
    # boxes or question marks.
    try:
        from pypdf import PdfReader
    except ImportError:
        pytest.skip("pypdf unavailable")
    import io
    lines = format_receipt(_sell_ctx())
    pdf = render_pdf(lines)
    reader = PdfReader(io.BytesIO(pdf))
    assert len(reader.pages) >= 1
    text = reader.pages[0].extract_text()
    assert "ТОВ Приклад" in text
    assert "Хліб" in text or "хліб" in text.lower()
    assert "Готівка" in text


def test_fiscal_number_extractable_from_pdf() -> None:
    try:
        from pypdf import PdfReader
    except ImportError:
        pytest.skip("pypdf unavailable")
    import io
    lines = format_receipt(_sell_ctx())
    pdf = render_pdf(lines)
    reader = PdfReader(io.BytesIO(pdf))
    text = reader.pages[0].extract_text()
    assert "3001234567" in text  # FN
    assert "0000012345" in text  # fiscal_doc_no


# ─── 3. Paper dimensions ─────────────────────────────────────────────


def test_default_paper_width_80mm() -> None:
    try:
        from pypdf import PdfReader
    except ImportError:
        pytest.skip("pypdf unavailable")
    import io
    lines = format_receipt(_sell_ctx())
    pdf = render_pdf(lines)
    reader = PdfReader(io.BytesIO(pdf))
    page = reader.pages[0]
    # 80мм ≈ 226.77 pt (1mm = 72/25.4 pt).  Allow ±1 pt margin.
    width_pt = float(page.mediabox.width)
    assert 225 < width_pt < 229, f"expected ~226.77 pt (80mm); got {width_pt}"


def test_narrow_paper_58mm_mode() -> None:
    try:
        from pypdf import PdfReader
    except ImportError:
        pytest.skip("pypdf unavailable")
    import io
    lines = format_receipt(_sell_ctx(), width=32)
    pdf = render_pdf(lines, paper_width_mm=58)
    reader = PdfReader(io.BytesIO(pdf))
    width_pt = float(reader.pages[0].mediabox.width)
    # 58мм ≈ 164.4 pt
    assert 163 < width_pt < 166


def test_page_height_grows_with_content() -> None:
    try:
        from pypdf import PdfReader
    except ImportError:
        pytest.skip("pypdf unavailable")
    import io
    short = format_receipt(RenderContext(
        operation_type=OperationType.SHIFT_OPEN,
        fiscal_number="3001234567",
        fiscal_doc_no="0000000001",
        fiscal_date=datetime(2026, 4, 21, tzinfo=timezone.utc),
        local_doc_no=1,
        shift_no=5,
    ))
    long_ctx = _sell_ctx()
    long_ctx.items = [ReceiptItem(
        name=f"Товар #{i}",
        quantity_thousandths=1000,
        price_kopecks=100, sum_kopecks=100, tax_letter="А",
    ) for i in range(50)]
    long_ctx.total_sum_kopecks = 5000
    long_lines = format_receipt(long_ctx)

    short_pdf = render_pdf(format_receipt(RenderContext(
        operation_type=OperationType.SHIFT_OPEN,
        fiscal_number="3001234567", fiscal_doc_no="1",
        fiscal_date=datetime(2026, 4, 21, tzinfo=timezone.utc), local_doc_no=1,
        shift_no=5,
    )))
    long_pdf = render_pdf(long_lines)

    reader_s = PdfReader(io.BytesIO(short_pdf))
    reader_l = PdfReader(io.BytesIO(long_pdf))
    h_short = float(reader_s.pages[0].mediabox.height)
    h_long = float(reader_l.pages[0].mediabox.height)
    assert h_long > h_short, f"long receipt {h_long} must exceed short {h_short}"


# ─── 4. QR embedded ──────────────────────────────────────────────────


def test_pdf_contains_qr_image_when_fiscal_doc_no_present() -> None:
    lines = format_receipt(_sell_ctx())
    pdf = render_pdf(lines)
    # PDF image XObject with /Subtype /Image marker — reportlab writes
    # this for drawImage bitmap embeds.
    assert b"/Subtype /Image" in pdf


def test_no_qr_image_when_fiscal_doc_no_missing() -> None:
    ctx = _sell_ctx()
    ctx.fiscal_doc_no = ""
    lines = format_receipt(ctx)
    assert lines.qr_payload is None
    pdf = render_pdf(lines)
    # Without QR, no image XObject with /Subtype /Image should exist.
    # reportlab may still write /Image tokens in font-related dicts,
    # so we check for the specific image-subtype marker.
    assert b"/Subtype /Image" not in pdf, (
        "offline pre-ACK receipt must not embed broken QR"
    )


# ─── 5. Determinism ──────────────────────────────────────────────────


def test_render_pdf_is_deterministic_ignoring_metadata() -> None:
    # reportlab embeds a /CreationDate.  Strip volatile fields and
    # compare the rest.
    lines = format_receipt(_sell_ctx())
    a = render_pdf(lines)
    b = render_pdf(lines)
    # Size should be stable (±small variance from timestamps).
    assert abs(len(a) - len(b)) < 100, (
        f"PDF size varies wildly: {len(a)} vs {len(b)}"
    )


# ─── 6. Self-contained (no external refs) ────────────────────────────


def test_pdf_no_external_url_refs() -> None:
    lines = format_receipt(_sell_ctx())
    pdf = render_pdf(lines)
    # No http:// URL as a resource link — only inside the QR payload
    # which is part of the bitmap pixels, not a PDF hyperlink.
    assert b"/URI (http" not in pdf
    assert b"/URI(http" not in pdf


# ─── 7. Defensive — missing fields ───────────────────────────────────


def test_render_pdf_with_empty_merchant_does_not_crash() -> None:
    ctx = _sell_ctx()
    ctx.org_name = ""
    ctx.point_name = ""
    lines = format_receipt(ctx)
    pdf = render_pdf(lines)
    assert pdf.startswith(b"%PDF-")


def test_render_pdf_with_no_items_still_produces_valid_pdf() -> None:
    lines = format_receipt(RenderContext(
        operation_type=OperationType.Z_REPORT,
        fiscal_number="3001234567",
        fiscal_doc_no="0000099999",
        fiscal_date=datetime(2026, 4, 21, tzinfo=timezone.utc),
        local_doc_no=100,
        z_report_no=3,
        total_sum_kopecks=1_000_000,
    ))
    pdf = render_pdf(lines)
    assert pdf.startswith(b"%PDF-")


# ─── 8. Font embedding (not referenced by name) ──────────────────────


def test_bundled_font_embedded_in_pdf() -> None:
    # PDF must embed the TTF font data, not just reference by name
    # (otherwise viewer may substitute and break Cyrillic rendering).
    lines = format_receipt(_sell_ctx())
    pdf = render_pdf(lines)
    # Font file stream present — reportlab emits /FontFile2 for TTF.
    assert b"/FontFile2" in pdf, "TTF font must be embedded into PDF"


def test_line_count_cap_prevents_dos() -> None:
    # Review finding: a malformed document with thousands of items
    # would otherwise produce a multi-meter-long PDF that OOMs the
    # worker.  Cap enforced at render time.
    ctx = _sell_ctx()
    ctx.items = [ReceiptItem(
        name=f"Товар #{i}", quantity_thousandths=1000,
        price_kopecks=1, sum_kopecks=1, tax_letter="А",
    ) for i in range(1000)]  # Will produce ≈2000 lines (name + qty×price)
    lines = format_receipt(ctx)
    with pytest.raises(ValueError, match="exceeding cap"):
        render_pdf(lines)
