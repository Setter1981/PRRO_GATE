from __future__ import annotations

from pydantic import ConfigDict

from ..models.common import StrictModel


class RuntimeHealthState(StrictModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True, validate_assignment=True)

    live: bool = True
    ready: bool = False
    startup_complete: bool = False
    phase: str = "CREATED"
    last_error: str | None = None


__all__ = ["RuntimeHealthState"]
