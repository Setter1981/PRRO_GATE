"""Proof tests for admin UI add-FN form (Phase 6).

`GET /admin/ui/settings/fns/new`  → render form
`POST /admin/ui/settings/fns/new` → validate, INSERT atomically into
    `fiscal_number_config` (+ optional first cashier into
    `sidecar_operators`), write audit row, redirect to `/settings/fns`.

Read-only viewer (Phase 5) stays untouched; this adds the first
mutating endpoint under /admin/ui/settings/.
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

_ADMIN_PASS = "add-fn-pilot"
_SESSION_SECRET = "y" * 32


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "addfn.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV-0001",
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "add-fn-tests",
        },
        "admin_ui": {
            "enabled": True,
            "password": _ADMIN_PASS,
            "session_secret": _SESSION_SECRET,
        },
    })


_CSRF_RE = re.compile(r'name="csrf_token"\s+value="([^"]+)"')


def _fetch_csrf(client: TestClient) -> str:
    r = client.get("/admin/ui/settings/fns/new")
    m = _CSRF_RE.search(r.text)
    assert m, f"CSRF token not found in form render: status={r.status_code}"
    return m.group(1)


def _logged_in_client(container: RuntimeContainer) -> TestClient:
    client = TestClient(create_app(container))
    client.__enter__()
    client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
    return client


def _submit(client: TestClient, **overrides: str):
    """POST form with a fresh CSRF token; merge overrides onto defaults."""
    data = {
        "csrf_token": _fetch_csrf(client),
        "fiscal_number": "4000000000",
        "tax_number": "12345678",
        "fiscal_mode": "test",
        "org_name": "ТОВ Демо",
        "point_name": "Точка #1",
        "point_address": "м. Київ",
        "tsp_enabled": "0",
        "offline_enabled": "1",
        "min_offline_codes": "0",
        "max_offline_codes": "0",
    }
    data.update(overrides)
    return client.post("/admin/ui/settings/fns/new", data=data, follow_redirects=False)


# ─── 1. GET form ─────────────────────────────────────────────────────


def test_new_fn_form_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/settings/fns/new", follow_redirects=False)
    assert r.status_code in (302, 303)


def test_new_fn_form_renders(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/fns/new")
        assert r.status_code == 200
        for needle in (
            'name="fiscal_number"',
            'name="tax_number"',
            'name="org_name"',
            'name="fiscal_mode"',
            'name="tsp_enabled"',
            'name="offline_enabled"',
            'method="post"',
        ):
            assert needle in r.text, f"missing: {needle}"
        # Optional-cashier section present.
        assert 'name="operator_name"' in r.text
        assert 'name="jks_path"' in r.text
    finally:
        client.__exit__(None, None, None)


def test_new_fn_form_link_visible_on_fns_page(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/fns")
        assert "/admin/ui/settings/fns/new" in r.text
    finally:
        client.__exit__(None, None, None)


# ─── 2. POST happy path ──────────────────────────────────────────────


def test_post_new_fn_minimal_success_redirects(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(client, fiscal_number="4000000001")
        assert r.status_code in (302, 303), r.text
        assert r.headers["location"].rstrip("/") == "/admin/ui/settings/fns"

        with container.connect() as conn:
            row = conn.execute(
                """SELECT fiscal_number, tax_number, fiscal_mode,
                          org_name, offline_enabled
                   FROM fiscal_number_config WHERE fiscal_number = ?""",
                ("4000000001",),
            ).fetchone()
        assert row is not None
        assert row[0] == "4000000001"
        assert row[1] == "12345678"
        assert row[2] == "test"
        assert row[3] == "ТОВ Демо"
        assert row[4] == 1
    finally:
        client.__exit__(None, None, None)


def test_post_new_fn_with_cashier_creates_sidecar_operator(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(
            client,
            fiscal_number="4000000002",
            operator_name="Іванов І.І.",
            operator_inn="1111111111",
            jks_path="/tmp/fake.jks",
            jks_password="unsafe-plain",
        )
        assert r.status_code in (302, 303), r.text

        with container.connect() as conn:
            op = conn.execute(
                """SELECT fiscal_number, operator_name, operator_inn,
                          jks_path, active
                   FROM sidecar_operators WHERE fiscal_number = ?""",
                ("4000000002",),
            ).fetchone()
        assert op is not None
        assert op[1] == "Іванов І.І."
        assert op[2] == "1111111111"
        assert op[3] == "/tmp/fake.jks"
        assert op[4] == 1
    finally:
        client.__exit__(None, None, None)


def test_post_new_fn_writes_audit_row(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _submit(client, fiscal_number="4000000003", org_name="ТОВ Аудит")
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_type, entity_id, severity, event_payload_json
                   FROM audit_log WHERE entity_type = 'fiscal_number_config'
                     AND entity_id = ?""",
                ("4000000003",),
            ).fetchone()
        assert row is not None, "audit_log row missing"
        assert row[0] == "fn_registered"
        assert row[2] == "INFO"
        payload = json.loads(row[3])
        assert payload["fiscal_mode"] == "test"
        assert payload["has_cashier"] is False
        assert payload["org_name"] == "ТОВ Аудит"
    finally:
        client.__exit__(None, None, None)


