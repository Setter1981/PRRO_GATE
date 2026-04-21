"""Proof tests for Phase 10 — `POST /admin/ui/settings/fns/parse-key`.

Uploads a key container, returns JSON identity pre-fill.  Real key
parsing is delegated to `prro_crypto.extract_private_key` (Rust wheel);
tests monkeypatch it so no real JKS/PFX fixture is needed.
"""
from __future__ import annotations

import re
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]
_ADMIN_PASS = "import-key-pilot"
_CSRF_RE = re.compile(r'name="csrf_token"\s+value="([^"]+)"')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "importkey.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV",
            "backend_profile_id": "b", "transport_profile_id": "t",
            "channel_owner": "ik-tests",
        },
        "admin_ui": {
            "enabled": True, "password": _ADMIN_PASS,
            "session_secret": "k" * 32,
        },
    })


def _logged_in_client(container: RuntimeContainer) -> TestClient:
    client = TestClient(create_app(container))
    client.__enter__()
    client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
    return client


def _fetch_csrf(client: TestClient) -> str:
    r = client.get("/admin/ui/settings/fns/new")
    m = _CSRF_RE.search(r.text)
    assert m, "CSRF not in form render"
    return m.group(1)


class _FakeCryptoOk:
    """Stand-in for the `prro_crypto` PyO3 wheel used by the handler."""

    @staticmethod
    def extract_private_key(data: bytes, password: str) -> dict:
        # Fixed DER output — the handler will pass it to
        # `extract_identity_from_cert` which we also monkeypatch below
        # for the route-level test; for the subject-DN monkeypatch
        # approach we use `extract_identity_from_subject_dn` directly.
        return {
            "format": "jks",
            "param_d_hex": "00" * 32,
            "certs": [b"FAKE-DER-END-ENTITY"],
        }


class _FakeCryptoBadPw:
    @staticmethod
    def extract_private_key(data: bytes, password: str) -> dict:
        raise ValueError("password incorrect")


class _FakeCryptoNoCert:
    @staticmethod
    def extract_private_key(data: bytes, password: str) -> dict:
        return {"format": "dat", "param_d_hex": "00", "certs": []}


# ─── 1. Route wiring ─────────────────────────────────────────────────


def test_parse_key_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.post(
            "/admin/ui/settings/fns/parse-key",
            files={"key_file": ("k.jks", b"x", "application/octet-stream")},
            data={"password": "p"},
            follow_redirects=False,
        )
    assert r.status_code in (302, 303)


