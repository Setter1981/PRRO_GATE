"""PERIODIC_REPORT read-path for the Maria 304 native ingress.

The Rust driver sends `CommandType::PeriodicReport` for four wire
opcodes:

* `FIRN<D4 first><D4 last>` — full report by Z-number range.
* `IREN<D4 first><D4 last>` — short report by Z-number range.
* `FIRP<yyyyMMdd><yyyyMMdd>` — full report by date range.
* `IREP<yyyyMMdd><yyyyMMdd>` — short report by date range.

This handler aggregates data from `fiscal_documents` WITHOUT writing to
`ingress_inbox` and WITHOUT creating a new fiscal document.  It is a
read-only query over the local archive, mirroring what the reference
WebCheck `DateReport.PeriodR` implementation does against its own
KSEF SQLite table.

The response shape extends `CanonicalResponse` (Rust-side untouched
because serde ignores unknown fields by default) with a `report`
object carrying the aggregate.
"""
from __future__ import annotations

import sqlite3
import uuid
from datetime import UTC, datetime
from typing import Any

from fastapi.responses import JSONResponse

from ..logging import get_logger

logger = get_logger("prro_gateway.runtime.rest.periodic")

_Z_NUMBER_OPCODES: frozenset[str] = frozenset({"FIRN", "IREN"})
_DATE_OPCODES: frozenset[str] = frozenset({"FIRP", "IREP"})
_SUPPORTED_OPCODES: frozenset[str] = _Z_NUMBER_OPCODES | _DATE_OPCODES


_METRIC_REPORT_SERVED = "ingress.maria304.periodic_report.served"
_METRIC_REPORT_NOT_WIRED = "ingress.maria304.periodic_report.not_wired_to_rust"
_METRIC_REPORT_ERROR = "ingress.maria304.periodic_report.error"


def handle_periodic_report(container: Any, raw: dict[str, Any]) -> JSONResponse:
    """Read-only handler for PERIODIC_REPORT commands.

    Returns 200 with the Rust `CanonicalResponse` shape plus a nested
    `report` object on success; 400 with `ok=false` on malformed input.
    """
    fiscal_number = raw.get("fiscal_number")
    if not isinstance(fiscal_number, str) or not fiscal_number:
        _try_metric_inc(container, _METRIC_REPORT_ERROR)
        return _err("SOFT_BAD_REQUEST", "fiscal_number missing")

    payload = raw.get("payload") or {}
    # Rust `ReceiptPayload` serialises `raw_frames` directly under
    # `payload`, not nested in an extra `receipt` object — that extra
    # nesting only happens inside the canonical envelope built by the
    # Python adapter.  Check both positions for resilience.
    frames = payload.get("raw_frames") or (payload.get("receipt") or {}).get("raw_frames") or []
    if not frames or not isinstance(frames[0], dict):
        _try_metric_inc(container, _METRIC_REPORT_ERROR)
        return _err("SOFT_BAD_REQUEST", "raw_frames[0] missing for PERIODIC_REPORT")
    opcode = frames[0].get("opcode")
    body = frames[0].get("body", "")
    if opcode not in _SUPPORTED_OPCODES:
        _try_metric_inc(container, _METRIC_REPORT_ERROR)
        return _err(
            "SOFT_BAD_REQUEST",
            f"unsupported PERIODIC_REPORT opcode: {opcode!r}",
        )
    if not isinstance(body, str):
        _try_metric_inc(container, _METRIC_REPORT_ERROR)
        return _err("SOFT_BAD_REQUEST", "raw_frames[0].body must be a string")

    try:
        range_args = _parse_range(opcode, body)
    except ValueError as exc:
        _try_metric_inc(container, _METRIC_REPORT_ERROR)
        return _err("SOFT_BAD_REQUEST", str(exc))

    with container.connect() as conn:
        report = _aggregate_periodic_report(
            conn, fiscal_number=fiscal_number, opcode=opcode, **range_args,
        )

    now_iso = datetime.now(UTC).isoformat()
    # KNOWN GAP (deferred to M7-Py-3d): the Rust-side `CanonicalResponse`
    # struct does not include a `report` field, so serde drops it
    # silently.  Aggregate data DOES NOT currently reach 1C — the Rust
    # driver just builds `DONE+READY` after a successful submit_report.
    # Log every report so operators can at least see it in the gateway
    # journal until the Rust consumer lands.
    logger.info(
        "maria304_periodic_report_served_not_wired_to_rust",
        extra={"extra_fields": {
            "fiscal_number": fiscal_number,
            "opcode": opcode,
            "z_count": report["z_count"],
            "sales_total_kopecks": report["sales_total_kopecks"],
            "returns_total_kopecks": report["returns_total_kopecks"],
            "report_not_wired_to_rust": True,
        }},
    )
    # Metrics surface for ops alerting — log-only signal is too quiet
    # at 50-point / 70-cashier scale (2nd review finding F1).
    _try_metric_inc(container, _METRIC_REPORT_SERVED)
    _try_metric_inc(container, _METRIC_REPORT_NOT_WIRED)
    return JSONResponse(status_code=200, content={
        "ok": True,
        "document_id": f"periodic-{uuid.uuid4().hex[:12]}",
        "fiscal_id": "",
        "fiscal_ts": now_iso,
        "document_state": "PERIODIC_REPORT",
        "sale_total_kopecks": report["sales_total_kopecks"],
        "return_total_kopecks": report["returns_total_kopecks"],
        "report": report,
    })


