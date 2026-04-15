from __future__ import annotations

from datetime import datetime
from typing import Protocol

from pydantic import Field

from .models.common import StrictModel


class CryptoProviderUnavailableError(RuntimeError):
    pass


class TransportRetryableError(RuntimeError):
    pass


class TransportRateLimitedError(TransportRetryableError):
    """DPS server returned rate-limit / exceeded-errors response.

    Carries a recommended cooldown so write_path can persist retry_after_seconds
    without sleeping in the request path.
    """

    def __init__(self, message: str, *, retry_after_seconds: int = 300) -> None:
        super().__init__(message)
        self.retry_after_seconds = retry_after_seconds


class TransportRejectedError(RuntimeError):
    pass


class DpsMacRecoveryError(TransportRejectedError):
    """DPS returned ERROR_BAD_HASH_PREV with the expected previous hash.

    Caller can extract `expected_mac`, persist it, re-build XML with corrected MAC,
    re-sign, and retry once.
    """

    def __init__(self, message: str, *, expected_mac: str, dps_status: int) -> None:
        super().__init__(message)
        self.expected_mac = expected_mac
        self.dps_status = dps_status


class SendResult(StrictModel):
    state: str | None = None
    transport_request_id: str | None = None
    submission_status: str | None = None
    server_fiscal_no: str | None = None
    server_fiscal_date: str | None = None
    response_json: str | None = None
    ack_at: datetime | None = None
    sent_at: datetime | None = None


class PollResult(StrictModel):
    state: str
    submission_status: str | None = None
    server_fiscal_no: str | None = None
    server_fiscal_date: str | None = None
    response_json: str | None = None
    ack_at: datetime | None = None
    kvt1_received_at: datetime | None = None
    kvt2_received_at: datetime | None = None
    retryable: bool = False


class CryptoProvider(Protocol):
    def sign(self, *, document_id: str, payload_json: str) -> str: ...


class TransportClient(Protocol):
    def send(
        self,
        *,
        document_id: str,
        signed_payload: str,
        fiscal_number: str,
        backend_profile_id: str,
        transport_profile_id: str,
        operation_type: str | None = None,
        request_payload: dict | None = None,
        request_payload_json: str | None = None,
        external_request_id: str | None = None,
        transport_profile = None,
    ) -> SendResult: ...


class TransportStatusClient(Protocol):
    def poll_status(
        self,
        *,
        document_id: str,
        fiscal_number: str,
        backend_profile_id: str,
        transport_profile_id: str,
        transport_request_id: str | None,
        operation_type: str | None = None,
        transport_profile = None,
    ) -> PollResult: ...


__all__ = [
    'CryptoProviderUnavailableError', 'TransportRetryableError', 'TransportRejectedError',
    'SendResult', 'PollResult', 'CryptoProvider', 'TransportClient', 'TransportStatusClient'
]
