"""F1 — BackupService: online backup, integrity check, rotation, STOP_MODE on corruption."""
from __future__ import annotations

import sqlite3
import uuid
from datetime import datetime, UTC, timedelta
from pathlib import Path

import pytest

from prro_gateway.enums import NodeMode
from prro_gateway.migrations.runner import apply_migrations_to_connection
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.services.backup import BackupService, BackupResult


ROOT = Path(__file__).resolve().parents[1]
SQL_ROOT = ROOT / "sql"


def _fresh_conn() -> sqlite3.Connection:
    conn = sqlite3.connect(":memory:")
    apply_migrations_to_connection(conn, SQL_ROOT)
    return conn


def _seed_fn(conn: sqlite3.Connection, fn: str = "FN000001") -> None:
    """Insert a minimal node_state row. node_id == fiscal_number (single-DB topology)."""
    conn.execute(
        """
        INSERT INTO node_state (node_id, fiscal_number, mode, shift_state, next_lnd)
        VALUES (?, ?, 'ONLINE', 'CREATED', 1)
        """,
        (fn, fn),
    )
    conn.commit()


# ---------------------------------------------------------------------------
# BK1: successful backup creates a valid SQLite file and updates last_backup_at
# ---------------------------------------------------------------------------

def test_bk1_success_creates_valid_backup_and_updates_last_backup_at(tmp_path):
    conn = _fresh_conn()
    _seed_fn(conn)

    svc = BackupService(db_path=":memory:", backup_dir=str(tmp_path), keep_count=7)
    before = datetime.now(UTC)
    result = svc.run_backup(conn, fiscal_numbers=["FN000001"])

    assert result.success is True
    assert result.path is not None
    assert result.error is None

    # File exists and has the right naming convention
    backup_file = Path(result.path)
    assert backup_file.exists()
    assert backup_file.name.startswith("prro.db.backup.")

    # Backup file must be a valid SQLite database, not just any bytes
    with sqlite3.connect(str(backup_file)) as chk:
        integrity = chk.execute("PRAGMA integrity_check").fetchone()[0]
    assert integrity == "ok", f"Backup file failed integrity check: {integrity}"

    # last_backup_at is set and is recent (within 60s of the test start)
    ns = NodeStateRepository.get_state(conn, "FN000001")
    assert ns is not None
    assert ns.last_backup_at is not None
    ts = datetime.fromisoformat(ns.last_backup_at)
    assert (ts - before).total_seconds() >= -1  # not before test started
    assert (datetime.now(UTC) - ts).total_seconds() < 60  # recent


# ---------------------------------------------------------------------------
# BK2: backup_dir is created automatically if it doesn't exist
# ---------------------------------------------------------------------------

def test_bk2_backup_dir_created_automatically(tmp_path):
    nested = tmp_path / "deep" / "nested" / "backups"
    assert not nested.exists()

    conn = _fresh_conn()
    _seed_fn(conn)

    svc = BackupService(db_path=":memory:", backup_dir=str(nested), keep_count=7)
    result = svc.run_backup(conn, fiscal_numbers=["FN000001"])

    assert result.success is True
    assert nested.exists()
    assert Path(result.path).exists()


# ---------------------------------------------------------------------------
# BK3: rotation keeps only keep_count files; non-matching files are left alone
# ---------------------------------------------------------------------------

def test_bk3_rotation_keeps_only_keep_count(tmp_path):
    conn = _fresh_conn()
    _seed_fn(conn)

    svc = BackupService(db_path=":memory:", backup_dir=str(tmp_path), keep_count=3)

    for _ in range(5):
        result = svc.run_backup(conn, fiscal_numbers=["FN000001"])
        assert result.success is True

    remaining = sorted(tmp_path.glob("prro.db.backup.*"))
    assert len(remaining) == 3


def test_bk3b_rotation_ignores_non_matching_files(tmp_path):
    """Files not matching the backup glob must not be rotated or deleted."""
    conn = _fresh_conn()
    _seed_fn(conn)

    # Pre-populate a file that should never be touched
    sentinel = tmp_path / "do_not_delete.txt"
    sentinel.write_text("sentinel")

    svc = BackupService(db_path=":memory:", backup_dir=str(tmp_path), keep_count=2)
    for _ in range(4):
        svc.run_backup(conn, fiscal_numbers=["FN000001"])

    assert sentinel.exists(), "Non-matching file was incorrectly deleted by rotation"
    assert len(list(tmp_path.glob("prro.db.backup.*"))) == 2


# ---------------------------------------------------------------------------
# BK4: files_deleted reflects rotated count
# ---------------------------------------------------------------------------

def test_bk4_files_deleted_count(tmp_path):
    conn = _fresh_conn()
    _seed_fn(conn)

    svc = BackupService(db_path=":memory:", backup_dir=str(tmp_path), keep_count=2)

    r1 = svc.run_backup(conn, fiscal_numbers=["FN000001"])
    r2 = svc.run_backup(conn, fiscal_numbers=["FN000001"])
    r3 = svc.run_backup(conn, fiscal_numbers=["FN000001"])  # triggers deletion of oldest

    assert r1.files_deleted == 0
    assert r2.files_deleted == 0
    assert r3.files_deleted == 1


# ---------------------------------------------------------------------------
# BK5: integrity check failure → all FNs set to STOP_MODE + backup file deleted
# ---------------------------------------------------------------------------

