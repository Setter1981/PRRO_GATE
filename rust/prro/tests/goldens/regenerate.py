#!/usr/bin/env python3
"""W4-C2 manual capture script — Python-serializer = oracle.

Per ADR-M2-3 + W4-C0 contract: the goldens' oracle is the Python
serializer at ``src/prro_gateway/serializers/dps_xml.py``.  Rust
does NOT freeze its own output as the golden — Rust is the candidate
that must match these Python-captured bytes.

This script is **manual-only**.  CI does NOT run it.  Re-capture is
a deliberate-spec-change action; every re-capture goes through
manual review of the new bytes + manifest diff.

Usage:

    cd /path/to/PRRO_GATE
    python3 rust/prro/tests/goldens/regenerate.py

    # Outputs land beside the script in the appropriate subdir:
    #   xml/shift_open.bin
    #   xml/sell.bin
    #   xml/return.bin
    #   xml/z_report.bin
    #   cms/deterministic_prefix.bin
    #   prevhash/seed.bin
    #   manifest.json   (sha256 + length per file; reviewer artefact)

    # Then review the diff against the previously-committed bytes:
    git diff rust/prro/tests/goldens/

    # If the diff is intended (deliberate spec change), commit.
    # Otherwise revert and investigate the source of drift.

The script is intentionally narrow:

  - hard-coded fixture payloads chosen to exercise the W4 first-round
    subset (the same subset the Rust builder ships in C1: items +
    payments + closing E for SELL/RETURN, M-summary + NC for
    Z_REPORT, no excise/discounts/header/footer/tax_groups);
  - cp1251 encoding via ``str.encode('cp1251')`` — the same
    encoding the Python serializer's downstream signing path
    expects;
  - manifest.json carries sha256 + length per file so a reviewer
    can spot any unintended drift even before opening the .bin.
"""
from __future__ import annotations

import hashlib
import json
import sys
from datetime import datetime, UTC
from pathlib import Path

# Discover the project root (repo) by walking up from this script.
SCRIPT_DIR = Path(__file__).resolve().parent  # rust/prro/tests/goldens/
REPO_ROOT = SCRIPT_DIR.parents[3]
PYTHON_SRC = REPO_ROOT / "src"

# Python imports ride off the project's `src/` layout.
sys.path.insert(0, str(PYTHON_SRC))

from prro_gateway.enums import OperationType  # noqa: E402
from prro_gateway.serializers.dps_xml import build_dps_xml  # noqa: E402


# ─── Common header values shared across the 4 XML fixtures ────────────

# Frozen test FN / TN / device.  Picked to be obviously-synthetic so
# nobody confuses the goldens with real production payloads.
FN = "1234567890"
TN = "12345678"
Z_NUMBER = 7
PREVIOUS_HASH = "deadbeef"

# Explicit datetime — passing a string would fall through
# `datetime.fromisoformat` (the Python serializer's path) and silently
# default to `datetime.now(UTC)`, which would make every regenerate.py
# run produce a different golden.  09:00 UTC = 12:00 Kyiv-local
# (UTC+3, summer) → wire TS = "20260506120000".
BUSINESS_TS = datetime(2026, 5, 6, 9, 0, 0, tzinfo=UTC)


# ─── Per-fixture payload builders ─────────────────────────────────────


def shift_open_xml() -> str:
    """SHIFT_OPEN: <C T="108"> + <O> + <E>."""
    return build_dps_xml(
        operation_type=OperationType.SHIFT_OPEN,
        fiscal_number=FN,
        local_number=0,
        business_ts=BUSINESS_TS,
        payload={"receipt": {"totals": {"total_sum": 0}}},
        tax_number=TN,
        z_number=Z_NUMBER,
        previous_hash=PREVIOUS_HASH,
    )


def sell_xml() -> str:
    """SELL: 1 item, 1 cash payment, total 1500 kop."""
    return build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number=FN,
        local_number=42,
        business_ts=BUSINESS_TS,
        payload={
            "receipt": {
                "goods": [
                    {
                        "code": "ART-1",
                        "name": "Apple",
                        "price": 1500,
                        "quantity": 1000,  # thousandths
                        "sum": 1500,
                    },
                ],
                "payments": [
                    {"type": "CASH", "amount": 1500},
                ],
                "totals": {"total_sum": 1500},
            },
        },
        tax_number=TN,
        z_number=Z_NUMBER,
        previous_hash=PREVIOUS_HASH,
    )


