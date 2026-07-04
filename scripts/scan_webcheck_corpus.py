#!/usr/bin/env python3
"""Mechanical leak-scanner for the WebCheck synthetic corpus (U2 §6).

FAILS if a committed fixture contains any NON-synthetic marker — the mechanical
gate that sits UNDER CP1 human review (audit C5). One pattern per §6 table row;
the paired test (`tests/test_scan_webcheck_corpus.py`) plants EACH leak class and
asserts it is caught (RED-first, honestly per class), and asserts clean synthetic
passes.
"""
from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

# ── §6 patterns (one per row) ────────────────────────────────────────────────
_FN_NOT_9 = re.compile(r"\b[0-8][0-9]{9}\b")  # 10-digit numeric NOT prefix 9 (dumps 4…, demo 7…)
_TIN = re.compile(r"\b[0-9]{8}\b")            # 8-digit ЄДРПОУ / TIN shape
_UUID = re.compile(
    r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b"
)
_HEX64 = re.compile(r"\b[0-9a-f]{64}\b")      # any sha256-shaped token
_ISO_TS = re.compile(r"\b(\d{4}-\d\d-\d\d)T\d\d:\d\d:\d\d")
_CYRILLIC = re.compile(r"[Ѐ-ӿ]")
_XML_MARKERS = ("<?xml", "<RQ", "<DAT", "<CHECK", "mmmaaaccc", "windows-1251")
_DPS_MARKERS = ("signedanswerfromficscal", "checksigned")
_SYNTH_EPOCH_DATE = "2026-01-01"


def _allowed_hashes(fixture: dict) -> set[str]:
    """The ONLY hashes a fixture may contain: recomputes of its own synthetic
    payloads (payload_sha256 + the previous_hash chain, which are prior recomputes)."""
    allowed: set[str] = set()
    for op in fixture.get("sequence", []):
        pj = op.get("canonical_command", {}).get("payload_json")
        if isinstance(pj, str):
            allowed.add(hashlib.sha256(pj.encode("utf-8")).hexdigest())
    return allowed


def scan_text(text: str, allowed_hashes: set[str]) -> list[tuple[str, str]]:
    leaks: list[tuple[str, str]] = []
    for m in _FN_NOT_9.finditer(text):
        leaks.append(("real_fn", m.group()))
    for m in _TIN.finditer(text):
        leaks.append(("tin", m.group()))
    for m in _UUID.finditer(text):
        leaks.append(("uuid", m.group()))
    for m in _ISO_TS.finditer(text):
        if m.group(1) != _SYNTH_EPOCH_DATE:
            leaks.append(("real_timestamp", m.group()))
    for marker in _XML_MARKERS:
        if marker in text:
            leaks.append(("raw_xml", marker))
    for marker in _DPS_MARKERS:
        if marker in text:
            leaks.append(("dps_blob", marker))
    if _CYRILLIC.search(text):
        leaks.append(("cyrillic", _CYRILLIC.search(text).group()))
    for m in _HEX64.finditer(text):
        if m.group() not in allowed_hashes:
            leaks.append(("nonsynthetic_hash", m.group()))
    return leaks


def scan_fixture_dir(fx_dir: Path) -> list[tuple[str, str]]:
    seq = json.loads((fx_dir / "sequence.json").read_text())
    allowed = _allowed_hashes(seq)
    leaks: list[tuple[str, str]] = []
    # §4 hash-consistency: each stored payload_sha256 must equal the recompute.
    for op in seq.get("sequence", []):
        cmd = op.get("canonical_command", {})
        pj, stored = cmd.get("payload_json"), cmd.get("payload_sha256")
        if isinstance(pj, str):
            recomputed = hashlib.sha256(pj.encode("utf-8")).hexdigest()
            if stored != recomputed:
                leaks.append(("hash_mismatch", f"{stored} != recompute"))
    for f in sorted(fx_dir.glob("*")):
        if f.is_file():
            leaks.extend(scan_text(f.read_text(errors="surrogatepass"), allowed))
    return leaks


def scan_corpus(corpus_dir: Path) -> list[tuple[str, str]]:
    leaks: list[tuple[str, str]] = []
    for seq in sorted(corpus_dir.rglob("sequence.json")):
        leaks.extend(scan_fixture_dir(seq.parent))
    return leaks


def repo_has_in_tree_export_dir(repo_root: Path) -> bool:
    """§6 belt: the in-tree export dir must never exist in the working tree."""
    return (repo_root / "var" / "webcheck_samples").exists()


def main(argv: list[str] | None = None) -> int:
    import argparse

    parser = argparse.ArgumentParser(description="Scan a WebCheck corpus dir for non-synthetic leaks.")
    parser.add_argument("corpus_dir", type=Path)
    args = parser.parse_args(argv)
    leaks = scan_corpus(args.corpus_dir)
    repo_root = Path(__file__).resolve().parents[1]
    if repo_has_in_tree_export_dir(repo_root):
        leaks.append(("in_tree_export_dir", "var/webcheck_samples exists"))
    if leaks:
        for klass, detail in leaks:
            print(f"LEAK[{klass}]: {detail}")
        return 1
    print("clean")
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
