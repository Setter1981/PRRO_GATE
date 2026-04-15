from pathlib import Path
import sqlite3

from prro_gateway.migrations.runner import apply_migrations

ROOT = Path(__file__).resolve().parents[1]


def test_apply_migrations(tmp_path):
    db_path = tmp_path / "test.db"
    executed = apply_migrations(db_path, ROOT / "sql")
    assert "001_hot_store_init.sql" in executed
    assert "002_seed_reference_data.sql" in executed
    conn = sqlite3.connect(db_path)
    try:
        row = conn.execute("SELECT COUNT(*) FROM schema_migrations").fetchone()
        assert row[0] >= 2
    finally:
        conn.close()
