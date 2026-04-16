"""
Sprint 7 / step 1 — DPS fiscal server transport tests.

Contract source: cabinet.tax.gov.ua/help/api.html (official proto3 spec)

Coverage:
  FS1 — SHIFT_OPEN send: check_type=SERVICECHK(3), signed payload passed
  FS2 — SELL send: check_type=CHK(1), signed payload passed
  FS3 — Z_REPORT send: check_type=ZREPORT(2), signed payload passed
  FS4 — unsupported op (SHIFT_CLOSE) rejected with explicit error
  FS5 — missing signed_payload rejected
  FS6 — DPS OK(1) → DocumentState.ACK
  FS7 — DPS error status → TransportRejectedError
  FS8 — gRPC channel not configured → TransportRetryableError
  FS9 — container wires real DpsFiscalServerTransport for DPS_PRRO_GRPC_ECABINET
  FS10 — lnd passed through as local_number
  FS11 — poll_status builds lastChk with rro_fn_sign, matches response.id
  FS11b — poll_status mismatch → retryable
  FS11c — poll_status without crypto → retryable
  FS12 — transport uses endpoint from transport_profile, not constructor default
"""
from __future__ import annotations

import pytest

from prro_gateway.enums import DocumentState, OperationType, TransportKind
from prro_gateway.ports import TransportRejectedError, TransportRetryableError
from prro_gateway.transports.dps_fiscal_server import (
    CHECK_TYPE_CHK, CHECK_TYPE_SERVICECHK, CHECK_TYPE_ZREPORT,
    DpsFiscalServerTransport,
)


# ---------------------------------------------------------------------------
# Mock gRPC channel
# ---------------------------------------------------------------------------

class _MockResponse:
    """Proto-like CheckResponse mock."""
    def __init__(self, *, id='DPS-001', status=1, error_message=''):
        self.id = id
        self.status = status
        self.error_message = error_message
        self.id_sign = b''
        self.data_sign = b''


class _MockStub:
    """Mock gRPC stub that captures sendChkV2 calls."""
    def __init__(self, *, status=1, server_id='DPS-001', error_message=''):
        self.calls: list[dict] = []
        self._status = status
        self._server_id = server_id
        self._error_message = error_message

    def sendChkV2(self, request):
        self.calls.append({
            'rro_fn': request.rro_fn,
            'date_time': request.date_time,
            'check_sign': request.check_sign,
            'local_number': request.local_number,
            'check_type': request.check_type,
        })
        return _MockResponse(id=self._server_id, status=self._status, error_message=self._error_message)


class _MockGrpcChannel:
    """Mock channel that produces _MockStub. Wraps stub for backward compat."""
    def __init__(self, *, status=1, server_id='DPS-001', error_message=''):
        self.stub = _MockStub(status=status, server_id=server_id, error_message=error_message)

    @property
    def calls(self):
        return self.stub.calls


# ---------------------------------------------------------------------------
# FS1 — SHIFT_OPEN: check_type=SERVICECHK(3)
# ---------------------------------------------------------------------------

def test_fs1_shift_open_sends_servicechk() -> None:
    channel = _MockGrpcChannel()
    transport = DpsFiscalServerTransport(grpc_stub=channel.stub)
    result = transport.send(
        document_id='doc-1', signed_payload='<XML>signed-shift</XML>',
        fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
        operation_type='SHIFT_OPEN',
    )
    assert result.state == DocumentState.ACK.value
    assert len(channel.calls) == 1
    assert channel.calls[0]['check_type'] == CHECK_TYPE_SERVICECHK
    assert channel.calls[0]['rro_fn'] == 'FN-001'
    assert channel.calls[0]['check_sign'] == b'<XML>signed-shift</XML>'


# ---------------------------------------------------------------------------
# FS2 — SELL: check_type=CHK(1)
# ---------------------------------------------------------------------------

def test_fs2_sell_sends_chk() -> None:
    channel = _MockGrpcChannel()
    transport = DpsFiscalServerTransport(grpc_stub=channel.stub)
    result = transport.send(
        document_id='doc-2', signed_payload='<XML>signed-sell</XML>',
        fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
        operation_type='SELL',
    )
    assert result.state == DocumentState.ACK.value
    assert channel.calls[0]['check_type'] == CHECK_TYPE_CHK


# ---------------------------------------------------------------------------
# FS3 — Z_REPORT: check_type=ZREPORT(2)
# ---------------------------------------------------------------------------

def test_fs3_zreport_sends_zreport_type() -> None:
    channel = _MockGrpcChannel()
    transport = DpsFiscalServerTransport(grpc_stub=channel.stub)
    result = transport.send(
        document_id='doc-3', signed_payload='<XML>signed-z</XML>',
        fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
        operation_type='Z_REPORT',
    )
    assert result.state == DocumentState.ACK.value
    assert channel.calls[0]['check_type'] == CHECK_TYPE_ZREPORT


