"""
DPS Fiscal Server transport — direct submission to prro.tax.gov.ua via gRPC.

Official contract (from cabinet.tax.gov.ua/help/api.html):
  Service:  ChkIncomeService
  Method:   sendChkV2(Check) returns (CheckResponse)
  Host:     prro.tax.gov.ua:443 (production)
            cabinet.tax.gov.ua:9443 (test)

  Check message:
    rro_fn:       string  — fiscal number (FN)
    date_time:    int64   — unix epoch seconds
    check_sign:   bytes   — signed XML document
    local_number: int32   — local document number (lnd)
    check_type:   enum    — CHK=1, ZREPORT=2, SERVICECHK=3
    id_offline:   string  — offline fiscal number (empty for online)
    id_cancel:    string  — cancel reference (empty normally)

  CheckResponse message:
    id:            string — server-assigned fiscal ID
    status:        enum   — OK=1, various ERROR_* < 0
    id_sign:       bytes  — server signature of ID
    data_sign:     bytes  — server signature of data
    error_message: string — human-readable error text

  Response model: IMMEDIATE (no async polling).

  Operation mapping:
    SHIFT_OPEN → check_type=SERVICECHK(3), XML T="108"
    SELL       → check_type=CHK(1), XML T="0"
    RETURN     → check_type=CHK(1), XML T="1"
    Z_REPORT   → check_type=ZREPORT(2)

Status mapping:
    OK(1)           → DocumentState.ACK
    ERROR_* (< 0)   → TransportRejectedError (non-retryable DPS rejection)

Recovery:
    lastChk(CheckRequest) → returns last check state for the fiscal number
"""
from __future__ import annotations

import logging
from datetime import UTC, datetime

import re

from ..enums import DocumentState, OperationType
from ..ports import DpsMacRecoveryError, SendResult, TransportRateLimitedError, TransportRejectedError, TransportRetryableError

logger = logging.getLogger('prro_gateway.transports.dps_fiscal_server')


def _kyiv_local_epoch(utc_dt) -> int:
    """Convert UTC datetime to Kyiv-local-as-epoch for DPS date_time field.

    DPS expects date_time to match the XML <TS> which uses Kyiv local time.
    We interpret the local time digits as if they were UTC to produce the epoch value.
    """
    try:
        from zoneinfo import ZoneInfo
        kyiv = ZoneInfo('Europe/Kyiv')
        local = utc_dt.astimezone(kyiv)
        from datetime import timezone
        fake = datetime(local.year, local.month, local.day,
                        local.hour, local.minute, local.second, tzinfo=timezone.utc)
        return int(fake.timestamp())
    except Exception:
        return int(utc_dt.timestamp())


# Official proto check_type enum values
CHECK_TYPE_CHK = 1          # receipt (SELL, RETURN, SERVICE_IN/OUT, CASH_WITHDRAWAL)
CHECK_TYPE_ZREPORT = 2      # Z-report (close-of-day)
CHECK_TYPE_SERVICECHK = 3   # service check (SHIFT_OPEN, mode transitions)

# Supported operations in this first slice
_SUPPORTED_OPS = {
    OperationType.SHIFT_OPEN,
    OperationType.SELL,
    OperationType.RETURN,
    OperationType.SERVICE_IN,
    OperationType.SERVICE_OUT,
    OperationType.CASH_WITHDRAWAL,
    OperationType.Z_REPORT,
}

# Map operation to proto check_type
_OP_TO_CHECK_TYPE: dict[OperationType, int] = {
    OperationType.SHIFT_OPEN: CHECK_TYPE_SERVICECHK,
    OperationType.SELL: CHECK_TYPE_CHK,
    OperationType.RETURN: CHECK_TYPE_CHK,
    OperationType.Z_REPORT: CHECK_TYPE_ZREPORT,
    OperationType.SERVICE_IN: CHECK_TYPE_CHK,
    OperationType.SERVICE_OUT: CHECK_TYPE_CHK,
    OperationType.CASH_WITHDRAWAL: CHECK_TYPE_CHK,
}


