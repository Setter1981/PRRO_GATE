"""Unit tests for services/shift_aggregation.py.

Covers aggregate_shift_data and aggregate_cash_withdrawals.
Documents are inserted directly into SQLite (no write_path).
"""
from __future__ import annotations

import json
import sqlite3

import pytest

from prro_gateway.services.shift_aggregation import (
    aggregate_cash_withdrawals,
    aggregate_shift_data,
)

# ---------------------------------------------------------------------------
# Constants — must match seeded reference data in sql/002_seed_reference_data.sql
# ---------------------------------------------------------------------------

FN = "FN-DEV-0001"
BACKEND = "backend_checkbox_default"
TRANSPORT = "transport_checkbox_rest_default"

# Each test uses its own shift_id so isolation is maintained even if tests
# run against a shared conn fixture (conn is per-test in conftest).
_seq = 0


def _next() -> int:
    global _seq
    _seq += 1
    return _seq


def _doc_id() -> str:
    return f"doc-agg-{_next()}"


def _req_id(doc_id: str) -> str:
    return f"req-{doc_id}"


# ---------------------------------------------------------------------------
# Insert helpers
# ---------------------------------------------------------------------------


def _insert_shift(conn: sqlite3.Connection, shift_id: str) -> None:
    """Insert a minimal shifts row (required by FK on fiscal_documents.shift_id)."""
    conn.execute(
        """INSERT OR IGNORE INTO shifts
           (shift_id, fiscal_number, state, open_mode,
            opened_via_backend_profile_id, opened_via_transport_profile_id,
            opened_via_protocol, opened_via_integration_owner,
            channel_lock_acquired_at)
           VALUES (?, ?, 'OPENED', 'ONLINE', ?, ?, 'INTERNAL', 'test',
                   '2026-04-17T09:00:00')""",
        (shift_id, FN, BACKEND, TRANSPORT),
    )


def _insert_inbox(conn: sqlite3.Connection, request_id: str, doc_type: str) -> None:
    """Insert a minimal ingress_inbox row (required by FK on fiscal_documents)."""
    # operation_type CHECK allows 'SELL','RETURN','CASH_WITHDRAWAL', etc.
    # Map doc_type → operation_type (they share the same names for the types we use).
    conn.execute(
        """INSERT INTO ingress_inbox
           (request_id, idempotency_key, protocol, operation_type,
            fiscal_number, payload_json, payload_sha256)
           VALUES (?, ?, 'INTERNAL', ?, ?, '{}', 'sha256')""",
        (request_id, f"idem-{request_id}", doc_type, FN),
    )


def _insert_doc(
    conn: sqlite3.Connection,
    shift_id: str,
    doc_type: str,
    state: str,
    payload: dict,
    lnd: int | None = None,
) -> str:
    """Insert a fiscal_document row and return its document_id."""
    doc_id = _doc_id()
    req_id = _req_id(doc_id)
    effective_lnd = lnd if lnd is not None else _next()
    _insert_inbox(conn, req_id, doc_type)
    conn.execute(
        """INSERT INTO fiscal_documents
           (document_id, request_id, fiscal_number, lnd, doc_type,
            backend_profile_id, transport_profile_id, fs_mode, state,
            business_ts, payload_json, payload_sha256, shift_id)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)""",
        (
            doc_id,
            req_id,
            FN,
            effective_lnd,
            doc_type,
            BACKEND,
            TRANSPORT,
            "ONLINE",
            state,
            "2026-04-17T10:00:00",
            json.dumps({"payload": payload}),
            f"sha-{doc_id}",
            shift_id,
        ),
    )
    return doc_id


def _sell_payload(goods: list[dict], payments: list[dict]) -> dict:
    return {"receipt": {"goods": goods, "payments": payments}}


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


def test_empty_shift_returns_zero_sums(conn: sqlite3.Connection) -> None:
    """Shift with no documents should return all-zero aggregation."""
    result = aggregate_shift_data(conn, "shift-empty-001")
    assert result["check_count"] == {"ni": 0, "no": 0}
    assert result["tax_sums"] == {}
    assert result["payment_sums"] == {}
    assert result["service_sums"] == {}


def test_single_sell_increments_ni_and_tax(conn: sqlite3.Connection) -> None:
    """One SELL ACK increments ni count and records tax_sums under correct key."""
    shift_id = "shift-sell-001"
    goods = [{"name": "Хліб", "price": 3000, "quantity": 1000, "sum": 3000, "tax_id": 20}]
    payments = [{"payment_type": "CASH", "amount": 3000}]
    _insert_shift(conn, shift_id)
    _insert_doc(conn, shift_id, "SELL", "ACK", _sell_payload(goods, payments))
    conn.commit()

    result = aggregate_shift_data(conn, shift_id)

    assert result["check_count"]["ni"] == 1
    assert result["check_count"]["no"] == 0
    # tax_id 20 → string key "20"
    assert "20" in result["tax_sums"]
    assert result["tax_sums"]["20"]["smi"] == 3000
    assert result["tax_sums"]["20"]["smo"] == 0


