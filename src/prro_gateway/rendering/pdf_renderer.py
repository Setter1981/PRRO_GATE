"""reportlab-based PDF emitter for `ReceiptLines`.

Produces a narrow-paper PDF (80мм default, 58мм alternative) with
each line positioned at a fixed baseline — mirroring the WebCheck
thermal layout but fully cross-platform.  Uses a bundled
DejaVu Sans Mono TTF for guaranteed Cyrillic rendering regardless of
host-OS font availability.

Typical callers:
* receipt archive (store PDF next to FiscalDocumentRecord)
* duplicate reprint (operator requests a second copy)
* email attachment (forward to customer/accountant)
* audit evidence (long-term legal record)

QR code is embedded as a reportlab `Image` flowable (PNG bitmap via
qrcode+Pillow).  No external resources, no network, no writable
filesystem side-effects.
"""
from __future__ import annotations

import io
from pathlib import Path
from threading import Lock
from typing import Final

from reportlab.lib import pagesizes
from reportlab.lib.units import mm
from reportlab.lib.utils import ImageReader
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfgen import canvas

from .models import ReceiptLines

_FONT_DIR: Final[Path] = Path(__file__).parent / "fonts"
_FONT_REGULAR: Final[str] = "PRRO-Mono"
_FONT_BOLD: Final[str] = "PRRO-Mono-Bold"
_FONT_SIZE_PT: Final[float] = 7.5          # Consolas 8pt equivalent; fits 42 chars in 80мм
_LINE_HEIGHT_PT: Final[float] = 9.5
_QR_SIZE_MM: Final[float] = 25.0

_MIN_PAPER_MM: Final[int] = 40
_MAX_PAPER_MM: Final[int] = 120

# DoS prevention — an adapter that leaks thousands of items into
# `lines.body` would otherwise render an arbitrarily tall single-page
# PDF and OOM the worker.  500 lines of 9.5pt gives ~66cm of paper —
# 3× the length of a realistic large-basket thermal receipt.
_MAX_TOTAL_LINES: Final[int] = 500

# reportlab TTF registry is module-global — register once, idempotent.
_FONT_LOCK = Lock()
_FONT_REGISTERED = False


def _ensure_fonts_registered() -> None:
    global _FONT_REGISTERED
    if _FONT_REGISTERED:
        return
    with _FONT_LOCK:
        if _FONT_REGISTERED:
            return
        regular_path = _FONT_DIR / "DejaVuSansMono.ttf"
        bold_path = _FONT_DIR / "DejaVuSansMono-Bold.ttf"
        if not regular_path.is_file():
            raise RuntimeError(
                f"bundled font missing at {regular_path} — "
                "reinstall package or restore src/prro_gateway/rendering/fonts/"
            )
        pdfmetrics.registerFont(TTFont(_FONT_REGULAR, str(regular_path)))
        if bold_path.is_file():
            pdfmetrics.registerFont(TTFont(_FONT_BOLD, str(bold_path)))
        _FONT_REGISTERED = True


def render_pdf(
    lines: ReceiptLines,
    *,
    paper_width_mm: int = 80,
) -> bytes:
    """Render `ReceiptLines` as PDF bytes.

    Paper height grows with content (auto-sized).  Width is clamped
    to `[40, 120]mm` to prevent CSS/PDF-dimension abuse when the
    renderer is wired into an HTTP endpoint.
    """
    _ensure_fonts_registered()

    paper_w = max(_MIN_PAPER_MM, min(_MAX_PAPER_MM, int(paper_width_mm)))
    page_w_pt = paper_w * mm
    all_lines = list(lines.header) + list(lines.body) + list(lines.footer)
    if len(all_lines) > _MAX_TOTAL_LINES:
        raise ValueError(
            f"receipt has {len(all_lines)} lines, exceeding cap of "
            f"{_MAX_TOTAL_LINES} — likely a malformed document or "
            "unbounded item list; reject upstream"
        )
    top_margin = 4 * mm
    bottom_margin = 4 * mm
    side_margin = 3 * mm
    content_h = _LINE_HEIGHT_PT * len(all_lines)
    qr_block_h = (_QR_SIZE_MM * mm + 4 * mm) if lines.qr_payload else 0
    page_h_pt = top_margin + content_h + qr_block_h + bottom_margin

    buffer = io.BytesIO()
    pdf = canvas.Canvas(buffer, pagesize=(page_w_pt, page_h_pt))
    pdf.setTitle("Фіскальний чек")
    pdf.setAuthor("PRRO Gateway")
    pdf.setFont(_FONT_REGULAR, _FONT_SIZE_PT)

    # Draw lines top-to-bottom (PDF origin is bottom-left).
    y = page_h_pt - top_margin - _FONT_SIZE_PT
    for line in all_lines:
        pdf.drawString(side_margin, y, line)
        y -= _LINE_HEIGHT_PT

    # QR image centred below the footer block.
    if lines.qr_payload:
        qr_png = _build_qr_png(lines.qr_payload)
        img = ImageReader(io.BytesIO(qr_png))
        qr_side = _QR_SIZE_MM * mm
        qr_x = (page_w_pt - qr_side) / 2
        qr_y = y - qr_side - 2 * mm
        pdf.drawImage(img, qr_x, qr_y, width=qr_side, height=qr_side,
                      preserveAspectRatio=True, mask='auto')

    pdf.showPage()
    pdf.save()
    return buffer.getvalue()


def _build_qr_png(payload: str) -> bytes:
    import qrcode
    qr = qrcode.QRCode(
        version=None,
        error_correction=qrcode.constants.ERROR_CORRECT_L,
        box_size=8,
        border=4,
    )
    qr.add_data(payload)
    qr.make(fit=True)
    image = qr.make_image(fill_color="black", back_color="white")
    buf = io.BytesIO()
    image.save(buf, format="PNG")
    return buf.getvalue()


__all__ = ["render_pdf"]