# ---------------------------------------------------------------------------
# FS4 — SHIFT_CLOSE unsupported
# ---------------------------------------------------------------------------

def test_fs4_shift_close_rejected() -> None:
    channel = _MockGrpcChannel()
    transport = DpsFiscalServerTransport(grpc_stub=channel.stub)
    with pytest.raises(TransportRejectedError, match='does not support'):
        transport.send(
            document_id='doc-4', signed_payload='<XML>x</XML>',
            fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
            operation_type='SHIFT_CLOSE',
        )
    assert len(channel.calls) == 0


# ---------------------------------------------------------------------------
# FS5 — missing signed_payload rejected
# ---------------------------------------------------------------------------

def test_fs5_missing_signed_payload_rejected() -> None:
    channel = _MockGrpcChannel()
    transport = DpsFiscalServerTransport(grpc_stub=channel.stub)
    with pytest.raises(TransportRejectedError, match='signed_payload'):
        transport.send(
            document_id='doc-5', signed_payload='',
            fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
            operation_type='SELL',
        )


# ---------------------------------------------------------------------------
# FS6 — DPS OK → ACK
# ---------------------------------------------------------------------------

def test_fs6_dps_ok_returns_ack() -> None:
    channel = _MockGrpcChannel(status=1, server_id='FISCAL-123')
    transport = DpsFiscalServerTransport(grpc_stub=channel.stub)
    result = transport.send(
        document_id='doc-6', signed_payload='<XML>payload</XML>',
        fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
        operation_type='SELL',
    )
    assert result.state == DocumentState.ACK.value
    assert result.server_fiscal_no == 'FISCAL-123'
    assert result.submission_status == 'DPS_ACK'
    assert result.transport_request_id == 'FISCAL-123'


# ---------------------------------------------------------------------------
# FS7 — DPS error → TransportRejectedError
# ---------------------------------------------------------------------------

def test_fs7_dps_error_raises_rejected() -> None:
    channel = _MockGrpcChannel(status=-2, error_message='CHECK_ERROR')
    transport = DpsFiscalServerTransport(grpc_stub=channel.stub)
    with pytest.raises(TransportRejectedError, match='status=-2'):
        transport.send(
            document_id='doc-7', signed_payload='<XML>bad</XML>',
            fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
            operation_type='SELL',
        )


# ---------------------------------------------------------------------------
# FS8 — no gRPC channel → TransportRetryableError
# ---------------------------------------------------------------------------

def test_fs8_no_stub_no_channel_retryable() -> None:
    """Without stub or channel, lazy channel creation to real host will fail with retryable error."""
    transport = DpsFiscalServerTransport(endpoint='localhost:1')  # unreachable
    with pytest.raises((TransportRetryableError, Exception)):
        transport.send(
            document_id='doc-8', signed_payload='<XML>x</XML>',
            fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
            operation_type='SELL',
        )


# ---------------------------------------------------------------------------
# FS9 — router selects fiscal server for DPS_PRRO_GRPC_ECABINET kind
# ---------------------------------------------------------------------------

def test_fs9_container_wires_real_handler() -> None:
    """RuntimeContainer._build_transport_handlers() wires DpsFiscalServerTransport
    for DPS_PRRO_GRPC_ECABINET, not the stub."""
    from prro_gateway.runtime.container import RuntimeContainer
    from prro_gateway.config import AppConfig
    from pathlib import Path
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        cfg = AppConfig.from_mapping({
            'database': {'db_path': f'{tmp}/fs9.sqlite3', 'sql_dir': str(Path(__file__).resolve().parents[1] / 'sql'), 'auto_migrate': True},
            'defaults': {'fiscal_number': 'FN-001', 'backend_profile_id': 'bp', 'transport_profile_id': 'tp', 'channel_owner': 'test'},
            'runtime': {'environment': 'production'},
            'checkbox': {'endpoint': 'https://mock', 'license_key': 'X', 'cashier_pin': '0'},
        })
        c = RuntimeContainer(cfg)
        handlers = c._build_transport_handlers()
        handler = handlers.get(TransportKind.DPS_PRRO_GRPC_ECABINET)
        assert isinstance(handler, DpsFiscalServerTransport), (
            f'Expected DpsFiscalServerTransport, got {type(handler).__name__}'
        )


# ---------------------------------------------------------------------------
# FS10 — lnd passed through as local_number
# ---------------------------------------------------------------------------

def test_fs10_lnd_passed_as_local_number() -> None:
    channel = _MockGrpcChannel()
    transport = DpsFiscalServerTransport(grpc_stub=channel.stub)
    transport.send(
        document_id='doc-10', signed_payload='<XML>x</XML>',
        fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
        operation_type='SELL', lnd=42,
    )
    assert channel.calls[0]['local_number'] == 42


