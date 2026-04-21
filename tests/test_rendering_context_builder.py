"""Proof tests for `rendering.context_builder` (Phase 8).

`build_render_context(conn, document_id) -> RenderContext | None`

Reads one `fiscal_documents` row, deserializes `payload_json`, joins
`fiscal_number_config` and `shifts`, aggregates per-group taxes.
Returns None if the document does not exist.  Never raises for
legitimate-but-sparse documents.
"""
from __future__ import annotations

import json
from pathlib import Path

from prro_gateway.config import AppConfig
from prro_gateway.enums import OperationType
from prro_gateway.rendering.context_builder import build_render_context
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app
from fastapi.testclient import TestClient

ROOT = Path(__file__).resolve().parents[1]


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        "database": {
            "db_path": str(tmp_path / "ctx.sqlite3"),
            "sql_dir": str(ROOT / "sql"),
            "auto_migrate": True,
        },
        "defaults": {
            "fiscal_number": "FN-DEV-0001",
            "backend_profile_id": "backend_checkbox_default",
            "transport_profile_id": "transport_checkbox_rest_default",
            "channel_owner": "ctx-tests",
        },
        "admin_ui": {"enabled": False, "password": "", "session_secret": "x" * 32},
    })


def _container(tmp_path: Path) -> RuntimeContainer:
    # App startup triggers migrations; we do not need a logged-in client.
    c = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(c)):
        pass
    return c


def _insert_sell_document(c: RuntimeContainer, *, doc_id: str = "DOC-1",
                          fn: str = "4000000001",
                          server_fiscal_no: str = "FD-0000123",
                          total_kop: int = 13500,
                          items: list[dict] | None = None,
                          payments: list[dict] | None = None,
                          shift_serial: int = 7,
                          cashier: str = "Іванов І.",
                          state: str = "ACK") -> None:
    items = items or [
        {"item_id": "g1", "item_no": 1, "name": "Хліб",
         "price": 1500, "quantity": 2000, "sum": 3000,
         "uktzed": "1905", "tax_id": 1},
        {"item_id": "g2", "item_no": 2, "name": "Молоко 1л",
         "price": 4000, "quantity": 1000, "sum": 4000,
         "uktzed": "0401", "tax_id": 1, "discount": 0},
        {"item_id": "g3", "item_no": 3, "name": "Цукерки Roshen",
         "price": 6500, "quantity": 1000, "sum": 6500,
         "uktzed": "1704", "tax_id": 2},
    ]
    payments = payments or [
        {"payment_id": "p1", "payment_type": "CASH", "amount": 5500,
         "resolved_nm": "Готівка"},
        {"payment_id": "p2", "payment_type": "CARD", "amount": 8000,
         "resolved_nm": "Картка", "card_mask": "****1234"},
    ]
    payload = {
        "protocol": "DPS_PRRO_GRPC_ECABINET",
        "operation_type": "SELL",
        "fiscal_number": fn,
        "business_ts": "2026-04-21T12:00:00+00:00",
        "payload": {
            "goods": items,
            "payments": payments,
            "totals": {"total_sum": total_kop},
            "cashier": cashier,
        },
    }
    with c.connect() as conn:
        conn.execute("PRAGMA foreign_keys = OFF")
        # Seed fn_config for org identity joins.
        conn.execute(
            """INSERT OR IGNORE INTO fiscal_number_config (
                    fiscal_number, tax_number, fiscal_mode, tsp_enabled,
                    offline_enabled, org_name, org_address,
                    min_offline_codes, max_offline_codes
               ) VALUES (?, '12345678', 'test', 0, 1, 'ТОВ "Демо"',
                         'м. Київ, вул. Тестова 1', 0, 0)""",
            (fn,),
        )
        # Tax groups (reference): group 1 = ПДВ 20% (А), 2 = ПДВ 7% (Б).
        # Letters are derived by the builder from ascending tax_id order
        # — there is no `letter` column in the schema.
        conn.execute(
            """INSERT OR IGNORE INTO tax_group_definitions (
                    fiscal_number, tax_id, name, tax_rate
               ) VALUES (?, '1', 'ПДВ 20%', 20.0)""",
            (fn,),
        )
        conn.execute(
            """INSERT OR IGNORE INTO tax_group_definitions (
                    fiscal_number, tax_id, name, tax_rate
               ) VALUES (?, '2', 'ПДВ 7%', 7.0)""",
            (fn,),
        )
        # Shift (serial is the visible "shift number").
        conn.execute(
            """INSERT OR IGNORE INTO shifts (
                    shift_id, fiscal_number, serial, state, open_mode,
                    opened_at,
                    opened_via_backend_profile_id, opened_via_transport_profile_id,
                    opened_via_protocol, opened_via_integration_owner,
                    channel_lock_acquired_at
               ) VALUES ('SHIFT-1', ?, ?, 'OPENED', 'ONLINE',
                         '2026-04-21T11:00:00+00:00',
                         'b', 't', 'p', 'o', '2026-04-21T11:00:00+00:00')""",
            (fn, shift_serial),
        )
        conn.execute(
            """INSERT INTO fiscal_documents (
                    document_id, request_id, fiscal_number, shift_id, lnd,
                    doc_type, state, backend_profile_id, transport_profile_id,
                    fs_mode, business_ts, server_fiscal_no, server_fiscal_date,
                    total_sum, payload_json, payload_sha256, previous_hash
               ) VALUES (?, ?, ?, 'SHIFT-1', 42,
                         'SELL', ?,
                         'b', 't', 'ONLINE',
                         '2026-04-21T12:00:00+00:00', ?, '2026-04-21T12:00:05+00:00',
                         ?, ?, 'xx', 'yy')""",
            (doc_id, f"req-{doc_id}", fn, state, server_fiscal_no,
             total_kop, json.dumps(payload, ensure_ascii=False)),
        )
        conn.commit()


