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
import re as _re

import httpx

from ..enums import DocumentState
from ..ports import SendResult, TransportRejectedError, TransportRetryableError

logger = logging.getLogger('prro_gateway.transports.fiscal_sidecar')

# HTTP status codes that indicate a transient upstream problem — safe to retry.
_RETRYABLE_HTTP = {429, 500, 502, 503, 504}

# DPS status codes that are transient — rate-limit or offline-session expiry.
# -4:  ERROR_TAX (server overloaded / rate-limited)
# -16: ERROR_OFFLINE_ID (DPS online but offline session expired — retry after reconnect)
_RETRYABLE_DPS_STATUSES = {-4, -16}

# sidecar_url must be loopback HTTP or any HTTPS — plain HTTP to non-loopback
# would transmit fiscal payloads unencrypted across the network.
_SAFE_URL_RE = _re.compile(
    r'^https://'
    r'|^http://(127\.\d+\.\d+\.\d+|localhost|\[::1\]|::1)(:\d+)?(/|$)',
    _re.IGNORECASE,
)


def _validate_sidecar_url(url: str) -> None:
    if not _SAFE_URL_RE.match(url):
        raise ValueError(
            f'sidecar_url must be a loopback http:// or any https:// URL, got {url!r}'
        )


class FiscalSidecarTransport:
    """Thin httpx client that posts canonical JSON to the Rust prro_sidecar."""

    def __init__(
        self,
        *,
        sidecar_url: str,
        http_client: httpx.Client | None = None,
        crypto_provider: str = 'passthrough',
    ) -> None:
        if crypto_provider != 'passthrough':
            raise ValueError(
                f'FiscalSidecarTransport requires crypto.provider=passthrough, '
                f'got {crypto_provider!r}. Combining with the Python crypto sidecar '
                f'would double-sign the payload.'
            )
        _validate_sidecar_url(sidecar_url)
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

    # status=1 (CheckResponse.Status.OK) is the only DPS-defined success code.
    # status=0 is UNKNOWN (proto placeholder, never returned by real DPS).
    # status=-1..-16 are named error codes (ERROR_VEREFY … ERROR_OFFLINE_ID).
    # Confirmed against official proto at cabinet.tax.gov.ua/help/api.html — no status>1 exists.
    if status == 1:
        return SendResult(
            state=DocumentState.ACK,
            transport_request_id=fiscal_id or None,
            submission_status='DPS_ACK',
            response_json=json.dumps(data),
        )

    msg = error_msg or f'DPS status={status}'
    if status in _RETRYABLE_DPS_STATUSES:
        raise TransportRetryableError(
            f'DPS transient error for document {document_id}: {msg} (status={status})'
        )
    raise TransportRejectedError(
        f'DPS rejected document {document_id}: {msg} (status={status})'
    )