def return_xml() -> str:
    """RETURN: same shape as SELL but <C T="1">."""
    return build_dps_xml(
        operation_type=OperationType.RETURN,
        fiscal_number=FN,
        local_number=13,
        business_ts=BUSINESS_TS,
        payload={
            "receipt": {
                "goods": [
                    {
                        "code": "ART-1",
                        "name": "Apple",
                        "price": 1500,
                        "quantity": 1000,
                        "sum": 1500,
                    },
                ],
                "payments": [
                    {"type": "CASH", "amount": 1500},
                ],
                "totals": {"total_sum": 1500},
            },
        },
        tax_number=TN,
        z_number=Z_NUMBER,
        previous_hash=PREVIOUS_HASH,
    )


def z_report_xml() -> str:
    """Z_REPORT (which IS the close-shift wire artifact).

    Subset: <Z NO="..."> + per-payment-type <M> + <NC NI NO>.
    Optional <TXS> / <IO> / <EPZ> deliberately omitted for first round.
    """
    return build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number=FN,
        local_number=100,
        business_ts=BUSINESS_TS,
        payload={
            "z_report_data": {
                "payment_sums": {
                    "CASH": {"smi": 5000, "smo": 0},
                },
                "check_count": {"ni": 17, "no": 2},
            },
        },
        tax_number=TN,
        z_number=Z_NUMBER,
        previous_hash=PREVIOUS_HASH,
    )


# ─── CMS deterministic prefix + prevhash seed ─────────────────────────


def cms_deterministic_prefix() -> bytes:
    """Frozen "deterministic prefix" for a CMS-signed XML golden.

    Per ADR-M2-3 the CMS goldens are split into:
      (a) the deterministic prefix — the XML-to-be-signed bytes,
          pinned byte-equivalent, and
      (b) the signature shape — parsed + verified, NOT byte-compared.

    For W4 first round we pin (a) only.  The simplest viable
    "XML-to-be-signed" is the SELL fixture above; reviewers can
    diff this artefact against ``xml/sell.bin`` to confirm the
    CMS path doesn't introduce its own canonicalisation drift.
    """
    return sell_xml().encode("cp1251")


def prevhash_seed() -> bytes:
    """First-after-bootstrap previous_hash seed.

    The Python pipeline uses an empty MAC for the first document of
    a fresh FN; the seed for that case is `b""`.  We pin a 32-byte
    sha256-shaped placeholder so the harness can prove the Rust
    builder doesn't accidentally fill in `Utc::now()` or some
    other drift source.
    """
    return b"\x00" * 32


# ─── Driver ───────────────────────────────────────────────────────────


def write(path: Path, data: bytes) -> dict:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {
        "path": str(path.relative_to(SCRIPT_DIR.parent)),
        "sha256": hashlib.sha256(data).hexdigest(),
        "length": len(data),
    }


def main() -> int:
    fixtures: list[tuple[Path, bytes]] = [
        (SCRIPT_DIR / "xml" / "shift_open.bin", shift_open_xml().encode("cp1251")),
        (SCRIPT_DIR / "xml" / "sell.bin", sell_xml().encode("cp1251")),
        (SCRIPT_DIR / "xml" / "return.bin", return_xml().encode("cp1251")),
        (SCRIPT_DIR / "xml" / "z_report.bin", z_report_xml().encode("cp1251")),
        (SCRIPT_DIR / "cms" / "deterministic_prefix.bin", cms_deterministic_prefix()),
        (SCRIPT_DIR / "prevhash" / "seed.bin", prevhash_seed()),
    ]

    manifest = {"oracle": "src/prro_gateway/serializers/dps_xml.py", "files": []}
    for path, data in fixtures:
        manifest["files"].append(write(path, data))

    manifest_path = SCRIPT_DIR / "manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    print(f"Wrote {len(fixtures)} fixtures + manifest.json")
    for entry in manifest["files"]:
        print(f"  {entry['sha256'][:12]}  {entry['length']:>6}  {entry['path']}")
    print(f"  manifest: {manifest_path.relative_to(REPO_ROOT)}")
    print()
    print("Review the diff before committing:")
    print(f"  git diff {SCRIPT_DIR.relative_to(REPO_ROOT)}/")
    return 0


if __name__ == "__main__":
    sys.exit(main())