def _parse_range(opcode: str, body: str) -> dict[str, Any]:
    """Extract the range endpoints from the opcode body.

    Raises:
        ValueError — malformed body (wrong length, non-digit, inverted).
    """
    if opcode in _Z_NUMBER_OPCODES:
        if len(body) != 8:
            raise ValueError(f"{opcode} body must be 8 chars; got {len(body)}")
        if not (body.isascii() and body.isdigit()):
            raise ValueError(f"{opcode} body must be ASCII digits")
        first = int(body[:4])
        last = int(body[4:])
        if first > last:
            raise ValueError(f"{opcode} first={first} > last={last}")
        return {"z_first": first, "z_last": last}
    # FIRP / IREP
    if len(body) != 16:
        raise ValueError(f"{opcode} body must be 16 chars; got {len(body)}")
    if not (body.isascii() and body.isdigit()):
        raise ValueError(f"{opcode} body must be ASCII digits")
    start = body[:8]
    end = body[8:]
    if start > end:
        raise ValueError(f"{opcode} start={start} > end={end}")
    return {"start_date": _iso_date(start), "end_date_exclusive": _iso_date_exclusive(end)}


def _iso_date(yyyymmdd: str) -> str:
    # 20260401 → 2026-04-01T00:00:00+00:00.  Validate against the
    # calendar (date() raises ValueError for 20260231 etc.) so a
    # malformed date is caught at parse time and not silently
    # lexicographically compared in SQL.
    from datetime import date
    y, m, d = int(yyyymmdd[:4]), int(yyyymmdd[4:6]), int(yyyymmdd[6:8])
    dt = date(y, m, d)
    return f"{dt.isoformat()}T00:00:00+00:00"


def _iso_date_exclusive(yyyymmdd: str) -> str:
    # 20260430 → 2026-05-01T00:00:00+00:00 (exclusive upper bound).
    from datetime import date, timedelta
    y, m, d = int(yyyymmdd[:4]), int(yyyymmdd[4:6]), int(yyyymmdd[6:8])
    dt = date(y, m, d) + timedelta(days=1)
    return f"{dt.isoformat()}T00:00:00+00:00"


def _aggregate_periodic_report(
    conn: sqlite3.Connection, *, fiscal_number: str, opcode: str,
    z_first: int | None = None, z_last: int | None = None,
    start_date: str | None = None, end_date_exclusive: str | None = None,
) -> dict[str, Any]:
    if opcode in _Z_NUMBER_OPCODES:
        where = (
            "fiscal_number = ? AND state IN ('KVT2','ACK') "
            "AND z_report_number BETWEEN ? AND ?"
        )
        params: tuple[Any, ...] = (fiscal_number, z_first, z_last)
    else:
        where = (
            "fiscal_number = ? AND state IN ('KVT2','ACK') "
            "AND business_ts >= ? AND business_ts < ?"
        )
        params = (fiscal_number, start_date, end_date_exclusive)

    row = conn.execute(
        f"""SELECT
            COUNT(CASE WHEN doc_type = 'Z_REPORT' THEN 1 END) AS z_count,
            MIN(CASE WHEN doc_type = 'Z_REPORT' THEN z_report_number END) AS z_start,
            MAX(CASE WHEN doc_type = 'Z_REPORT' THEN z_report_number END) AS z_finish,
            SUM(CASE WHEN doc_type = 'SELL' THEN COALESCE(total_sum, 0) ELSE 0 END) AS sales_total,
            SUM(CASE WHEN doc_type = 'RETURN' THEN COALESCE(total_sum, 0) ELSE 0 END) AS returns_total,
            MIN(business_ts) AS start_ts,
            MAX(business_ts) AS end_ts
        FROM fiscal_documents
        WHERE {where}""",
        params,
    ).fetchone()

    return {
        "kind": opcode,
        "fiscal_number": fiscal_number,
        "z_count": int(row[0] or 0),
        "z_start": int(row[1]) if row[1] is not None else None,
        "z_finish": int(row[2]) if row[2] is not None else None,
        "sales_total_kopecks": int(row[3] or 0),
        "returns_total_kopecks": int(row[4] or 0),
        "start_date": row[5],
        "end_date": row[6],
    }


def _err(code: str, message: str) -> JSONResponse:
    logger.warning("maria304_periodic_bad_request",
                   extra={"extra_fields": {"code": code, "message": message}})
    return JSONResponse(status_code=400, content={
        "ok": False, "error_code": code, "error_message": message,
    })


def _try_metric_inc(container: Any, name: str) -> None:
    metrics = getattr(container, "metrics", None)
    if metrics is None:
        return
    try:
        metrics.inc(name)
    except Exception:  # noqa: BLE001
        pass


__all__ = ["handle_periodic_report"]
