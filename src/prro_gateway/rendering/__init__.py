"""Cross-platform receipt rendering for PRRO Gateway.

Pure-Python pipeline: canonical document → ASCII lines → (HTML | PDF |
ESC-POS) emitter.  The formatter is the single source of truth for
layout; downstream emitters only iterate lines.

The ASCII mid-format mirrors the approach used by the desktop WebCheck
reference (`FormPrint.cs`) — pre-computed list of fixed-width lines
painted on thermal/PDF/HTML surface.  Unlike WebCheck's Windows-only
GDI+ / iTextSharp stack, this module is OS-agnostic and uses only
libraries that work identically on Windows, Linux, and macOS.
"""
from .formatter import format_receipt
from .html_renderer import render_html
from .models import (
    ReceiptItem,
    ReceiptLines,
    ReceiptPayment,
    ReceiptTax,
    RenderContext,
)

__all__ = [
    "format_receipt",
    "render_html",
    "ReceiptItem",
    "ReceiptLines",
    "ReceiptPayment",
    "ReceiptTax",
    "RenderContext",
]