def _create_grpc_channel(endpoint: str, *, tls_root_certs: bytes | None = None):
    """Create a secure gRPC channel for the given endpoint.

    Parses endpoint from transport profile format (https://host:port) to host:port.
    Uses system trust store by default; optional PEM root certs for custom CA.

    Channel options tuned for DPS: empirically the server holds idle TLS sockets
    for 60–120 s and emits HTTP/2 PING frames ~every 30 s (verified via
    scripts/probe_dps_keepalive.py). Our client-side PING every 30 s keeps
    firewall/NAT state alive and surfaces silent network breakage within 10 s
    instead of hanging indefinitely.
    """
    import grpc
    # Normalize endpoint: strip protocol prefix
    host_port = endpoint
    for prefix in ('https://', 'http://'):
        if host_port.startswith(prefix):
            host_port = host_port[len(prefix):]
            break
    credentials = grpc.ssl_channel_credentials(root_certificates=tls_root_certs)
    options = [
        ('grpc.keepalive_time_ms', 30_000),
        ('grpc.keepalive_timeout_ms', 10_000),
        ('grpc.keepalive_permit_without_calls', 1),
        ('grpc.http2.max_pings_without_data', 0),
    ]
    return grpc.secure_channel(host_port, credentials, options=options)


class DpsFiscalServerTransport:
    """Direct DPS fiscal server transport via gRPC to prro.tax.gov.ua.

    Uses the official sendChkV2 method with generated proto stubs.
    Channel is created lazily from transport_profile endpoint on first use.
    tls_policy is currently DEFAULT-only (system trust store + optional PEM override).
    For tests: pass grpc_stub to skip real channel creation.
    """

    def __init__(self, *, grpc_channel=None, grpc_stub=None,
                 endpoint: str = 'prro.tax.gov.ua:443',
                 tls_root_certs: bytes | None = None) -> None:
        self._default_endpoint = endpoint
        self._channel = grpc_channel
        self._tls_root_certs = tls_root_certs
        self._stub = grpc_stub  # injectable for tests
        self._stubs: dict[str, object] = {}  # per-endpoint stub cache

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
        transport_profile=None,
        **kwargs,
    ) -> SendResult:
        op = OperationType(operation_type) if operation_type else None
        if op not in _SUPPORTED_OPS:
            raise TransportRejectedError(
                f'DPS fiscal server does not support operation {operation_type} in this slice. '
                f'Supported: {", ".join(o.value for o in _SUPPORTED_OPS)}'
            )

        if not signed_payload:
            raise TransportRejectedError(
                f'DPS fiscal server requires signed_payload (check_sign). '
                f'Document {document_id} has no signed artifact.'
            )

        check_type = _OP_TO_CHECK_TYPE.get(op, CHECK_TYPE_CHK)

        # SHIFT_OPEN always uses local_number=0 per DPS contract
        local_number = 0 if op == OperationType.SHIFT_OPEN else kwargs.get('lnd', 0)

        # RETURN linkage: id_cancel = server fiscal ID of original receipt
        id_cancel = kwargs.get('related_receipt_id', '') or ''

        # Offline: id_offline = pre-allocated offline fiscal number
        id_offline = str(kwargs.get('offline_fiscal_no', '') or '')

        now = datetime.now(UTC)
        # Use business_ts for date_time to match XML <TS>, fall back to now
        business_ts = kwargs.get('business_ts', now)

        # --- gRPC call via proto stub ---
        # Resolve endpoint from transport_profile if available, else constructor default
        effective_endpoint = self._default_endpoint
        tls_root_certs = self._tls_root_certs
        if transport_profile is not None:
            effective_endpoint = getattr(transport_profile, 'endpoint', None) or effective_endpoint
            import json as _json
            try:
                profile_config = _json.loads(getattr(transport_profile, 'config_json', '{}') or '{}')
            except (ValueError, TypeError):
                profile_config = {}
            pem_path = profile_config.get('tls_root_certs_path')
            if pem_path and tls_root_certs is None:
                try:
                    with open(pem_path, 'rb') as f:
                        tls_root_certs = f.read()
                except OSError:
                    pass
        stub = self._get_stub(endpoint=effective_endpoint, tls_root_certs=tls_root_certs)
        # check_sign: use bytes directly if provider returned bytes (CMS DER from sign_raw),
        # or encode str to utf-8 (passthrough/dev). No heuristic base64 guessing.
        check_sign_bytes = signed_payload if isinstance(signed_payload, bytes) else signed_payload.encode('utf-8')

        try:
            from .proto import fiscal_server_pb2 as pb
            request = pb.Check(
                rro_fn=fiscal_number,
                date_time=_kyiv_local_epoch(business_ts),
                check_sign=check_sign_bytes,
                local_number=local_number,
                check_type=check_type,
                id_offline=id_offline,
                id_cancel=id_cancel,
            )
            response = stub.sendChkV2(request, timeout=30.0)
        except TransportRejectedError:
            raise
        except TransportRetryableError:
            raise
        except Exception as exc:
            raise TransportRetryableError(f'DPS fiscal server call failed: {exc}') from exc

        # Map proto response
        status = response.status
        server_id = response.id
        error_message = response.error_message

        if status == 1:  # OK
            return SendResult(
                state=DocumentState.ACK.value,
                transport_request_id=server_id,
                submission_status='DPS_ACK',
                server_fiscal_no=server_id,
                server_fiscal_date=now.isoformat(),
                response_json=str(response),
                sent_at=now,
                ack_at=now,
            )
        _raise_classified_dps_error(status, error_message)

    def poll_status(
        self,
        *,
        document_id: str,
        fiscal_number: str,
        backend_profile_id: str,
        transport_profile_id: str,
        transport_request_id: str | None,
        operation_type: str | None = None,
        transport_profile=None,
        **kwargs,
    ):
        """Recovery via lastChk: returns the last check for this fiscal number.

        Match rule: if response.id == transport_request_id, the local doc was received by DPS.
        If no match or no transport_request_id, stays retryable.
        Requires crypto_provider with sign_raw() for rro_fn_sign.
        """
        from ..ports import PollResult
        if not transport_request_id:
            return PollResult(
                state=DocumentState.ERROR_RETRYABLE.value,
                submission_status='MISSING_TRANSPORT_REQUEST_ID',
                retryable=True,
            )
        crypto = kwargs.get('crypto_provider')
        if crypto is None or not hasattr(crypto, 'sign_raw'):
            return PollResult(
                state=DocumentState.ERROR_RETRYABLE.value,
                submission_status='DPS_RECOVERY_NO_CRYPTO_FOR_SIGN_RAW',
                retryable=True,
            )
        try:
            from .proto import fiscal_server_pb2 as pb
            rro_fn_sign = crypto.sign_raw(data=fiscal_number.encode('utf-8'))
            request = pb.CheckRequest(rro_fn_sign=rro_fn_sign)

            effective_endpoint = self._default_endpoint
            tls_root_certs = self._tls_root_certs
            if transport_profile is not None:
                effective_endpoint = getattr(transport_profile, 'endpoint', None) or effective_endpoint
                import json as _json
                try:
                    profile_config = _json.loads(getattr(transport_profile, 'config_json', '{}') or '{}')
                except (ValueError, TypeError):
                    profile_config = {}
                pem_path = profile_config.get('tls_root_certs_path')
                if pem_path and tls_root_certs is None:
                    try:
                        with open(pem_path, 'rb') as f:
                            tls_root_certs = f.read()
                    except OSError:
                        pass
            stub = self._get_stub(endpoint=effective_endpoint, tls_root_certs=tls_root_certs)
            response = stub.lastChk(request, timeout=15.0)
        except Exception as exc:
            logger.warning('dps_lastchk_failed', extra={'extra_fields': {'error': str(exc), 'fiscal_number': fiscal_number}})
            return PollResult(
                state=DocumentState.ERROR_RETRYABLE.value,
                submission_status=f'DPS_LASTCHK_ERROR: {exc}',
                retryable=True,
            )

        if response.status != 1:  # not OK
            return PollResult(
                state=DocumentState.ERROR_RETRYABLE.value,
                submission_status=f'DPS_LASTCHK_STATUS_{response.status}',
                retryable=True,
            )

        # Match: response.id must equal our transport_request_id
        if response.id == transport_request_id:
            now = datetime.now(UTC)
            return PollResult(
                state=DocumentState.ACK.value,
                submission_status='DPS_ACK',
                server_fiscal_no=response.id,
                server_fiscal_date=now.isoformat(),
                response_json=f'{{"id":"{response.id}","status":{response.status}}}',
                ack_at=now,
            )
        else:
            # Last check on DPS is a different document — ours may not have been received
            return PollResult(
                state=DocumentState.ERROR_RETRYABLE.value,
                submission_status='DPS_LASTCHK_MISMATCH',
                retryable=True,
            )

    def _get_stub(self, *, endpoint: str | None = None, tls_root_certs: bytes | None = None):
        """Lazy-initialize gRPC stub. Injected stub always wins. Otherwise per-endpoint cache."""
        if self._stub is not None:
            return self._stub
        ep = endpoint or self._default_endpoint
        if ep in self._stubs:
            return self._stubs[ep]
        channel = self._channel or _create_grpc_channel(ep, tls_root_certs=tls_root_certs)
        from .proto import fiscal_server_pb2_grpc
        stub = fiscal_server_pb2_grpc.ChkIncomeServiceStub(channel)
        self._stubs[ep] = stub
        return stub


    def ping(self, *, fiscal_number: str, transport_profile=None) -> bool:
        """Ping DPS fiscal server (T=111). Returns True if reachable, False on any error."""
        try:
            from .proto import fiscal_server_pb2 as pb
            effective_endpoint = self._default_endpoint
            tls_root_certs = self._tls_root_certs
            if transport_profile is not None:
                effective_endpoint = getattr(transport_profile, 'endpoint', None) or effective_endpoint
            stub = self._get_stub(endpoint=effective_endpoint, tls_root_certs=tls_root_certs)
            request = pb.Check(
                rro_fn=fiscal_number,
                date_time=_kyiv_local_epoch(datetime.now(UTC)),
                check_sign=b'',
                local_number=0,
                check_type=CHECK_TYPE_SERVICECHK,
            )
            response = stub.ping(request)
            return response.status == 1
        except Exception as exc:
            logger.debug('dps_ping_failed', extra={'extra_fields': {'fiscal_number': fiscal_number, 'error': str(exc)}})
            return False

    def request_offline_codes(
        self,
        *,
        fiscal_number: str,
        qty: int,
        signed_payload: bytes | str,
        transport_profile=None,
    ) -> list[int]:
        """Request offline fiscal number codes from DPS via T=112.

        signed_payload: CMS-signed T=112 XML (same convention as send()).
        Returns sorted list of allocated fiscal number integers.
        Raises TransportRetryableError on transport or server error.
        """
        try:
            from .proto import fiscal_server_pb2 as pb
            effective_endpoint = self._default_endpoint
            tls_root_certs = self._tls_root_certs
            if transport_profile is not None:
                effective_endpoint = getattr(transport_profile, 'endpoint', None) or effective_endpoint
            stub = self._get_stub(endpoint=effective_endpoint, tls_root_certs=tls_root_certs)
            check_sign_bytes = signed_payload if isinstance(signed_payload, bytes) else signed_payload.encode('utf-8')
            request = pb.Check(
                rro_fn=fiscal_number,
                date_time=_kyiv_local_epoch(datetime.now(UTC)),
                check_sign=check_sign_bytes,
                local_number=0,
                check_type=CHECK_TYPE_SERVICECHK,
            )
            response = stub.sendChkV2(request)
        except TransportRetryableError:
            raise
        except Exception as exc:
            raise TransportRetryableError(f'DPS offline codes request failed: {exc}') from exc

        if response.status != 1:
            raise TransportRetryableError(
                f'DPS offline codes rejected: status={response.status}, error={response.error_message}'
            )
        return _parse_fns_data(response.fns_data)

    def probe_status(self, *, fiscal_number: str, crypto_provider, endpoint: str | None = None,
                      tls_root_certs: bytes | None = None) -> dict:
        """Call statusRro — explicit ops probe, NOT automatic polling."""
        from .proto import fiscal_server_pb2 as pb
        rro_fn_sign = crypto_provider.sign_raw(data=fiscal_number.encode('utf-8'))
        stub = self._get_stub(endpoint=endpoint or self._default_endpoint, tls_root_certs=tls_root_certs)
        response = stub.statusRro(pb.CheckRequest(rro_fn_sign=rro_fn_sign))
        return {
            'open_shift': response.open_shift,
            'online': response.online,
            'last_signer': response.last_signer,
            'status': response.status,
            'error_message': response.error_message,
        }

    def probe_info(self, *, fiscal_number: str, crypto_provider, endpoint: str | None = None,
                    tls_root_certs: bytes | None = None) -> dict:
        """Call infoRro — explicit ops probe, NOT automatic polling."""
        from .proto import fiscal_server_pb2 as pb
        rro_fn_sign = crypto_provider.sign_raw(data=fiscal_number.encode('utf-8'))
        stub = self._get_stub(endpoint=endpoint or self._default_endpoint, tls_root_certs=tls_root_certs)
        response = stub.infoRro(pb.CheckRequest(rro_fn_sign=rro_fn_sign))
        return {
            'status': response.status,
            'status_rro': response.status_rro,
            'open_shift': response.open_shift,
            'online': response.online,
            'last_signer': response.last_signer,
            'name': response.name,
            'name_to': response.name_to,
            'addr': response.addr,
            'single_tax': response.single_tax,
            'offline_allowed': response.offline_allowed,
            'lnum': response.lnum,
            'pn': response.pn,
            'tins': response.tins,
            'error_message': response.error_message,
        }


