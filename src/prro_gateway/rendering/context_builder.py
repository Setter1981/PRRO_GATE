"""Build RenderContext from a persisted fiscal_documents row.

Single authoritative source for the receipt UI: `payload_json` on
`fiscal_documents` (written at ingress, preserved for every document).
Joined with `fiscal_number_config` for org identity, `shifts` for the
visible shift number, and `tax_group_definitions` for VAT letter / rate
lookups.

Tax-letter convention (Ukrainian retail): groups are assigned А/Б/В…
in ascending `tax_id` order per fiscal_number.  There is no `letter`
column in the schema, so the builder derives it deterministically.
"""
from __future__ import annotations

import json
import sqlite3
from datetime import datetime, timezone

from ..enums import OperationType
from .models import ReceiptItem, ReceiptPayment, ReceiptTax, RenderContext

_ALPHABET_UA = "АБВГҐДЕЄЖЗИІЇЙКЛМНОПРСТУФХЦЧШЩЬЮЯ"

# doc_type values that have a defined printable receipt layout in
# `formatter.py`.  Anything outside this set is a non-renderable
# internal operation (STATUS, OFFLINE_BEGIN, ...) and the builder
# returns None → the UI 404s.
_RENDERABLE_OP_TYPES = {
    OperationType.SELL,
    OperationType.RETURN,
    OperationType.SHIFT_OPEN,
    OperationType.SHIFT_CLOSE,
    OperationType.Z_REPORT,
    OperationType.SERVICE_IN,
    OperationType.SERVICE_OUT,
    OperationType.CASH_WITHDRAWAL,
}


def _safe_int(value: object, default: int = 0) -> int:
    """Coerce JSON-sourced value to int, surviving garbage (str/dict/…).

    Fiscal data written by adapters is well-typed, but older rows from
    pre-migration adapters — or legitimate malformed payload_json —
    must not 500 the admin UI.
    """
    if value is None:
        return default
    try:
        return int(value)
    except (TypeError, ValueError, OverflowError):
        try:
            # Accept "123" but reject "abc" / dict / list.
            return int(str(value))
        except (TypeError, ValueError, OverflowError):
            return default


def _parse_iso(raw: str | None) -> datetime:
    if not raw:
        return datetime.now(tz=timezone.utc)
    try:
        # sqlite CURRENT_TIMESTAMP may produce naive UTC; handle both.
        s = raw.replace("Z", "+00:00")
        dt = datetime.fromisoformat(s)
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return dt
    except ValueError:
        return datetime.now(tz=timezone.utc)


def _load_tax_groups(
    conn: sqlite3.Connection, fn: str,
) -> dict[str, tuple[str, float]]:
    """Return {tax_id_str: (letter, rate_pct)} for the given FN.

    Letters are assigned by ascending numeric tax_id.
    """
    rows = conn.execute(
        """SELECT tax_id, tax_rate FROM tax_group_definitions
           WHERE fiscal_number = ?""",
        (fn,),
    ).fetchall()
    if not rows:
        return {}
    try:
        ordered = sorted(rows, key=lambda r: int(r[0]))
    except (TypeError, ValueError, OverflowError):
        # tax_id that isn't numeric — fall back to lexicographic.
        ordered = sorted(rows, key=lambda r: str(r[0]))
    result: dict[str, tuple[str, float]] = {}
    for idx, (tid, rate) in enumerate(ordered):
        letter = _ALPHABET_UA[idx] if idx < len(_ALPHABET_UA) else "?"
        result[str(tid)] = (letter, float(rate or 0))
    return result


def _payment_label(payment: dict) -> str:
    # DPS-path adapter pre-resolves human label into `resolved_nm`.
    # Checkbox and offline paths fall through to bare `payment_type`.
    nm = (payment.get("resolved_nm") or "").strip()
    if nm:
        return nm
    pt = (payment.get("payment_type") or "").strip()
    return pt.capitalize() if pt else "—"


def _aggregate_taxes(
    items: list[dict],
    groups: dict[str, tuple[str, float]],
) -> list[ReceiptTax]:
    sums: dict[str, int] = {}
    for it in items:
        raw = it.get("tax_id")
        if raw is None:
            continue
        key = str(raw)
        sums[key] = sums.get(key, 0) + _safe_int(it.get("sum", 0))
    out: list[ReceiptTax] = []
    for key, total in sums.items():
        letter, rate = groups.get(key, ("?", 0.0))
        # VAT is included in price: tax = sum * r / (100 + r), rounded.
        tax_kop = 0
        if rate > 0:
            tax_kop = round(total * rate / (100 + rate))
        out.append(ReceiptTax(
            letter=letter, rate_pct=rate,
            sum_kopecks=total, tax_kopecks=tax_kop,
        ))
    out.sort(key=lambda t: t.letter)
    return out


