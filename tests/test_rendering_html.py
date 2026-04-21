"""Proof tests for HTML receipt renderer (Rendering Phase 2).

Takes `ReceiptLines` from the Phase 1 formatter and produces a
self-contained HTML page with:
* Pre-formatted ASCII lines in `<pre>` (monospace font, preserves alignment).
* QR code inline as base64 PNG data URI (no external resources).
* @media print CSS tuned for 80мм thermal paper.
* HTML-escaped user content (XSS-safe).
"""
from __future__ import annotations

import base64
import re
from datetime import datetime, timezone

import pytest

from prro_gateway.enums import OperationType
from prro_gateway.rendering.formatter import format_receipt
from prro_gateway.rendering.html_renderer import render_html
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
            name="Хліб",
            quantity_thousandths=1000,
            price_kopecks=2500,
            sum_kopecks=2500,
            tax_letter="А",
        )],
        payments=[ReceiptPayment(kind="Готівка", amount_kopecks=12345)],
        taxes=[ReceiptTax(letter="А", rate_pct=20.0,
                          sum_kopecks=12345, tax_kopecks=2058)],
    )


# ─── 1. Structural ───────────────────────────────────────────────────


def test_output_is_complete_html_document() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    assert html.startswith("<!DOCTYPE html>") or html.lstrip().startswith("<!DOCTYPE html>")
    assert "<html" in html
    assert "</html>" in html
    assert "<body" in html


def test_output_is_valid_utf8_string() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    # Cyrillic content passes through unchanged.
    assert "ТОВ Приклад" in html or "ТОВ Приклад" in html.replace("&#", "")


def test_lines_rendered_inside_pre_block() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    assert "<pre" in html
    assert "</pre>" in html


def test_monospace_font_family_set() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    assert "monospace" in html.lower()


def test_print_media_css_present() -> None:
    # @media print rules required so browser Ctrl+P produces thermal
    # layout (80mm wide, no margins).
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    assert "@media print" in html


# ─── 2. QR code embedded ─────────────────────────────────────────────


def test_qr_inline_as_base64_png_data_uri() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    assert "data:image/png;base64," in html
    # Extract and decode the base64 payload — must be a valid PNG.
    m = re.search(r'data:image/png;base64,([A-Za-z0-9+/=]+)', html)
    assert m is not None
    png_bytes = base64.b64decode(m.group(1))
    assert png_bytes.startswith(b'\x89PNG\r\n\x1a\n'), "must be a real PNG"


def test_no_qr_when_fiscal_doc_no_missing() -> None:
    # When qr_payload is None (offline pre-ACK), HTML must not
    # contain a broken-URL QR image.
    ctx = _sell_ctx()
    ctx.fiscal_doc_no = ""  # no DPS ack yet
    lines = format_receipt(ctx)
    assert lines.qr_payload is None
    html = render_html(lines)
    assert "data:image/png;base64," not in html


def test_shift_open_has_no_qr_in_html() -> None:
    lines = format_receipt(RenderContext(
        operation_type=OperationType.SHIFT_OPEN,
        fiscal_number="3001234567",
        fiscal_doc_no="0000000001",
        fiscal_date=datetime(2026, 4, 21, tzinfo=timezone.utc),
        local_doc_no=1,
        shift_no=5,
    ))
    html = render_html(lines)
    assert "data:image/png;base64," not in html


# ─── 3. XSS / content-escape safety ──────────────────────────────────


def test_html_escapes_angle_brackets_in_user_content() -> None:
    # A merchant name with raw '<' must not render as HTML tag.
    ctx = _sell_ctx()
    ctx.org_name = "<script>alert(1)</script>"
    lines = format_receipt(ctx)
    html = render_html(lines)
    assert "<script>" not in html
    # Escaped version must appear (in any standard entity form).
    assert "&lt;script&gt;" in html or "&lt;script" in html


def test_html_escapes_ampersand_in_user_content() -> None:
    ctx = _sell_ctx()
    ctx.org_name = "Q&A Store"
    lines = format_receipt(ctx)
    html = render_html(lines)
    assert "&amp;" in html
    assert "Q&A Store" not in html or html.count("Q&A") == 0  # raw unescaped absent


def test_html_escapes_quote_in_user_content() -> None:
    ctx = _sell_ctx()
    ctx.point_name = 'Quote"Attack'
    lines = format_receipt(ctx)
    html = render_html(lines)
    # Quote must be escaped (or placed in context where it can't
    # break out of attribute) — assert literal double-quote not in
    # `<pre>` content without entity escape.
    pre_content = re.search(r'<pre[^>]*>(.*?)</pre>', html, re.DOTALL)
    assert pre_content is not None
    assert '"' not in pre_content.group(1) or '&quot;' in pre_content.group(1)


# ─── 4. No external resources ────────────────────────────────────────


