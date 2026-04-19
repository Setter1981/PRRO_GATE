"""F2 — RetentionService: TTL-based purge of audit, trace, and inbox rows."""
from __future__ import annotations

import sqlite3
import uuid
from datetime import datetime, timedelta, UTC
from pathlib import Path

import pytest

from prro_gateway.migrations.runner import apply_migrations_to_connection
from prro_gateway.services.retention import RetentionService, PurgeResult


ROOT = Path(__file__).resolve().parents[1]
SQL_ROOT = ROOT / "sql"


def _fresh_conn() -> sqlite3.Connection:
    conn = sqlite3.connect(":memory:")
    apply_migrations_to_connection(conn, SQL_ROOT)
    return conn


# ---------------------------------------------------------------------------
# Timestamp helpers
#
# IMPORTANT: audit_log/trace/inbox use DEFAULT CURRENT_TIMESTAMP which stores
# "YYYY-MM-DD HH:MM:SS" (space, no tz).  RetentionService cutoffs use the same
# format so that lexicographic comparison is correct on the boundary day.
#
# Tests that explicitly set created_at use the same space-format to mimic
# production.  One test (_rt_production_*) uses DEFAULT CURRENT_TIMESTAMP
# (omits created_at) to verify the exact production path.
# ---------------------------------------------------------------------------

def _old_sql_ts(days: int = 100) -> str:
    """SQLite CURRENT_TIMESTAMP format, N days in the past."""
    return (datetime.now(UTC) - timedelta(days=days)).strftime('%Y-%m-%d %H:%M:%S')


def _new_sql_ts(days: int = 1) -> str:
    """SQLite CURRENT_TIMESTAMP format, N days in the past (recent)."""
    return (datetime.now(UTC) - timedelta(days=days)).strftime('%Y-%m-%d %H:%M:%S')


# ---------------------------------------------------------------------------
# Insert helpers (all use space-format timestamps matching CURRENT_TIMESTAMP)
# ---------------------------------------------------------------------------

def _insert_audit(conn, entity_id: str = "FN1", created_at: str | None = None) -> int:
    cur = conn.execute(
        """
        INSERT INTO audit_log
            (entity_type, entity_id, event_type, severity, created_at)
        VALUES ('NODE', ?, 'TEST_EVENT', 'INFO',
                COALESCE(?, CURRENT_TIMESTAMP))
        """,
        (entity_id, created_at),
    )
    conn.commit()
    return cur.lastrowid


def _insert_proto_trace(conn, created_at: str | None = None) -> str:
    trace_id = str(uuid.uuid4())
    conn.execute(
        """
        INSERT INTO protocol_trace_log
            (trace_id, fiscal_number, protocol, direction, trace_level, created_at)
        VALUES (?, 'FN1', 'INTERNAL', 'IN', 'SAFE',
                COALESCE(?, CURRENT_TIMESTAMP))
        """,
        (trace_id, created_at),
    )
    conn.commit()
    return trace_id


def _insert_transport_trace(conn, created_at: str | None = None) -> str:
    trace_id = str(uuid.uuid4())
    conn.execute(
        """
        INSERT INTO transport_trace_log
            (trace_id, fiscal_number, direction, created_at)
        VALUES (?, 'FN1', 'OUT',
                COALESCE(?, CURRENT_TIMESTAMP))
        """,
        (trace_id, created_at),
    )
    conn.commit()
    return trace_id


def _insert_inbox(conn, status: str = "DONE", created_at: str | None = None) -> str:
    import hashlib
    req_id = str(uuid.uuid4())
    idem_key = str(uuid.uuid4())
    payload = "{}"
    sha = hashlib.sha256(payload.encode()).hexdigest()
    conn.execute(
        """
        INSERT INTO ingress_inbox
            (request_id, idempotency_key, fiscal_number, protocol, operation_type,
             payload_json, payload_sha256, status, created_at)
        VALUES (?, ?, 'FN1', 'INTERNAL', 'PING', ?, ?, ?,
                COALESCE(?, CURRENT_TIMESTAMP))
        """,
        (req_id, idem_key, payload, sha, status, created_at),
    )
    conn.commit()
    return req_id


def _insert_inbox_with_doc(conn, status: str = "DONE", created_at: str | None = None) -> str:
    """Insert an inbox row that has a referencing fiscal_document (FK constraint)."""
    req_id = _insert_inbox(conn, status=status, created_at=created_at)
    conn.execute(
        """
        INSERT INTO fiscal_documents
            (document_id, request_id, fiscal_number, lnd, doc_type, state,
             backend_profile_id, transport_profile_id, fs_mode, business_ts,
             payload_json, payload_sha256)
        VALUES (?, ?, 'FN1', 1, 'STATUS', 'ACK',
                'backend_checkbox_default', 'transport_checkbox_rest_default',
                'ONLINE', CURRENT_TIMESTAMP, '{}', 'sha256-placeholder')
        """,
        (str(uuid.uuid4()), req_id),
    )
    conn.commit()
    return req_id


