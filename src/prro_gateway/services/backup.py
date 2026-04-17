"""F1 — SQLite online backup with rotation and integrity check.

Invariants preserved:
- No network or crypto calls (pure local I/O).
- No long SQLite write transactions: backup() uses the read path; node_state
  updates are done in a short BEGIN IMMEDIATE..COMMIT block.
- STOP_MODE is set atomically per-FN inside a committed transaction.
"""
from __future__ import annotations

import sqlite3
from dataclasses import dataclass, field
from datetime import datetime, UTC
from pathlib import Path

from ..enums import NodeMode
from ..repositories.audit import AuditRepository
from ..repositories.node_state import NodeStateRepository


@dataclass
class BackupResult:
    success: bool
    path: str | None = None
    error: str | None = None
    files_deleted: int = 0


class BackupService:
    _BACKUP_GLOB = 'prro.db.backup.*'
    _BACKUP_PREFIX = 'prro.db.backup.'

    def __init__(self, db_path: str, backup_dir: str, keep_count: int = 7) -> None:
        self.db_path = db_path
        self.backup_dir = Path(backup_dir)
        self.keep_count = max(1, keep_count)

    def run_backup(
        self,
        conn: sqlite3.Connection,
        fiscal_numbers: list[str],
    ) -> BackupResult:
        """Perform online backup, integrity check, rotation, last_backup_at update.

        Steps:
        1. Create backup_dir (idempotent).
        2. Copy live DB to timestamped file via conn.backup().
        3. Run PRAGMA integrity_check on the backup file.
        4. On failure: set all FNs to STOP_MODE + emit BACKUP_INTEGRITY_FAILED audit.
        5. On success: rotate old files (keep keep_count), update last_backup_at.

        Precondition: conn must have no open transaction.  The caller owns the
        connection lifecycle; passing a conn with an active transaction will cause
        the internal BEGIN IMMEDIATE calls to raise OperationalError.
        """
        if conn.in_transaction:
            return BackupResult(success=False, error='run_backup called with an open transaction on conn')

        self.backup_dir.mkdir(parents=True, exist_ok=True)

        ts = datetime.now(UTC).strftime('%Y%m%d_%H%M%S_%f')
        backup_path = self.backup_dir / f'{self._BACKUP_PREFIX}{ts}'

        # Online backup (safe with WAL; does not block readers/writers)
        try:
            with sqlite3.connect(str(backup_path)) as target:
                conn.backup(target)
        except Exception as exc:
            return BackupResult(success=False, error=f'backup() failed: {exc}')

        # Integrity check on the backup file (not the live DB)
        ok, msg = self._check_integrity(str(backup_path))
        if not ok:
            backup_path.unlink(missing_ok=True)
            self._set_stop_mode(conn, fiscal_numbers, msg, str(backup_path))
            return BackupResult(success=False, path=str(backup_path), error=f'integrity_check failed: {msg}')

        # Rotate old backups
        deleted = self._rotate()

        # Update last_backup_at for all FNs (short write transaction)
        now_iso = datetime.now(UTC).isoformat()
        conn.execute('BEGIN IMMEDIATE')
        for fn in fiscal_numbers:
            NodeStateRepository.update_last_backup_at(conn, fiscal_number=fn, timestamp=now_iso)
        conn.commit()

        return BackupResult(success=True, path=str(backup_path), files_deleted=deleted)

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _check_integrity(self, db_path: str) -> tuple[bool, str]:
        """Run PRAGMA integrity_check on the backup file. Returns (ok, message)."""
        try:
            with sqlite3.connect(db_path) as chk:
                row = chk.execute('PRAGMA integrity_check').fetchone()
                result = row[0] if row else 'no result'
                return (result == 'ok'), result
        except Exception as exc:
            return False, str(exc)

    def _rotate(self) -> int:
        """Delete oldest backup files, keeping only keep_count. Returns deleted count."""
        existing = sorted(
            self.backup_dir.glob(self._BACKUP_GLOB),
            key=lambda p: p.name,
        )
        to_delete = existing[:-self.keep_count] if len(existing) > self.keep_count else []
        for f in to_delete:
            try:
                f.unlink(missing_ok=True)
            except Exception:
                pass
        return len(to_delete)

    def _set_stop_mode(
        self,
        conn: sqlite3.Connection,
        fiscal_numbers: list[str],
        integrity_msg: str,
        backup_path: str,
    ) -> None:
        """Transition all FNs to STOP_MODE and emit BACKUP_INTEGRITY_FAILED audit events."""
        import json as _json
        conn.execute('BEGIN IMMEDIATE')
        for fn in fiscal_numbers:
            ns = NodeStateRepository.get_state(conn, fn)
            if ns is not None:
                NodeStateRepository.update_mode(conn, fiscal_number=fn, mode=NodeMode.STOP_MODE)
                AuditRepository.log_event(
                    conn,
                    entity_type='NODE',
                    entity_id=fn,
                    event_type='BACKUP_INTEGRITY_FAILED',
                    severity='CRITICAL',
                    event_payload_json=_json.dumps({
                        'integrity_result': integrity_msg,
                        'backup_path': backup_path,
                    }),
                )
        conn.commit()


__all__ = ['BackupService', 'BackupResult']
