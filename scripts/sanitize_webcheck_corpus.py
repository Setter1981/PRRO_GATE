#!/usr/bin/env python3
"""Sanitize a WebCheck dump SLICE into a synthetic, hash-recomputed corpus fixture.

WebCheck U2 (Phase-2). This is the privacy boundary of the corpus pipeline:

    export (outside-tree, real data) -> [SANITIZE] -> synthetic fixture -> scan -> commit

The sanitizer is **allow-list by construction**: it reads ONLY the abstract SHAPE
of a dump slice (op-type sequence, shift boundaries, offline-code pattern, the
U0 lnd-translation) and emits a FULLY SYNTHETIC `CanonicalFiscalCommand` sequence.
It never copies a dump field through — no amounts, names, times, ids, hashes, or
raw XML cross into a fixture. Every hash is recomputed over the synthetic bytes.

Data discipline:
  - `extract_shape()` reads a dump SNAPSHOT **read-only** (`mode=ro`), selecting
    ONLY structural codes (DocType / offline / shiftid) in `ksef.ID ASC` order —
    never content (checkxml / sum / mac / dt / identifiers).
  - `synthesize_*()` are PURE and DATA-FREE (a synthetic shape in, a synthetic
    fixture out) — the whole synthesis path is unit-testable without any dump.

LOCKED synthetic constants (CP1 decision 3): FN = 10-digit numeric, prefix 9
(dumps are 4…, demo 7… -> a clean scanner rule); epoch 2026-01-01, 60 s buckets;
cashier-NN; SYNTH goods; 5000-kop amounts.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sqlite3
from dataclasses import dataclass
from pathlib import Path

# ── LOCKED synthetic constants (CP1 decision 3) ──────────────────────────────
SYNTH_FN = "9000000001"  # 10-digit numeric, prefix 9 — never a real dump FN
SYNTH_EPOCH = "2026-01-01T00:00:00Z"
SYNTH_BUCKET_SECONDS = 60
SYNTH_CASHIER = "cashier-01"
SYNTH_GOOD_NAME = "Synthetic good"
SYNTH_GOOD_CODE = "SYNTH"
SYNTH_UNIT_KOP = 5000

SCHEMA_VERSION = "1.0.1"

# ── WebCheck DocType -> our op-type (U0 §1/§3; CP1 decision 2 DROP 10/12) ─────
# Sell DocTypes per U0 (SendingOfflineChecks TypToPROTO) and shift-control codes.
_DOCTYPE_OP = {
    8: "SHIFT_OPEN",
    80: "SHIFT_CLOSE",
    0: "SELL",
    1: "SELL",
    3: "SELL",
    4: "SELL",
    # 9 = type-9 offline-control (drain/close boundary, U0) -> shape-DRAIN marker
    9: "DRAIN",
    # 10, 12: NOT in the U0 lifecycle tables -> DROP (CP1 decision 2, U0-gate:
    # cannot rely on a behavior absent from U0; extend U0 first if needed).
}
DROPPED_DOCTYPES = (10, 12)


def map_doctype(doc_type: int) -> str | None:
    """Op-type for a WebCheck DocType, or None if intentionally DROPPED."""
    return _DOCTYPE_OP.get(int(doc_type))


def offline_class(offline: int) -> str:
    """U0 §2 offline-lifecycle CLASS (abstract label, never the raw value)."""
    return {
        0: "online",
        1: "offline_drained",
        2: "offline_pending",
        3: "offline_transitional",
        -1: "cancelled",
    }.get(int(offline), "unknown")


@dataclass(frozen=True)
class ShapeRecord:
    """The ONLY things that cross from a dump doc: structural codes (no content)."""

    doc_type: int
    offline: int
    shiftid: int


def _canonical_json(payload: dict) -> str:
    """Deterministic canonical serialization (recursively sorted, compact) — the
    bytes the recomputed hash is taken over (matches the golden webcheck_* form)."""
    return json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def _synth_ts(seq_pos: int) -> str:
    """A synthetic monotone time-bucket keyed on sequence position (never a real
    dump timestamp). Bucket granularity is `SYNTH_BUCKET_SECONDS`."""
    from datetime import datetime, timedelta, timezone

    base = datetime(2026, 1, 1, tzinfo=timezone.utc)
    ts = base + timedelta(seconds=seq_pos * SYNTH_BUCKET_SECONDS)
    return ts.strftime("%Y-%m-%dT%H:%M:%SZ")


def synthesize_command(op_type: str, lnd: int, seq_pos: int) -> dict:
    """A single FULLY SYNTHETIC CanonicalFiscalCommand for one op.

    PURE: derives nothing from any dump field — only from (op_type, lnd, seq_pos).
    `payload_sha256` is recomputed over the synthetic `payload_json`."""
    payload: dict
    if op_type == "SELL":
        payload = {
            "method": "SellCheck",
            "currency": "UAH",
            "cashier_id": SYNTH_CASHIER,
            "goods_count": 1,
            "payments_count": 1,
            "receipt": {
                "type": "SELL",
                "goods": [
                    {
                        "item_id": f"item-{lnd}",
                        "item_no": 1,
                        "code": SYNTH_GOOD_CODE,
                        "name": SYNTH_GOOD_NAME,
                        "price": SYNTH_UNIT_KOP,
                        "quantity": 1000,
                        "sum": SYNTH_UNIT_KOP,
                    }
                ],
                "payments": [
                    {
                        "payment_id": "payment-1",
                        "payment_type": "CASH",
                        "label": "CASH",
                        "amount": SYNTH_UNIT_KOP,
                    }
                ],
                "totals": {"total_sum": SYNTH_UNIT_KOP},
            },
        }
    else:  # SHIFT_OPEN / SHIFT_CLOSE / DRAIN — control ops carry a minimal payload
        payload = {"method": op_type, "cashier_id": SYNTH_CASHIER}

    payload_json = _canonical_json(payload)
    payload_sha256 = hashlib.sha256(payload_json.encode("utf-8")).hexdigest()
    return {
        "schema_version": SCHEMA_VERSION,
        "request_id": f"cmd-{seq_pos:04d}",
        "idempotency_key": f"{op_type.lower()}:{SYNTH_FN}:{seq_pos:04d}",
        "protocol": "WEBCHECK_XMLRPC",
        "operation_type": op_type if op_type == "SELL" else op_type,
        "fiscal_number": SYNTH_FN,
        "business_ts": _synth_ts(seq_pos),
        "payload": payload,
        "payload_json": payload_json,
        "payload_sha256": payload_sha256,
    }


def synthesize_fixture(shape: list[ShapeRecord], name: str, replay: bool = True) -> dict:
    """SHAPE (ordered structural codes) -> a synthetic fixture (sequence + observables).

    PURE / DATA-FREE. Applies the U0 lnd-translation: dense per-FN lnd = 1,2,3,…
    over the shape in `ksef.ID ASC` order (the caller supplies the shape in that
    order). DROPPED DocTypes (10/12, decision 2) are skipped (honest-drop)."""
    sequence: list[dict] = []
    lnd = 0
    dropped = 0
    codes_consumed = 0
    for seq_pos, rec in enumerate(shape):
        op = map_doctype(rec.doc_type)
        if op is None:
            dropped += 1
            continue
        cls = offline_class(rec.offline)
        # offline-ISSUE consumes a code (U0/A07: fnsupdate10 at INSERT offline=2;
        # here the drained/pending offline sells stand for that consumption).
        if op == "SELL" and cls in ("offline_pending", "offline_drained"):
            codes_consumed += 1
        lnd += 1
        cmd = synthesize_command(op, lnd, seq_pos)
        sequence.append(
            {
                "op_index": len(sequence),
                "operation_type": op,
                "lnd": lnd,
                "offline_lifecycle_class": cls,
                # abstract DPS outcome class for U3's ScriptedDps (never a real blob)
                "dps_outcome_class": "OfflineAck" if cls != "online" else "Ack",
                "canonical_command": cmd,
            }
        )

    # Chain over the RECOMPUTED synthetic hashes (never a real ksef.mac).
    chain: list[str | None] = []
    prev: str | None = None
    for op in sequence:
        chain.append(prev)
        prev = op["canonical_command"]["payload_sha256"]

    observables = {
        "lnd_sequence": [op["lnd"] for op in sequence],
        "state_class_sequence": [op["offline_lifecycle_class"] for op in sequence],
        "offline_codes_consumed": codes_consumed,
        "previous_hash_chain": chain,
        "issued_lnds": [op["lnd"] for op in sequence if op["operation_type"] == "SELL"],
        "dropped_doctype_count": dropped,
    }
    return {
        "corpus_schema_version": 1,
        "shape_name": name,
        "fiscal_number": SYNTH_FN,
        "synthetic": True,
        "replay": replay,
        "sequence": sequence,
        "expected_observables": observables,
    }


def extract_shape(
    snapshot_db: Path, limit: int | None = None, shift_id: int | None = None
) -> list[ShapeRecord]:
    """Read a dump SNAPSHOT read-only for the SHAPE ONLY (structural codes, no
    content), in `ksef.ID ASC` order (U0 §4 lnd-translation ordering key).  With
    `shift_id`, restrict to ONE shift (a clean single-shift slice — CP1 decision)."""
    uri = f"{snapshot_db.resolve().as_uri()}?mode=ro"
    where = "" if shift_id is None else f" WHERE shiftid = {int(shift_id)}"
    limit_sql = f" LIMIT {int(limit)}" if limit else ""
    with sqlite3.connect(uri, uri=True) as conn:
        rows = conn.execute(
            f"SELECT DocType, offline, shiftid FROM ksef{where} ORDER BY ID ASC{limit_sql}"
        ).fetchall()
    return [ShapeRecord(int(dt or 0), int(off or 0), int(sh or 0)) for dt, off, sh in rows]


def write_fixture(fixture: dict, out_dir: Path) -> Path:
    """Write a fixture dir (sequence.json + expected_observables.json + SHAPE.md)."""
    fx_dir = out_dir / fixture["shape_name"]
    fx_dir.mkdir(parents=True, exist_ok=True)
    (fx_dir / "sequence.json").write_text(
        json.dumps(
            {k: fixture[k] for k in ("corpus_schema_version", "shape_name", "fiscal_number", "synthetic", "replay", "sequence")},
            ensure_ascii=True,
            indent=2,
        )
        + "\n"
    )
    (fx_dir / "expected_observables.json").write_text(
        json.dumps(fixture["expected_observables"], ensure_ascii=True, indent=2) + "\n"
    )
    (fx_dir / "SHAPE.md").write_text(
        f"# {fixture['shape_name']}\n\n"
        f"Synthetic WebCheck corpus fixture (U2). {len(fixture['sequence'])} ops; "
        f"{fixture['expected_observables']['offline_codes_consumed']} offline code(s) consumed; "
        f"{fixture['expected_observables']['dropped_doctype_count']} dropped DocType(s) (10/12, "
        f"CP1 decision 2 — not in U0 tables).\n\n"
        "Provenance-free: no FN, no counts that identify. All content synthetic; "
        "every hash recomputed over synthetic bytes.\n"
    )
    return fx_dir


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Sanitize a WebCheck dump slice into a synthetic corpus fixture.")
    parser.add_argument("--shape-name", required=True, help="Fixture shape name, e.g. online_sell_run.")
    parser.add_argument("--out-dir", type=Path, required=True, help="Output dir (MUST be OUTSIDE the repo tree for the raw stage).")
    src = parser.add_mutually_exclusive_group(required=True)
    src.add_argument("--snapshot", type=Path, help="Read the SHAPE from this read-only dump snapshot.")
    src.add_argument("--shape-json", type=Path, help="Read a synthetic SHAPE (list of {doc_type,offline,shiftid}) — for tests.")
    parser.add_argument("--limit", type=int, default=None, help="Max ksef rows (slice bound).")
    parser.add_argument("--shift-id", type=int, default=None, help="Restrict to a single shift (clean single-shift slice).")
    parser.add_argument("--no-replay", action="store_true", help="Flag the fixture exported-but-NOT-replayed (Z shapes, A2/MED#8).")
    args = parser.parse_args(argv)

    if args.snapshot is not None:
        shape = extract_shape(args.snapshot, args.limit, args.shift_id)
    else:
        raw = json.loads(args.shape_json.read_text())
        shape = [ShapeRecord(r["doc_type"], r["offline"], r["shiftid"]) for r in raw]

    fixture = synthesize_fixture(shape, args.shape_name, replay=not args.no_replay)
    fx_dir = write_fixture(fixture, args.out_dir)
    print(json.dumps({"shape_name": fixture["shape_name"], "ops": len(fixture["sequence"]), "out": str(fx_dir)}))
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