# ─── 1. Core mapping ─────────────────────────────────────────────────


def test_returns_none_for_unknown_id(tmp_path: Path) -> None:
    c = _container(tmp_path)
    with c.connect() as conn:
        assert build_render_context(conn, "DOES-NOT-EXIST") is None


def test_sell_document_maps_core_identity(tmp_path: Path) -> None:
    c = _container(tmp_path)
    _insert_sell_document(c)
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.operation_type == OperationType.SELL
    assert ctx.fiscal_number == "4000000001"
    assert ctx.fiscal_doc_no == "FD-0000123"
    assert ctx.local_doc_no == 42
    assert ctx.total_sum_kopecks == 13500


def test_org_identity_is_joined_from_fn_config(tmp_path: Path) -> None:
    c = _container(tmp_path)
    _insert_sell_document(c)
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.org_name == 'ТОВ "Демо"'
    assert ctx.point_addr == "м. Київ, вул. Тестова 1"
    assert ctx.tin == "12345678"


def test_shift_serial_is_joined(tmp_path: Path) -> None:
    c = _container(tmp_path)
    _insert_sell_document(c, shift_serial=7)
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.shift_no == 7


def test_cashier_extracted_from_payload(tmp_path: Path) -> None:
    c = _container(tmp_path)
    _insert_sell_document(c, cashier="Петров П.")
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.cashier == "Петров П."


# ─── 2. Items / taxes / payments ─────────────────────────────────────


def test_items_are_copied_with_tax_letter_lookup(tmp_path: Path) -> None:
    c = _container(tmp_path)
    _insert_sell_document(c)
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert len(ctx.items) == 3
    bread = ctx.items[0]
    assert bread.name == "Хліб"
    assert bread.quantity_thousandths == 2000
    assert bread.price_kopecks == 1500
    assert bread.sum_kopecks == 3000
    assert bread.tax_letter == "А"
    assert ctx.items[2].tax_letter == "Б"  # Roshen — group 2


def test_taxes_are_aggregated_per_group(tmp_path: Path) -> None:
    # 20% group: хліб 3000 + молоко 4000 = 7000 → tax = 7000*20/120 ≈ 1167
    # 7% group: цукерки 6500 → tax = 6500*7/107 ≈ 425
    c = _container(tmp_path)
    _insert_sell_document(c)
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    by_letter = {t.letter: t for t in ctx.taxes}
    assert set(by_letter) == {"А", "Б"}
    assert by_letter["А"].sum_kopecks == 7000
    assert by_letter["А"].rate_pct == 20.0
    assert by_letter["Б"].sum_kopecks == 6500
    assert by_letter["Б"].rate_pct == 7.0
    # tax amount = price * r / (100 + r) rounded to kopeck
    assert 1150 <= by_letter["А"].tax_kopecks <= 1200
    assert 410 <= by_letter["Б"].tax_kopecks <= 430


def test_payments_use_resolved_nm_when_present(tmp_path: Path) -> None:
    c = _container(tmp_path)
    _insert_sell_document(c)
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert len(ctx.payments) == 2
    assert ctx.payments[0].kind == "Готівка"
    assert ctx.payments[0].amount_kopecks == 5500
    assert ctx.payments[1].kind == "Картка"


def test_payments_fall_back_to_payment_type(tmp_path: Path) -> None:
    # When `resolved_nm` is absent (Checkbox path) we degrade to the
    # bare payment_type label.
    c = _container(tmp_path)
    _insert_sell_document(c, payments=[
        {"payment_id": "p1", "payment_type": "CASH", "amount": 5000},
    ])
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert len(ctx.payments) == 1
    assert ctx.payments[0].kind  # non-empty; exact label impl-defined


