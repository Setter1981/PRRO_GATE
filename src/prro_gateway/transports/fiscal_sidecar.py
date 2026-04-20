"""
Fiscal Sidecar V2 transport — posts canonical JSON to Rust prro_sidecar.

The Rust sidecar owns: XML build → CMS sign (DSTU 4145) → gRPC sendChkV2.
This module is the thin HTTP shim between the Python write_path and the binary.

Prerequisite: crypto.provider must be 'passthrough'. The write_path emits the
canonical JSON as signed_payload; the sidecar handles signing internally from
its own JKS store. Combining with the Python crypto sidecar would double-sign.

POST /fiscal/send
  Body:   CanonicalCommand JSON (the canonical payload passed through verbatim)
  200 {status: int, fiscal_id: str, error_message?: str}
    status=1           → DocumentState.ACK
    status<0           → TransportRejectedError (DPS rejection)
  4xx                  → TransportRejectedError (bad request / license / not found)
  5xx                  → TransportRetryableError (transient upstream failure)
  network error        → TransportRetryableError
"""
from __future__ import annotations

import json
import logging
from typing import TYPE_CHECKING

import httpx

from ..enums import DocumentState
from ..ports import SendResult, TransportRejectedError, TransportRetryableError

if TYPE_CHECKING:
    pass

logger = logging.getLogger('prro_gateway.transports.fiscal_sidecar_v2')

# HTTP status codes that indicate a transient upstream problem — safe to retry.
_RETRYABLE_HTTP = {429, 500, 502, 503, 504}


class FiscalSidecarTransport:
    """Thin httpx client that posts canonical JSON to the Rust prro_sidecar."""

    def __init__(self, *, sidecar_url: str, http_client: httpx.Client | None = None) -> None:
        self._base = sidecar_url.rstrip('/')
        self._client = http_client or httpx.Client(timeout=120.0)

    def send(
        self,
        *,
        document_id: str,
        signed_payload,
        fiscal_number: str,
        backend_profile_id: str,
        transport_profile_id: str,
        operation_type: str | None = None,
        request_payload: dict | None = None,
        request_payload_json: str | None = None,
        external_request_id: str | None = None,
        transport_profile=None,
        **kwargs,
    ) -> SendResult:
        # Resolve sidecar URL: transport_profile overrides constructor default.
        base = self._base
        if transport_profile is not None:
            override = getattr(transport_profile, 'endpoint', None)
            if override:
                base = override.rstrip('/')

        # signed_payload = canonical JSON (passthrough provider)
        if isinstance(signed_payload, (bytes, bytearray)):
            body = json.loads(signed_payload)
        elif isinstance(signed_payload, str):
            body = json.loads(signed_payload)
        else:
            body = signed_payload

        logger.debug('fiscal_sidecar send document_id=%s fn=%s op=%s',
                     document_id, fiscal_number, operation_type)

        try:
            resp = self._client.post(f'{base}/fiscal/send', json=body)
        except httpx.TransportError as exc:
            raise TransportRetryableError(
                f'prro_sidecar unreachable for document {document_id}: {exc}'
            ) from exc

        if resp.status_code in _RETRYABLE_HTTP:
            raise TransportRetryableError(
                f'prro_sidecar returned {resp.status_code} for document {document_id}: '
                f'{resp.text[:300]}'
            )
        if resp.status_code >= 400:
            raise TransportRejectedError(
                f'prro_sidecar rejected document {document_id} '
                f'(HTTP {resp.status_code}): {resp.text[:300]}'
            )

        data = resp.json()
        return _map_dps_response(data, document_id)


def _map_dps_response(data: dict, document_id: str) -> SendResult:
    status   = data.get('status', 0)
    fiscal_id = data.get('fiscal_id', '')
    error_msg = data.get('error_message')

    if status == 1:
        return SendResult(
            state=DocumentState.ACK,
            transport_request_id=fiscal_id or None,
            submission_status='DPS_ACK',
            response_json=json.dumps(data),
        )

    msg = error_msg or f'DPS status={status}'
    raise TransportRejectedError(
        f'DPS rejected document {document_id}: {msg} (status={status})'
    )
