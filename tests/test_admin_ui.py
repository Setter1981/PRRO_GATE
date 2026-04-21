"""Proof tests for admin UI skeleton (AdminUI Phase 4).

Scope: login form + session auth + dashboard + documents list/detail.
No settings forms yet (Phase 5).  Self-contained CSS; no CDN links.
"""
from __future__ import annotations

from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

_ADMIN_PASS = "p1lot-secret"
_SESSION_SECRET = "development-only-session-secret-32-chars-minimum-length-for-hmac"


def _config(tmp_path: Path, **admin_overrides) -> AppConfig:
    admin_cfg = {
        "enabled": True,
        "password": _ADMIN_PASS,
        "session_secret": _SESSION_SECRET,
    }
    admin_cfg.update(admin_overrides)
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "admin-ui.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV-0001",
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "admin-ui-tests",
        },
        "admin_ui": admin_cfg,
    })


# ─── 1. Login page ───────────────────────────────────────────────────


def test_login_page_accessible_without_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/login")
    assert r.status_code == 200
    assert "<form" in r.text
    assert "password" in r.text.lower()


def test_login_page_is_html() -> None:
    # Content-Type header should be text/html; no JSON leaks here.
    container = RuntimeContainer(_config(Path("/tmp/admin-ui-login")))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/login")
    assert "text/html" in r.headers.get("content-type", "").lower()


# ─── 2. POST login — correct/wrong password ──────────────────────────


def test_correct_password_logs_in_and_redirects_to_dashboard(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.post(
            "/admin/ui/login",
            data={"password": _ADMIN_PASS},
            follow_redirects=False,
        )
    # 302/303 redirect to /admin/ui/
    assert r.status_code in (302, 303)
    assert r.headers["location"].endswith("/admin/ui/") or r.headers["location"].endswith("/admin/ui")
    # Session cookie set
    assert "session" in {c.name for c in r.cookies.jar}


def test_wrong_password_rejected(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.post(
            "/admin/ui/login",
            data={"password": "wrong"},
            follow_redirects=False,
        )
    assert r.status_code in (401, 403)
    # No session cookie on failed auth.
    assert not any(c.name == "session" for c in r.cookies.jar)


# ─── 3. Dashboard requires auth ──────────────────────────────────────


def test_dashboard_without_auth_redirects_to_login(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/", follow_redirects=False)
    assert r.status_code in (302, 303)
    assert "/admin/ui/login" in r.headers["location"]


def test_dashboard_with_auth_returns_200(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
        r = client.get("/admin/ui/")
    assert r.status_code == 200
    assert "Панель" in r.text or "Dashboard" in r.text or "PRRO Gateway" in r.text


def test_dashboard_shows_nav_bar(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
        r = client.get("/admin/ui/")
    # Navigation links to key sections.
    text = r.text.lower()
    assert "/admin/ui/documents" in r.text or "documents" in text
    assert "logout" in text or "вихід" in text.lower()


# ─── 4. Documents list + detail ──────────────────────────────────────


def test_documents_list_accessible_after_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
        r = client.get("/admin/ui/documents/")
    assert r.status_code == 200
    assert "<table" in r.text or "Документ" in r.text or "фіск" in r.text.lower()


def test_documents_list_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/documents/", follow_redirects=False)
    assert r.status_code in (302, 303)


def test_documents_list_shows_seeded_documents(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        with container.connect() as conn:
            # Seed an inbox + document pair.
            conn.execute(
                """INSERT INTO ingress_inbox (
                    request_id, idempotency_key, protocol, operation_type,
                    fiscal_number, payload_json, payload_sha256, status
                ) VALUES (?, ?, 'MARIA_304_NATIVE', 'SELL', ?, '{}', 'x', 'DONE')""",
                ("req-admin-1", "idem-admin-1", "FN-DEV-0001"),
            )
            conn.execute(
                """INSERT INTO fiscal_documents (
                    document_id, request_id, fiscal_number, lnd, doc_type, state,
                    backend_profile_id, transport_profile_id, fs_mode,
                    receipt_type, business_ts, payload_json, payload_sha256,
                    total_sum, server_fiscal_no
                ) VALUES ('doc-admin-1', 'req-admin-1', 'FN-DEV-0001', 1, 'SELL', 'KVT2',
                          'backend_checkbox_default', 'transport_checkbox_rest_default', 'ONLINE',
                          'SELL', '2026-04-21T10:00:00+00:00', '{}', 'x', 12345, '0000000001')""",
            )
            conn.commit()
        client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
        r = client.get("/admin/ui/documents/")
    assert "doc-admin-1" in r.text or "0000000001" in r.text


# ─── 5. Logout ───────────────────────────────────────────────────────


def test_logout_clears_session(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
        # Confirm logged in.
        r1 = client.get("/admin/ui/", follow_redirects=False)
        assert r1.status_code == 200
        # Logout.
        client.get("/admin/ui/logout", follow_redirects=False)
        # Now dashboard should redirect again.
        r2 = client.get("/admin/ui/", follow_redirects=False)
    assert r2.status_code in (302, 303)


# ─── 6. Security — self-contained + misconfig ────────────────────────


def test_login_page_has_no_external_cdn_links(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/login")
    # Operators may run on isolated networks — self-contained CSS/JS
    # mandatory.  No bootstrap CDN, no Google Fonts, no jsdelivr.
    for forbidden in ("cdn.jsdelivr", "cdnjs.cloudflare", "fonts.googleapis",
                      "unpkg.com", "bootstrapcdn"):
        assert forbidden not in r.text, f"external CDN reference: {forbidden}"


def test_missing_session_secret_is_misconfig(tmp_path: Path) -> None:
    # Blank session_secret must refuse to start (session signing would
    # use a predictable key).
    cfg = _config(tmp_path, session_secret="")
    container = RuntimeContainer(cfg)
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/login")
    # Either 503 misconfig or 500 with clear message.
    assert r.status_code in (500, 503)


def test_disabled_admin_ui_returns_404_not_500(tmp_path: Path) -> None:
    cfg = _config(tmp_path, enabled=False)
    container = RuntimeContainer(cfg)
    with TestClient(create_app(container)) as client:
        r = client.get("/admin/ui/login")
    assert r.status_code == 404


def test_xss_in_login_form_escaped(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.post(
            "/admin/ui/login",
            data={"password": "<script>alert(1)</script>"},
            follow_redirects=False,
        )
    # Response must not echo raw tags back.
    assert "<script>alert(1)</script>" not in r.text