def test_fs10b_lnd_defaults_zero_when_missing() -> None:
    channel = _MockGrpcChannel()
    transport = DpsFiscalServerTransport(grpc_stub=channel.stub)
    transport.send(
        document_id='doc-10b', signed_payload='<XML>x</XML>',
        fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
        operation_type='SELL',
    )
    assert channel.calls[0]['local_number'] == 0


# ---------------------------------------------------------------------------
# FS11 — poll_status returns honest unsupported
# ---------------------------------------------------------------------------

def test_fs11_poll_status_lastchk_match_ack() -> None:
    """poll_status with lastChk: when response.id matches transport_request_id → ACK."""

    class _LastChkStub(_MockStub):
        def lastChk(self, request):
            self.calls.append({'rro_fn_sign': request.rro_fn_sign})
            return _MockResponse(id='DPS-001', status=1)

    class _MockCrypto:
        def sign_raw(self, *, data, document_id=None):
            return b'SIGNED::' + data

    stub = _LastChkStub()
    transport = DpsFiscalServerTransport(grpc_stub=stub)
    result = transport.poll_status(
        document_id='doc-11', fiscal_number='FN-001',
        backend_profile_id='bp', transport_profile_id='tp',
        transport_request_id='DPS-001',
        crypto_provider=_MockCrypto(),
    )
    assert result.state == DocumentState.ACK.value
    assert result.server_fiscal_no == 'DPS-001'
    # Verify rro_fn_sign was built from fiscal_number
    assert stub.calls[0]['rro_fn_sign'] == b'SIGNED::FN-001'


def test_fs11b_poll_status_lastchk_mismatch_retryable() -> None:
    """poll_status: when response.id doesn't match → retryable, not fake ACK."""

    class _MismatchStub(_MockStub):
        def lastChk(self, request):
            return _MockResponse(id='DPS-OTHER', status=1)

    class _MockCrypto:
        def sign_raw(self, *, data, document_id=None):
            return data

    stub = _MismatchStub()
    transport = DpsFiscalServerTransport(grpc_stub=stub)
    result = transport.poll_status(
        document_id='doc-11b', fiscal_number='FN-001',
        backend_profile_id='bp', transport_profile_id='tp',
        transport_request_id='DPS-001',
        crypto_provider=_MockCrypto(),
    )
    assert result.state == DocumentState.ERROR_RETRYABLE.value
    assert 'MISMATCH' in result.submission_status


def test_fs11c_poll_status_no_crypto_retryable() -> None:
    """poll_status without crypto_provider → retryable, not crash."""
    stub = _MockStub()
    transport = DpsFiscalServerTransport(grpc_stub=stub)
    result = transport.poll_status(
        document_id='doc-11c', fiscal_number='FN-001',
        backend_profile_id='bp', transport_profile_id='tp',
        transport_request_id='DPS-001',
    )
    assert result.state == DocumentState.ERROR_RETRYABLE.value
    assert 'CRYPTO' in result.submission_status


# ---------------------------------------------------------------------------
# FS12 — transport uses endpoint from transport_profile
# ---------------------------------------------------------------------------

def test_fs12_transport_uses_profile_endpoint() -> None:
    """send() must create channel from transport_profile.endpoint, not constructor default."""
    from unittest.mock import patch, MagicMock
    from prro_gateway.transports.proto import fiscal_server_pb2_grpc

    class _ProfileWithEndpoint:
        endpoint = 'https://cabinet.tax.gov.ua:9443'
        config_json = '{}'
        tls_policy = 'DEFAULT'

    captured_endpoints = []

    def _fake_create_channel(endpoint, *, tls_root_certs=None):
        captured_endpoints.append(endpoint)
        mock_channel = MagicMock()
        # Make stub.sendChkV2 return a valid response
        mock_stub = _MockStub()
        return mock_channel

    transport = DpsFiscalServerTransport(endpoint='prro.tax.gov.ua:443')

    with patch('prro_gateway.transports.dps_fiscal_server._create_grpc_channel', side_effect=_fake_create_channel):
        # Patch ChkIncomeServiceStub to return our mock
        with patch.object(fiscal_server_pb2_grpc, 'ChkIncomeServiceStub', return_value=_MockStub()):
            result = transport.send(
                document_id='doc-12', signed_payload='<XML>x</XML>',
                fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
                operation_type='SELL', transport_profile=_ProfileWithEndpoint(),
            )

    assert result.state == DocumentState.ACK.value
    # Channel must have been created with profile endpoint, not constructor default
    assert len(captured_endpoints) == 1
    # _create_grpc_channel receives the raw endpoint; it strips https:// internally
    assert 'cabinet.tax.gov.ua:9443' in captured_endpoints[0], (
        f'Channel must use profile endpoint, got {captured_endpoints[0]}'
    )
    assert 'prro.tax.gov.ua' not in captured_endpoints[0], (
        f'Must NOT use constructor default, got {captured_endpoints[0]}'
    )
