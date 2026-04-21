"""Proof tests for Phase 9 — VAT IPN column + tax_number immutable on edit.

- sql/020_vat_payer_inn.sql adds `vat_payer_inn TEXT` to
  fiscal_number_config.
- Add-FN form accepts optional 12-digit VAT IPN.
- Edit-FN form renders tax_number as readonly and discards any POSTed
  tax_number value (same immutability contract as fiscal_number).
- Settings FNs list shows a new "ПДВ" column.
"""
from __future__ import annotations

import re
from pathlib import Path

from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]
_ADMIN_PASS = "vat-phase9-pilot"
_CSRF_RE = re.compile(r'name="csrf_token"\s+value="([^"]+)"')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "vat.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV",
            "backend_profile_id": "b", "transport_profile_id": "t",
            "channel_owner": "vat-tests",
        },
        "admin_ui": {
            "enabled": True, "password": _ADMIN_PASS,
            "session_secret": "v" * 32,
        },
    })


def _logged_in_client(container: RuntimeContainer) -> TestClient:
    client = TestClient(create_app(container))
    client.__enter__()
    client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
    return client


def _fetch_csrf(client: TestClient, path: str) -> str:
    r = client.get(path)
    assert r.status_code == 200, f"{path}: {r.status_code}"
    m = _CSRF_RE.search(r.text)
    assert m, f"CSRF missing at {path}"
    return m.group(1)


def _seed_fn(container: RuntimeContainer, fn: str = "4000009100",
             vat: str | None = None) -> None:
    with container.connect() as conn:
        conn.execute(
            """INSERT INTO fiscal_number_config (
                fiscal_number, tax_number, fiscal_mode,
                tsp_enabled, offline_enabled,
                org_name, org_address,
                min_offline_codes, max_offline_codes, vat_payer_inn
            ) VALUES (?, '12345678', 'test', 0, 1, 'Seed Co', 'addr', 0, 0, ?)""",
            (fn, vat),
        )
        conn.commit()


# ─── 1. Migration 020 ────────────────────────────────────────────────


def test_migration_020_adds_vat_payer_inn_column(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)):
        pass
    with container.connect() as conn:
        cols = {row[1] for row in conn.execute(
            "PRAGMA table_info(fiscal_number_config)"
        ).fetchall()}
    assert "vat_payer_inn" in cols


# ─── 2. Add-FN — new VAT field ───────────────────────────────────────


def test_add_form_renders_vat_field(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/fns/new")
        assert r.status_code == 200
        assert 'name="vat_payer_inn"' in r.text
        # Visible Ukrainian label hint.
        assert "ПДВ" in r.text
    finally:
        client.__exit__(None, None, None)


def test_add_fn_stores_vat_payer_inn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/new")
        r = client.post(
            "/admin/ui/settings/fns/new",
            data={
                "csrf_token": csrf,
                "fiscal_number": "4000009200",
                "tax_number": "12345678",
                "fiscal_mode": "test",
                "org_name": "ТОВ Plat", "point_name": "p", "point_address": "a",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                "vat_payer_inn": "123456789012",
            },
            follow_redirects=False,
        )
        assert r.status_code in (302, 303), r.text
        with container.connect() as conn:
            vat = conn.execute(
                "SELECT vat_payer_inn FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000009200",),
            ).fetchone()[0]
        assert vat == "123456789012"
    finally:
        client.__exit__(None, None, None)


def test_add_fn_vat_is_optional(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/new")
        r = client.post(
            "/admin/ui/settings/fns/new",
            data={
                "csrf_token": csrf,
                "fiscal_number": "4000009201",
                "tax_number": "12345678", "fiscal_mode": "test",
                "org_name": "x", "point_name": "x", "point_address": "x",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                # vat_payer_inn omitted
            },
            follow_redirects=False,
        )
        assert r.status_code in (302, 303), r.text
        with container.connect() as conn:
            vat = conn.execute(
                "SELECT vat_payer_inn FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000009201",),
            ).fetchone()[0]
        assert vat is None
    finally:
        client.__exit__(None, None, None)