# ─── 3. Edge / missing fields ────────────────────────────────────────


def test_document_without_shift_row_still_builds(tmp_path: Path) -> None:
    c = _container(tmp_path)
    _insert_sell_document(c)
    # Drop the shift row — document references SHIFT-1 but it no longer exists.
    with c.connect() as conn:
        conn.execute("PRAGMA foreign_keys = OFF")
        conn.execute("DELETE FROM shifts WHERE shift_id='SHIFT-1'")
        conn.commit()
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.shift_no is None


def test_document_without_fn_config_row_still_builds(tmp_path: Path) -> None:
    c = _container(tmp_path)
    _insert_sell_document(c)
    with c.connect() as conn:
        conn.execute(
            "DELETE FROM fiscal_number_config WHERE fiscal_number='4000000001'"
        )
        conn.commit()
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.org_name == ""
    assert ctx.tin == ""


def test_malformed_payload_json_yields_empty_items(tmp_path: Path) -> None:
    # Contract: a row with unparseable payload_json must NOT raise —
    # builder returns a context with empty items/payments so the admin
    # UI can still render the fiscal identity envelope.
    c = _container(tmp_path)
    _insert_sell_document(c)
    with c.connect() as conn:
        conn.execute("PRAGMA foreign_keys = OFF")
        conn.execute(
            "UPDATE fiscal_documents SET payload_json = ? WHERE document_id='DOC-1'",
            ("not-a-json {",),
        )
        conn.commit()
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.items == []
    assert ctx.payments == []
    assert ctx.taxes == []


def test_non_printable_doc_type_returns_none(tmp_path: Path) -> None:
    # doc_type=STATUS is internal/non-printable; builder must return None
    # so the UI 404s instead of rendering a blank "SELL" receipt.
    c = _container(tmp_path)
    _insert_sell_document(c)
    with c.connect() as conn:
        conn.execute("PRAGMA foreign_keys = OFF")
        conn.execute(
            "UPDATE fiscal_documents SET doc_type = 'STATUS' WHERE document_id='DOC-1'"
        )
        conn.commit()
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is None


def test_item_with_null_tax_id_builds_without_letter(tmp_path: Path) -> None:
    # Technical returns / items without a VAT group: tax_id may be None.
    # Builder must leave tax_letter empty and exclude the item from
    # the taxes aggregation.
    c = _container(tmp_path)
    _insert_sell_document(c, items=[
        {"item_id": "g1", "item_no": 1, "name": "Тех. повернення",
         "price": 1000, "quantity": 1000, "sum": 1000},
    ], payments=[
        {"payment_id": "p1", "payment_type": "CASH", "amount": 1000},
    ])
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert len(ctx.items) == 1
    assert ctx.items[0].tax_letter == ""
    assert ctx.taxes == []


def test_return_doc_type_maps_correctly(tmp_path: Path) -> None:
    c = _container(tmp_path)
    _insert_sell_document(c)
    with c.connect() as conn:
        conn.execute("PRAGMA foreign_keys = OFF")
        conn.execute(
            "UPDATE fiscal_documents SET doc_type = 'RETURN' WHERE document_id='DOC-1'"
        )
        conn.commit()
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.operation_type == OperationType.RETURN


def test_garbage_numeric_fields_survive(tmp_path: Path) -> None:
    # Pre-migration / buggy adapters may have persisted strings or dicts
    # where integers were expected.  Builder must coerce safely (→ 0),
    # not 500.
    c = _container(tmp_path)
    _insert_sell_document(c, items=[
        {"item_id": "g1", "item_no": 1, "name": "x",
         "price": "not-a-num", "quantity": {"x": 1}, "sum": "0",
         "tax_id": 1},
    ])
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.items[0].price_kopecks == 0
    assert ctx.items[0].quantity_thousandths == 0
    assert ctx.items[0].sum_kopecks == 0


def test_document_before_ack_has_empty_fiscal_doc_no(tmp_path: Path) -> None:
    # Offline / pre-ACK document — server_fiscal_no is NULL.
    c = _container(tmp_path)
    _insert_sell_document(c, state="OFFLINE_LOCAL_ACK",
                          server_fiscal_no=None)  # type: ignore[arg-type]
    # The seed helper always writes a server_fiscal_no — patch it out.
    with c.connect() as conn:
        conn.execute(
            "UPDATE fiscal_documents SET server_fiscal_no = NULL WHERE document_id='DOC-1'"
        )
        conn.commit()
    with c.connect() as conn:
        ctx = build_render_context(conn, "DOC-1")
    assert ctx is not None
    assert ctx.fiscal_doc_no == ""
