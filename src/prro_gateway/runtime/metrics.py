from __future__ import annotations

from collections import Counter
from dataclasses import dataclass, field
from threading import Lock
from time import monotonic
from typing import Any


@dataclass
class MetricsCollector:
    counters: Counter[str] = field(default_factory=Counter)
    gauges: dict[str, float] = field(default_factory=dict)
    _lock: Lock = field(default_factory=Lock, repr=False)
    _started_at: float = field(default_factory=monotonic, repr=False)

    def inc(self, name: str, amount: int = 1) -> None:
        with self._lock:
            self.counters[name] += amount

    def set_gauge(self, name: str, value: float) -> None:
        with self._lock:
            self.gauges[name] = value

    def snapshot(self) -> dict[str, Any]:
        with self._lock:
            return {
                'uptime_seconds': round(monotonic() - self._started_at, 3),
                'counters': dict(self.counters),
                'gauges': dict(self.gauges),
            }


__all__ = ['MetricsCollector']