def test_add_fn_rejects_short_vat(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/new")
        r = client.post(
            "/admin/ui/settings/fns/new",
            data={
                "csrf_token": csrf,
                "fiscal_number": "4000009202",
                "tax_number": "12345678", "fiscal_mode": "test",
                "org_name": "x", "point_name": "x", "point_address": "x",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                "vat_payer_inn": "1234",  # too short
            },
            follow_redirects=False,
        )
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_add_fn_rejects_non_digit_vat(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/new")
        r = client.post(
            "/admin/ui/settings/fns/new",
            data={
                "csrf_token": csrf,
                "fiscal_number": "4000009203",
                "tax_number": "12345678", "fiscal_mode": "test",
                "org_name": "x", "point_name": "x", "point_address": "x",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                "vat_payer_inn": "12345678901X",
            },
            follow_redirects=False,
        )
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


# ─── 3. Edit-FN: tax_number immutable, VAT editable ──────────────────


def test_edit_form_renders_tax_number_readonly(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_fn(container)
        r = client.get("/admin/ui/settings/fns/4000009100/edit")
        assert r.status_code == 200
        # Tax number field must be present BUT not editable.
        assert 'name="tax_number"' in r.text or "12345678" in r.text
        # Readonly / disabled markup near tax_number.
        assert "readonly" in r.text or "disabled" in r.text
    finally:
        client.__exit__(None, None, None)


def test_edit_post_ignores_tax_number_in_body(tmp_path: Path) -> None:
    # Even when an attacker POSTs a different tax_number, the DB row
    # must retain the original value — like fiscal_number immutability.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_fn(container)
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/4000009100/edit")
        r = client.post(
            "/admin/ui/settings/fns/4000009100/edit",
            data={
                "csrf_token": csrf,
                "tax_number": "99999999",  # smuggle attempt
                "fiscal_mode": "test",
                "org_name": "Renamed", "point_name": "p", "point_address": "a",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
            },
            follow_redirects=False,
        )
        assert r.status_code in (302, 303), r.text
        with container.connect() as conn:
            tin = conn.execute(
                "SELECT tax_number FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000009100",),
            ).fetchone()[0]
            org = conn.execute(
                "SELECT org_name FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000009100",),
            ).fetchone()[0]
        assert tin == "12345678"   # unchanged
        assert org == "Renamed"    # mutable fields updated
    finally:
        client.__exit__(None, None, None)


def test_edit_form_renders_vat_field_prefilled(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_fn(container, vat="987654321098")
        r = client.get("/admin/ui/settings/fns/4000009100/edit")
        assert r.status_code == 200
        assert 'name="vat_payer_inn"' in r.text
        assert 'value="987654321098"' in r.text
    finally:
        client.__exit__(None, None, None)


def test_edit_updates_vat_payer_inn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_fn(container, vat=None)
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/4000009100/edit")
        r = client.post(
            "/admin/ui/settings/fns/4000009100/edit",
            data={
                "csrf_token": csrf,
                "fiscal_mode": "test",
                "org_name": "Seed Co", "point_name": "p", "point_address": "a",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                "vat_payer_inn": "111222333444",
            },
            follow_redirects=False,
        )
        assert r.status_code in (302, 303)
        with container.connect() as conn:
            vat = conn.execute(
                "SELECT vat_payer_inn FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000009100",),
            ).fetchone()[0]
        assert vat == "111222333444"
    finally:
        client.__exit__(None, None, None)


def test_edit_can_clear_vat_payer_inn(tmp_path: Path) -> None:
    # Operator flipping out of VAT-payer status must be able to clear
    # the field by posting an empty value.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_fn(container, vat="555666777888")
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/4000009100/edit")
        r = client.post(
            "/admin/ui/settings/fns/4000009100/edit",
            data={
                "csrf_token": csrf,
                "fiscal_mode": "test",
                "org_name": "Seed Co", "point_name": "p", "point_address": "a",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                "vat_payer_inn": "",
            },
            follow_redirects=False,
        )
        assert r.status_code in (302, 303)
        with container.connect() as conn:
            vat = conn.execute(
                "SELECT vat_payer_inn FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000009100",),
            ).fetchone()[0]
        assert vat is None
    finally:
        client.__exit__(None, None, None)