def build_render_context(
    conn: sqlite3.Connection, document_id: str,
) -> RenderContext | None:
    row = conn.execute(
        """SELECT document_id, fiscal_number, shift_id, lnd, doc_type,
                  server_fiscal_no, server_fiscal_date, total_sum,
                  business_ts, payload_json
           FROM fiscal_documents WHERE document_id = ?""",
        (document_id,),
    ).fetchone()
    if row is None:
        return None

    fn = row[1]
    shift_id = row[2]
    lnd = _safe_int(row[3], default=0)
    doc_type = row[4]
    server_fiscal_no = row[5] or ""
    server_fiscal_date = row[6]
    total_sum_kop = _safe_int(row[7], default=0)
    business_ts = row[8]
    payload_json = row[9] or "{}"

    try:
        envelope = json.loads(payload_json)
    except (json.JSONDecodeError, TypeError):
        envelope = {}
    if not isinstance(envelope, dict):
        envelope = {}
    # Canonical envelope wraps op-specific fields under `payload`.  Older
    # rows persisted before the wrapper migration (or alt adapters) may
    # have stored goods/payments at top level — fall back transparently.
    inner = envelope.get("payload")
    if not isinstance(inner, dict):
        inner = envelope if any(
            k in envelope for k in ("goods", "payments", "totals", "cashier")
        ) else {}

    try:
        op = OperationType(doc_type)
    except (TypeError, ValueError, OverflowError):
        return None
    if op not in _RENDERABLE_OP_TYPES:
        # Internal / non-printable operation (STATUS, OFFLINE_BEGIN, …).
        return None

    # Org identity from fn_config.
    org_row = conn.execute(
        """SELECT COALESCE(org_name,''), COALESCE(org_address,''),
                  COALESCE(tax_number,'')
           FROM fiscal_number_config WHERE fiscal_number = ?""",
        (fn,),
    ).fetchone()
    org_name = org_row[0] if org_row else ""
    point_addr = org_row[1] if org_row else ""
    tin = org_row[2] if org_row else ""

    shift_no: int | None = None
    if shift_id:
        srow = conn.execute(
            "SELECT serial FROM shifts WHERE shift_id = ?",
            (shift_id,),
        ).fetchone()
        if srow and srow[0] is not None:
            try:
                shift_no = int(srow[0])
            except (TypeError, ValueError, OverflowError):
                shift_no = None

    groups = _load_tax_groups(conn, fn)

    items_raw_any = inner.get("goods") or []
    items_raw = [g for g in items_raw_any if isinstance(g, dict)]
    items: list[ReceiptItem] = []
    for g in items_raw:
        tid = g.get("tax_id")
        letter = groups.get(str(tid), ("", 0.0))[0] if tid is not None else ""
        items.append(ReceiptItem(
            name=str(g.get("name", "")),
            quantity_thousandths=_safe_int(g.get("quantity", 0)),
            price_kopecks=_safe_int(g.get("price", 0)),
            sum_kopecks=_safe_int(g.get("sum", 0)),
            uktzed=g.get("uktzed") if isinstance(g.get("uktzed"), str) else None,
            tax_letter=letter,
            discount_kopecks=_safe_int(g.get("discount", 0)),
        ))

    payments_raw_any = inner.get("payments") or []
    payments_raw = [p for p in payments_raw_any if isinstance(p, dict)]
    payments = [
        ReceiptPayment(kind=_payment_label(p),
                       amount_kopecks=_safe_int(p.get("amount", 0)))
        for p in payments_raw
    ]

    taxes = _aggregate_taxes(items_raw, groups)

    fiscal_date_src = server_fiscal_date or business_ts
    fiscal_date = _parse_iso(fiscal_date_src)

    return RenderContext(
        operation_type=op,
        fiscal_number=fn,
        fiscal_doc_no=server_fiscal_no,
        fiscal_date=fiscal_date,
        local_doc_no=lnd,
        org_name=org_name,
        point_name="",   # no schema column yet (Phase 6 follow-up)
        point_addr=point_addr,
        tin=tin,
        cashier=str(inner.get("cashier") or "") if isinstance(inner.get("cashier"), (str, int, float)) else "",
        total_sum_kopecks=total_sum_kop,
        items=items,
        payments=payments,
        taxes=taxes,
        shift_no=shift_no,
        z_report_no=None,
    )


__all__ = ["build_render_context"]
