"""Jinja2-based HTML emitter for `ReceiptLines`.

Takes Phase 1 formatter output and produces a self-contained HTML
page (no external resources) suitable for:
* Browser preview (admin UI, email attachment).
* Browser print via Ctrl+P for hosts without dedicated thermal printer.
* Document archive / audit snapshots.

The QR payload (if present) is rendered inline as a base64 PNG data URI
via `qrcode` + Pillow.  Autoescape on — user-sourced content (merchant
name, item labels) is XSS-safe.

Cross-platform: pure Python, works identically on Windows/Linux/macOS.
"""
from __future__ import annotations

import base64
import io
from pathlib import Path
from typing import Final

import qrcode
from jinja2 import Environment, FileSystemLoader, select_autoescape

from .models import ReceiptLines

_TEMPLATE_DIR: Final[Path] = Path(__file__).parent / "templates"
_TEMPLATE_NAME: Final[str] = "receipt.html.j2"

# 80mm thermal paper — typical receipt printer width.  58mm supported
# via `paper_width_mm=58` for narrower printers.
_DEFAULT_PAPER_WIDTH_MM: Final[int] = 80
_DEFAULT_QR_SIZE_MM: Final[int] = 30

_env: Environment = Environment(
    loader=FileSystemLoader(str(_TEMPLATE_DIR)),
    # Autoescape on for ALL templates + string renders.  A future
    # refactor renaming `.j2` → `.jinja` or using `from_string` would
    # otherwise silently lose XSS protection.  Review finding F1.
    autoescape=select_autoescape(default_for_string=True, default=True),
    trim_blocks=False,
    lstrip_blocks=False,
)

# CSS dimension bounds — prevents CSS injection via unvalidated caller
# input (e.g., admin UI query parameter forwarded into the renderer).
# Review finding F2.
_MIN_PAPER_WIDTH_MM = 40
_MAX_PAPER_WIDTH_MM = 120
_MIN_QR_SIZE_MM = 10
_MAX_QR_SIZE_MM = 60


def render_html(
    lines: ReceiptLines,
    *,
    paper_width_mm: int = _DEFAULT_PAPER_WIDTH_MM,
    qr_size_mm: int = _DEFAULT_QR_SIZE_MM,
) -> str:
    """Render `ReceiptLines` as a self-contained HTML document.

    Args:
        lines: Output from `format_receipt(ctx)` (Phase 1).
        paper_width_mm: Target paper width (80 for standard thermal,
            58 for narrower printers).
        qr_size_mm: QR code side length in mm when rendered on paper.

    Returns:
        HTML string — ready to write to file, serve via HTTP, or
        embed in email.  Cyrillic content UTF-8-encoded.
    """
    qr_data_uri = _build_qr_data_uri(lines.qr_payload) if lines.qr_payload else None

    # Coerce + clamp CSS dimensions to prevent injection through
    # unvalidated caller input (review finding F2).
    safe_paper = max(_MIN_PAPER_WIDTH_MM,
                     min(_MAX_PAPER_WIDTH_MM, int(paper_width_mm)))
    safe_qr = max(_MIN_QR_SIZE_MM,
                  min(_MAX_QR_SIZE_MM, int(qr_size_mm)))

    template = _env.get_template(_TEMPLATE_NAME)
    return template.render(
        header=lines.header,
        body=lines.body,
        footer=lines.footer,
        qr_data_uri=qr_data_uri,
        paper_width_mm=safe_paper,
        qr_size_mm=safe_qr,
    )


def _build_qr_data_uri(payload: str) -> str:
    """Encode `payload` as a PNG QR code, return as base64 data URI.

    Uses the `qrcode` library's default error-correction level (L,
    matches DPS reference) and auto-sized version.  No external
    network calls — QR is generated in-process.
    """
    qr = qrcode.QRCode(
        version=None,                     # auto-fit
        error_correction=qrcode.constants.ERROR_CORRECT_L,
        box_size=8,
        # QR spec mandates ≥4 modules of quiet zone around the data.
        # Some DPS / mobile scanners refuse to decode with border=2.
        # Review finding F3.
        border=4,
    )
    qr.add_data(payload)
    qr.make(fit=True)
    image = qr.make_image(fill_color="black", back_color="white")
    buf = io.BytesIO()
    image.save(buf, format="PNG")
    b64 = base64.b64encode(buf.getvalue()).decode("ascii")
    return f"data:image/png;base64,{b64}"


__all__ = ["render_html"]
