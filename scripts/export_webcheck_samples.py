#!/usr/bin/env python3
"""Export representative WebCheck receipts from SQLite databases and XML files.

The script is intentionally read-only against WebCheck databases. By default it
creates a temporary SQLite backup first and reads from that snapshot, so a live
WebCheck database is not queried for a long time.

Typical usage:

    python scripts/export_webcheck_samples.py "D:/WebCheck/Backup/4000162280.db"
    python scripts/export_webcheck_samples.py "D:/WebCheck/Archive" --xml-limit 200
    python scripts/export_webcheck_samples.py "D:/WebCheck" --limit 200 --xml-limit 200

Output goes to --output-dir, which is REQUIRED and MUST be OUTSIDE the repo tree
(e.g. ~/webcheck_dumps/exports/<run>). WebCheck exports contain real fiscal data;
the never-transit rule (U2 / re-check F7) forbids writing them under the repo, so
there is no in-tree default and an in-tree --output-dir is refused.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import sqlite3
import tempfile
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal, InvalidOperation, ROUND_HALF_UP
from pathlib import Path
from typing import Any
from xml.etree import ElementTree as ET


DOC_TYPE_MAP = {
    "0": "SELL",
    "1": "RETURN",
    "2": "SERVICE",
    "8": "SHIFT_OPEN",
    "80": "Z_REPORT",
    "108": "SHIFT_OPEN",
    "109": "GO_OFFLINE",
    "110": "GO_ONLINE",
    "112": "ASK_OFFLINE_CODES",
}

XML_COMMAND_MAP = {
    "0": "SELL",
    "1": "RETURN",
    "2": "SERVICE",
    "8": "SHIFT_OPEN",
    "80": "Z_REPORT",
    "108": "SHIFT_OPEN",
    "109": "GO_OFFLINE",
    "110": "GO_ONLINE",
    "112": "ASK_OFFLINE_CODES",
}

CURATED_CATEGORY_NAMES = (
    "sell_plain",
    "sell_without_excise",
    "sell_with_uktzed",
    "sell_with_excise",
    "sell_with_discount",
    "sell_mixed_payment",
    "return",
    "service_in",
    "service_out",
    "z_report",
    "offline",
    "db_backed",
    "xml_backed",
)


@dataclass(frozen=True)
class ExportOptions:
    output_dir: Path
    limit: int | None = None
    xml_limit: int | None = None
    fiscal_number: str | None = None
    doc_types: tuple[str, ...] = ()
    from_dt: str | None = None
    to_dt: str | None = None
    include_xml: bool = True
    snapshot: bool = True
    keep_db_snapshot: bool = False
    curated_selection: bool = True
    selection_quota: int = 3


def _reject_in_tree_output(output_dir: Path) -> None:
    """Refuse an output dir inside the repo tree (never-transit rule, U2/F7).

    WebCheck exports contain real fiscal data (raw XML, MAC, sums, identifiers);
    they must never be written under the repo — even transiently. Point
    --output-dir at an outside-tree location (e.g. ~/webcheck_dumps/exports/<run>).
    """
    repo_root = Path(__file__).resolve().parents[1]
    if output_dir == repo_root or output_dir.is_relative_to(repo_root):
        raise SystemExit(
            f"--output-dir {output_dir} is inside the repo tree ({repo_root}); "
            "WebCheck exports must stay OUTSIDE the tree (never-transit rule). "
            "Use e.g. ~/webcheck_dumps/exports/<run>."
        )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Export WebCheck receipt examples from SQLite .db files and XML archives.",
    )
    parser.add_argument(
        "paths",
        nargs="+",
        help="WebCheck .db/.sqlite/.sqlite3/.xml file(s) or directories to scan recursively.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        required=True,
        help=(
            "REQUIRED. Output directory — MUST be OUTSIDE the repo tree "
            "(e.g. ~/webcheck_dumps/exports/<run>). WebCheck exports contain real "
            "fiscal data; the never-transit rule (U2 / re-check F7) forbids an "
            "in-tree default."
        ),
    )
    parser.add_argument("--limit", type=int, default=None, help="Maximum ksef rows per database.")
    parser.add_argument("--xml-limit", type=int, default=None, help="Maximum XML files to export.")
    parser.add_argument(
        "--selection-quota",
        type=int,
        default=3,
        help="Target number of samples per curated category. Default: 3.",
    )
    parser.add_argument(
        "--no-curated-selection",
        action="store_true",
        help="Do not create selected/ and selected_manifest.json.",
    )
    parser.add_argument("--fiscal-number", help="Filter by RRO fiscal number when available.")
    parser.add_argument(
        "--doc-type",
        action="append",
        default=[],
        help="Filter by WebCheck ksef.DocType. May be repeated, e.g. --doc-type 0 --doc-type 80.",
    )
    parser.add_argument("--from-dt", help="Filter ksef.dt >= value. Uses WebCheck string comparison.")
    parser.add_argument("--to-dt", help="Filter ksef.dt <= value. Uses WebCheck string comparison.")
    parser.add_argument("--no-xml", action="store_true", help="Do not include raw XML fields in JSON.")
    parser.add_argument(
        "--no-snapshot",
        action="store_true",
        help="Read the source database directly in SQLite read-only mode.",
    )
    parser.add_argument(
        "--keep-db-snapshot",
        action="store_true",
        help="Keep the SQLite snapshot in output_dir/_source_snapshots for audit.",
    )
    args = parser.parse_args(argv)

    output_dir = args.output_dir.expanduser().resolve()
    _reject_in_tree_output(output_dir)
    options = ExportOptions(
        output_dir=output_dir,
        limit=args.limit,
        xml_limit=args.xml_limit,
        fiscal_number=args.fiscal_number,
        doc_types=tuple(str(v) for v in args.doc_type),
        from_dt=args.from_dt,
        to_dt=args.to_dt,
        include_xml=not args.no_xml,
        snapshot=not args.no_snapshot,
        keep_db_snapshot=args.keep_db_snapshot,
        curated_selection=not args.no_curated_selection,
        selection_quota=max(1, args.selection_quota),
    )

    db_files, xml_files = discover_source_files([Path(p) for p in args.paths])
    if not db_files and not xml_files:
        raise SystemExit("No .db/.sqlite/.sqlite3 or .xml files found")

    manifest = export_sources(db_files, xml_files, options)
    print(json.dumps({
        "output_dir": str(options.output_dir),
        "database_count": len(db_files),
        "xml_file_count": len(xml_files),
        "sample_count": manifest["sample_count"],
    }, ensure_ascii=False, indent=2))
    return 0


def discover_source_files(paths: list[Path]) -> tuple[list[Path], list[Path]]:
    db_files: list[Path] = []
    xml_files: list[Path] = []
    for path in paths:
        if path.is_file():
            if path.suffix.lower() in {".db", ".sqlite", ".sqlite3"}:
                db_files.append(path)
            elif path.suffix.lower() == ".xml":
                xml_files.append(path)
            continue
        if path.is_dir():
            for p in path.rglob("*"):
                if not p.is_file():
                    continue
                suffix = p.suffix.lower()
                if suffix in {".db", ".sqlite", ".sqlite3"}:
                    db_files.append(p)
                elif suffix == ".xml":
                    xml_files.append(p)
    return (
        sorted(dict.fromkeys(p.resolve() for p in db_files)),
        sorted(dict.fromkeys(p.resolve() for p in xml_files)),
    )


def discover_db_files(paths: list[Path]) -> list[Path]:
    db_files, _xml_files = discover_source_files(paths)
    return db_files


def export_sources(db_files: list[Path], xml_files: list[Path], options: ExportOptions) -> dict[str, Any]:
    options.output_dir.mkdir(parents=True, exist_ok=True)
    samples_dir = options.output_dir / "samples"
    samples_dir.mkdir(parents=True, exist_ok=True)
    snapshot_dir = options.output_dir / "_source_snapshots"
    if options.keep_db_snapshot:
        snapshot_dir.mkdir(parents=True, exist_ok=True)

    manifest: dict[str, Any] = {
        "schema_version": 1,
        "created_at": datetime.now(UTC).isoformat(),
        "source": "WebCheck SQLite export",
        "notes": [
            "Generated for controlled pilot acceptance.",
            "Output may contain fiscal, organization, and customer-sensitive data.",
            "Store under var/ or another non-versioned location unless explicitly sanitized.",
        ],
        "filters": {
            "limit": options.limit,
            "xml_limit": options.xml_limit,
            "fiscal_number": options.fiscal_number,
            "doc_types": list(options.doc_types),
            "from_dt": options.from_dt,
            "to_dt": options.to_dt,
            "include_xml": options.include_xml,
        },
        "source_databases": [],
        "source_xml_files": [],
        "samples": [],
        "sample_count": 0,
    }

    with tempfile.TemporaryDirectory(prefix="webcheck_export_") as tmp:
        for db_path in db_files:
            readable_path = db_path
            kept_snapshot_path: Path | None = None
            if options.snapshot:
                target_dir = snapshot_dir if options.keep_db_snapshot else Path(tmp)
                readable_path = target_dir / f"{sanitize_filename(db_path.stem)}.snapshot.db"
                snapshot_sqlite_database(db_path, readable_path)
                kept_snapshot_path = readable_path if options.keep_db_snapshot else None

            db_entry = {
                "path": str(db_path),
                "snapshot_path": str(kept_snapshot_path) if kept_snapshot_path else None,
                "samples": 0,
                "errors": [],
            }
            try:
                exported = export_single_database(readable_path, db_path, samples_dir, options)
            except sqlite3.DatabaseError as exc:
                db_entry["errors"].append(f"sqlite error: {exc}")
                exported = []

            for sample in exported:
                rel_path = sample.pop("_output_path")
                manifest["samples"].append({
                    "sample_id": sample["sample_id"],
                    "path": str(rel_path),
                    "source_db": str(db_path),
                    "operation_type": sample["operation_type"],
                    "fiscal_number": sample.get("fiscal_number"),
                    "features": sample["features"],
                })
            db_entry["samples"] = len(exported)
            manifest["source_databases"].append(db_entry)

    xml_exported = export_xml_files(xml_files, samples_dir, options)
    for sample in xml_exported:
        rel_path = sample.pop("_output_path")
        manifest["samples"].append({
            "sample_id": sample["sample_id"],
            "path": str(rel_path),
            "source_xml": sample["source"]["xml_path"],
            "operation_type": sample["operation_type"],
            "fiscal_number": sample.get("fiscal_number"),
            "features": sample["features"],
        })
        manifest["source_xml_files"].append(sample["source"]["xml_path"])

    manifest["sample_count"] = len(manifest["samples"])
    (options.output_dir / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    if options.curated_selection:
        manifest["curated_selection"] = build_curated_selection(options.output_dir, manifest, options.selection_quota)
        (options.output_dir / "manifest.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
    return manifest


def export_databases(db_files: list[Path], options: ExportOptions) -> dict[str, Any]:
    return export_sources(db_files, [], options)


def build_curated_selection(output_dir: Path, manifest: dict[str, Any], quota: int) -> dict[str, Any]:
    selected_root = output_dir / "selected"
    if selected_root.exists():
        shutil.rmtree(selected_root)
    selected_root.mkdir(parents=True, exist_ok=True)

    samples = list(manifest.get("samples", []))
    source_coverage = build_source_coverage(samples)
    selected_ids: set[str] = set()
    source_pick_counts: dict[str, int] = {}
    categories: dict[str, Any] = {}

    for category in CURATED_CATEGORY_NAMES:
        candidates = [sample for sample in samples if category in classify_sample_categories(sample)]
        chosen = choose_category_samples(candidates, quota, selected_ids, source_pick_counts)
        category_dir = selected_root / category
        category_dir.mkdir(parents=True, exist_ok=True)
        selected_entries = []
        for sample in chosen:
            source_rel = Path(sample["path"])
            source_path = output_dir / source_rel
            target_path = category_dir / source_path.name
            if source_path.exists():
                shutil.copy2(source_path, target_path)
            selected_ids.add(sample["sample_id"])
            source_key = sample_source_key(sample)
            source_pick_counts[source_key] = source_pick_counts.get(source_key, 0) + 1
            selected_entries.append({
                "sample_id": sample["sample_id"],
                "source": source_key,
                "operation_type": sample["operation_type"],
                "fiscal_number": sample.get("fiscal_number"),
                "path": str(target_path.relative_to(output_dir)),
            })
        categories[category] = {
            "target": quota,
            "available": len(candidates),
            "selected": selected_entries,
            "missing": len(chosen) == 0,
        }

    selection = {
        "created_at": datetime.now(UTC).isoformat(),
        "quota_per_category": quota,
        "category_order": list(CURATED_CATEGORY_NAMES),
        "source_coverage": source_coverage,
        "categories": categories,
        "missing_categories": [
            name for name, data in categories.items()
            if data["missing"]
        ],
    }
    (output_dir / "selected_manifest.json").write_text(
        json.dumps(selection, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return selection


def choose_category_samples(
    candidates: list[dict[str, Any]],
    quota: int,
    selected_ids: set[str],
    source_pick_counts: dict[str, int],
) -> list[dict[str, Any]]:
    chosen: list[dict[str, Any]] = []

    def sort_key(sample: dict[str, Any]) -> tuple[int, str, str]:
        source_key = sample_source_key(sample)
        return (source_pick_counts.get(source_key, 0), source_key, sample["sample_id"])

    for sample in sorted(candidates, key=sort_key):
        if len(chosen) >= quota:
            return chosen
        if sample["sample_id"] in selected_ids:
            continue
        chosen.append(sample)

    if len(chosen) < quota:
        chosen_ids = {sample["sample_id"] for sample in chosen}
        for sample in sorted(candidates, key=sort_key):
            if len(chosen) >= quota:
                break
            if sample["sample_id"] not in chosen_ids:
                chosen.append(sample)
                chosen_ids.add(sample["sample_id"])
    return chosen


def build_source_coverage(samples: list[dict[str, Any]]) -> dict[str, Any]:
    coverage: dict[str, Any] = {}
    for sample in samples:
        source_key = sample_source_key(sample)
        entry = coverage.setdefault(source_key, {
            "sample_count": 0,
            "operations": {},
            "features": {},
            "fiscal_numbers": {},
        })
        entry["sample_count"] += 1
        op = sample.get("operation_type") or "UNKNOWN"
        entry["operations"][op] = entry["operations"].get(op, 0) + 1
        fn = sample.get("fiscal_number") or "UNKNOWN"
        entry["fiscal_numbers"][fn] = entry["fiscal_numbers"].get(fn, 0) + 1
        for feature, enabled in (sample.get("features") or {}).items():
            if enabled:
                entry["features"][feature] = entry["features"].get(feature, 0) + 1
    return coverage


def classify_sample_categories(sample: dict[str, Any]) -> set[str]:
    op = sample.get("operation_type")
    features = sample.get("features") or {}
    categories: set[str] = set()
    if "source_db" in sample:
        categories.add("db_backed")
    if "source_xml" in sample:
        categories.add("xml_backed")
    if features.get("offline"):
        categories.add("offline")
    if op == "SELL":
        if not features.get("has_excise"):
            categories.add("sell_without_excise")
        if (
            features.get("has_goods")
            and not features.get("has_excise")
            and not features.get("has_uktzed")
            and not features.get("has_discounts")
            and not features.get("mixed_payment")
        ):
            categories.add("sell_plain")
        if features.get("has_uktzed"):
            categories.add("sell_with_uktzed")
        if features.get("has_excise"):
            categories.add("sell_with_excise")
        if features.get("has_discounts"):
            categories.add("sell_with_discount")
        if features.get("mixed_payment"):
            categories.add("sell_mixed_payment")
    elif op == "RETURN":
        categories.add("return")
    elif op == "SERVICE_IN":
        categories.add("service_in")
    elif op == "SERVICE_OUT":
        categories.add("service_out")
    elif op == "Z_REPORT":
        categories.add("z_report")
    return categories


def sample_source_key(sample: dict[str, Any]) -> str:
    return str(sample.get("source_db") or sample.get("source_xml") or "unknown")


def export_xml_files(xml_files: list[Path], samples_dir: Path, options: ExportOptions) -> list[dict[str, Any]]:
    selected = xml_files[: options.xml_limit] if options.xml_limit is not None else xml_files
    samples: list[dict[str, Any]] = []
    for xml_path in selected:
        text, encoding = read_xml_text(xml_path)
        xml_summary = parse_webcheck_xml(text)
        ksef_like = {
            "checkxml": text,
            "checkidficscal": (xml_summary.get("dat") or {}).get("FN"),
            "DocType": xml_summary.get("command_type"),
            "sum": (xml_summary.get("totals") or {}).get("SM"),
            "dt": xml_summary.get("timestamp"),
            "offline": None,
        }
        operation_type = infer_operation_type(ksef_like, xml_summary)
        normalized_receipt = normalize_receipt(ksef_like, None, [], [], [], [], xml_summary)
        features = detect_features(ksef_like, normalized_receipt, xml_summary, [])
        sample = {
            "schema_version": 1,
            "sample_id": make_xml_sample_id(xml_path, operation_type),
            "source": {
                "system": "WebCheck XML",
                "xml_path": str(xml_path),
                "encoding": encoding,
                "sha256": hashlib.sha256(text.encode("utf-8", errors="surrogatepass")).hexdigest(),
            },
            "operation_type": operation_type,
            "fiscal_number": ksef_like["checkidficscal"],
            "webcheck": {
                "doc_type": ksef_like["DocType"],
                "local_check_number": (xml_summary.get("totals") or {}).get("NO"),
                "shift_id": None,
                "offline": None,
                "dt": ksef_like["dt"],
                "sum": ksef_like["sum"],
                "mac": None,
            },
            "features": features,
            "normalized_receipt": normalized_receipt,
            "xml_summary": xml_summary if options.include_xml else without_raw_xml(xml_summary),
            "raw_tables": {},
        }
        if options.fiscal_number and sample.get("fiscal_number") != options.fiscal_number:
            continue
        output_path = samples_dir / sample_filename(xml_path, sample)
        output_path.write_text(
            json.dumps(sample, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        sample["_output_path"] = output_path.relative_to(samples_dir.parent)
        samples.append(sample)
    return samples


def snapshot_sqlite_database(source_path: Path, snapshot_path: Path) -> None:
    uri = f"{source_path.resolve().as_uri()}?mode=ro"
    with sqlite3.connect(uri, uri=True) as src:
        with sqlite3.connect(snapshot_path) as dst:
            src.backup(dst)


def export_single_database(
    readable_db_path: Path,
    original_db_path: Path,
    samples_dir: Path,
    options: ExportOptions,
) -> list[dict[str, Any]]:
    with sqlite3.connect(readable_db_path) as conn:
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA query_only = ON")
        if not table_exists(conn, "ksef"):
            return []

        ksef_rows = select_ksef_rows(conn, options)
        db_columns = {
            table: table_columns(conn, table)
            for table in ("CHECKHEAD", "CHECKBODY", "CHECKPAY", "CHECKTAX", "CHECKEXCISE", "SHIFTS")
            if table_exists(conn, table)
        }
        samples: list[dict[str, Any]] = []
        for row in ksef_rows:
            sample = build_sample(conn, original_db_path, row, db_columns, options)
            if options.fiscal_number and sample.get("fiscal_number") != options.fiscal_number:
                continue
            filename = sample_filename(original_db_path, sample)
            output_path = samples_dir / filename
            output_path.write_text(
                json.dumps(sample, ensure_ascii=False, indent=2) + "\n",
                encoding="utf-8",
            )
            sample["_output_path"] = output_path.relative_to(samples_dir.parent)
            samples.append(sample)
        return samples


def table_exists(conn: sqlite3.Connection, table: str) -> bool:
    row = conn.execute(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND lower(name) = lower(?)",
        (table,),
    ).fetchone()
    return row is not None


def table_columns(conn: sqlite3.Connection, table: str) -> set[str]:
    return {str(row[1]).lower() for row in conn.execute(f"PRAGMA table_info({quote_ident(table)})")}


def select_ksef_rows(conn: sqlite3.Connection, options: ExportOptions) -> list[sqlite3.Row]:
    where = ["1 = 1"]
    params: list[Any] = []
    if options.doc_types:
        placeholders = ", ".join("?" for _ in options.doc_types)
        where.append(f"CAST(DocType AS TEXT) IN ({placeholders})")
        params.extend(options.doc_types)
    if options.from_dt:
        where.append("dt >= ?")
        params.append(options.from_dt)
    if options.to_dt:
        where.append("dt <= ?")
        params.append(options.to_dt)

    sql = f"SELECT * FROM ksef WHERE {' AND '.join(where)} ORDER BY ID DESC"
    if options.limit is not None:
        sql += " LIMIT ?"
        params.append(options.limit)
    rows = list(conn.execute(sql, params))
    return list(reversed(rows))


def build_sample(
    conn: sqlite3.Connection,
    original_db_path: Path,
    ksef_row: sqlite3.Row,
    db_columns: dict[str, set[str]],
    options: ExportOptions,
) -> dict[str, Any]:
    ksef = row_to_dict(ksef_row)
    checkhead = find_checkhead(conn, ksef, db_columns.get("CHECKHEAD", set()))
    checkhead_id = checkhead.get("ID") or checkhead.get("id") if checkhead else None

    checkbody = select_child_rows(conn, "CHECKBODY", checkhead_id) if checkhead_id else []
    checkpay = select_child_rows(conn, "CHECKPAY", checkhead_id) if checkhead_id else []
    checktax = select_child_rows(conn, "CHECKTAX", checkhead_id) if checkhead_id else []
    checkexcise = select_child_rows(conn, "CHECKEXCISE", checkhead_id) if checkhead_id else []
    shift = find_shift(conn, ksef, db_columns.get("SHIFTS", set()))

    xml_text = first_non_empty(ksef.get("checkxml"), ksef.get("checksigned"))
    xml_summary = parse_webcheck_xml(xml_text)
    operation_type = infer_operation_type(ksef, xml_summary)
    normalized_receipt = normalize_receipt(ksef, checkhead, checkbody, checkpay, checktax, checkexcise, xml_summary)
    features = detect_features(ksef, normalized_receipt, xml_summary, checkexcise)
    sample_id = make_sample_id(original_db_path, ksef, operation_type)

    raw: dict[str, Any] = {
        "ksef": maybe_without_xml(ksef, include_xml=options.include_xml),
        "checkhead": checkhead,
        "checkbody": checkbody,
        "checkpay": checkpay,
        "checktax": checktax,
        "checkexcise": checkexcise,
        "shift": shift,
    }

    return {
        "schema_version": 1,
        "sample_id": sample_id,
        "source": {
            "system": "WebCheck",
            "database_path": str(original_db_path),
            "ksef_id": ksef.get("ID") or ksef.get("id"),
            "checkhead_id": checkhead_id,
        },
        "operation_type": operation_type,
        "fiscal_number": (xml_summary.get("dat") or {}).get("FN") or (checkhead or {}).get("FN") or ksef.get("checkidficscal"),
        "webcheck": {
            "doc_type": ksef.get("DocType") or ksef.get("doctype"),
            "local_check_number": ksef.get("localchecknumber"),
            "shift_id": ksef.get("shiftid"),
            "offline": ksef.get("offline"),
            "dt": ksef.get("dt"),
            "sum": ksef.get("sum"),
            "mac": ksef.get("mac"),
        },
        "features": features,
        "normalized_receipt": normalized_receipt,
        "xml_summary": xml_summary if options.include_xml else without_raw_xml(xml_summary),
        "raw_tables": raw,
    }


def find_checkhead(
    conn: sqlite3.Connection,
    ksef: dict[str, Any],
    columns: set[str],
) -> dict[str, Any] | None:
    if not columns:
        return None

    candidates: list[tuple[str, tuple[Any, ...]]] = []
    fiscal_id = ksef.get("checkidficscal")
    if fiscal_id and "ordertaxnum" in columns:
        candidates.append(("SELECT * FROM CHECKHEAD WHERE ORDERTAXNUM = ? ORDER BY ID DESC LIMIT 1", (fiscal_id,)))

    local_no = ksef.get("localchecknumber")
    shift_id = ksef.get("shiftid")
    if local_no is not None and shift_id is not None and {"ordernum", "shiftid"} <= columns:
        candidates.append((
            "SELECT * FROM CHECKHEAD WHERE CAST(ORDERNUM AS TEXT) = CAST(? AS TEXT) "
            "AND CAST(SHIFTID AS TEXT) = CAST(? AS TEXT) ORDER BY ID DESC LIMIT 1",
            (local_no, shift_id),
        ))

    checkid = ksef.get("checkid")
    if checkid and "uid" in columns:
        candidates.append(("SELECT * FROM CHECKHEAD WHERE UID = ? ORDER BY ID DESC LIMIT 1", (checkid,)))

    for sql, params in candidates:
        row = conn.execute(sql, params).fetchone()
        if row is not None:
            return row_to_dict(row)
    return None


def find_shift(conn: sqlite3.Connection, ksef: dict[str, Any], columns: set[str]) -> dict[str, Any] | None:
    shift_id = ksef.get("shiftid")
    if not columns or shift_id is None:
        return None
    row = conn.execute(
        "SELECT * FROM SHIFTS WHERE CAST(ID AS TEXT) = CAST(? AS TEXT) LIMIT 1",
        (shift_id,),
    ).fetchone()
    return row_to_dict(row) if row is not None else None


def select_child_rows(conn: sqlite3.Connection, table: str, checkhead_id: Any) -> list[dict[str, Any]]:
    if checkhead_id is None or not table_exists(conn, table):
        return []
    return [
        row_to_dict(row)
        for row in conn.execute(
            f"SELECT * FROM {quote_ident(table)} WHERE CAST(CHECKID AS TEXT) = CAST(? AS TEXT) ORDER BY ID",
            (checkhead_id,),
        )
    ]


def parse_webcheck_xml(xml_text: Any) -> dict[str, Any]:
    text = str(xml_text or "").strip()
    summary: dict[str, Any] = {
        "present": bool(text),
        "parse_error": None,
        "dat": {},
        "timestamp": None,
        "command_type": None,
        "goods": [],
        "payments": [],
        "taxes": [],
        "discounts": [],
        "service_io": [],
        "z_report": {},
    }
    if not text:
        return summary

    summary["raw_xml"] = text
    parse_text = re.sub(r"^\s*<\?xml[^>]*\?>", "", text, count=1)
    try:
        root = ET.fromstring(parse_text)
    except ET.ParseError as exc:
        summary["parse_error"] = str(exc)
        return summary

    dat = root.find(".//DAT")
    if dat is not None:
        summary["dat"] = dict(dat.attrib)

    ts = root.find(".//TS")
    if ts is not None and ts.text:
        summary["timestamp"] = ts.text.strip()

    command = root.find(".//C")
    if command is not None:
        summary["command_type"] = command.attrib.get("T")
        for elem in command:
            if elem.tag == "P":
                good = dict(elem.attrib)
                good["excise_marks"] = [dict(child.attrib) | {"text": child.text} for child in elem if child.tag == "CA"]
                summary["goods"].append(good)
            elif elem.tag == "M":
                summary["payments"].append(dict(elem.attrib))
            elif elem.tag in {"D", "S"}:
                discount = dict(elem.attrib)
                discount["kind"] = "DISCOUNT" if elem.tag == "D" else "SURCHARGE"
                summary["discounts"].append(discount)
            elif elem.tag == "E":
                summary["totals"] = dict(elem.attrib)
                summary["taxes"].extend(dict(child.attrib) for child in elem if child.tag == "TX")
            elif elem.tag in {"I", "O"}:
                item = dict(elem.attrib)
                item["kind"] = "SERVICE_IN" if elem.tag == "I" else "SERVICE_OUT"
                summary["service_io"].append(item)

    z_elem = root.find(".//Z")
    if z_elem is not None:
        summary["z_report"] = {
            "attributes": dict(z_elem.attrib),
            "taxes": [dict(child.attrib) for child in z_elem if child.tag == "TXS"],
            "payments": [dict(child.attrib) for child in z_elem if child.tag == "M"],
            "counts": [dict(child.attrib) for child in z_elem if child.tag == "NC"],
            "service_io": [dict(child.attrib) for child in z_elem if child.tag == "IO"],
            "epz": [dict(child.attrib) for child in z_elem if child.tag == "EPZ"],
        }
    return summary


def infer_operation_type(ksef: dict[str, Any], xml_summary: dict[str, Any]) -> str:
    command_type = normalize_doc_type(xml_summary.get("command_type"))
    if command_type == "2":
        service_io = xml_summary.get("service_io") or []
        kinds = {item.get("kind") for item in service_io}
        if "SERVICE_IN" in kinds and "SERVICE_OUT" not in kinds:
            return "SERVICE_IN"
        if "SERVICE_OUT" in kinds and "SERVICE_IN" not in kinds:
            return "SERVICE_OUT"
    if command_type in XML_COMMAND_MAP:
        return XML_COMMAND_MAP[command_type]

    doc_type = normalize_doc_type(ksef.get("DocType") or ksef.get("doctype"))
    if doc_type in DOC_TYPE_MAP:
        return DOC_TYPE_MAP[doc_type]
    if (xml_summary.get("z_report") or {}).get("attributes"):
        return "Z_REPORT"
    return "UNKNOWN"


def normalize_doc_type(value: Any) -> str | None:
    if value is None:
        return None
    text = str(value).strip()
    if text.startswith("-") and text[1:].isdigit():
        text = text[1:]
    return text


def normalize_receipt(
    ksef: dict[str, Any],
    checkhead: dict[str, Any] | None,
    checkbody: list[dict[str, Any]],
    checkpay: list[dict[str, Any]],
    checktax: list[dict[str, Any]],
    checkexcise: list[dict[str, Any]],
    xml_summary: dict[str, Any],
) -> dict[str, Any]:
    goods = [
        {
            "source": "CHECKBODY",
            "code": row.get("CODE"),
            "uktzed": row.get("UKTZED"),
            "name": row.get("GOODSNAME"),
            "quantity": row.get("AMOUNT"),
            "price": row.get("PRICE"),
            "tax_group": row.get("LETTER"),
            "sum": row.get("COST"),
            "letter_excise": row.get("LETTEREXCISE"),
        }
        for row in checkbody
    ]
    if not goods:
        for row in xml_summary.get("goods", []):
            goods.append({
                "source": "XML",
                "code": row.get("C"),
                "uktzed": row.get("CZD"),
                "name": row.get("NM"),
                "quantity": row.get("Q"),
                "price": row.get("PRC"),
                "tax_group": row.get("TX"),
                "sum": row.get("SM"),
                "excise_marks": row.get("excise_marks", []),
            })

    payments = [
        {
            "source": "CHECKPAY",
            "payment_form": row.get("PAYMENTFORM"),
            "sum": row.get("TOTALSUM"),
        }
        for row in checkpay
    ]
    if not payments:
        for row in xml_summary.get("payments", []):
            payments.append({
                "source": "XML",
                "payment_type": row.get("T"),
                "name": row.get("NM"),
                "sum": row.get("SM"),
            })

    taxes = [
        {
            "source": "CHECKTAX",
            "tax_code": row.get("TAXCODE"),
            "tax_percent": row.get("TAXPRC"),
            "tax_sum": row.get("TAXSUM"),
        }
        for row in checktax
    ]
    if not taxes:
        for row in xml_summary.get("taxes", []):
            taxes.append({
                "source": "XML",
                "tax_code": row.get("TX"),
                "tax_percent": row.get("TXPR"),
                "tax_sum": row.get("TXSM") or row.get("TXI") or row.get("TXO"),
                "raw": row,
            })

    total = (checkhead or {}).get("TOTALSUM") or ksef.get("sum") or (xml_summary.get("totals") or {}).get("SM")
    return {
        "total": total,
        "total_kopecks": money_to_kopecks(total),
        "goods": goods,
        "payments": payments,
        "taxes": taxes,
        "excise": checkexcise,
        "discounts": xml_summary.get("discounts", []),
        "service_io": xml_summary.get("service_io", []),
        "z_report": xml_summary.get("z_report", {}),
    }


def detect_features(
    ksef: dict[str, Any],
    receipt: dict[str, Any],
    xml_summary: dict[str, Any],
    checkexcise: list[dict[str, Any]],
) -> dict[str, bool]:
    goods = receipt.get("goods", [])
    payments = receipt.get("payments", [])
    xml_goods = xml_summary.get("goods", [])
    has_xml_excise = any(good.get("excise_marks") for good in xml_goods)
    return {
        "offline": str(ksef.get("offline") or "0") not in {"", "0", "None"},
        "has_uktzed": any(bool(good.get("uktzed")) for good in goods),
        "has_excise": bool(checkexcise) or has_xml_excise,
        "has_discounts": bool(receipt.get("discounts")),
        "mixed_payment": len(payments) > 1,
        "has_goods": bool(goods),
        "has_z_report": bool((receipt.get("z_report") or {}).get("attributes")),
        "has_service_io": bool(receipt.get("service_io")),
    }


def money_to_kopecks(value: Any) -> int | None:
    if value is None or str(value).strip() == "":
        return None
    text = str(value).strip().replace(",", ".")
    try:
        dec = Decimal(text)
    except InvalidOperation:
        return None
    return int((dec * Decimal("100")).quantize(Decimal("1"), rounding=ROUND_HALF_UP))


def maybe_without_xml(row: dict[str, Any], *, include_xml: bool) -> dict[str, Any]:
    if include_xml:
        return row
    return {
        key: value
        for key, value in row.items()
        if key.lower() not in {"checkxml", "checksigned", "signedanswerfromficscal"}
    }


def without_raw_xml(xml_summary: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in xml_summary.items() if key != "raw_xml"}


def row_to_dict(row: sqlite3.Row) -> dict[str, Any]:
    return {key: row[key] for key in row.keys()}


def first_non_empty(*values: Any) -> Any:
    for value in values:
        if value is not None and str(value).strip():
            return value
    return None


def read_xml_text(path: Path) -> tuple[str, str]:
    data = path.read_bytes()
    for encoding in ("utf-8-sig", "cp1251", "windows-1251"):
        try:
            return data.decode(encoding), encoding
        except UnicodeDecodeError:
            continue
    return data.decode("utf-8", errors="replace"), "utf-8-replace"


def sample_filename(db_path: Path, sample: dict[str, Any]) -> str:
    return f"{sanitize_filename(sample['sample_id'])}.json"


def make_sample_id(db_path: Path, ksef: dict[str, Any], operation_type: str) -> str:
    ksef_id = ksef.get("ID") or ksef.get("id") or "unknown"
    return f"{sanitize_filename(db_path.stem)}__ksef_{ksef_id}__{operation_type.lower()}"


def make_xml_sample_id(xml_path: Path, operation_type: str) -> str:
    digest = hashlib.sha256(str(xml_path.resolve()).encode("utf-8", errors="surrogatepass")).hexdigest()[:10]
    return f"xml__{sanitize_filename(xml_path.stem)}__{digest}__{operation_type.lower()}"


def sanitize_filename(value: Any) -> str:
    text = str(value)
    text = re.sub(r"[^A-Za-z0-9_.-]+", "_", text)
    return text.strip("._") or "sample"


def quote_ident(identifier: str) -> str:
    return '"' + identifier.replace('"', '""') + '"'


if __name__ == "__main__":
    raise SystemExit(main())
