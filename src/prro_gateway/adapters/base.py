from __future__ import annotations

from datetime import datetime
from hashlib import sha256
from typing import Any

from pydantic import Field

from ..constants import SCHEMA_VERSION
from ..enums import OperationType, Protocol
from ..models.canonical import CanonicalFiscalCommand, TraceContext
from ..models.common import StrictModel
from ..utils.json_codec import dumps_json


class AdapterMappingError(Exception):
    def __init__(self, message: str, *, code: str = "UNKNOWN_METHOD") -> None:
        super().__init__(message)
        self.code = code
        self.message = message


class AdapterContext(StrictModel):
    request_id: str
    fiscal_number: str
    route_key: str | None = None
    backend_profile_id: str | None = None
    transport_profile_id: str | None = None
    channel_owner: str | None = None
    business_ts: datetime
    trace_context: TraceContext | None = None


class CanonicalEnvelopeBuilder:
    @staticmethod
    def build(
        *,
        context: AdapterContext,
        protocol: Protocol,
        operation_type: OperationType,
        payload: dict[str, Any],
        external_request_id: str | None,
        requires_shift: bool,
        requires_offline_code: bool,
        requires_signature: bool = True,
        requires_fiscal_number: bool = True,
        idempotency_hint: str | None = None,
    ) -> CanonicalFiscalCommand:
        payload_json = dumps_json(payload) or "{}"
        payload_sha256 = sha256(payload_json.encode("utf-8")).hexdigest()
        idempotency_key = CanonicalEnvelopeBuilder._build_idempotency_key(
            operation_type=operation_type,
            fiscal_number=context.fiscal_number,
            external_request_id=external_request_id,
            payload_sha256=payload_sha256,
            idempotency_hint=idempotency_hint,
        )
        correlation_id = context.trace_context.correlation_id if context.trace_context else None
        return CanonicalFiscalCommand(
            schema_version=SCHEMA_VERSION,
            request_id=context.request_id,
            idempotency_key=idempotency_key,
            protocol=protocol,
            operation_type=operation_type,
            fiscal_number=context.fiscal_number,
            route_key=context.route_key,
            backend_profile_id=context.backend_profile_id,
            transport_profile_id=context.transport_profile_id,
            channel_owner=context.channel_owner,
            external_request_id=external_request_id,
            business_ts=context.business_ts,
            payload=payload,
            payload_json=payload_json,
            payload_sha256=payload_sha256,
            trace_context=context.trace_context,
            requires_signature=requires_signature,
            requires_shift=requires_shift,
            requires_fiscal_number=requires_fiscal_number,
            requires_offline_code=requires_offline_code,
            correlation_id=correlation_id,
        )

    @staticmethod
    def _build_idempotency_key(
        *,
        operation_type: OperationType,
        fiscal_number: str,
        external_request_id: str | None,
        payload_sha256: str,
        idempotency_hint: str | None,
    ) -> str:
        op = operation_type.value.lower()
        fiscal = fiscal_number.lower()
        if external_request_id:
            return f"{op}:{fiscal}:{external_request_id}"
        if idempotency_hint:
            return f"{op}:{fiscal}:{idempotency_hint}"
        return f"{op}:{fiscal}:{payload_sha256}"


__all__ = ["AdapterContext", "AdapterMappingError", "CanonicalEnvelopeBuilder"]
