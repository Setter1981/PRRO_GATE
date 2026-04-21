"""Proof tests for Phase 11a — key selection modal dialog.

Backend route `/admin/ui/settings/fns/parse-key` is unchanged (Phase 10).
This phase refactors the add-FN template to trigger key import from a
modal <dialog> element instead of an always-visible fieldset.
"""
from __future__ import annotations

from pathlib import Path

from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]
_ADMIN_PASS = "key-dialog-pilot"


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "kd.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV",
            "backend_profile_id": "b", "transport_profile_id": "t",
            "channel_owner": "kd-tests",
        },
        "admin_ui": {
            "enabled": True, "password": _ADMIN_PASS,
            "session_secret": "d" * 32,
        },
    })


def _logged_in_client(container: RuntimeContainer) -> TestClient:
    client = TestClient(create_app(container))
    client.__enter__()
    client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
    return client


def _body(tmp_path: Path) -> str:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/fns/new")
        assert r.status_code == 200
        return r.text
    finally:
        client.__exit__(None, None, None)


# ─── 1. Dialog markup ────────────────────────────────────────────────


def test_form_contains_dialog_element(tmp_path: Path) -> None:
    html = _body(tmp_path)
    assert "<dialog" in html
    assert 'id="key_import_dialog"' in html


def test_dialog_has_trigger_button(tmp_path: Path) -> None:
    # Button outside the dialog that opens it with showModal().
    html = _body(tmp_path)
    assert 'id="key_dialog_open_btn"' in html
    assert "Обрати ключ" in html


def test_dialog_has_cancel_button(tmp_path: Path) -> None:
    # Cancel must be a submit with method=dialog so ESC/cancel both work
    # without fetching the backend.
    html = _body(tmp_path)
    assert 'id="key_dialog_cancel_btn"' in html
    assert 'formmethod="dialog"' in html or 'method="dialog"' in html


def test_dialog_retains_file_and_password_inputs(tmp_path: Path) -> None:
    html = _body(tmp_path)
    assert 'id="import_key_file"' in html
    assert 'id="import_key_password"' in html


def test_dialog_has_status_surface(tmp_path: Path) -> None:
    html = _body(tmp_path)
    assert 'id="import_key_status"' in html


def test_dialog_js_uses_show_modal(tmp_path: Path) -> None:
    # The open button must trigger `showModal()` — not just toggling
    # visibility — so focus-trap + backdrop + ESC-close come for free.
    html = _body(tmp_path)
    assert "showModal" in html


def test_dialog_closes_after_successful_parse(tmp_path: Path) -> None:
    # JS must call dialog.close() on fetch-success so the modal goes
    # away after pre-fill.
    html = _body(tmp_path)
    # Either `dialog.close()` or `keyDialog.close()` or similar.
    assert ".close()" in html


# ─── 2. Phase-10 behaviour preserved ─────────────────────────────────


def test_dialog_js_reads_csrf_from_hidden_input(tmp_path: Path) -> None:
    # Security: the dialog fetch must use the CSRF value from the main
    # form's hidden input, NOT re-interpolate `{{ csrf_token }}` inside
    # the JS literal (the token generator is url-safe today but could
    # change; embedding strings inside JS is a drift risk).
    html = _body(tmp_path)
    assert 'csrf_token' in html
    assert 'mainForm.elements["csrf_token"]' in html or \
           "form.elements['csrf_token']" in html


def test_dialog_js_clears_password_in_finally(tmp_path: Path) -> None:
    # Security: password must be wiped from the DOM regardless of
    # success / error / network failure — a wrong-password retry
    # is the worst time to leave cleartext lying around.
    html = _body(tmp_path)
    # `finally` block must contain pwEl.value = "".
    # Low-effort regex: any assignment of pwEl.value to empty string
    # must be reachable from a finally clause.
    assert "finally" in html
    assert 'pwEl.value = ""' in html or "pwEl.value=''" in html


def test_parse_key_endpoint_reachable_via_dialog(tmp_path: Path) -> None:
    # The dialog JS still POSTs to the Phase-10 endpoint; full behaviour
    # coverage lives in test_admin_ui_import_key.py.  Here we just
    # assert the endpoint URL is still wired.
    html = _body(tmp_path)
    assert "/admin/ui/settings/fns/parse-key" in html


def test_non_dialog_fieldset_is_removed(tmp_path: Path) -> None:
    # The old always-visible <fieldset><legend>🔑 Імпорт з ключа...</legend>
    # must not be rendered as a top-level fieldset anymore (it lives
    # inside the dialog now).  Accept the legend text appearing only
    # inside the dialog's subtree.
    html = _body(tmp_path)
    # Assert: there is no <fieldset> before the <dialog> that contains
    # the key-import legend text (we cannot easily parse HTML here, so
    # instead assert the import-key legend text appears _after_ the
    # opening <dialog> tag, not before it).
    dialog_pos = html.find("<dialog")
    legend_pos = html.find("Імпорт з ключа")
    assert dialog_pos >= 0
    assert legend_pos >= 0
    assert legend_pos > dialog_pos, (
        "Import-key UI must live inside the <dialog>, not as a top-level fieldset."
    )


def test_form_still_has_all_required_fields(tmp_path: Path) -> None:
    # Regression guard — the refactor must not remove any add-FN field.
    html = _body(tmp_path)
    for name in (
        'name="fiscal_number"', 'name="tax_number"', 'name="fiscal_mode"',
        'name="tsp_enabled"', 'name="offline_enabled"', 'name="org_name"',
        'name="point_name"', 'name="point_address"',
        'name="min_offline_codes"', 'name="max_offline_codes"',
        'name="operator_name"', 'name="operator_inn"',
        'name="jks_path"', 'name="jks_password"', 'name="csrf_token"',
    ):
        assert name in html, f"missing field {name}"