# ---------------------------------------------------------------------------
# RT1: old audit rows deleted; recent rows kept
# ---------------------------------------------------------------------------

def test_rt1_old_audit_deleted_recent_kept():
    conn = _fresh_conn()
    _insert_audit(conn, created_at=_old_sql_ts(100))
    _insert_audit(conn, created_at=_new_sql_ts(1))

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is True
    assert result.audit_deleted == 1
    assert conn.execute("SELECT COUNT(*) FROM audit_log").fetchone()[0] == 1


# ---------------------------------------------------------------------------
# RT2: recent audit rows not deleted
# ---------------------------------------------------------------------------

def test_rt2_recent_audit_not_deleted():
    conn = _fresh_conn()
    _insert_audit(conn, created_at=_new_sql_ts(1))
    _insert_audit(conn, created_at=_new_sql_ts(2))

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is True
    assert result.audit_deleted == 0


# ---------------------------------------------------------------------------
# RT3: old protocol and transport trace rows deleted; recent kept
# ---------------------------------------------------------------------------

def test_rt3_old_trace_rows_deleted():
    conn = _fresh_conn()
    _insert_proto_trace(conn, created_at=_old_sql_ts(40))
    _insert_proto_trace(conn, created_at=_new_sql_ts(1))
    _insert_transport_trace(conn, created_at=_old_sql_ts(40))
    _insert_transport_trace(conn, created_at=_new_sql_ts(1))

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is True
    assert result.protocol_trace_deleted == 1
    assert result.transport_trace_deleted == 1


# ---------------------------------------------------------------------------
# RT4: old completed inbox rows (DONE/ERROR/DEAD) deleted;
#      NEW/PROCESSING rows are NOT deleted even when old
# ---------------------------------------------------------------------------

def test_rt4_old_completed_inbox_deleted_active_kept():
    conn = _fresh_conn()
    done_req  = _insert_inbox(conn, status="DONE",       created_at=_old_sql_ts(100))
    err_req   = _insert_inbox(conn, status="ERROR",      created_at=_old_sql_ts(100))
    dead_req  = _insert_inbox(conn, status="DEAD",       created_at=_old_sql_ts(100))
    new_req   = _insert_inbox(conn, status="NEW",        created_at=_old_sql_ts(100))
    proc_req  = _insert_inbox(conn, status="PROCESSING", created_at=_old_sql_ts(100))

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is True
    assert result.inbox_deleted == 3  # DONE + ERROR + DEAD

    # Active rows must physically still exist in the DB
    new_row  = conn.execute("SELECT 1 FROM ingress_inbox WHERE request_id = ?", (new_req,)).fetchone()
    proc_row = conn.execute("SELECT 1 FROM ingress_inbox WHERE request_id = ?", (proc_req,)).fetchone()
    assert new_row is not None,  "NEW row must not be deleted"
    assert proc_row is not None, "PROCESSING row must not be deleted"

    # Verify terminal rows are actually gone
    for req_id in (done_req, err_req, dead_req):
        row = conn.execute("SELECT 1 FROM ingress_inbox WHERE request_id = ?", (req_id,)).fetchone()
        assert row is None, f"Terminal row {req_id} should have been deleted"


# ---------------------------------------------------------------------------
# RT5: inbox rows referenced by fiscal_documents are NOT deleted (FK-safe);
#      orphan rows (no FK reference) ARE deleted
# ---------------------------------------------------------------------------

def test_rt5_fk_safe_inbox_purge():
    conn = _fresh_conn()

    # This row has a fiscal_document referencing it → must survive
    referenced_req = _insert_inbox_with_doc(conn, status="DONE", created_at=_old_sql_ts(100))

    # This row has no fiscal_document → must be deleted
    orphan_req = _insert_inbox(conn, status="DONE", created_at=_old_sql_ts(100))

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is True
    assert result.inbox_deleted == 1  # only the orphan

    # Orphan must be gone
    orphan = conn.execute(
        "SELECT 1 FROM ingress_inbox WHERE request_id = ?", (orphan_req,)
    ).fetchone()
    assert orphan is None, "Orphan inbox row must have been deleted"

    # Referenced row must survive
    referenced = conn.execute(
        "SELECT 1 FROM ingress_inbox WHERE request_id = ?", (referenced_req,)
    ).fetchone()
    assert referenced is not None, "FK-referenced inbox row must NOT be deleted"


# ---------------------------------------------------------------------------
# RT6: recent inbox rows not deleted regardless of status
# ---------------------------------------------------------------------------

