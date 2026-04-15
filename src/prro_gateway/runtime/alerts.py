from __future__ import annotations

import json
import sqlite3
from dataclasses import dataclass
from typing import Any

from ..repositories.audit import AuditRepository


@dataclass(slots=True)
class AlertEvent:
    entity_type: str
    entity_id: str
    event_type: str
    severity: str = 'WARNING'
    payload: dict[str, Any] | None = None


class AlertSink:
    def __init__(self, *, enabled: bool = True, persist_to_audit: bool = True) -> None:
        self.enabled = enabled
        self.persist_to_audit = persist_to_audit

    def emit(self, conn: sqlite3.Connection | None, *, event: AlertEvent) -> None:
        if not self.enabled:
            return
        if self.persist_to_audit and conn is not None:
            AuditRepository.log_event(
                conn,
                entity_type=event.entity_type,
                entity_id=event.entity_id,
                event_type=event.event_type,
                severity=event.severity,
                event_payload_json=json.dumps(event.payload or {}, separators=(',', ':'), sort_keys=True),
            )


__all__ = ['AlertEvent', 'AlertSink']
