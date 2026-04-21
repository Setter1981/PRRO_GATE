"""Proof tests for admin UI receipt preview (Phase 8).

`GET /admin/ui/documents/{id}/receipt.html` — renders the receipt for a
stored document via `build_render_context` + `render_html`.
"""
from __future__ import annotations

import json
from pathlib import Path

from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]
_ADMIN_PASS = "receipt-preview-pilot"


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "receipt.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV", "backend_profile_id": "b",
            "transport_profile_id": "t", "channel_owner": "x",
        },
        "admin_ui": {
            "enabled": True, "password": _ADMIN_PASS,
            "session_secret": "r" * 32,
        },
    })


def _logged_in_client(container: RuntimeContainer) -> TestClient:
    client = TestClient(create_app(container))
    client.__enter__()
    client.post("/admin/ui/login", data={"password": _ADMIN_PASS})
    return client


def _seed_sell_doc(container: RuntimeContainer, doc_id: str = "DOC-PREVIEW") -> None:
    payload = {
        "protocol": "DPS_PRRO_GRPC_ECABINET",
        "operation_type": "SELL",
        "fiscal_number": "4000000001",
        "payload": {
            "goods": [
                {"item_id": "g1", "item_no": 1, "name": "Хліб",
                 "price": 1500, "quantity": 1000, "sum": 1500, "tax_id": 1},
            ],
            "payments": [
                {"payment_id": "p1", "payment_type": "CASH",
                 "amount": 1500, "resolved_nm": "Готівка"},
            ],
            "totals": {"total_sum": 1500},
            "cashier": "Касир А.",
        },
    }
    with container.connect() as conn:
        conn.execute("PRAGMA foreign_keys = OFF")
        conn.execute(
            """INSERT OR IGNORE INTO fiscal_number_config (
                fiscal_number, tax_number, fiscal_mode, tsp_enabled,
                offline_enabled, org_name, org_address,
                min_offline_codes, max_offline_codes
            ) VALUES ('4000000001', '12345678', 'test', 0, 1,
                      'ТОВ Preview', 'вул. Preview 1', 0, 0)""",
        )
        conn.execute(
            """INSERT OR IGNORE INTO tax_group_definitions (
                fiscal_number, tax_id, name, tax_rate
            ) VALUES ('4000000001', '1', 'ПДВ 20%', 20.0)""",
        )
        conn.execute(
            """INSERT INTO fiscal_documents (
                document_id, request_id, fiscal_number, lnd, doc_type, state,
                backend_profile_id, transport_profile_id, fs_mode,
                business_ts, server_fiscal_no, server_fiscal_date,
                total_sum, payload_json, payload_sha256, previous_hash
            ) VALUES (?, ?, '4000000001', 1, 'SELL', 'ACK',
                      'b', 't', 'ONLINE',
                      '2026-04-21T12:00:00+00:00',
                      'FD-PREVIEW-1', '2026-04-21T12:00:05+00:00',
                      1500, ?, 'x', 'y')""",
            (doc_id, f"req-{doc_id}", json.dumps(payload, ensure_ascii=False)),
        )
        conn.commit()


def test_receipt_html_requires_auth(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_sell_doc(container)
    finally:
        client.__exit__(None, None, None)
    with TestClient(create_app(container)) as anon:
        r = anon.get(
            "/admin/ui/documents/DOC-PREVIEW/receipt.html",
            follow_redirects=False,
        )
    assert r.status_code in (302, 303)


def test_receipt_html_404_for_unknown_doc(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        r = client.get("/admin/ui/documents/NOPE/receipt.html")
        assert r.status_code == 404
    finally:
        client.__exit__(None, None, None)


def test_receipt_html_renders_sell_document(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_sell_doc(container)
        r = client.get("/admin/ui/documents/DOC-PREVIEW/receipt.html")
        assert r.status_code == 200
        assert "text/html" in r.headers["content-type"]
        body = r.text
        assert "ТОВ Preview" in body
        assert "Хліб" in body
        assert "Касир А." in body
        assert "FD-PREVIEW-1" in body
    finally:
        client.__exit__(None, None, None)


def test_receipt_html_escapes_injected_payload(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        payload = {
            "protocol": "DPS_PRRO_GRPC_ECABINET",
            "operation_type": "SELL",
            "fiscal_number": "4000000001",
            "payload": {
                "goods": [{"item_id": "g1", "item_no": 1,
                           "name": "<script>alert(1)</script>",
                           "price": 100, "quantity": 1000, "sum": 100,
                           "tax_id": 1}],
                "payments": [{"payment_id": "p1", "payment_type": "CASH",
                              "amount": 100, "resolved_nm": "Готівка"}],
                "totals": {"total_sum": 100},
                "cashier": "Касир",
            },
        }
        with container.connect() as conn:
            conn.execute("PRAGMA foreign_keys = OFF")
            conn.execute(
                """INSERT OR IGNORE INTO fiscal_number_config (
                    fiscal_number, tax_number, fiscal_mode, tsp_enabled,
                    offline_enabled, org_name, org_address,
                    min_offline_codes, max_offline_codes
                ) VALUES ('4000000001', '12345678', 'test', 0, 1,
                          'org', 'addr', 0, 0)""",
            )
            conn.execute(
                """INSERT OR IGNORE INTO tax_group_definitions (
                    fiscal_number, tax_id, name, tax_rate
                ) VALUES ('4000000001', '1', 'ПДВ 20%', 20.0)""",
            )
            conn.execute(
                """INSERT INTO fiscal_documents (
                    document_id, request_id, fiscal_number, lnd, doc_type, state,
                    backend_profile_id, transport_profile_id, fs_mode,
                    business_ts, server_fiscal_no, server_fiscal_date,
                    total_sum, payload_json, payload_sha256, previous_hash
                ) VALUES ('DOC-XSS', 'req-xss', '4000000001', 1, 'SELL', 'ACK',
                          'b', 't', 'ONLINE',
                          '2026-04-21T12:00:00+00:00',
                          'FD-XSS', '2026-04-21T12:00:05+00:00',
                          100, ?, 'x', 'y')""",
                (json.dumps(payload, ensure_ascii=False),),
            )
            conn.commit()
        r = client.get("/admin/ui/documents/DOC-XSS/receipt.html")
        assert r.status_code == 200
        assert "<script>alert(1)</script>" not in r.text
    finally:
        client.__exit__(None, None, None)


def test_document_list_links_to_receipt_preview(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    client = _logged_in_client(container)
    try:
        _seed_sell_doc(container)
        r = client.get("/admin/ui/documents")
        assert r.status_code == 200
        assert "/admin/ui/documents/DOC-PREVIEW/receipt.html" in r.text
    finally:
        client.__exit__(None, None, None)
