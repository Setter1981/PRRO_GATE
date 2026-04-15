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


__all__ = ['StartupSupervisor', 'StartupRunReport']