def test_edit_rejects_invalid_vat(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_fn(container)
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/4000009100/edit")
        r = client.post(
            "/admin/ui/settings/fns/4000009100/edit",
            data={
                "csrf_token": csrf,
                "fiscal_mode": "test",
                "org_name": "x", "point_name": "x", "point_address": "x",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                "vat_payer_inn": "abc",
            },
            follow_redirects=False,
        )
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_edit_audit_records_vat_change(tmp_path: Path) -> None:
    # Auditability: every VAT-payer registration / deregistration is a
    # fiscal-identity event and must be recoverable from audit_log.
    import json
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_fn(container, vat="100000000001")
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/4000009100/edit")
        client.post(
            "/admin/ui/settings/fns/4000009100/edit",
            data={
                "csrf_token": csrf, "fiscal_mode": "test",
                "org_name": "Seed Co", "point_name": "p", "point_address": "a",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                "vat_payer_inn": "200000000002",
            },
            follow_redirects=False,
        )
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_payload_json FROM audit_log
                   WHERE entity_id='4000009100' AND event_type='fn_updated'
                   ORDER BY audit_id DESC LIMIT 1"""
            ).fetchone()
        assert row is not None
        payload = json.loads(row[0])
        assert payload["previous_vat_payer_inn"] == "100000000001"
        assert payload["vat_payer_inn"] == "200000000002"
        assert payload["vat_changed"] is True
    finally:
        client.__exit__(None, None, None)


def test_add_audit_records_vat_presence(tmp_path: Path) -> None:
    import json
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/new")
        client.post(
            "/admin/ui/settings/fns/new",
            data={
                "csrf_token": csrf,
                "fiscal_number": "4000009300",
                "tax_number": "12345678", "fiscal_mode": "test",
                "org_name": "x", "point_name": "x", "point_address": "x",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                "vat_payer_inn": "321321321321",
            },
            follow_redirects=False,
        )
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_payload_json FROM audit_log
                   WHERE entity_id='4000009300' AND event_type='fn_registered'"""
            ).fetchone()
        assert row is not None
        payload = json.loads(row[0])
        assert payload["vat_payer_inn_set"] is True
    finally:
        client.__exit__(None, None, None)


def test_migration_check_constraint_rejects_invalid_vat_at_sql_level(
    tmp_path: Path,
) -> None:
    # Defense-in-depth: even if handler validation is bypassed the
    # SQLite CHECK must reject empty string, non-digit, and wrong-length
    # vat values.
    import sqlite3 as _sqlite3
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)):
        pass
    for bad in ("12345678901", "1234567890123", "12345678901a", ""):
        with container.connect() as conn:
            try:
                conn.execute(
                    """INSERT INTO fiscal_number_config (
                        fiscal_number, tax_number, fiscal_mode,
                        tsp_enabled, offline_enabled, org_name, org_address,
                        min_offline_codes, max_offline_codes, vat_payer_inn
                    ) VALUES (?, '12345678', 'test', 0, 1, 'x', 'y', 0, 0, ?)""",
                    ("4000009999", bad),
                )
                raise AssertionError(f"CHECK did not reject: {bad!r}")
            except _sqlite3.IntegrityError:
                pass  # expected


def test_add_fn_rejects_too_long_vat(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        csrf = _fetch_csrf(client, "/admin/ui/settings/fns/new")
        r = client.post(
            "/admin/ui/settings/fns/new",
            data={
                "csrf_token": csrf,
                "fiscal_number": "4000009400",
                "tax_number": "12345678", "fiscal_mode": "test",
                "org_name": "x", "point_name": "x", "point_address": "x",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
                "vat_payer_inn": "1234567890123",  # 13 digits
            },
            follow_redirects=False,
        )
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


# ─── 4. Settings list — VAT column ───────────────────────────────────


def test_fns_list_shows_vat_column(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_fn(container, vat="999888777666")
        r = client.get("/admin/ui/settings/fns")
        assert r.status_code == 200
        assert "ПДВ" in r.text
        assert "999888777666" in r.text
    finally:
        client.__exit__(None, None, None)


def test_fns_list_handles_mixed_null_and_valued_vat(tmp_path: Path) -> None:
    # Defense against a regression where `row[8] or '—'` accidentally
    # blew up on NULL or emitted the literal None string.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_fn(container, fn="4000009500", vat="123456789012")
        _seed_fn(container, fn="4000009501", vat=None)
        r = client.get("/admin/ui/settings/fns")
        assert r.status_code == 200
        assert "123456789012" in r.text
        assert "None" not in r.text  # NULL must render as placeholder, not "None"
    finally:
        client.__exit__(None, None, None)
