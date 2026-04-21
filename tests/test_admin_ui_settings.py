"""Proof tests for admin UI settings viewer (Phase 5, read-only).

Landing page with WebCheck-style tabs: FN / Operators / Node / DPS.
All pages require auth; all display current config without mutation.
"""
from __future__ import annotations

from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

_ADMIN_PASS = "settings-pilot"
_SESSION_SECRET = "x" * 32


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "settings.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV-0001",
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "settings-tests",
        },
        "admin_ui": {
            "enabled": True,
            "password": _ADMIN_PASS,
            "session_secret": _SESSION_SECRET,
        },
    })


def _logged_in_client(container: RuntimeContainer) -> TestClient:
    client = TestClient(create_app(container))
    client.__enter__()
    client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
    return client


# ─── 1. Landing + nav ────────────────────────────────────────────────


def test_settings_landing_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/settings/", follow_redirects=False)
    assert r.status_code in (302, 303)


def test_settings_landing_authed_renders_tabs(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/")
        assert r.status_code == 200
        for needle in ("Фіскальні номери", "Касири", "Стан", "ДПС"):
            assert needle in r.text
    finally:
        client.__exit__(None, None, None)


def test_main_nav_has_settings_link(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/")
        assert "/admin/ui/settings/" in r.text
    finally:
        client.__exit__(None, None, None)


# ─── 2. FN list ──────────────────────────────────────────────────────


def test_fns_page_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/settings/fns", follow_redirects=False)
    assert r.status_code in (302, 303)


def test_fns_page_shows_seeded_fn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        with container.connect() as conn:
            conn.execute(
                """INSERT INTO fiscal_number_config (
                    fiscal_number, tax_number, fiscal_mode,
                    tsp_enabled, offline_enabled, org_name,
                    min_offline_codes, max_offline_codes
                ) VALUES ('FN-UI-0001', '9876543210', 'test',
                          0, 1, 'Test Merchant LLC', 100, 1000)""",
            )
            conn.commit()
        r = client.get("/admin/ui/settings/fns")
        assert r.status_code == 200
        assert "FN-UI-0001" in r.text
        assert "9876543210" in r.text
    finally:
        client.__exit__(None, None, None)


# ─── 3. Operators page ───────────────────────────────────────────────


def test_operators_page_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/settings/operators", follow_redirects=False)
    assert r.status_code in (302, 303)


def test_operators_page_accessible(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/operators")
        assert r.status_code == 200
        assert "Касир" in r.text or "Оператор" in r.text
    finally:
        client.__exit__(None, None, None)


def test_operators_page_shows_seeded_row(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        with container.connect() as conn:
            conn.execute(
                """INSERT INTO operator_certs (
                    fiscal_number, cert_fingerprint, ski_hex, cert_der,
                    subject_dn, fetched_at, source
                ) VALUES ('FN-OP-0001',
                          'ab' || hex(randomblob(31)),
                          'CAFEBABECAFEBABECAFEBABECAFEBABECAFEBABECAFEBABECAFEBABECAFEBABE',
                          x'00',
                          'CN=Test Cashier,O=Test',
                          CURRENT_TIMESTAMP,
                          'container')""",
            )
            conn.commit()
        r = client.get("/admin/ui/settings/operators")
        assert r.status_code == 200
        assert "FN-OP-0001" in r.text
        assert "CAFEBABE" in r.text  # at least the prefix (template truncates)
    finally:
        client.__exit__(None, None, None)


def test_operators_page_empty_db_renders(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/operators")
        assert r.status_code == 200
        assert "не зареєстровано" in r.text.lower() or "Касир" in r.text
    finally:
        client.__exit__(None, None, None)


# ─── 4. Node state page ──────────────────────────────────────────────


def test_node_state_page_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/settings/node", follow_redirects=False)
    assert r.status_code in (302, 303)


def test_node_state_page_accessible(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/node")
        assert r.status_code == 200
        assert "ONLINE" in r.text or "Стан" in r.text
    finally:
        client.__exit__(None, None, None)


def test_node_state_page_shows_seeded_row(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        with container.connect() as conn:
            conn.execute(
                """INSERT INTO node_state (
                    node_id, fiscal_number, mode, shift_state, next_lnd
                ) VALUES ('node-ui-1', 'FN-NODE-UI-1', 'ONLINE', 'CLOSED', 42)""",
            )
            conn.commit()
        r = client.get("/admin/ui/settings/node")
        assert r.status_code == 200
        assert "FN-NODE-UI-1" in r.text
        assert "ONLINE" in r.text
        assert "42" in r.text  # next_lnd
    finally:
        client.__exit__(None, None, None)


def test_node_state_page_empty_db_renders(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/node")
        assert r.status_code == 200
    finally:
        client.__exit__(None, None, None)


# ─── 2b. FN list (extra) ─────────────────────────────────────────────


def test_fns_page_empty_db_renders(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/fns")
        assert r.status_code == 200
    finally:
        client.__exit__(None, None, None)


def test_fns_page_escapes_injected_html(tmp_path: Path) -> None:
    # Defence-in-depth: even though fiscal_number is externally-validated,
    # verify Jinja autoescape actually fires for DB content.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        with container.connect() as conn:
            conn.execute(
                """INSERT INTO fiscal_number_config (
                    fiscal_number, tax_number, fiscal_mode,
                    tsp_enabled, offline_enabled, org_name,
                    min_offline_codes, max_offline_codes
                ) VALUES ('<script>alert(1)</script>', '1234567890',
                          'test', 0, 1, 'O&M "Co"', 0, 0)""",
            )
            conn.commit()
        r = client.get("/admin/ui/settings/fns")
        assert r.status_code == 200
        # Raw script tag must NOT appear; escaped form MUST.
        assert "<script>alert(1)</script>" not in r.text
        assert "&lt;script&gt;alert(1)&lt;/script&gt;" in r.text
        # Org name with ampersand + quotes is escaped too.
        assert 'O&M "Co"' not in r.text
    finally:
        client.__exit__(None, None, None)


# ─── 5. DPS config page ──────────────────────────────────────────────


def test_dps_config_page_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/settings/dps", follow_redirects=False)
    assert r.status_code in (302, 303)


def test_dps_config_page_shows_crypto_provider(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/dps")
        assert r.status_code == 200
        # Shows the crypto.provider value (default "passthrough" in tests).
        assert "passthrough" in r.text.lower() or "sidecar" in r.text.lower() or "crypto" in r.text.lower()
    finally:
        client.__exit__(None, None, None)


def test_redact_url_userinfo_unit() -> None:
    from prro_gateway.admin_ui.routes import _redact_url_userinfo as red

    assert red(None) == "—"
    assert red("") == "—"
    # No userinfo — pass through unchanged.
    assert red("https://host:9443/v1") == "https://host:9443/v1"
    # user:pass — redacted, host:port preserved.
    assert red("https://u:p@host:9443/v1") == "https://***@host:9443/v1"
    # username only.
    assert red("https://u@host/v1") == "https://***@host/v1"
    # IPv6 — brackets preserved.
    out = red("https://alice:secret@[::1]:8443/v1")
    assert "secret" not in out and "alice" not in out
    assert "[::1]:8443" in out
    # Malformed port — must NOT raise and must NOT leak credentials.
    out_bad = red("https://u:p@h:notaport/")
    assert "u:p" not in out_bad and "p@" not in out_bad


def test_dps_sidecar_url_redacts_userinfo(tmp_path: Path) -> None:
    # Security: if operator embeds user:pass in sidecar URL the UI must
    # not echo credentials back.
    cfg = AppConfig.from_mapping({
        **_config(tmp_path).model_dump(mode="json", exclude_none=True),
        "crypto": {
            "provider": "sidecar",
            "sidecar_url": "https://alice:supersecretpw@sidecar.local:8443/v1",
        },
    })
    container = RuntimeContainer(cfg)
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/dps")
        assert r.status_code == 200
        assert "supersecretpw" not in r.text
        assert "alice" not in r.text
        # But host should still be visible so operators can verify endpoint.
        assert "sidecar.local" in r.text
    finally:
        client.__exit__(None, None, None)


def test_dps_config_shows_token_presence_when_configured(tmp_path: Path) -> None:
    cfg = AppConfig.from_mapping({
        **_config(tmp_path).model_dump(mode="json", exclude_none=True),
        "ingress": {
            "maria304": {
                "enabled": True,
                "shared_token": "some-token-value",
                "response_timeout_seconds": 10,
            },
        },
    })
    container = RuntimeContainer(cfg)
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/dps")
        assert r.status_code == 200
        # Template must render a positive presence indicator.
        assert "Налаштовано" in r.text or "configured" in r.text.lower()
    finally:
        client.__exit__(None, None, None)


def test_dps_config_does_not_leak_shared_tokens(tmp_path: Path) -> None:
    # Security: `ingress.maria304.shared_token` is a secret.  UI view
    # must NOT echo the token value back — only show presence/absence.
    cfg = AppConfig.from_mapping({
        **_config(tmp_path).model_dump(mode="json", exclude_none=True),
        "ingress": {
            "maria304": {
                "enabled": True,
                "shared_token": "super-secret-token-DO-NOT-LEAK",
                "response_timeout_seconds": 10,
            },
        },
    })
    container = RuntimeContainer(cfg)
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/dps")
        assert "super-secret-token-DO-NOT-LEAK" not in r.text, (
            "shared token must be redacted in settings UI"
        )
    finally:
        client.__exit__(None, None, None)


# ─── 6. No write operations ──────────────────────────────────────────


def test_settings_routes_have_no_mutating_endpoints_yet(tmp_path: Path) -> None:
    # Phase 5 is read-only.  No mutating HTTP method may be accepted
    # under /admin/ui/settings/*.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        paths = (
            "/admin/ui/settings/fns",
            "/admin/ui/settings/operators",
            "/admin/ui/settings/node",
            "/admin/ui/settings/dps",
        )
        for path in paths:
            for method, call in (
                ("POST", lambda p=path: client.post(p, data={"x": "y"})),
                ("PUT", lambda p=path: client.put(p, data={"x": "y"})),
                ("PATCH", lambda p=path: client.patch(p, data={"x": "y"})),
                ("DELETE", lambda p=path: client.delete(p)),
            ):
                r = call()
                assert r.status_code in (404, 405), (
                    f"{method} {path} accepted: {r.status_code}"
                )
    finally:
        client.__exit__(None, None, None)


def test_admin_ui_misconfigured_short_secret_returns_503(tmp_path: Path) -> None:
    # If admin_ui.session_secret is missing/short the UI must refuse
    # to mount and return 503 rather than run with a forgeable cookie.
    cfg = AppConfig.from_mapping({
        **_config(tmp_path).model_dump(mode="json", exclude_none=True),
        "admin_ui": {
            "enabled": True,
            "password": _ADMIN_PASS,
            "session_secret": "too-short",  # 9 chars
        },
    })
    container = RuntimeContainer(cfg)
    with TestClient(create_app(container)) as client:
        for path in ("/admin/ui/settings/",
                     "/admin/ui/settings/fns",
                     "/admin/ui/settings/dps"):
            r = client.get(path)
            assert r.status_code == 503, f"{path}: {r.status_code}"


# ─── 7. Self-contained ───────────────────────────────────────────────


def test_settings_pages_no_external_cdn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        for path in ("/admin/ui/settings/", "/admin/ui/settings/fns",
                     "/admin/ui/settings/operators", "/admin/ui/settings/node",
                     "/admin/ui/settings/dps"):
            r = client.get(path)
            if r.status_code == 200:
                for forbidden in ("cdn.jsdelivr", "cdnjs.cloudflare",
                                   "fonts.googleapis", "unpkg.com"):
                    assert forbidden not in r.text, f"{path}: {forbidden}"
    finally:
        client.__exit__(None, None, None)