def test_return_increments_no(conn: sqlite3.Connection) -> None:
    """One RETURN ACK increments no count."""
    shift_id = "shift-return-001"
    goods = [{"name": "Молоко", "price": 2500, "quantity": 1000, "sum": 2500, "tax_id": 7}]
    payments = [{"payment_type": "CASH", "amount": 2500}]
    _insert_shift(conn, shift_id)

    _insert_doc(conn, shift_id, "RETURN", "ACK", _sell_payload(goods, payments))
    conn.commit()

    result = aggregate_shift_data(conn, shift_id)

    assert result["check_count"]["ni"] == 0
    assert result["check_count"]["no"] == 1
    assert "7" in result["tax_sums"]
    assert result["tax_sums"]["7"]["smo"] == 2500
    assert result["tax_sums"]["7"]["smi"] == 0


def test_tax_sums_accumulate_across_multiple_docs(conn: sqlite3.Connection) -> None:
    """Multiple SELL docs with the same tax_id sum correctly."""
    shift_id = "shift-multi-tax-001"
    goods = [{"name": "Item", "price": 1000, "quantity": 1000, "sum": 1000, "tax_id": 20}]
    _insert_shift(conn, shift_id)

    _insert_doc(conn, shift_id, "SELL", "ACK", _sell_payload(goods, []))
    _insert_doc(conn, shift_id, "SELL", "ACK", _sell_payload(goods, []))
    conn.commit()

    result = aggregate_shift_data(conn, shift_id)

    assert result["check_count"]["ni"] == 2
    assert result["tax_sums"]["20"]["smi"] == 2000


def test_exclude_document_id_removes_doc(conn: sqlite3.Connection) -> None:
    """exclude_document_id causes that document to be excluded from the aggregation."""
    shift_id = "shift-exclude-001"
    goods = [{"name": "Сир", "price": 8000, "quantity": 1000, "sum": 8000, "tax_id": 20}]
    _insert_shift(conn, shift_id)

    doc_id = _insert_doc(conn, shift_id, "SELL", "ACK", _sell_payload(goods, []))
    conn.commit()

    result_full = aggregate_shift_data(conn, shift_id)
    assert result_full["check_count"]["ni"] == 1

    result_ex = aggregate_shift_data(conn, shift_id, exclude_document_id=doc_id)
    assert result_ex["check_count"]["ni"] == 0
    assert result_ex["tax_sums"] == {}


def test_cash_withdrawals_count_and_sum(conn: sqlite3.Connection) -> None:
    """Two CASH_WITHDRAWAL ACK docs are counted and summed correctly."""
    shift_id = "shift-withdrawal-001"
    _insert_shift(conn, shift_id)

    _insert_doc(
        conn, shift_id, "CASH_WITHDRAWAL", "ACK",
        {"cash_withdrawal_sum": 20000, "receipt": {"payments": []}},
    )
    _insert_doc(
        conn, shift_id, "CASH_WITHDRAWAL", "ACK",
        {"cash_withdrawal_sum": 15000, "receipt": {"payments": []}},
    )
    conn.commit()

    result = aggregate_cash_withdrawals(conn, shift_id)

    assert result["count"] == 2
    assert result["sum"] == 35000
    assert result["commission"] == 0


def test_non_ack_docs_excluded(conn: sqlite3.Connection) -> None:
    """Documents in ERROR_RETRYABLE state are not counted in aggregation."""
    shift_id = "shift-noack-001"
    goods = [{"name": "Товар", "price": 1000, "quantity": 1000, "sum": 1000, "tax_id": 20}]
    _insert_shift(conn, shift_id)

    _insert_doc(conn, shift_id, "SELL", "ERROR_RETRYABLE", _sell_payload(goods, []))
    conn.commit()

    result = aggregate_shift_data(conn, shift_id)
    assert result["check_count"]["ni"] == 0
    assert result["tax_sums"] == {}


def test_offline_local_ack_docs_included(conn: sqlite3.Connection) -> None:
    """Documents with OFFLINE_LOCAL_ACK state are included (same as ACK)."""
    shift_id = "shift-offline-ack-001"
    goods = [{"name": "Офлайн товар", "price": 5000, "quantity": 1000, "sum": 5000, "tax_id": 20}]
    _insert_shift(conn, shift_id)

    _insert_doc(conn, shift_id, "SELL", "OFFLINE_LOCAL_ACK", _sell_payload(goods, []))
    conn.commit()

    result = aggregate_shift_data(conn, shift_id)
    assert result["check_count"]["ni"] == 1
    assert result["tax_sums"]["20"]["smi"] == 5000


def test_sell_and_return_mixed_in_same_shift(conn: sqlite3.Connection) -> None:
    """SELL and RETURN in same shift: ni/no counts and smi/smo per tax_id are correct."""
    shift_id = "shift-mixed-001"
    sell_goods = [{"name": "Масло", "price": 4000, "quantity": 1000, "sum": 4000, "tax_id": 20}]
    ret_goods = [{"name": "Масло", "price": 2000, "quantity": 500, "sum": 2000, "tax_id": 20}]
    _insert_shift(conn, shift_id)

    _insert_doc(conn, shift_id, "SELL", "ACK", _sell_payload(sell_goods, []))
    _insert_doc(conn, shift_id, "RETURN", "ACK", _sell_payload(ret_goods, []))
    conn.commit()

    result = aggregate_shift_data(conn, shift_id)

    assert result["check_count"] == {"ni": 1, "no": 1}
    assert result["tax_sums"]["20"]["smi"] == 4000
    assert result["tax_sums"]["20"]["smo"] == 2000
