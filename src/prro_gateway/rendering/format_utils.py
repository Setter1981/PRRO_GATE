"""Small fixed-width formatting helpers for the ASCII layout.

All helpers are codepoint-based (not byte-based) so Cyrillic glyphs
align correctly — UTF-8 byte-count would produce wrong visual
widths for Ukrainian merchant names, cashier names, and item labels.

All user-sourced strings MUST go through `sanitize_line` first.  A
merchant name containing `\\n`/`\\r`/control chars would otherwise
produce spurious line breaks on the printed receipt — tampering
surface on a legally-evidentiary document.
"""
from __future__ import annotations

import unicodedata

# Replace control chars that could break line-by-line emission.
# C0 (0x00–0x1F) + DEL (0x7F) + C1 (0x80–0x9F) + line/paragraph seps.
_CONTROL_CHARS = frozenset(
    chr(c) for c in (*range(0x00, 0x20), 0x7F, *range(0x80, 0xA0),
                      0x2028, 0x2029)
)


def sanitize_line(text: str) -> str:
    """Strip control chars and normalize whitespace.

    Tabs/CRs/NLs are mapped to a single space.  Other control chars
    (including C1 and line/paragraph separators) are dropped.  Trailing
    whitespace is stripped, internal runs collapsed to one space.
    """
    if not text:
        return ""
    # Normalize to NFC so combining marks don't duplicate codepoint count.
    text = unicodedata.normalize("NFC", text)
    # Map ANY control char to space, then collapse.
    cleaned_chars = []
    for ch in text:
        if ch in _CONTROL_CHARS:
            cleaned_chars.append(" ")
        else:
            cleaned_chars.append(ch)
    cleaned = "".join(cleaned_chars)
    # Collapse runs of whitespace to single space.
    return " ".join(cleaned.split())


def center(text: str, width: int) -> str:
    """Centre-align `text` within `width` codepoints, pad with spaces."""
    text = text[:width]
    total_pad = width - len(text)
    if total_pad <= 0:
        return text
    left = total_pad // 2
    right = total_pad - left
    return " " * left + text + " " * right


def right_align(text: str, width: int) -> str:
    """Right-align `text` within `width` codepoints."""
    text = text[:width]
    pad = width - len(text)
    if pad <= 0:
        return text
    return " " * pad + text


def two_column(left: str, right: str, width: int) -> str:
    """Left-align `left`, right-align `right`, join with spaces.

    If the two strings together exceed `width`, the left side is
    truncated with ellipsis-like `…` so the right (usually monetary)
    column stays aligned.
    """
    right = right[:width]
    max_left = width - len(right) - 1
    if max_left <= 0:
        return right_align(right, width)
    if len(left) > max_left:
        left = left[: max_left - 1] + "…"
    padding = width - len(left) - len(right)
    return left + " " * padding + right


def separator(char: str, width: int) -> str:
    """Return `char` repeated to fill `width`."""
    if len(char) != 1:
        raise ValueError("separator character must be a single codepoint")
    return char * width


def wrap_long(text: str, width: int) -> list[str]:
    """Word-wrap `text` into lines ≤ `width`.

    Greedy single-pass: split on whitespace, pack words.  A single
    word longer than `width` is hard-broken to stop silent line
    overflow (e.g., mashed UKTZED codes).
    """
    if len(text) <= width:
        return [text] if text else []
    lines: list[str] = []
    current = ""
    for word in text.split():
        if len(word) > width:
            # Flush accumulated current line first.
            if current:
                lines.append(current)
                current = ""
            # Hard-break the oversized word into width-sized chunks.
            for start in range(0, len(word), width):
                chunk = word[start : start + width]
                if len(chunk) == width:
                    lines.append(chunk)
                else:
                    current = chunk
            continue
        candidate = f"{current} {word}" if current else word
        if len(candidate) > width:
            lines.append(current)
            current = word
        else:
            current = candidate
    if current:
        lines.append(current)
    return lines


def kopecks_to_uah(kopecks: int) -> str:
    """Format kopecks as `UAH.kk` string (always 2 decimals)."""
    sign = "-" if kopecks < 0 else ""
    abs_k = abs(int(kopecks))
    uah, kop = divmod(abs_k, 100)
    return f"{sign}{uah}.{kop:02d}"


__all__ = [
    "sanitize_line",
    "center",
    "right_align",
    "two_column",
    "separator",
    "wrap_long",
    "kopecks_to_uah",
]