# --- Offline codes response parsing -------------------------------------------

def _parse_fns_data(fns_data: str) -> list[int]:
    """Parse fns_data field from DPS T=112 CheckResponse into sorted list of integers.

    Attempts XML <ID> element parsing first, then whitespace/comma split.
    """
    if not fns_data:
        return []
    # Try XML: <ID>12345</ID> elements
    try:
        import xml.etree.ElementTree as ET
        # fns_data may be a fragment without a root element — wrap it
        root = ET.fromstring(fns_data if fns_data.strip().startswith('<') else f'<R>{fns_data}</R>')
        ids = [el.text.strip() for el in root.iter('ID') if el.text and el.text.strip()]
        if ids:
            return sorted(int(i) for i in ids if i.isdigit())
    except Exception:
        pass
    # Fallback: comma or whitespace separated numbers
    parts = fns_data.replace(',', ' ').split()
    return sorted(int(p) for p in parts if p.strip().isdigit())


# --- DPS error classification (evidence-based, live-proven codes only) --------

_BAD_HASH_RE = re.compile(r'ERROR_BAD_HASH_PREV\s+store\s+([0-9a-fA-F]{64})\b')


def _extract_expected_mac(error_message: str) -> str | None:
    """Extract the expected previous hash from DPS ERROR_BAD_HASH_PREV response.

    Format observed live: 'ERROR_BAD_HASH_PREV  store {64hex} chk {64hex}'
    Returns the 'store' hash (what DPS expects) or None if not parseable.
    """
    m = _BAD_HASH_RE.search(error_message or '')
    return m.group(1) if m else None