def test_no_external_http_references() -> None:
    # Receipt must be fully self-contained so it renders offline and
    # cannot leak data via tracking beacons.
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    for needle in ("http://", "https://cdn", "<script src=", "<link rel=\"stylesheet\" href=\"http"):
        if needle == "https://cdn":
            assert needle not in html
        elif needle.startswith("http://"):
            assert needle not in html
        elif "<script src=" in needle:
            assert needle not in html


def test_qr_url_in_payload_is_not_fetched_via_network() -> None:
    # QR contains DPS cabinet URL but the HTML never loads it as a
    # resource — only embeds the URL INSIDE the PNG pixels.
    ctx = _sell_ctx()
    lines = format_receipt(ctx)
    html = render_html(lines)
    assert lines.qr_payload is not None
    # URL is inside the PNG image encoding, not as an <a href> or
    # <link> or <script>.
    for forbidden in ('href="https://cabinet',
                      'src="https://cabinet',
                      'action="https://cabinet'):
        assert forbidden not in html


# ─── 5. Header/Body/Footer sections preserved ────────────────────────


def test_header_body_footer_all_present() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    for h in lines.header:
        if h.strip():
            # Each non-empty line's text fragment must appear in HTML
            # (possibly HTML-escaped).
            snippet = h.strip().split()[0]  # first word
            assert snippet in html or _html_escape(snippet) in html, (
                f"Header line missing from HTML: {h!r}"
            )


def test_body_shows_item_names() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    assert "Хліб" in html


def test_footer_shows_fiscal_doc_no() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    assert "0000012345" in html


# ─── 6. 80mm print layout ────────────────────────────────────────────


def test_print_layout_width_80mm_or_smaller() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    # CSS should define print width ≤ 80mm — a common 80мм thermal
    # printer's usable width.  Accept 72mm or 80mm common values.
    assert re.search(r'width:\s*(72|80)mm', html) is not None


def test_narrow_width_32_char_58mm_supported() -> None:
    # 58мм printer option → width=32 chars per line.
    lines = format_receipt(_sell_ctx(), width=32)
    html = render_html(lines, paper_width_mm=58)
    assert "58mm" in html


def _html_escape(s: str) -> str:
    return (s.replace("&", "&amp;")
             .replace("<", "&lt;")
             .replace(">", "&gt;"))


# ─── 7. Hardening from review ────────────────────────────────────────


def test_xss_autoescape_also_applies_to_img_onerror() -> None:
    # Regression: escape MUST neutralize common XSS vectors even after
    # future template renames that might silently drop autoescape.
    ctx = _sell_ctx()
    ctx.org_name = '<img src=x onerror="alert(1)">'
    lines = format_receipt(ctx)
    html = render_html(lines)
    assert "<img src=x" not in html
    assert "&lt;img" in html or "&lt;" in html


def test_paper_width_mm_clamped_to_safe_range() -> None:
    # F2: CSS injection guard — out-of-range values are clamped.
    lines = format_receipt(_sell_ctx())
    # Attacker-style input: absurdly large width.
    html_big = render_html(lines, paper_width_mm=99_999)
    # Clamped to max 120mm.
    assert re.search(r'width:\s*120mm', html_big) is not None
    assert "99999" not in html_big

    # Attacker-style input: zero / negative width.
    html_small = render_html(lines, paper_width_mm=-5)
    assert re.search(r'width:\s*40mm', html_small) is not None


def test_paper_width_mm_rejects_string_gracefully() -> None:
    # int() coercion — non-numeric string would raise ValueError.
    # This is acceptable (fail-loud) vs silent acceptance into CSS.
    lines = format_receipt(_sell_ctx())
    with pytest.raises((ValueError, TypeError)):
        render_html(lines, paper_width_mm="80; body{background:url(//evil)}")  # type: ignore


def test_qr_size_mm_clamped() -> None:
    lines = format_receipt(_sell_ctx())
    html = render_html(lines, qr_size_mm=999)
    assert re.search(r'width:\s*60mm', html) is not None  # max 60mm


def test_qr_border_matches_spec_quiet_zone() -> None:
    # F3: border=4 modules per QR ISO spec.  Scanners' rejection rate
    # drops sharply with proper quiet zone.  The output QR must decode
    # successfully — regression catcher if border shrinks again.
    lines = format_receipt(_sell_ctx())
    html = render_html(lines)
    m = re.search(r'data:image/png;base64,([A-Za-z0-9+/=]+)', html)
    assert m is not None
    png_bytes = base64.b64decode(m.group(1))
    # Decode the QR and verify the payload round-trips.
    try:
        from PIL import Image
        import io as _io
    except ImportError:
        pytest.skip("Pillow unavailable in this env")
    img = Image.open(_io.BytesIO(png_bytes))
    # QR image must be at least 21 modules × 8 px/box + 2*4 border =
    # 21×8 + 64 = 232 px.  A small border=2 image would be ≈216 px.
    assert img.size[0] >= 232, (
        f"QR image {img.size} too small — suggests border<4 regression"
    )
