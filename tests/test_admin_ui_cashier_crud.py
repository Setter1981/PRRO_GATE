"""Proof tests for Phase 12a — cashier CRUD (sidecar_operators).

Routes:
- GET  /admin/ui/settings/fns/{fn}/operators/new   — add form
- POST /admin/ui/settings/fns/{fn}/operators/new   — INSERT + audit
- GET  /admin/ui/settings/operators/{id}/edit      — edit form
- POST /admin/ui/settings/operators/{id}/edit      — UPDATE + audit
- POST /admin/ui/settings/operators/{id}/delete    — soft delete (active=0)

Read-only list at /admin/ui/settings/operators exists (Phase 5) —
this phase adds the management actions alongside.
"""
from __future__ import annotations

import json
import re
from pathlib import Path
from unittest.mock import MagicMock, patch

from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]
_ADMIN_PASS = "cashier-crud-pilot"
_CSRF_RE = re.compile(r'name="csrf_token"\s+value="([^"]+)"')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "cashier.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV",
            "backend_profile_id": "b", "transport_profile_id": "t",
            "channel_owner": "cashier-tests",
        },
        "admin_ui": {
            "enabled": True, "password": _ADMIN_PASS,
            "session_secret": "c" * 32,
        },
    })


def _logged_in(container: RuntimeContainer) -> TestClient:
    client = TestClient(create_app(container))
    client.__enter__()
    client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
    return client


def _seed_fn(container: RuntimeContainer, fn: str = "4000012100") -> None:
    with container.connect() as conn:
        conn.execute(
            """INSERT INTO fiscal_number_config (
                fiscal_number, tax_number, fiscal_mode, tsp_enabled,
                offline_enabled, org_name, org_address,
                min_offline_codes, max_offline_codes
            ) VALUES (?, '12345678', 'test', 0, 1, 'TestCo', 'addr', 0, 0)""",
            (fn,),
        )
        conn.commit()


def _seed_operator(container: RuntimeContainer, fn: str = "4000012100",
                   name: str = "Петро П.",
                   inn: str = "1111111111",
                   path: str = "/tmp/seed.jks",
                   pw: str = "seedpw",
                   active: int = 1) -> int:
    with container.connect() as conn:
        cur = conn.execute(
            """INSERT INTO sidecar_operators (
                fiscal_number, operator_name, operator_inn, jks_path,
                jks_password, active
            ) VALUES (?, ?, ?, ?, ?, ?)""",
            (fn, name, inn, path, pw, active),
        )
        op_id = cur.lastrowid
        conn.commit()
    assert op_id is not None
    return op_id


def _csrf(client: TestClient, path: str) -> str:
    r = client.get(path)
    assert r.status_code == 200, f"{path}: {r.status_code}"
    m = _CSRF_RE.search(r.text)
    assert m, f"csrf not at {path}"
    return m.group(1)


# ─── 1. Add cashier — GET form ───────────────────────────────────────


def test_add_form_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get(
            "/admin/ui/settings/fns/4000012100/operators/new",
            follow_redirects=False,
        )
    assert r.status_code in (302, 303)


def test_add_form_404_for_unknown_fn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        r = client.get("/admin/ui/settings/fns/9999999999/operators/new")
        assert r.status_code == 404
    finally:
        client.__exit__(None, None, None)