def test_rt6_recent_inbox_not_deleted():
    conn = _fresh_conn()
    _insert_inbox(conn, status="DONE", created_at=_new_sql_ts(1))

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is True
    assert result.inbox_deleted == 0


# ---------------------------------------------------------------------------
# RT7: total_deleted == sum of individual counters
# ---------------------------------------------------------------------------

def test_rt7_total_deleted():
    conn = _fresh_conn()
    _insert_audit(conn, created_at=_old_sql_ts(100))
    _insert_audit(conn, created_at=_old_sql_ts(100))
    _insert_proto_trace(conn, created_at=_old_sql_ts(40))
    _insert_transport_trace(conn, created_at=_old_sql_ts(40))
    _insert_inbox(conn, status="DONE", created_at=_old_sql_ts(100))

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is True
    expected = (
        result.audit_deleted
        + result.protocol_trace_deleted
        + result.transport_trace_deleted
        + result.inbox_deleted
    )
    assert result.total_deleted == expected == 5


# ---------------------------------------------------------------------------
# RT8: empty database returns success with all-zero counts
# ---------------------------------------------------------------------------

def test_rt8_empty_db_returns_success():
    conn = _fresh_conn()

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is True
    assert result.total_deleted == 0
    assert result.error is None


# ---------------------------------------------------------------------------
# RT9: production-like — rows inserted via DEFAULT CURRENT_TIMESTAMP are deleted
#
# This is the critical production path: actual rows use CURRENT_TIMESTAMP
# (space format "YYYY-MM-DD HH:MM:SS") inserted by normal application code.
# The retention cutoff must also use the same format to compare correctly.
# ---------------------------------------------------------------------------

def test_rt9_production_default_timestamp_rows_are_deleted():
    """Rows inserted without explicit created_at (uses CURRENT_TIMESTAMP) must be
    purged correctly by a very short TTL (1-second window artificially created by
    inserting 1 day ago via SQL datetime arithmetic).

    Strategy: insert rows with created_at = datetime('now', '-2 days') which
    SQLite evaluates to CURRENT_TIMESTAMP-minus-2-days using the same YYYY-MM-DD
    format, then verify a 1-day TTL deletes them.
    """
    conn = _fresh_conn()

    # Insert using SQLite's own datetime arithmetic — same format as CURRENT_TIMESTAMP
    conn.execute(
        "INSERT INTO audit_log (entity_type, entity_id, event_type, severity, created_at)"
        " VALUES ('NODE', 'FN_PROD', 'PROD_TEST', 'INFO', datetime('now', '-2 days'))"
    )
    conn.commit()

    svc = RetentionService(audit_ttl_days=1, trace_ttl_days=1, inbox_ttl_days=1)
    result = svc.run_purge(conn)

    assert result.success is True
    assert result.audit_deleted == 1, (
        "Row inserted via SQLite datetime() (CURRENT_TIMESTAMP format) must be deleted "
        "— format mismatch between T-format cutoff and space-format DB values would break this"
    )


# ---------------------------------------------------------------------------
# RT10: boundary — row at exactly the cutoff is NOT deleted (strict < comparison)
# ---------------------------------------------------------------------------

def test_rt10_boundary_row_exactly_at_cutoff_not_deleted():
    """A row with created_at exactly equal to the TTL cutoff must survive (< not <=)."""
    conn = _fresh_conn()

    # Create a row whose timestamp equals the cutoff exactly
    from prro_gateway.services.retention import RetentionService as _Svc
    now = datetime.now(UTC)
    cutoff_ts = (now - timedelta(days=90)).strftime('%Y-%m-%d %H:%M:%S')

    conn.execute(
        "INSERT INTO audit_log (entity_type, entity_id, event_type, severity, created_at)"
        " VALUES ('NODE', 'FN_BNDRY', 'BOUNDARY', 'INFO', ?)",
        (cutoff_ts,),
    )
    conn.commit()

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is True
    assert result.audit_deleted == 0, (
        "Row at exactly the cutoff timestamp must NOT be deleted (< not <=)"
    )


# ---------------------------------------------------------------------------
# RT11: calling run_purge with an open transaction returns error, not exception
# ---------------------------------------------------------------------------

def test_rt11_open_transaction_returns_error_not_exception():
    """If conn already has an open transaction, run_purge must return a failure
    result rather than raising, and the caller's transaction must remain intact."""
    conn = _fresh_conn()
    _insert_audit(conn, created_at=_old_sql_ts(100))

    conn.execute("BEGIN IMMEDIATE")

    svc = RetentionService(audit_ttl_days=90, trace_ttl_days=30, inbox_ttl_days=90)
    result = svc.run_purge(conn)

    assert result.success is False
    assert result.error is not None
    assert result.total_deleted == 0

    # Caller's transaction must still be usable
    conn.rollback()
