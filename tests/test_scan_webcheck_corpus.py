from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import pytest


def _load(name: str):
    path = Path(__file__).resolve().parents[1] / "scripts" / f"{name}.py"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _clean_fixture(tmp_path: Path) -> Path:
    sz = _load("sanitize_webcheck_corpus")
    S = sz.ShapeRecord
    shape = [S(8, 0, 1), S(0, 0, 1), S(0, 1, 1), S(9, 1, 1), S(80, 0, 1)]
    fx = sz.synthesize_fixture(shape, "online_sell_run")
    sz.write_fixture(fx, tmp_path)
    return tmp_path / "online_sell_run"


def test_clean_synthetic_corpus_passes(tmp_path):
    """NEG tooth: a clean synthetic fixture yields ZERO leaks (no false-positive)."""
    scan = _load("scan_webcheck_corpus")
    leaks = scan.scan_fixture_dir(_clean_fixture(tmp_path))
    assert leaks == [], leaks


# POS teeth — one planted leak PER §6 row; each MUST be caught (RED-first honestly).
@pytest.mark.parametrize(
    ("planted", "expected_class"),
    [
        ("1234567890", "real_fn"),                                  # 10-digit not prefix 9 (synthetic)
        ("12345678", "tin"),                                        # 8-digit ЄДРПОУ shape
        ("550e8400-e29b-41d4-a716-446655440000", "uuid"),           # RFC-4122 UUID
        ("2024-05-06T10:00:00Z", "real_timestamp"),                 # ts outside synth epoch
        ("<?xml version='1.0'?>", "raw_xml"),                       # raw WebCheck XML
        ("signedanswerfromficscal", "dps_blob"),                    # DPS blob marker
        ("Оператор", "cyrillic"),                                   # Cyrillic org/name
        ("f" * 64, "nonsynthetic_hash"),                            # foreign 64-hex hash
    ],
)
def test_scanner_catches_each_leak_class(tmp_path, planted, expected_class):
    scan = _load("scan_webcheck_corpus")
    fx_dir = _clean_fixture(tmp_path)
    shape_md = fx_dir / "SHAPE.md"
    shape_md.write_text(shape_md.read_text() + f"\nPLANTED_LEAK: {planted}\n")
    classes = {k for k, _ in scan.scan_fixture_dir(fx_dir)}
    assert expected_class in classes, (expected_class, classes)


def test_scanner_catches_hash_mismatch(tmp_path):
    """§4 hash-consistency: a perturbed payload_sha256 (not a recompute) is caught."""
    scan = _load("scan_webcheck_corpus")
    fx_dir = _clean_fixture(tmp_path)
    seq_file = fx_dir / "sequence.json"
    data = json.loads(seq_file.read_text())
    data["sequence"][0]["canonical_command"]["payload_sha256"] = "0" * 64
    seq_file.write_text(json.dumps(data))
    classes = {k for k, _ in scan.scan_fixture_dir(fx_dir)}
    assert "hash_mismatch" in classes, classes
