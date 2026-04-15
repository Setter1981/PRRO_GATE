from __future__ import annotations

from datetime import datetime
from typing import Any
from pydantic import Field

from ..constants import SCHEMA_VERSION
from ..enums import HubErrorCode
from .common import StrictModel


class HubDocumentEnvelope(StrictModel):
    schema_version: str = Field(default=SCHEMA_VERSION)
    fiscal_number: str
    artifact_class: str
    aggregate_type: str
    aggregate_id: str
    sequence_no: int = Field(ge=0)
    idempotency_key: str
    created_at: datetime
    payload: dict[str, Any]


class HubError(StrictModel):
    hub_error_code: HubErrorCode
    retryable: bool
    retry_after_seconds: int | None = Field(default=None, ge=0)
    message: str


__all__ = ['HubDocumentEnvelope', 'HubError']
