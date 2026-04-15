from __future__ import annotations

from typing import Any
from pydantic import BaseModel, Field, ConfigDict

from ..enums import CanonicalErrorCode


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)


class CanonicalError(StrictModel):
    code: CanonicalErrorCode
    message: str
    retryable: bool
    retry_after_seconds: int | None = Field(default=None, ge=0)
    details: dict[str, Any] | None = None


__all__ = ['StrictModel', 'CanonicalError']
