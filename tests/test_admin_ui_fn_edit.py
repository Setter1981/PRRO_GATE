"""Proof tests for admin UI edit-FN form (Phase 7).

`GET /admin/ui/settings/fns/{fn}/edit`  — pre-filled edit form.
`POST /admin/ui/settings/fns/{fn}/edit` — UPDATE fiscal_number_config,
    audit `fn_updated`, redirect back to /settings/fns.

Invariants:
- fiscal_number (primary key) is NEVER modified by this route.
- If node_state reports an open shift (OPENING/OPENED/CLOSING) for this
  FN, mode flip test↔prod is rejected with 409.
- CSRF + length-cap regime inherited from Phase 6.
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

_ADMIN_PASS = "edit-fn-pilot"
_SESSION_SECRET = "z" * 32
_CSRF_RE = re.compile(r'name="csrf_token"\s+value="([^"]+)"')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "editfn.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV-0001",
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "edit-fn-tests",
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


def _logged_in_with_fn(container: RuntimeContainer, fn: str = "4000000100", *,
                       mode: str = "test", org_name: str = "ТОВ Seed") -> TestClient:
    client = _logged_in_client(container)
    with container.connect() as conn:
        conn.execute(
            """INSERT INTO fiscal_number_config (
                    fiscal_number, tax_number, fiscal_mode,
                    tsp_enabled, offline_enabled,
                    org_name, org_address,
                    min_offline_codes, max_offline_codes
               ) VALUES (?, '12345678', ?, 0, 1, ?, 'addr', 0, 0)""",
            (fn, mode, org_name),
        )
        conn.commit()
    return client


def _seed_node_state(container: RuntimeContainer, fn: str,
                     shift_state: str = "CLOSED") -> None:
    with container.connect() as conn:
        conn.execute(
            """INSERT INTO node_state (
                    node_id, fiscal_number, mode, shift_state, next_lnd
               ) VALUES (?, ?, 'ONLINE', ?, 1)""",
            (f"node-{fn}", fn, shift_state),
        )
        conn.commit()


def _fetch_csrf(client: TestClient, path: str) -> str:
    r = client.get(path)
    assert r.status_code == 200, f"GET {path} → {r.status_code}"
    m = _CSRF_RE.search(r.text)
    assert m, f"csrf_token not in {path}"
    return m.group(1)


def _submit_edit(client: TestClient, fn: str, **overrides: str):
    path = f"/admin/ui/settings/fns/{fn}/edit"
    data = {
        "csrf_token": _fetch_csrf(client, path),
        "tax_number": "12345678",
        "fiscal_mode": "test",
        "org_name": "ТОВ Edit",
        "point_name": "Точка #2",
        "point_address": "нова адреса",
        "tsp_enabled": "0",
        "offline_enabled": "1",
        "min_offline_codes": "0",
        "max_offline_codes": "0",
    }
    data.update(overrides)
    return client.post(path, data=data, follow_redirects=False)


# ─── 1. GET edit form ────────────────────────────────────────────────


def test_edit_form_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        r = client.get(
            "/admin/ui/settings/fns/4000000100/edit", follow_redirects=False,
        )
        assert r.status_code in (302, 303)


def test_edit_form_404_for_unknown_fn(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/settings/fns/9999999999/edit")
        assert r.status_code == 404
    finally:
        client.__exit__(None, None, None)


def test_edit_form_prefills_existing_values(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container, org_name="ТОВ Before")
    try:
        r = client.get("/admin/ui/settings/fns/4000000100/edit")
        assert r.status_code == 200
        assert 'value="ТОВ Before"' in r.text
        assert 'value="12345678"' in r.text
        # FN is shown but NOT editable (readonly or no name=fiscal_number)
        assert "4000000100" in r.text
    finally:
        client.__exit__(None, None, None)


def test_edit_form_does_not_offer_editable_fn_field(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        r = client.get("/admin/ui/settings/fns/4000000100/edit")
        assert r.status_code == 200
        # Primary-key invariant: either the input is absent entirely,
        # or it is explicitly readonly/disabled.  Vacuous "absent"
        # case is acceptable because the handler ignores any POSTed
        # fiscal_number regardless.
        if 'name="fiscal_number"' in r.text:
            assert "readonly" in r.text or "disabled" in r.text, (
                "editable fiscal_number field exposes primary-key drift"
            )
    finally:
        client.__exit__(None, None, None)


def test_edit_link_visible_on_fns_list(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        r = client.get("/admin/ui/settings/fns")
        assert "/admin/ui/settings/fns/4000000100/edit" in r.text
    finally:
        client.__exit__(None, None, None)


# ─── 2. POST happy path ──────────────────────────────────────────────


def test_post_edit_updates_row_and_redirects(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container, org_name="ТОВ Before")
    try:
        r = _submit_edit(client, "4000000100",
                         org_name="ТОВ After", point_address="нова адреса")
        assert r.status_code in (302, 303), r.text
        assert r.headers["location"].rstrip("/") == "/admin/ui/settings/fns"
        with container.connect() as conn:
            row = conn.execute(
                """SELECT org_name, org_address FROM fiscal_number_config
                   WHERE fiscal_number = '4000000100'"""
            ).fetchone()
        assert (row[0], row[1]) == ("ТОВ After", "нова адреса")
    finally:
        client.__exit__(None, None, None)


def test_post_edit_writes_audit_row(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        _submit_edit(client, "4000000100", org_name="Аудит")
        with container.connect() as conn:
            row = conn.execute(
                """SELECT event_type, severity, event_payload_json
                   FROM audit_log WHERE entity_id = '4000000100'
                   ORDER BY audit_id DESC LIMIT 1"""
            ).fetchone()
        assert row is not None
        assert row[0] == "fn_updated"
        assert row[1] == "INFO"
        payload = json.loads(row[2])
        # Only changed fields listed in payload (optional — accept any
        # shape but require the FN reference).
        assert "fiscal_mode" in payload or "changes" in payload or "org_name" in payload
    finally:
        client.__exit__(None, None, None)


def test_post_edit_preserves_fiscal_number_even_if_submitted(tmp_path: Path) -> None:
    # A malicious / mistaken POST that smuggles a different
    # fiscal_number value must never cause a primary-key change.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        r = _submit_edit(client, "4000000100",
                         fiscal_number="4000000999",  # should be ignored
                         org_name="Re-ID Attempt")
        assert r.status_code in (302, 303)
        with container.connect() as conn:
            rows = conn.execute(
                "SELECT fiscal_number FROM fiscal_number_config ORDER BY fiscal_number"
            ).fetchall()
        # Exactly one row, still the original.
        assert [r[0] for r in rows] == ["4000000100"]
    finally:
        client.__exit__(None, None, None)


# ─── 3. Validation ───────────────────────────────────────────────────


def test_post_edit_rejects_bad_tin(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        r = _submit_edit(client, "4000000100", tax_number="abc")
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_post_edit_rejects_unknown_mode(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        r = _submit_edit(client, "4000000100", fiscal_mode="staging")
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_post_edit_rejects_bad_offline_range(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        r = _submit_edit(client, "4000000100",
                         min_offline_codes="500", max_offline_codes="100")
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


def test_post_edit_rejects_oversized_field(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        r = _submit_edit(client, "4000000100", org_name="x" * 10_000)
        assert r.status_code == 400
    finally:
        client.__exit__(None, None, None)


# ─── 4. CSRF ─────────────────────────────────────────────────────────


def test_post_edit_without_csrf_rejected(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        r = client.post(
            "/admin/ui/settings/fns/4000000100/edit",
            data={
                "tax_number": "12345678", "fiscal_mode": "test",
                "tsp_enabled": "0", "offline_enabled": "1",
                "min_offline_codes": "0", "max_offline_codes": "0",
            },
            follow_redirects=False,
        )
        assert r.status_code == 403
    finally:
        client.__exit__(None, None, None)


def test_post_edit_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container)
    try:
        with TestClient(create_app(container)) as anon:
            r = anon.post(
                "/admin/ui/settings/fns/4000000100/edit",
                data={"tax_number": "99999999"},
                follow_redirects=False,
            )
            assert r.status_code in (302, 303)
        with container.connect() as conn:
            tax = conn.execute(
                "SELECT tax_number FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000000100",),
            ).fetchone()[0]
        assert tax == "12345678"  # original untouched
    finally:
        client.__exit__(None, None, None)


# ─── 5. Open-shift guard (mode flip) ─────────────────────────────────


def test_mode_flip_rejected_when_shift_is_open(tmp_path: Path) -> None:
    # Invariant: channel/mode switch is forbidden with an open shift.
    # Editing the mode while node_state.shift_state=OPENED must 409.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container, mode="test")
    _seed_node_state(container, "4000000100", shift_state="OPENED")
    try:
        r = _submit_edit(client, "4000000100", fiscal_mode="prod")
        assert r.status_code == 409
        with container.connect() as conn:
            current = conn.execute(
                "SELECT fiscal_mode FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000000100",),
            ).fetchone()[0]
        assert current == "test"  # rolled back / not applied
    finally:
        client.__exit__(None, None, None)


def test_non_mode_fields_editable_even_with_open_shift(tmp_path: Path) -> None:
    # When shift is open you can still change org_name / addr — only
    # mode flip is forbidden.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container, mode="test")
    _seed_node_state(container, "4000000100", shift_state="OPENED")
    try:
        r = _submit_edit(client, "4000000100",
                         fiscal_mode="test",  # unchanged
                         org_name="Rebranded LLC")
        assert r.status_code in (302, 303), r.text
        with container.connect() as conn:
            name = conn.execute(
                "SELECT org_name FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000000100",),
            ).fetchone()[0]
        assert name == "Rebranded LLC"
    finally:
        client.__exit__(None, None, None)


def test_mode_flip_rejected_when_shift_state_is_error(tmp_path: Path) -> None:
    # ERROR shift requires operator reconciliation before a profile
    # flip — invariant #8.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container, mode="test")
    _seed_node_state(container, "4000000100", shift_state="ERROR")
    try:
        r = _submit_edit(client, "4000000100", fiscal_mode="prod")
        assert r.status_code == 409
        with container.connect() as conn:
            m = conn.execute(
                "SELECT fiscal_mode FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000000100",),
            ).fetchone()[0]
        assert m == "test"
    finally:
        client.__exit__(None, None, None)


def test_mode_flip_allowed_with_no_node_state_row(tmp_path: Path) -> None:
    # Fresh FN (never produced a document) has no node_state row yet.
    # Mode flip must succeed — and an audit row with mode_flipped:true
    # must be written.
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container, mode="test")
    try:
        r = _submit_edit(client, "4000000100", fiscal_mode="prod")
        assert r.status_code in (302, 303), r.text
        with container.connect() as conn:
            mode = conn.execute(
                "SELECT fiscal_mode FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000000100",),
            ).fetchone()[0]
            audit = conn.execute(
                """SELECT event_payload_json FROM audit_log
                   WHERE entity_id = '4000000100' AND event_type = 'fn_updated'
                   ORDER BY audit_id DESC LIMIT 1"""
            ).fetchone()
        assert mode == "prod"
        payload = json.loads(audit[0])
        assert payload["mode_flipped"] is True
        assert payload["previous_fiscal_mode"] == "test"
        assert payload["fiscal_mode"] == "prod"
    finally:
        client.__exit__(None, None, None)


def test_mode_flip_allowed_when_shift_is_closed(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_with_fn(container, mode="test")
    _seed_node_state(container, "4000000100", shift_state="CLOSED")
    try:
        r = _submit_edit(client, "4000000100", fiscal_mode="prod")
        assert r.status_code in (302, 303), r.text
        with container.connect() as conn:
            mode = conn.execute(
                "SELECT fiscal_mode FROM fiscal_number_config WHERE fiscal_number=?",
                ("4000000100",),
            ).fetchone()[0]
        assert mode == "prod"
    finally:
        client.__exit__(None, None, None)
