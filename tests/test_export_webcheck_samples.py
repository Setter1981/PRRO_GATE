from __future__ import annotations

import importlib.util
import json
import sqlite3
import sys
from pathlib import Path

import pytest


def _load_module():
    path = Path(__file__).resolve().parents[1] / "scripts" / "export_webcheck_samples.py"
    spec = importlib.util.spec_from_file_location("export_webcheck_samples", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _receipt_xml() -> str:
    return (
        "<?xml version='1.0' encoding='windows-1251'?>"
        "<RQ V='1'><DAT FN='4000162280' TN='13667753' DI='doc-1' ZN='0' V='1'>"
        "<C T='0'>"
        "<P N='1' C='SKU-1' NM='Alcohol' SM='120.00' Q='1.000' PRC='120.00' "
        "TX='1' CD='1' CZD='2203000100'><CA CA='AA123456'></CA></P>"
        "<D N='2' NI='1' TX='1' SM='5.00' TR='0' TY='0'/>"
        "<M N='3' T='0' NM='CASH' SM='115.00'/>"
        "<E N='4' NO='7' SM='115.00' TS='20260421110000' CS='1'>"
        "<TX TX='1' TXPR='20.00' TXSM='19.17' TXTY='0' TXAL='0'/>"
        "</E>"
        "</C><TS>20260421110000</TS></DAT><MAC></MAC></RQ>"
    )


def _create_webcheck_db(path: Path) -> None:
    with sqlite3.connect(path) as conn:
        conn.executescript(
            """
            CREATE TABLE ksef(
                checkid TEXT,
                checkxml TEXT,
                checksigned TEXT,
                signedanswerfromficscal TEXT,
                checkidficscal TEXT,
                localchecknumber INTEGER,
                DocType INTEGER,
                sum DECIMAL(17, 2),
                mac TEXT,
                shiftid INTEGER,
                dt DATETIME,
                ID INTEGER PRIMARY KEY AUTOINCREMENT,
                offline INTEGER DEFAULT 0
            );
            CREATE TABLE CHECKHEAD(
                ID INTEGER PRIMARY KEY AUTOINCREMENT,
                SHIFTID INTEGER,
                UID VARCHAR(36),
                DOCTYPE INT,
                VER INT,
                ORDERDATE DATETIME,
                ORDERNUM TEXT,
                ORDERTAXNUM TEXT,
                FN BIGINT,
                TOTALSUM DECIMAL(17, 2)
            );
            CREATE TABLE CHECKBODY(
                ID INTEGER PRIMARY KEY AUTOINCREMENT,
                CHECKID INTEGER,
                CODE VARCHAR(64),
                UKTZED VARCHAR(15),
                GOODSNAME VARCHAR(128),
                AMOUNT DECIMAL(17, 3),
                PRICE DECIMAL(17, 2),
                LETTER VARCHAR(1),
                COST DECIMAL(17, 2)
            );
            CREATE TABLE CHECKPAY(
                ID INTEGER PRIMARY KEY AUTOINCREMENT,
                CHECKID INTEGER,
                PAYMENTFORM VARCHAR(64),
                TOTALSUM DECIMAL(17, 2)
            );
            CREATE TABLE CHECKTAX(
                ID INTEGER PRIMARY KEY AUTOINCREMENT,
                CHECKID INTEGER,
                TAXCODE VARCHAR(3),
                TAXPRC DECIMAL(17, 2),
                TAXSUM DECIMAL(17, 2)
            );
            CREATE TABLE CHECKEXCISE(
                ID INTEGER PRIMARY KEY AUTOINCREMENT,
                CHECKID INTEGER,
                EXCISECODE VARCHAR(64),
                EXCISEPRC DECIMAL(17, 2),
                EXCISESUM DECIMAL(17, 2)
            );
            CREATE TABLE SHIFTS(
                ID INTEGER PRIMARY KEY AUTOINCREMENT,
                DATEBEG DATETIME,
                DATEEND DATETIME,
                RROFISCAL BIGINT,
                LastLocalCheckNumber INTEGER
            );
            """
        )
        conn.execute(
            "INSERT INTO SHIFTS (ID, DATEBEG, DATEEND, RROFISCAL, LastLocalCheckNumber) "
            "VALUES (1, '2026-04-21 10:00:00', 'NULL', 4000162280, 7)"
        )
        conn.execute(
            "INSERT INTO CHECKHEAD (ID, SHIFTID, UID, DOCTYPE, VER, ORDERDATE, ORDERNUM, "
            "ORDERTAXNUM, FN, TOTALSUM) VALUES "
            "(10, 1, 'doc-1', 0, 1, '2026-04-21 11:00:00', '7', 'FISCAL-001', 4000162280, '115.00')"
        )
        conn.execute(
            "INSERT INTO CHECKBODY (CHECKID, CODE, UKTZED, GOODSNAME, AMOUNT, PRICE, LETTER, COST) "
            "VALUES (10, 'SKU-1', '2203000100', 'Alcohol', '1.000', '120.00', 'A', '120.00')"
        )
        conn.execute(
            "INSERT INTO CHECKPAY (CHECKID, PAYMENTFORM, TOTALSUM) VALUES (10, 'CASH', '115.00')"
        )
        conn.execute(
            "INSERT INTO CHECKTAX (CHECKID, TAXCODE, TAXPRC, TAXSUM) VALUES (10, 'A', '20.00', '19.17')"
        )
        conn.execute(
            "INSERT INTO CHECKEXCISE (CHECKID, EXCISECODE, EXCISEPRC, EXCISESUM) "
            "VALUES (10, 'EXC', '5.00', '5.00')"
        )
        conn.execute(
            "INSERT INTO ksef (checkid, checkxml, checksigned, signedanswerfromficscal, "
            "checkidficscal, localchecknumber, DocType, sum, mac, shiftid, dt, offline) "
            "VALUES ('doc-1', ?, '', '', 'FISCAL-001', 7, 0, '115.00', 'mac', 1, "
            "'2026-04-21 11:00:00', 0)",
            (_receipt_xml(),),
        )


def test_export_database_sample_contains_semantic_fields(tmp_path: Path) -> None:
    module = _load_module()
    db_path = tmp_path / "4000162280.db"
    out_dir = tmp_path / "out"
    _create_webcheck_db(db_path)

    options = module.ExportOptions(output_dir=out_dir, limit=10)
    manifest = module.export_databases([db_path], options)

    assert manifest["sample_count"] == 1
    assert manifest["curated_selection"]["categories"]["sell_with_excise"]["selected"]
    sample_path = out_dir / manifest["samples"][0]["path"]
    sample = json.loads(sample_path.read_text(encoding="utf-8"))

    assert sample["operation_type"] == "SELL"
    assert sample["fiscal_number"] == "4000162280"
    assert sample["features"]["has_uktzed"] is True
    assert sample["features"]["has_excise"] is True
    assert sample["features"]["has_discounts"] is True
    assert sample["normalized_receipt"]["total_kopecks"] == 11500
    assert sample["normalized_receipt"]["goods"][0]["uktzed"] == "2203000100"
    assert (out_dir / "selected_manifest.json").exists()


def test_export_xml_file_without_database(tmp_path: Path) -> None:
    module = _load_module()
    xml_path = tmp_path / "receipt.xml"
    out_dir = tmp_path / "out"
    xml_path.write_bytes(_receipt_xml().encode("cp1251"))

    options = module.ExportOptions(output_dir=out_dir, xml_limit=10)
    manifest = module.export_sources([], [xml_path], options)

    assert manifest["sample_count"] == 1
    sample_path = out_dir / manifest["samples"][0]["path"]
    sample = json.loads(sample_path.read_text(encoding="utf-8"))

    assert sample["source"]["system"] == "WebCheck XML"
    assert sample["operation_type"] == "SELL"
    assert sample["fiscal_number"] == "4000162280"
    assert sample["features"]["has_excise"] is True
    assert sample["normalized_receipt"]["payments"][0]["sum"] == "115.00"


def test_curated_classification_distinguishes_plain_and_excise() -> None:
    module = _load_module()
    plain = {
        "sample_id": "plain",
        "source_db": "a.db",
        "operation_type": "SELL",
        "features": {
            "has_goods": True,
            "has_excise": False,
            "has_uktzed": False,
            "has_discounts": False,
            "mixed_payment": False,
        },
    }
    excise = {
        "sample_id": "excise",
        "source_db": "b.db",
        "operation_type": "SELL",
        "features": {
            "has_goods": True,
            "has_excise": True,
            "has_uktzed": True,
            "has_discounts": False,
            "mixed_payment": False,
        },
    }

    assert "sell_plain" in module.classify_sample_categories(plain)
    assert "sell_without_excise" in module.classify_sample_categories(plain)
    assert "sell_with_excise" not in module.classify_sample_categories(plain)
    assert "sell_with_excise" in module.classify_sample_categories(excise)
    assert "sell_with_uktzed" in module.classify_sample_categories(excise)


# ── WebCheck U2 (re-check F7): --output-dir is REQUIRED + must be outside-tree ──


def test_output_dir_is_required_no_in_tree_default():
    """Omitting --output-dir must be a hard argparse error (exit 2), never a
    silent in-tree default (var/webcheck_samples) — the never-transit rule."""
    module = _load_module()
    with pytest.raises(SystemExit) as excinfo:
        module.main(["nonexistent.db"])  # no --output-dir
    assert excinfo.value.code == 2


def test_output_dir_rejects_in_tree_path():
    """An --output-dir inside the repo tree is refused before any export —
    WebCheck exports carry real fiscal data and must stay outside the tree."""
    module = _load_module()
    repo_root = Path(__file__).resolve().parents[1]
    in_tree = repo_root / "var" / "webcheck_samples" / "run"
    with pytest.raises(SystemExit) as excinfo:
        module.main(["nonexistent.db", "--output-dir", str(in_tree)])
    assert "inside the repo tree" in str(excinfo.value)