def _raise_classified_dps_error(status: int, error_message: str) -> None:
    """Raise the appropriate exception based on DPS response status code.

    Classification aligned with official proto (fiscal_server.proto) and API docs (262576):
      -1  : ERROR_VEREFY              → rejected (signature/crypto)
      -2  : ERROR_CHECK               → rejected (RRO verification)
      -3  : ERROR_SAVE                → rejected (write error / duplicate)
      -4  : ERROR_UNKNOWN             → rejected (general server error)
      -5  : ERROR_TYPE                → rejected (wrong check_type)
      -6  : ERROR_NOT_PREV_ZREPORT    → rejected (missing previous Z-report)
      -7  : ERROR_XML                 → rejected (invalid XML format)
      -8  : ERROR_XML_DATE            → rejected (date mismatch)
      -9  : ERROR_XML_CHK             → rejected (invalid check format)
      -10 : ERROR_XML_ZREPORT         → rejected (invalid Z-report format)
      -11 : ERROR_OFFLINE_168         → rejected (168h offline limit exceeded)
      -12 : ERROR_BAD_HASH_PREV       → MAC recovery if hash extractable, else rejected
      -13 : ERROR_NOT_REGISTERED_RRO  → rejected (PRRO not registered)
      -14 : ERROR_NOT_REGISTERED_SIGNER → rejected (signer not registered)
      -15 : ERROR_NOT_OPEN_SHIFT      → rejected (shift not open)
      -16 : ERROR_OFFLINE_ID          → rejected (invalid offline ID)

    Only network/gRPC/transport failures (not CheckResponse.status) → TransportRetryableError.
    TransportRateLimitedError is NOT derived from CheckResponse.status.
    """
    msg = f'DPS fiscal server rejected: status={status}, error={error_message}'

    # MAC recovery: only for ERROR_BAD_HASH_PREV with extractable expected hash
    if status == -12:
        expected = _extract_expected_mac(error_message)
        if expected:
            raise DpsMacRecoveryError(msg, expected_mac=expected, dps_status=status)

    # All negative proto statuses — DPS business/protocol rejection (terminal)
    raise TransportRejectedError(msg)


__all__ = ['DpsFiscalServerTransport']
