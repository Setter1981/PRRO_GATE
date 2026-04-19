from __future__ import annotations

import sqlite3
import time
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Callable

from ..models.common import StrictModel
from ..runtime.health import RuntimeHealthState
from ..services.reconciliation import ReconciliationRunResult, ReconciliationService


class StartupRunReport(StrictModel):
    started_at: str
    finished_at: str
    phase1_ok: bool
    phase2_attempted: bool
    ready_after_phase1: bool
    reconciliation_checked: int = 0
    reconciliation_acked: int = 0
    reconciliation_rejected: int = 0
    reconciliation_retryable: int = 0
    reconciliation_still_pending: int = 0
    reconciliation_manual: int = 0


@dataclass
class StartupSupervisor:
    health: RuntimeHealthState
    connect_factory: Callable[[], object]
    migrate: Callable[[], None]
    startup_ready: bool = True
    reconcile_on_startup: bool = True
    phase2_budget_seconds: int = 300
    reconciliation_service: ReconciliationService | None = None

    def run(self) -> StartupRunReport:
        started = datetime.now(UTC)
        self.health.phase = 'PHASE1_STARTING'
        self.health.ready = False
        self.health.startup_complete = False
        self.health.last_error = None
        self.migrate()
        self.health.phase = 'PHASE1_COMPLETE'
        # M1: reset OPEN offline session started_at to now so that process downtime is not
        # counted as offline time against the monthly/continuous limit.  The session was not
        # actively serving receipts during the outage; accumulated_month_seconds already
        # captures any time that was accrued up to the last ops_tick before the crash.
        with self.connect_factory() as conn:
            self._reset_open_offline_sessions(conn, now=started)
        if self.startup_ready:
            self.health.ready = True
        phase2_attempted = False
        recon_result = ReconciliationRunResult()
        if self.reconcile_on_startup and self.reconciliation_service is not None:
            phase2_attempted = True
            self.health.phase = 'PHASE2_RECONCILING'
            t0 = time.monotonic()
            with self.connect_factory() as conn:
                recon_result = self.reconciliation_service.reconcile_pending(conn)
            elapsed = time.monotonic() - t0
            if elapsed > self.phase2_budget_seconds:
                self.health.last_error = f'phase2 budget exceeded: {elapsed:.3f}s > {self.phase2_budget_seconds}s'
        self.health.phase = 'RUNNING'
        self.health.startup_complete = True
        finished = datetime.now(UTC)
        return StartupRunReport(
            started_at=started.isoformat(),
            finished_at=finished.isoformat(),
            phase1_ok=True,
            phase2_attempted=phase2_attempted,
            ready_after_phase1=self.health.ready,
            reconciliation_checked=recon_result.checked,
            reconciliation_acked=recon_result.acked,
            reconciliation_rejected=recon_result.rejected,
            reconciliation_retryable=recon_result.retryable,
            reconciliation_still_pending=recon_result.still_pending,
            reconciliation_manual=recon_result.manual,
        )


    def _reset_open_offline_sessions(self, conn: sqlite3.Connection, now: datetime) -> None:
        """Reset started_at for any OPEN offline sessions to exclude process downtime.

        When the process crashes while in OFFLINE mode, the session's started_at continues
        to age even though no receipts were being served. Resetting it to now() on startup
        ensures only actual offline-service time counts against the monthly limit.
        """
        now_iso = now.isoformat()
        rows = conn.execute(
            "SELECT offline_session_id, fiscal_number FROM offline_sessions WHERE status = 'OPEN'"
        ).fetchall()
        if not rows:
            return
        conn.execute('BEGIN IMMEDIATE')
        for (session_id, fiscal_number) in rows:
            conn.execute(
                "UPDATE offline_sessions SET started_at = ? WHERE offline_session_id = ?",
                (now_iso, session_id),
            )
        conn.commit()


__all__ = ['StartupSupervisor', 'StartupRunReport']