def test_parse_key_without_csrf_rejected(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.post(
            "/admin/ui/settings/fns/parse-key",
            files={"key_file": ("k.jks", b"x", "application/octet-stream")},
            data={"password": "p"},  # no csrf_token
            follow_redirects=False,
        )
        assert r.status_code == 403
    finally:
        client.__exit__(None, None, None)


def test_parse_key_success_returns_identity_json(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    import prro_gateway.admin_ui.routes as routes_mod
    from prro_gateway.services.onboarding_key_identity import KeyIdentity

    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        monkeypatch.setattr(routes_mod, "_prro_crypto_module", _FakeCryptoOk)

        def fake_extract(der: bytes) -> KeyIdentity:
            assert der == b"FAKE-DER-END-ENTITY"
            return KeyIdentity(
                subject_dn="CN=ТОВ Демо, O=ТОВ Демо, "
                           "1.2.804.2.1.1.1.11.1.4.1.1=12345678, C=UA",
                org_name="ТОВ Демо",
                edrpou="12345678",
                drfo=None,
                operator_name=None,
                is_legal_entity=True,
            )
        monkeypatch.setattr(
            "prro_gateway.admin_ui.routes.extract_identity_from_cert",
            fake_extract,
        )

        csrf = _fetch_csrf(client)
        r = client.post(
            "/admin/ui/settings/fns/parse-key",
            files={"key_file": ("k.jks", b"fake-key-bytes",
                                "application/octet-stream")},
            data={"password": "pw", "csrf_token": csrf},
        )
        assert r.status_code == 200, r.text
        body = r.json()
        assert body["edrpou"] == "12345678"
        assert body["org_name"] == "ТОВ Демо"
        assert body["drfo"] is None
        assert body["is_legal_entity"] is True
        assert body["subject_dn"].startswith("CN=ТОВ Демо")
    finally:
        client.__exit__(None, None, None)


def test_parse_key_bad_password_returns_400(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    import prro_gateway.admin_ui.routes as routes_mod
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        monkeypatch.setattr(routes_mod, "_prro_crypto_module", _FakeCryptoBadPw)
        csrf = _fetch_csrf(client)
        r = client.post(
            "/admin/ui/settings/fns/parse-key",
            files={"key_file": ("k.jks", b"x", "application/octet-stream")},
            data={"password": "wrong", "csrf_token": csrf},
        )
        assert r.status_code == 400
        # Must NOT echo any part of the password.
        assert "wrong" not in r.text
    finally:
        client.__exit__(None, None, None)


def test_parse_key_container_without_cert_returns_400(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    # Key-6.dat: valid key but no embedded cert — we can't pre-fill.
    import prro_gateway.admin_ui.routes as routes_mod
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        monkeypatch.setattr(routes_mod, "_prro_crypto_module", _FakeCryptoNoCert)
        csrf = _fetch_csrf(client)
        r = client.post(
            "/admin/ui/settings/fns/parse-key",
            files={"key_file": ("k.dat", b"x", "application/octet-stream")},
            data={"password": "p", "csrf_token": csrf},
        )
        assert r.status_code == 400
        assert "cert" in r.text.lower() or "серт" in r.text.lower()
    finally:
        client.__exit__(None, None, None)


def test_parse_key_503_when_wheel_absent(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    import prro_gateway.admin_ui.routes as routes_mod
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        monkeypatch.setattr(routes_mod, "_prro_crypto_module", None)
        csrf = _fetch_csrf(client)
        r = client.post(
            "/admin/ui/settings/fns/parse-key",
            files={"key_file": ("k.jks", b"x", "application/octet-stream")},
            data={"password": "p", "csrf_token": csrf},
        )
        assert r.status_code == 503
    finally:
        client.__exit__(None, None, None)


def test_parse_key_get_is_405(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/fns/parse-key")
        assert r.status_code == 405
    finally:
        client.__exit__(None, None, None)


def test_parse_key_non_ukrainian_cert_returns_nulls(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    import prro_gateway.admin_ui.routes as routes_mod
    from prro_gateway.services.onboarding_key_identity import KeyIdentity
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        monkeypatch.setattr(routes_mod, "_prro_crypto_module", _FakeCryptoOk)
        monkeypatch.setattr(
            "prro_gateway.admin_ui.routes.extract_identity_from_cert",
            lambda der: KeyIdentity(
                subject_dn="CN=Foreign, C=DE",
                org_name=None, edrpou=None, drfo=None,
                operator_name="Foreign", is_legal_entity=False,
            ),
        )
        csrf = _fetch_csrf(client)
        r = client.post(
            "/admin/ui/settings/fns/parse-key",
            files={"key_file": ("k.jks", b"x", "application/octet-stream")},
            data={"password": "p", "csrf_token": csrf},
        )
        assert r.status_code == 200
        body = r.json()
        assert body["edrpou"] is None
        assert body["drfo"] is None
        assert body["org_name"] is None
        assert body["is_legal_entity"] is False
    finally:
        client.__exit__(None, None, None)


def test_parse_key_rejects_oversized_file(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    # 6 MB — over the 5 MB cap.
    import prro_gateway.admin_ui.routes as routes_mod
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        monkeypatch.setattr(routes_mod, "_prro_crypto_module", _FakeCryptoOk)
        big = b"\x00" * (6 * 1024 * 1024)
        csrf = _fetch_csrf(client)
        r = client.post(
            "/admin/ui/settings/fns/parse-key",
            files={"key_file": ("k.jks", big, "application/octet-stream")},
            data={"password": "p", "csrf_token": csrf},
        )
        assert r.status_code in (400, 413)
    finally:
        client.__exit__(None, None, None)


# ─── 2. UI wiring ────────────────────────────────────────────────────


def test_add_fn_form_has_import_button(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/fns/new")
        assert r.status_code == 200
        assert "Імпорт з ключа" in r.text
        # Hidden file input + password field for the async upload.
        assert 'id="import_key_file"' in r.text
        assert 'id="import_key_password"' in r.text
        # JS fetch POSTs to the parse-key endpoint.
        assert "/admin/ui/settings/fns/parse-key" in r.text
    finally:
        client.__exit__(None, None, None)
