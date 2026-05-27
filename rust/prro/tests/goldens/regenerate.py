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
from prro_gateway.repositories.tax_groups import TaxGroupDef  # noqa: E402
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


# ─── W4-Z1 piece 8 — extended-shape fixtures ──────────────────────────
#
# Goal: byte-equivalence verification of the EXTENDED FSCO Table 23
# coverage that W4-Z1 added (excise / UKTZED / TX groups / per-item
# discount / EPZ slip / check-level adjustments / header+footer L /
# <TX> in <E> via tax_groups).  Each fixture exercises a distinct
# code path in the Rust builder + the Python oracle.


def _tax_group(tax_id: str, rate: float, algo: int = 0,
               add_rate: float = 0.0, ttype: int = 0) -> TaxGroupDef:
    """Synthetic tax group fixture (frozen for reproducibility)."""
    return TaxGroupDef(
        fiscal_number=FN,
        tax_id=tax_id,
        name=f"Tax {tax_id}",
        tax_rate=rate,
        additional_rate=add_rate,
        tax_type=ttype,
        tax_algorithm=algo,
        requires_uktzed=False,
        requires_excise_mark=False,
        is_active=True,
    )


# Two-group fixture: TX=1 → 20% VAT, TX=2 → 7% VAT.  Used by both
# the extended SELL fixture (<TX> children in <E>) and as a stable
# parity oracle for the Rust `derive_tax_summaries` helper.
TAX_GROUPS_EXTENDED = {
    "1": _tax_group("1", 20.0, algo=0),
    "2": _tax_group("2", 7.0, algo=0),
}


def sell_extended_xml() -> str:
    """SELL with the full W4-Z1 attribute set:

      - 2 items, both with tax_group_1 (one 20% VAT, one 7% VAT)
      - per-item barcode + UKTZED on the wine item
      - per-item excise stamps on the wine item
      - per-item discount on the bread item
      - 1 EPZ card payment with full acquirer slip
      - check-level discount
      - header + footer text lines
      - <TX> children in <E> via tax_groups
    """
    return build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number=FN,
        local_number=99,
        business_ts=BUSINESS_TS,
        payload={
            "receipt": {
                "header": "ТОВ Магазинчик\nм. Київ, вул. Дніпровська 33",
                "footer": "Дякуємо за покупку!",
                "goods": [
                    {
                        "code": "WINE-001",
                        "name": "Вино червоне",
                        "price": 25000,
                        "quantity": 1000,
                        "sum": 25000,
                        "barcode": "4820012345678",
                        "uktzed": "22042100",
                        "tax_id": "1",
                        "excise_barcodes": ["UA1234567890", "UA0987654321"],
                    },
                    {
                        "code": "BREAD-1",
                        "name": "Хліб",
                        "price": 1200,
                        "quantity": 1000,
                        "sum": 1200,
                        "tax_id": "2",
                        "discounts": [
                            {"type": "DISCOUNT", "mode": "VALUE",
                             "value": 100, "name": "Промо"},
                        ],
                    },
                ],
                "discounts": [
                    {"type": "DISCOUNT", "mode": "VALUE", "value": 200,
                     "name": "Лояльність"},
                ],
                "payments": [
                    {
                        "type": "CASHLESS_1",
                        "amount": 25900,
                        "rrn": "RRN-000123456",
                        "payment_system": "VISA",
                        "bank_name": "ПриватБанк",
                        "terminal": "T-123",
                        "label": "OnlineLabel",
                        "card_mask": "411111****1234",
                        "auth_code": "AUTH-9876",
                        "commission": 100,
                    },
                ],
                "totals": {"total_sum": 26200},
            },
        },
        tax_number=TN,
        z_number=Z_NUMBER,
        previous_hash=PREVIOUS_HASH,
        tax_groups=TAX_GROUPS_EXTENDED,
    )


def z_report_extended_xml() -> str:
    """Z_REPORT with TXS / IO / EPZ aggregations."""
    return build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number=FN,
        local_number=100,
        business_ts=BUSINESS_TS,
        payload={
            "z_report_data": {
                "tax_sums": {
                    "1": {"smi": 12000, "smo": 0},
                    "2": {"smi": 700, "smo": 0},
                },
                "payment_sums": {
                    "CASH": {"smi": 5000, "smo": 0},
                    "CARD": {"smi": 7700, "smo": 0},
                },
                "service_sums": {
                    "SERVICE_IN": {"smi": 1000, "smo": 0},
                    "SERVICE_OUT": {"smi": 0, "smo": 500},
                },
                "check_count": {"ni": 17, "no": 2},
                "epz_sums": {"epc": 3, "epcs": 2, "epsm": 7700},
            },
        },
        tax_number=TN,
        z_number=Z_NUMBER,
        previous_hash=PREVIOUS_HASH,
        tax_groups=TAX_GROUPS_EXTENDED,
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
        # W4-Z1 piece 8 — extended-shape fixtures
        (SCRIPT_DIR / "xml" / "sell_extended.bin", sell_extended_xml().encode("cp1251")),
        (SCRIPT_DIR / "xml" / "z_report_extended.bin", z_report_extended_xml().encode("cp1251")),
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
