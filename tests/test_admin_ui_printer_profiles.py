"""Proof tests for Phase 13 Step 4a — printer_profiles CRUD.

New "Принтери" tab + routes:
- GET  /admin/ui/settings/printers
- GET  /admin/ui/settings/printers/new
- POST /admin/ui/settings/printers/new
- GET  /admin/ui/settings/printers/{id}/edit
- POST /admin/ui/settings/printers/{id}/edit
- POST /admin/ui/settings/printers/{id}/delete

Pattern mirrors Phase 12a cashier CRUD (CSRF, audit, soft-delete).
"""
from __future__ import annotations

import json
import re
from pathlib import Path

from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]
_ADMIN_PASS = "printer-crud-pilot"
_CSRF_RE = re.compile(r'name="csrf_token"\s+value="([^"]+)"')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "printer.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV",
            "backend_profile_id": "b", "transport_profile_id": "t",
            "channel_owner": "printer-tests",
        },
        "admin_ui": {
            "enabled": True, "password": _ADMIN_PASS,
            "session_secret": "p" * 32,
        },
    })


def _logged_in(container: RuntimeContainer) -> TestClient:
    client = TestClient(create_app(container))
    client.__enter__()
    client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
    return client


def _csrf(client: TestClient, path: str) -> str:
    r = client.get(path)
    assert r.status_code == 200, f"{path}: {r.status_code}"
    m = _CSRF_RE.search(r.text)
    assert m, f"csrf missing at {path}"
    return m.group(1)


def _seed_printer(container: RuntimeContainer, *, name: str = "Kitchen",
                  profile_key: str = "tm-t88ii",
                  host: str = "192.168.1.50", port: int = 9100,
                  active: int = 1) -> int:
    with container.connect() as conn:
        cur = conn.execute(
            """INSERT INTO printer_profiles (
                name, profile_key, destination_type, host, port,
                paper_width_mm, timeout_ms, active
            ) VALUES (?, ?, 'tcp', ?, ?, 80, 5000, ?)""",
            (name, profile_key, host, port, active),
        )
        pid = cur.lastrowid
        conn.commit()
    assert pid is not None
    return pid


# ─── 1. Migration ────────────────────────────────────────────────────


def test_migration_022_creates_table(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)):
        pass
    with container.connect() as conn:
        cols = {row[1] for row in conn.execute(
            "PRAGMA table_info(printer_profiles)"
        ).fetchall()}
    assert "profile_key" in cols
    assert "destination_type" in cols
    assert "host" in cols and "port" in cols
    assert "active" in cols


# ─── 2. Tab + list page ──────────────────────────────────────────────


def test_settings_tabs_contain_printers(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        r = client.get("/admin/ui/settings/")
        assert r.status_code == 200
        assert "Принтери" in r.text
        assert "/admin/ui/settings/printers" in r.text
    finally:
        client.__exit__(None, None, None)


def test_list_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/settings/printers",
                       follow_redirects=False)
    assert r.status_code in (302, 303)


