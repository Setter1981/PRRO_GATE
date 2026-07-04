from __future__ import annotations

import hashlib
import importlib.util
import sys
from pathlib import Path


def _load():
    path = Path(__file__).resolve().parents[1] / "scripts" / "sanitize_webcheck_corpus.py"
    spec = importlib.util.spec_from_file_location("sanitize_webcheck_corpus", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _online_sell_run_shape(module):
    # SHIFT_OPEN(8), 3 online SELLs(0), SHIFT_CLOSE(80) — all offline=0, one shift.
    S = module.ShapeRecord
    return [S(8, 0, 1), S(0, 0, 1), S(0, 0, 1), S(0, 0, 1), S(80, 0, 1)]


def test_synthesize_hash_consistency_every_command():
    """Every command's payload_sha256 must equal sha256(payload_json) — the §4
    recompute-over-synthetic-bytes invariant, byte-exact."""
    m = _load()
    fx = m.synthesize_fixture(_online_sell_run_shape(m), "online_sell_run")
    for op in fx["sequence"]:
        cmd = op["canonical_command"]
        recomputed = hashlib.sha256(cmd["payload_json"].encode("utf-8")).hexdigest()
        assert cmd["payload_sha256"] == recomputed, op["operation_type"]


def test_synthesize_is_synthetic_only():
    """No real data: FN is the synthetic prefix-9 constant; timestamps are in the
    synthetic epoch; content is the fixed synthetic goods/cashier."""
    m = _load()
    fx = m.synthesize_fixture(_online_sell_run_shape(m), "online_sell_run")
    assert fx["fiscal_number"] == m.SYNTH_FN
    assert fx["fiscal_number"].startswith("9") and fx["fiscal_number"].isdigit()
    for op in fx["sequence"]:
        cmd = op["canonical_command"]
        assert cmd["fiscal_number"] == m.SYNTH_FN
        assert cmd["business_ts"].startswith("2026-01-01T")
        assert m.SYNTH_CASHIER in cmd["payload_json"]


def test_synthesize_drops_doctype_10_and_12():
    """CP1 decision 2: DocType 10/12 are DROPPED (not in U0 tables), honest-counted."""
    m = _load()
    S = m.ShapeRecord
    shape = [S(8, 0, 1), S(0, 0, 1), S(10, 0, 1), S(12, 0, 1), S(80, 0, 1)]
    fx = m.synthesize_fixture(shape, "with_dropped")
    ops = [op["operation_type"] for op in fx["sequence"]]
    assert ops == ["SHIFT_OPEN", "SELL", "SHIFT_CLOSE"]
    assert fx["expected_observables"]["dropped_doctype_count"] == 2


def test_synthesize_lnd_is_dense_per_fn():
    """U0 §4 lnd-translation: dense per-FN lnd = 1,2,3,… over the shape order."""
    m = _load()
    fx = m.synthesize_fixture(_online_sell_run_shape(m), "online_sell_run")
    assert fx["expected_observables"]["lnd_sequence"] == [1, 2, 3, 4, 5]


def test_synthesize_offline_codes_consumed():
    """Offline-issue sells (drained/pending class) count as a consumed code."""
    m = _load()
    S = m.ShapeRecord
    # SHIFT_OPEN, 2 offline-drained SELLs (offline=1), a type-9 DRAIN, SHIFT_CLOSE.
    shape = [S(8, 0, 1), S(0, 1, 1), S(0, 1, 1), S(9, 1, 1), S(80, 0, 1)]
    fx = m.synthesize_fixture(shape, "offline_session_drain")
    assert fx["expected_observables"]["offline_codes_consumed"] == 2