def test_bk5_integrity_failure_triggers_stop_mode_and_deletes_file(tmp_path, monkeypatch):
    conn = _fresh_conn()
    _seed_fn(conn, "FN111")
    _seed_fn(conn, "FN222")

    svc = BackupService(db_path=":memory:", backup_dir=str(tmp_path), keep_count=7)
    monkeypatch.setattr(svc, "_check_integrity", lambda path: (False, "page 3 of 10: corrupt"))

    result = svc.run_backup(conn, fiscal_numbers=["FN111", "FN222"])

    assert result.success is False
    assert "integrity_check failed" in (result.error or "")

    # Both FNs must be in STOP_MODE
    ns1 = NodeStateRepository.get_state(conn, "FN111")
    ns2 = NodeStateRepository.get_state(conn, "FN222")
    assert ns1 is not None and ns1.mode == NodeMode.STOP_MODE
    assert ns2 is not None and ns2.mode == NodeMode.STOP_MODE

    # Corrupted backup file must be deleted, not left on disk
    assert result.path is not None
    assert not Path(result.path).exists(), "Corrupted backup file must be removed"


# ---------------------------------------------------------------------------
# BK6: STOP_MODE audit events — CRITICAL severity, correct event_type
# ---------------------------------------------------------------------------

def test_bk6_stop_mode_audit_events_emitted(tmp_path, monkeypatch):
    conn = _fresh_conn()
    _seed_fn(conn, "FN999")

    svc = BackupService(db_path=":memory:", backup_dir=str(tmp_path), keep_count=7)
    monkeypatch.setattr(svc, "_check_integrity", lambda path: (False, "corrupt"))

    result = svc.run_backup(conn, fiscal_numbers=["FN999"])

    # Backup file must be deleted (not left as a corrupt artifact)
    assert result.path is not None
    assert not Path(result.path).exists()

    rows = conn.execute(
        "SELECT event_type, severity, event_payload_json FROM audit_log"
        " WHERE entity_id = ? ORDER BY audit_id",
        ("FN999",),
    ).fetchall()

    matching = [r for r in rows if r[0] == "BACKUP_INTEGRITY_FAILED" and r[1] == "CRITICAL"]
    assert matching, f"Expected BACKUP_INTEGRITY_FAILED/CRITICAL in audit_log, got: {rows}"

    # Payload must contain the integrity_result field
    import json
    payload = json.loads(matching[0][2])
    assert "integrity_result" in payload
    assert "corrupt" in payload["integrity_result"]


# ---------------------------------------------------------------------------
# BK7: STOP_MODE guard in write_path rejects fiscal ops; inbox marked ERROR
# ---------------------------------------------------------------------------

def test_bk7_stop_mode_guard_rejects_fiscal_ops():
    """After STOP_MODE is set, write-path rejects non-GET_STATUS ops and
    marks the inbox row as ERROR with canonical_error_code=STOP_MODE_ACTIVE."""
    from datetime import datetime, UTC
    from prro_gateway.enums import CanonicalErrorCode, OperationType, Protocol
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories.inbox import InboxRepository
    from prro_gateway.services.write_path import WritePathWorker

    conn = _fresh_conn()
    _seed_fn(conn, "FN_STOP")

    conn.execute("UPDATE node_state SET mode = 'STOP_MODE' WHERE fiscal_number = 'FN_STOP'")
    conn.commit()

    worker = WritePathWorker(
        crypto_provider=None,
        transport_client=None,
        require_return_linkage=False,
        validate_timestamps=False,
    )

    rid = str(uuid.uuid4())
    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=f'idem-{rid}',
        protocol=Protocol.INTERNAL, operation_type=OperationType.SELL,
        fiscal_number='FN_STOP', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='test', external_request_id=f'ext-{rid}',
        business_ts=datetime(2026, 4, 16, 22, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100, 'tax_id': '1'}],
                'payments': [{'amount': 100, 'type': 'CASH'}],
                'totals': {'total_sum': 100},
            },
        },
        payload_sha256=f'sha-{rid}',
        trace_context=TraceContext(
            source_ip='127.0.0.1', source_port=1,
            session_id='s', correlation_id=f'c-{rid}',
        ),
        correlation_id=f'c-{rid}',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id,
        transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-16T23:00:00Z',
    )
    conn.commit()

    result = worker.process_next(conn, fiscal_number="FN_STOP")

    # Return value
    assert result is not None
    assert result.canonical_error is not None
    assert result.canonical_error.code == CanonicalErrorCode.STOP_MODE_ACTIVE

    # DB side-effect: inbox row must be marked ERROR with the correct code
    row = conn.execute(
        "SELECT status, canonical_error_code FROM ingress_inbox WHERE request_id = ?",
        (rid,),
    ).fetchone()
    assert row is not None, "Inbox row not found after processing"
    assert row[0] == "ERROR", f"Expected status=ERROR, got: {row[0]}"
    assert row[1] == "STOP_MODE_ACTIVE", f"Expected canonical_error_code=STOP_MODE_ACTIVE, got: {row[1]}"


# ---------------------------------------------------------------------------
# BK8: empty fiscal_numbers — backup succeeds, no node_state rows updated
# ---------------------------------------------------------------------------

def test_bk8_empty_fiscal_numbers(tmp_path):
    """Backup with no fiscal_numbers must succeed and create a valid backup file."""
    conn = _fresh_conn()

    svc = BackupService(db_path=":memory:", backup_dir=str(tmp_path), keep_count=7)
    result = svc.run_backup(conn, fiscal_numbers=[])

    assert result.success is True
    assert result.path is not None
    assert Path(result.path).exists()

    # With empty fiscal_numbers, no last_backup_at should be updated on any node_state row
    # (The seed migration inserts FN-DEV-0001; its last_backup_at must remain NULL)
    row = conn.execute(
        "SELECT last_backup_at FROM node_state WHERE fiscal_number = 'FN-DEV-0001'"
    ).fetchone()
    if row is not None:
        assert row[0] is None, "last_backup_at must not be set when fiscal_numbers=[]"