def test_list_renders_empty_and_with_rows(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        r = client.get("/admin/ui/settings/printers")
        assert r.status_code == 200
        assert "Принтери" in r.text

        _seed_printer(container, name="Kitchen", host="10.0.0.5")
        r = client.get("/admin/ui/settings/printers")
        assert "Kitchen" in r.text
        assert "10.0.0.5" in r.text
    finally:
        client.__exit__(None, None, None)


# ─── 3. Add printer ──────────────────────────────────────────────────


def test_add_form_renders(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        r = client.get("/admin/ui/settings/printers/new")
        assert r.status_code == 200
        for needle in (
            'name="name"',
            'name="profile_key"',
            'name="host"',
            'name="port"',
            'name="paper_width_mm"',
            'name="csrf_token"',
        ):
            assert needle in r.text, f"missing {needle}"
        # Dropdown should include bundled profile keys.
        assert "tm-t88ii" in r.text
        assert "pp8000l" in r.text
        assert "cts310ii" in r.text
    finally:
        client.__exit__(None, None, None)


def test_add_post_creates_row_and_audits(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        csrf = _csrf(client, "/admin/ui/settings/printers/new")
        r = client.post(
            "/admin/ui/settings/printers/new",
            data={
                "csrf_token": csrf,
                "name": "Cashier #1",
                "profile_key": "tm-t88ii",
                "host": "192.168.1.100",
                "port": "9100",
                "paper_width_mm": "80",
                "timeout_ms": "5000",
            },
            follow_redirects=False,
        )
        assert r.status_code in (302, 303), r.text
        assert r.headers["location"].rstrip("/") == "/admin/ui/settings/printers"
        with container.connect() as conn:
            row = conn.execute(
                """SELECT name, profile_key, destination_type, host, port,
                          paper_width_mm, active
                   FROM printer_profiles WHERE name = 'Cashier #1'"""
            ).fetchone()
        assert row is not None
        assert row[0] == "Cashier #1"
        assert row[1] == "tm-t88ii"
        assert row[2] == "tcp"
        assert row[3] == "192.168.1.100"
        assert row[4] == 9100
        assert row[5] == 80
        assert row[6] == 1

        with container.connect() as conn:
            audit = conn.execute(
                """SELECT event_type, event_payload_json FROM audit_log
                   WHERE entity_type='printer_profiles'
                   ORDER BY audit_id DESC LIMIT 1"""
            ).fetchone()
        assert audit is not None
        assert audit[0] == "printer_registered"
        payload = json.loads(audit[1])
        assert payload["profile_key"] == "tm-t88ii"
    finally:
        client.__exit__(None, None, None)


def test_add_rejects_missing_csrf(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        r = client.post(
            "/admin/ui/settings/printers/new",
            data={
                "name": "x", "profile_key": "tm-t88ii",
                "host": "1.1.1.1", "port": "9100",
                "paper_width_mm": "80", "timeout_ms": "5000",
            },
            follow_redirects=False,
        )
        assert r.status_code == 403
    finally:
        client.__exit__(None, None, None)


def test_add_rejects_empty_name(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        csrf = _csrf(client, "/admin/ui/settings/printers/new")
        r = client.post(
            "/admin/ui/settings/printers/new",
            data={
                "csrf_token": csrf, "name": "",
                "profile_key": "tm-t88ii",
                "host": "1.1.1.1", "port": "9100",
                "paper_width_mm": "80", "timeout_ms": "5000",
            },
            follow_redirects=False,
        )
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_add_rejects_bad_port(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        csrf = _csrf(client, "/admin/ui/settings/printers/new")
        for bad in ("abc", "0", "99999", "-1"):
            r = client.post(
                "/admin/ui/settings/printers/new",
                data={
                    "csrf_token": csrf, "name": "x",
                    "profile_key": "tm-t88ii",
                    "host": "1.1.1.1", "port": bad,
                    "paper_width_mm": "80", "timeout_ms": "5000",
                },
                follow_redirects=False,
            )
            assert r.status_code == 400, f"{bad!r}"
    finally:
        client.__exit__(None, None, None)


def test_add_rejects_bad_paper_width(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        csrf = _csrf(client, "/admin/ui/settings/printers/new")
        r = client.post(
            "/admin/ui/settings/printers/new",
            data={
                "csrf_token": csrf, "name": "x",
                "profile_key": "tm-t88ii",
                "host": "1.1.1.1", "port": "9100",
                "paper_width_mm": "100",  # not in {58, 80, 112}
                "timeout_ms": "5000",
            },
            follow_redirects=False,
        )
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_add_rejects_empty_host(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        csrf = _csrf(client, "/admin/ui/settings/printers/new")
        r = client.post(
            "/admin/ui/settings/printers/new",
            data={
                "csrf_token": csrf, "name": "x",
                "profile_key": "tm-t88ii",
                "host": "", "port": "9100",
                "paper_width_mm": "80", "timeout_ms": "5000",
            },
            follow_redirects=False,
        )
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


# ─── 4. Edit ─────────────────────────────────────────────────────────


def test_edit_form_prefills(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        pid = _seed_printer(container, name="Edit Me", host="10.10.10.10")
        r = client.get(f"/admin/ui/settings/printers/{pid}/edit")
        assert r.status_code == 200
        assert 'value="Edit Me"' in r.text
        assert 'value="10.10.10.10"' in r.text
    finally:
        client.__exit__(None, None, None)


def test_edit_form_404_for_unknown_id(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        r = client.get("/admin/ui/settings/printers/99999/edit")
        assert r.status_code == 404
    finally:
        client.__exit__(None, None, None)


def test_edit_post_updates_row(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        pid = _seed_printer(container)
        csrf = _csrf(client, f"/admin/ui/settings/printers/{pid}/edit")
        r = client.post(
            f"/admin/ui/settings/printers/{pid}/edit",
            data={
                "csrf_token": csrf,
                "name": "Renamed",
                "profile_key": "pp8000l",
                "host": "5.6.7.8",
                "port": "9100",
                "paper_width_mm": "58",
                "timeout_ms": "3000",
            },
            follow_redirects=False,
        )
        assert r.status_code in (302, 303), r.text
        with container.connect() as conn:
            row = conn.execute(
                """SELECT name, profile_key, host, paper_width_mm, timeout_ms
                   FROM printer_profiles WHERE id=?""",
                (pid,),
            ).fetchone()
        assert row[0] == "Renamed"
        assert row[1] == "pp8000l"
        assert row[2] == "5.6.7.8"
        assert row[3] == 58
        assert row[4] == 3000
    finally:
        client.__exit__(None, None, None)


def test_edit_post_requires_csrf(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        pid = _seed_printer(container)
        r = client.post(
            f"/admin/ui/settings/printers/{pid}/edit",
            data={
                "name": "x", "profile_key": "tm-t88ii",
                "host": "1.1.1.1", "port": "9100",
                "paper_width_mm": "80", "timeout_ms": "5000",
            },
            follow_redirects=False,
        )
        assert r.status_code == 403
    finally:
        client.__exit__(None, None, None)


def test_edit_audit_event(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        pid = _seed_printer(container, host="1.1.1.1")
        csrf = _csrf(client, f"/admin/ui/settings/printers/{pid}/edit")
        client.post(
            f"/admin/ui/settings/printers/{pid}/edit",
            data={
                "csrf_token": csrf,
                "name": "Renamed", "profile_key": "tm-t88ii",
                "host": "2.2.2.2", "port": "9100",
                "paper_width_mm": "80", "timeout_ms": "5000",
            },
            follow_redirects=False,
        )
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_type, event_payload_json FROM audit_log
                   WHERE entity_type='printer_profiles' AND entity_id=?
                   ORDER BY audit_id DESC LIMIT 1""",
                (str(pid),),
            ).fetchone()
        assert row is not None
        assert row[0] == "printer_updated"
        payload = json.loads(row[1])
        assert payload["previous_host"] == "1.1.1.1"
        assert payload["host"] == "2.2.2.2"
    finally:
        client.__exit__(None, None, None)


# ─── 5. Delete (soft) ────────────────────────────────────────────────


def test_delete_sets_active_zero(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        pid = _seed_printer(container)
        csrf = _csrf(client, f"/admin/ui/settings/printers/{pid}/edit")
        r = client.post(
            f"/admin/ui/settings/printers/{pid}/delete",
            data={"csrf_token": csrf},
            follow_redirects=False,
        )
        assert r.status_code in (302, 303)
        with container.connect() as conn:
            active = conn.execute(
                "SELECT active FROM printer_profiles WHERE id=?",
                (pid,),
            ).fetchone()[0]
        assert active == 0
    finally:
        client.__exit__(None, None, None)


def test_delete_requires_csrf(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        pid = _seed_printer(container)
        r = client.post(
            f"/admin/ui/settings/printers/{pid}/delete",
            data={},
            follow_redirects=False,
        )
        assert r.status_code == 403
        with container.connect() as conn:
            active = conn.execute(
                "SELECT active FROM printer_profiles WHERE id=?",
                (pid,),
            ).fetchone()[0]
        assert active == 1
    finally:
        client.__exit__(None, None, None)


def test_delete_audit_event(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        pid = _seed_printer(container)
        csrf = _csrf(client, f"/admin/ui/settings/printers/{pid}/edit")
        client.post(
            f"/admin/ui/settings/printers/{pid}/delete",
            data={"csrf_token": csrf},
            follow_redirects=False,
        )
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_type FROM audit_log
                   WHERE entity_type='printer_profiles' AND entity_id=?
                   ORDER BY audit_id DESC LIMIT 1""",
                (str(pid),),
            ).fetchone()
        assert row is not None
        assert row[0] == "printer_deleted"
    finally:
        client.__exit__(None, None, None)