def test_add_form_renders(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        r = client.get("/admin/ui/settings/fns/4000012100/operators/new")
        assert r.status_code == 200
        for needle in (
            'name="operator_name"',
            'name="operator_inn"',
            'name="jks_path"',
            'name="jks_password"',
            'name="csrf_token"',
            'method="post"',
        ):
            assert needle in r.text, f"missing {needle}"
        assert "4000012100" in r.text
    finally:
        client.__exit__(None, None, None)


def test_operators_list_has_add_link_per_fn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        _seed_operator(container)
        r = client.get("/admin/ui/settings/operators")
        assert "/admin/ui/settings/fns/4000012100/operators/new" in r.text
    finally:
        client.__exit__(None, None, None)


# ─── 2. Add cashier — POST ───────────────────────────────────────────


def _mock_cli_success() -> MagicMock:
    m = MagicMock()
    m.returncode = 0
    m.stdout = "added operator id=1 inn=2222222222 fn=4000012100 (xor_soft)\n"
    m.stderr = ""
    return m


def test_add_post_success_redirects(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012100/operators/new")
        with patch("subprocess.run", return_value=_mock_cli_success()):
            r = client.post(
                "/admin/ui/settings/fns/4000012100/operators/new",
                data={
                    "csrf_token": csrf,
                    "operator_name": "Іванов І.І.",
                    "operator_inn": "2222222222",
                    "jks_path": "/keys/iv.jks",
                    "jks_password": "mega-pw",
                },
                follow_redirects=False,
            )
        assert r.status_code in (302, 303), r.text
        assert r.headers["location"].rstrip("/") == "/admin/ui/settings/operators"
    finally:
        client.__exit__(None, None, None)


def test_add_post_writes_audit_row(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012100/operators/new")
        with patch("subprocess.run", return_value=_mock_cli_success()):
            client.post(
                "/admin/ui/settings/fns/4000012100/operators/new",
                data={
                    "csrf_token": csrf,
                    "operator_name": "Audit O.",
                    "operator_inn": "3333333333",
                    "jks_path": "/keys/a.jks",
                    "jks_password": "pw",
                },
                follow_redirects=False,
            )
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_type, severity, event_payload_json
                   FROM audit_log WHERE entity_type = 'sidecar_operators'
                   ORDER BY audit_id DESC LIMIT 1"""
            ).fetchone()
        assert row is not None
        assert row[0] == "cashier_registered"
        payload = json.loads(row[2])
        assert payload["fiscal_number"] == "4000012100"
        assert payload["operator_inn"] == "3333333333"
        assert "jks_password" not in row[2]
        assert "pw" not in row[2]
    finally:
        client.__exit__(None, None, None)


def test_add_post_requires_csrf(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        r = client.post(
            "/admin/ui/settings/fns/4000012100/operators/new",
            data={
                "operator_name": "x", "operator_inn": "4444444444",
                "jks_path": "/a", "jks_password": "b",
            },
            follow_redirects=False,
        )
        assert r.status_code == 403
    finally:
        client.__exit__(None, None, None)


def test_add_post_rejects_bad_inn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012100/operators/new")
        for bad in ("abc", "1", "12345678901"):
            r = client.post(
                "/admin/ui/settings/fns/4000012100/operators/new",
                data={
                    "csrf_token": csrf, "operator_name": "x",
                    "operator_inn": bad,
                    "jks_path": "/a", "jks_password": "b",
                },
                follow_redirects=False,
            )
            assert r.status_code == 400, f"{bad!r}"
    finally:
        client.__exit__(None, None, None)


def test_add_post_rejects_empty_jks_path(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012100/operators/new")
        r = client.post(
            "/admin/ui/settings/fns/4000012100/operators/new",
            data={
                "csrf_token": csrf, "operator_name": "x",
                "operator_inn": "5555555555",
                "jks_path": "", "jks_password": "b",
            },
            follow_redirects=False,
        )
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_add_post_404_on_unknown_fn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012100/operators/new")
        r = client.post(
            "/admin/ui/settings/fns/9999999999/operators/new",
            data={
                "csrf_token": csrf, "operator_name": "x",
                "operator_inn": "6666666666",
                "jks_path": "/a", "jks_password": "b",
            },
            follow_redirects=False,
        )
        assert r.status_code == 404
    finally:
        client.__exit__(None, None, None)


def test_add_post_calls_prro_admin_subprocess(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012100/operators/new")
        with patch("subprocess.run", return_value=_mock_cli_success()) as mock_run:
            r = client.post(
                "/admin/ui/settings/fns/4000012100/operators/new",
                data={
                    "csrf_token": csrf,
                    "operator_name": "CLI Test",
                    "operator_inn": "7777777711",
                    "jks_path": "/keys/cl.jks",
                    "jks_password": "cli-pw",
                },
                follow_redirects=False,
            )
        assert r.status_code in (302, 303), r.text
        mock_run.assert_called_once()
        call_args = mock_run.call_args[0][0]
        assert "add-operator" in call_args
        assert "4000012100" in call_args
        assert "7777777711" in call_args
    finally:
        client.__exit__(None, None, None)


def test_add_post_returns_409_on_fn_not_registered(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012100/operators/new")
        failed = MagicMock()
        failed.returncode = 1
        failed.stderr = "fiscal_number '9999999999' is not registered — run 'register-fn' first"
        with patch("subprocess.run", return_value=failed):
            r = client.post(
                "/admin/ui/settings/fns/4000012100/operators/new",
                data={
                    "csrf_token": csrf,
                    "operator_name": "X",
                    "operator_inn": "8888888888",
                    "jks_path": "/keys/x.jks",
                    "jks_password": "pw",
                },
                follow_redirects=False,
            )
        assert r.status_code == 409, r.text
    finally:
        client.__exit__(None, None, None)


# ─── 3. Edit cashier ─────────────────────────────────────────────────


def test_edit_form_prefills_existing(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container, name="Ed O.", inn="7777777777",
                               path="/keys/seed.jks")
        r = client.get(f"/admin/ui/settings/operators/{op_id}/edit")
        assert r.status_code == 200
        assert 'value="Ed O."' in r.text
        assert 'value="7777777777"' in r.text
        assert 'value="/keys/seed.jks"' in r.text
        # Password must NOT be echoed back in prefill.
        assert "seedpw" not in r.text
    finally:
        client.__exit__(None, None, None)


def test_edit_form_404_for_unknown_id(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        r = client.get("/admin/ui/settings/operators/999999/edit")
        assert r.status_code == 404
    finally:
        client.__exit__(None, None, None)


def test_edit_post_updates_row(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container, inn="8888888888")
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        r = client.post(
            f"/admin/ui/settings/operators/{op_id}/edit",
            data={
                "csrf_token": csrf,
                "operator_name": "Нове ім'я",
                "operator_inn": "8888888888",  # unchanged — INN is identity
                "jks_path": "/keys/new.jks",
                "jks_password": "",  # empty = keep existing
            },
            follow_redirects=False,
        )
        assert r.status_code in (302, 303)
        with container.connect() as conn:
            row = conn.execute(
                "SELECT operator_name, jks_path, jks_password FROM sidecar_operators WHERE id=?",
                (op_id,),
            ).fetchone()
        assert row[0] == "Нове ім'я"
        assert row[1] == "/keys/new.jks"
        assert row[2] == "seedpw"  # unchanged — empty password keeps old
    finally:
        client.__exit__(None, None, None)


def test_edit_post_updates_password_when_provided(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container, pw="old")
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        client.post(
            f"/admin/ui/settings/operators/{op_id}/edit",
            data={
                "csrf_token": csrf,
                "operator_name": "x", "operator_inn": "1111111111",
                "jks_path": "/tmp/seed.jks",
                "jks_password": "newpw",
            },
            follow_redirects=False,
        )
        with container.connect() as conn:
            pw = conn.execute(
                "SELECT jks_password FROM sidecar_operators WHERE id=?",
                (op_id,),
            ).fetchone()[0]
        assert pw == "newpw"
    finally:
        client.__exit__(None, None, None)


def test_edit_post_requires_csrf(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container)
        r = client.post(
            f"/admin/ui/settings/operators/{op_id}/edit",
            data={"operator_name": "x", "operator_inn": "1111111111",
                  "jks_path": "/a", "jks_password": "b"},
            follow_redirects=False,
        )
        assert r.status_code == 403
    finally:
        client.__exit__(None, None, None)


def test_edit_audit_row_on_update(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container, pw="old")
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        client.post(
            f"/admin/ui/settings/operators/{op_id}/edit",
            data={
                "csrf_token": csrf,
                "operator_name": "Новий", "operator_inn": "1111111111",
                "jks_path": "/tmp/seed.jks", "jks_password": "newer",
            },
            follow_redirects=False,
        )
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_type, event_payload_json FROM audit_log
                   WHERE entity_type='sidecar_operators' AND entity_id=?
                   ORDER BY audit_id DESC LIMIT 1""",
                (str(op_id),),
            ).fetchone()
        assert row is not None
        assert row[0] == "cashier_updated"
        payload = json.loads(row[1])
        assert payload["password_changed"] is True
        assert "newer" not in row[1]
        assert "old" not in row[1]
    finally:
        client.__exit__(None, None, None)


# ─── 4. Delete cashier (soft) ────────────────────────────────────────


def test_delete_sets_active_zero(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container)
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        r = client.post(
            f"/admin/ui/settings/operators/{op_id}/delete",
            data={"csrf_token": csrf},
            follow_redirects=False,
        )
        assert r.status_code in (302, 303), r.text
        with container.connect() as conn:
            active = conn.execute(
                "SELECT active FROM sidecar_operators WHERE id=?",
                (op_id,),
            ).fetchone()[0]
        assert active == 0
    finally:
        client.__exit__(None, None, None)


def test_delete_requires_csrf(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container)
        r = client.post(
            f"/admin/ui/settings/operators/{op_id}/delete",
            data={},
            follow_redirects=False,
        )
        assert r.status_code == 403
        with container.connect() as conn:
            active = conn.execute(
                "SELECT active FROM sidecar_operators WHERE id=?",
                (op_id,),
            ).fetchone()[0]
        assert active == 1
    finally:
        client.__exit__(None, None, None)


def test_delete_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container)
    finally:
        client.__exit__(None, None, None)
    with TestClient(create_app(container)) as anon:
        r = anon.post(
            f"/admin/ui/settings/operators/{op_id}/delete",
            data={"csrf_token": "x"},
            follow_redirects=False,
        )
        assert r.status_code in (302, 303, 403)
    with container.connect() as conn:
        active = conn.execute(
            "SELECT active FROM sidecar_operators WHERE id=?",
            (op_id,),
        ).fetchone()[0]
    assert active == 1


def test_delete_404_on_unknown_id(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container)
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        r = client.post(
            "/admin/ui/settings/operators/9999999/delete",
            data={"csrf_token": csrf},
            follow_redirects=False,
        )
        assert r.status_code == 404
    finally:
        client.__exit__(None, None, None)


def test_add_rejects_duplicate_active_inn_same_fn(tmp_path: Path) -> None:
    # Invariant #10: signer identity must be unambiguous — two ACTIVE
    # cashiers with same INN under the same FN are forbidden.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        _seed_operator(container, inn="5000000001")
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012100/operators/new")
        r = client.post(
            "/admin/ui/settings/fns/4000012100/operators/new",
            data={
                "csrf_token": csrf,
                "operator_name": "Duplicate",
                "operator_inn": "5000000001",  # already present
                "jks_path": "/keys/dup.jks",
                "jks_password": "pw",
            },
            follow_redirects=False,
        )
        assert r.status_code == 409, r.text
    finally:
        client.__exit__(None, None, None)


def test_add_allows_same_inn_under_different_fn(tmp_path: Path) -> None:
    # Constraint is per (FN, INN) — same person can be cashier on
    # multiple PRROs (fiscal_number), which is a real-world scenario.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container, fn="4000012101")
        _seed_fn(container, fn="4000012102")
        _seed_operator(container, fn="4000012101", inn="5000000002")
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012102/operators/new")
        with patch("subprocess.run", return_value=_mock_cli_success()):
            r = client.post(
                "/admin/ui/settings/fns/4000012102/operators/new",
                data={
                    "csrf_token": csrf,
                    "operator_name": "Same person",
                    "operator_inn": "5000000002",
                    "jks_path": "/keys/x.jks",
                    "jks_password": "pw",
                },
                follow_redirects=False,
            )
        assert r.status_code in (302, 303), r.text
    finally:
        client.__exit__(None, None, None)


def test_add_allows_new_after_soft_delete(tmp_path: Path) -> None:
    # Re-hiring the same INN after deactivation must succeed — the
    # partial unique index only covers active=1.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        _seed_operator(container, inn="5000000003", active=0)
        csrf = _csrf(client, "/admin/ui/settings/fns/4000012100/operators/new")
        with patch("subprocess.run", return_value=_mock_cli_success()):
            r = client.post(
                "/admin/ui/settings/fns/4000012100/operators/new",
                data={
                    "csrf_token": csrf,
                    "operator_name": "Re-hired",
                    "operator_inn": "5000000003",
                    "jks_path": "/keys/rh.jks",
                    "jks_password": "pw",
                },
                follow_redirects=False,
            )
        assert r.status_code in (302, 303), r.text
    finally:
        client.__exit__(None, None, None)


def test_delete_blocked_when_shift_open(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container, inn="6000000001")
        with container.connect() as conn:
            conn.execute(
                """INSERT INTO node_state (
                    node_id, fiscal_number, mode, shift_state, next_lnd
                ) VALUES ('n1', '4000012100', 'ONLINE', 'OPENED', 1)"""
            )
            conn.commit()
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        r = client.post(
            f"/admin/ui/settings/operators/{op_id}/delete",
            data={"csrf_token": csrf},
            follow_redirects=False,
        )
        assert r.status_code == 409
        with container.connect() as conn:
            active = conn.execute(
                "SELECT active FROM sidecar_operators WHERE id=?",
                (op_id,),
            ).fetchone()[0]
        assert active == 1  # not deleted
    finally:
        client.__exit__(None, None, None)


def test_edit_inn_blocked_when_shift_open(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container, inn="6000000002")
        with container.connect() as conn:
            conn.execute(
                """INSERT INTO node_state (
                    node_id, fiscal_number, mode, shift_state, next_lnd
                ) VALUES ('n2', '4000012100', 'ONLINE', 'OPENED', 1)"""
            )
            conn.commit()
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        r = client.post(
            f"/admin/ui/settings/operators/{op_id}/edit",
            data={
                "csrf_token": csrf,
                "operator_name": "same",
                "operator_inn": "9999999999",  # INN mutation
                "jks_path": "/tmp/seed.jks",
                "jks_password": "",
            },
            follow_redirects=False,
        )
        assert r.status_code == 409
    finally:
        client.__exit__(None, None, None)


def test_edit_rename_allowed_during_open_shift(tmp_path: Path) -> None:
    # Pure name rename — no identity change — must still work during
    # an open shift.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container, inn="6000000003")
        with container.connect() as conn:
            conn.execute(
                """INSERT INTO node_state (
                    node_id, fiscal_number, mode, shift_state, next_lnd
                ) VALUES ('n3', '4000012100', 'ONLINE', 'OPENED', 1)"""
            )
            conn.commit()
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        r = client.post(
            f"/admin/ui/settings/operators/{op_id}/edit",
            data={
                "csrf_token": csrf,
                "operator_name": "New name",
                "operator_inn": "6000000003",  # unchanged
                "jks_path": "/tmp/seed.jks",   # unchanged
                "jks_password": "",
            },
            follow_redirects=False,
        )
        assert r.status_code in (302, 303), r.text
        with container.connect() as conn:
            name = conn.execute(
                "SELECT operator_name FROM sidecar_operators WHERE id=?",
                (op_id,),
            ).fetchone()[0]
        assert name == "New name"
    finally:
        client.__exit__(None, None, None)


def test_edit_audit_records_previous_inn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container, inn="7000000001")
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        client.post(
            f"/admin/ui/settings/operators/{op_id}/edit",
            data={
                "csrf_token": csrf,
                "operator_name": "x",
                "operator_inn": "7000000009",  # new INN
                "jks_path": "/tmp/seed.jks",
                "jks_password": "",
            },
            follow_redirects=False,
        )
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_payload_json FROM audit_log
                   WHERE entity_type='sidecar_operators' AND entity_id=?
                   ORDER BY audit_id DESC LIMIT 1""",
                (str(op_id),),
            ).fetchone()
        payload = json.loads(row[0])
        assert payload["previous_operator_inn"] == "7000000001"
        assert payload["operator_inn"] == "7000000009"
        assert payload["inn_changed"] is True
    finally:
        client.__exit__(None, None, None)


def test_delete_audit_event(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in(container)
    try:
        _seed_fn(container)
        op_id = _seed_operator(container)
        csrf = _csrf(client, f"/admin/ui/settings/operators/{op_id}/edit")
        client.post(
            f"/admin/ui/settings/operators/{op_id}/delete",
            data={"csrf_token": csrf},
            follow_redirects=False,
        )
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_type FROM audit_log
                   WHERE entity_type='sidecar_operators' AND entity_id=?
                   ORDER BY audit_id DESC LIMIT 1""",
                (str(op_id),),
            ).fetchone()
        assert row is not None and row[0] == "cashier_deleted"
    finally:
        client.__exit__(None, None, None)