def test_audit_row_does_not_contain_jks_password(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _submit(
            client,
            fiscal_number="4000000010",
            operator_name="Касир",
            operator_inn="2222222222",
            jks_path="/tmp/x.jks",
            jks_password="SUPER-SECRET-LEAK-TEST",
        )
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_payload_json FROM audit_log
                   WHERE entity_id = '4000000010'"""
            ).fetchone()
        assert row is not None
        assert "SUPER-SECRET-LEAK-TEST" not in row[0]
        assert "jks_password" not in row[0]
    finally:
        client.__exit__(None, None, None)


def test_re_render_does_not_echo_jks_password(tmp_path: Path) -> None:
    # On validation-error re-render, the `jks_password` value must not
    # appear in the returned HTML.  Trigger an error by making
    # operator_inn invalid while filling in a password.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(
            client,
            fiscal_number="4000000011",
            operator_name="Касир",
            operator_inn="bad",   # invalid → form re-render
            jks_path="/tmp/x.jks",
            jks_password="ECHO-TEST-PW",
        )
        assert r.status_code == 400
        assert "ECHO-TEST-PW" not in r.text
    finally:
        client.__exit__(None, None, None)


# ─── 3. Validation ───────────────────────────────────────────────────


def test_post_rejects_non_10_digit_fn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(client, fiscal_number="12345")
        assert r.status_code == 400
        assert "фіскального номера" in r.text
        with container.connect() as conn:
            count = conn.execute(
                "SELECT COUNT(*) FROM fiscal_number_config"
            ).fetchone()[0]
        assert count == 0
    finally:
        client.__exit__(None, None, None)


def test_post_rejects_non_numeric_fn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(client, fiscal_number="400000000X")
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_post_rejects_bad_tin(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(client, fiscal_number="4000000099", tax_number="abc")
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_post_rejects_unknown_mode(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(client, fiscal_number="4000000099", fiscal_mode="staging")
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_post_rejects_duplicate_fn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r1 = _submit(client, fiscal_number="4000000004")
        assert r1.status_code in (302, 303)
        r2 = _submit(client, fiscal_number="4000000004")
        assert r2.status_code == 409
        with container.connect() as conn:
            count = conn.execute(
                "SELECT COUNT(*) FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000000004",),
            ).fetchone()[0]
        assert count == 1
    finally:
        client.__exit__(None, None, None)


def test_post_rejects_bad_offline_code_range(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(client, fiscal_number="4000000005",
                    min_offline_codes="500", max_offline_codes="100")
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_post_rejects_oversized_field(tmp_path: Path) -> None:
    # Defense-in-depth: server-side max-length must cap operator-supplied
    # strings even when client-side `maxlength=` is bypassed.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(client, fiscal_number="4000000020", org_name="x" * 10_000)
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_post_without_csrf_rejected(tmp_path: Path) -> None:
    # SameSite=Lax does NOT block top-level cross-origin form POST, so a
    # CSRF token is required.  POST without it must 403.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.post(
            "/admin/ui/settings/fns/new",
            data={
                "fiscal_number": "4000000030",
                "tax_number": "12345678",
                "fiscal_mode": "test",
                # no csrf_token on purpose
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
            },
            follow_redirects=False,
        )
        assert r.status_code == 403
        with container.connect() as conn:
            count = conn.execute(
                "SELECT COUNT(*) FROM fiscal_number_config"
            ).fetchone()[0]
        assert count == 0
    finally:
        client.__exit__(None, None, None)


def test_post_with_wrong_csrf_rejected(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        client.get("/admin/ui/settings/fns/new")  # mint a real token
        r = client.post(
            "/admin/ui/settings/fns/new",
            data={
                "csrf_token": "not-the-session-token",
                "fiscal_number": "4000000031",
                "tax_number": "12345678",
                "fiscal_mode": "test",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
            },
            follow_redirects=False,
        )
        assert r.status_code == 403
    finally:
        client.__exit__(None, None, None)


# ─── 4. Auth + mutating guard ────────────────────────────────────────


def test_post_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.post(
            "/admin/ui/settings/fns/new",
            data={"fiscal_number": "4000000006", "tax_number": "12345678"},
            follow_redirects=False,
        )
        assert r.status_code in (302, 303)
        with container.connect() as conn:
            count = conn.execute(
                "SELECT COUNT(*) FROM fiscal_number_config"
            ).fetchone()[0]
        assert count == 0


def test_atomic_rollback_on_cashier_validation_error(tmp_path: Path) -> None:
    # If operator block is present but invalid, the whole request fails
    # and NO fiscal_number_config row is written.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(
            client,
            fiscal_number="4000000007",
            operator_name="Касир",
            operator_inn="bad-inn",  # non-digit
            jks_path="/tmp/x.jks",
            jks_password="pw",
        )
        assert r.status_code == 400
        with container.connect() as conn:
            fn_count = conn.execute(
                "SELECT COUNT(*) FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000000007",),
            ).fetchone()[0]
            op_count = conn.execute(
                "SELECT COUNT(*) FROM sidecar_operators WHERE fiscal_number=?",
                ("4000000007",),
            ).fetchone()[0]
        assert fn_count == 0
        assert op_count == 0
    finally:
        client.__exit__(None, None, None)


def test_post_escapes_error_message_for_xss(tmp_path: Path) -> None:
    # Error re-render must escape user input.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = _submit(client, fiscal_number="<script>alert(1)</script>")
        assert r.status_code == 400
        assert "<script>alert(1)</script>" not in r.text
    finally:
        client.__exit__(None, None, None)
