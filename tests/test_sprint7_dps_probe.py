"""
Sprint 7 / step 4 — DPS fiscal-server ops probe tests.

Coverage:
  DP1 — probe_status builds correct rro_fn_sign request
  DP2 — probe_info returns mapped response fields
  DP3 — REST /v1/admin/dps-probe returns status + info on success
  DP4 — REST /v1/admin/dps-probe returns error on failure
  DP5 — REST probe uses active profile endpoint, not constructor default
"""
from __future__ import annotations

import pytest

from prro_gateway.enums import DocumentState
from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport


# ---------------------------------------------------------------------------
# Mock proto responses
# ---------------------------------------------------------------------------

class _MockStatusResponse:
    open_shift = True
    online = True
    last_signer = 'cashier-01'
    status = 1
    error_message = ''


class _MockInfoResponse:
    status = 1
    status_rro = 0
    open_shift = True
    online = True
    last_signer = 'cashier-01'
    name = 'TestRRO'
    name_to = 'TestOrg'
    addr = 'Test Addr'
    single_tax = False
    offline_allowed = True
    add_num = 0
    pn = '1234567890'
    tins = '9876543210'
    lnum = 42
    name_pay = ''
    error_message = ''


class _MockProbeStub:
    def __init__(self):
        self.status_calls = []
        self.info_calls = []

    def statusRro(self, request):
        self.status_calls.append(request.rro_fn_sign)
        return _MockStatusResponse()

    def infoRro(self, request):
        self.info_calls.append(request.rro_fn_sign)
        return _MockInfoResponse()

    # Keep send/lastChk stubs for transport compat
    def sendChkV2(self, request):
        pass

    def lastChk(self, request):
        pass


class _MockCrypto:
    def sign_raw(self, *, data):
        return b'SIGNED::' + data

    def sign(self, *, document_id, payload_json):
        return payload_json


# ---------------------------------------------------------------------------
# DP1 — probe_status request shape
# ---------------------------------------------------------------------------

def test_dp1_probe_status_request_shape() -> None:
    stub = _MockProbeStub()
    transport = DpsFiscalServerTransport(grpc_stub=stub)
    result = transport.probe_status(fiscal_number='FN-001', crypto_provider=_MockCrypto())

    assert len(stub.status_calls) == 1
    assert stub.status_calls[0] == b'SIGNED::FN-001'
    assert result['open_shift'] is True
    assert result['online'] is True
    assert result['last_signer'] == 'cashier-01'
    assert result['status'] == 1


# ---------------------------------------------------------------------------
# DP2 — probe_info response mapping
# ---------------------------------------------------------------------------

def test_dp2_probe_info_response_mapping() -> None:
    stub = _MockProbeStub()
    transport = DpsFiscalServerTransport(grpc_stub=stub)
    result = transport.probe_info(fiscal_number='FN-001', crypto_provider=_MockCrypto())

    assert result['name'] == 'TestRRO'
    assert result['offline_allowed'] is True
    assert result['lnum'] == 42
    assert result['pn'] == '1234567890'
    assert result['tins'] == '9876543210'


# ---------------------------------------------------------------------------
# DP3 — REST endpoint success
# ---------------------------------------------------------------------------

def test_dp3_rest_probe_success(tmp_path) -> None:
    from pathlib import Path
    import httpx
    from fastapi.testclient import TestClient
    from prro_gateway.config import AppConfig
    from prro_gateway.enums import TransportKind
    from prro_gateway.runtime.container import RuntimeContainer
    from prro_gateway.runtime.rest_app import create_app

    ROOT = Path(__file__).resolve().parents[1]

    def _mock(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'tok'})
        return httpx.Response(200, json={'id': '001', 'status': 'DONE'})

    cfg = AppConfig.from_mapping({
        'database': {'db_path': str(tmp_path / 'dp3.sqlite3'), 'sql_dir': str(ROOT / 'sql'), 'auto_migrate': True},
        'defaults': {'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default',
                     'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'dp-test'},
        'runtime': {'process_immediately': True},
        'checkbox': {'endpoint': 'https://api.mock/api/v1', 'license_key': 'X', 'cashier_pin': '0'},
    })

    # Inject DPS transport with probe stub
    stub = _MockProbeStub()
    dps_transport = DpsFiscalServerTransport(grpc_stub=stub)
    c = RuntimeContainer(
        cfg,
        transport_handlers={TransportKind.DPS_PRRO_GRPC_ECABINET: dps_transport},
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)),
    )

    with TestClient(create_app(c)) as client:
        resp = client.post('/v1/admin/dps-probe', json={'fiscal_number': 'FN-DEV-0001'})

    assert resp.status_code == 200
    body = resp.json()
    assert body['fiscal_number'] == 'FN-DEV-0001'
    assert body['status_rro']['online'] is True
    assert body['info_rro']['name'] == 'TestRRO'


# ---------------------------------------------------------------------------
# DP4 — REST endpoint failure/degraded
# ---------------------------------------------------------------------------

def test_dp4_rest_probe_failure(tmp_path) -> None:
    from pathlib import Path
    import httpx
    from fastapi.testclient import TestClient
    from prro_gateway.config import AppConfig
    from prro_gateway.enums import TransportKind
    from prro_gateway.runtime.container import RuntimeContainer
    from prro_gateway.runtime.rest_app import create_app

    ROOT = Path(__file__).resolve().parents[1]

    def _mock(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'tok'})
        return httpx.Response(200, json={'id': '001', 'status': 'DONE'})

    cfg = AppConfig.from_mapping({
        'database': {'db_path': str(tmp_path / 'dp4.sqlite3'), 'sql_dir': str(ROOT / 'sql'), 'auto_migrate': True},
        'defaults': {'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default',
                     'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'dp-test'},
        'runtime': {'process_immediately': True},
        'checkbox': {'endpoint': 'https://api.mock/api/v1', 'license_key': 'X', 'cashier_pin': '0'},
    })

    # Inject DPS transport with failing probe
    class _FailStub:
        def statusRro(self, req): raise ConnectionError('DPS unreachable')
        def infoRro(self, req): raise ConnectionError('DPS unreachable')
        def sendChkV2(self, req): pass
        def lastChk(self, req): pass

    dps_transport = DpsFiscalServerTransport(grpc_stub=_FailStub())
    c = RuntimeContainer(
        cfg,
        transport_handlers={TransportKind.DPS_PRRO_GRPC_ECABINET: dps_transport},
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)),
    )

    with TestClient(create_app(c)) as client:
        resp = client.post('/v1/admin/dps-probe', json={'fiscal_number': 'FN-DEV-0001'})

    assert resp.status_code == 200
    body = resp.json()
    assert 'error' in body['status_rro']
    assert 'error' in body['info_rro']
    assert 'unreachable' in body['status_rro']['error'].lower()


# ---------------------------------------------------------------------------
# DP5 — REST probe uses active profile endpoint
# ---------------------------------------------------------------------------

def test_dp5_rest_probe_uses_profile_endpoint(tmp_path) -> None:
    """Probe must resolve endpoint from active DPS transport profile, not constructor default."""
    from pathlib import Path
    from unittest.mock import patch, MagicMock
    import httpx
    from fastapi.testclient import TestClient
    from prro_gateway.config import AppConfig
    from prro_gateway.enums import TransportKind
    from prro_gateway.runtime.container import RuntimeContainer
    from prro_gateway.runtime.rest_app import create_app
    from prro_gateway.transports.proto import fiscal_server_pb2_grpc

    ROOT = Path(__file__).resolve().parents[1]

    def _mock(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'tok'})
        return httpx.Response(200, json={'id': '001', 'status': 'DONE'})

    cfg = AppConfig.from_mapping({
        'database': {'db_path': str(tmp_path / 'dp5.sqlite3'), 'sql_dir': str(ROOT / 'sql'), 'auto_migrate': True},
        'defaults': {'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default',
                     'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'dp-test'},
        'runtime': {'process_immediately': True},
        'checkbox': {'endpoint': 'https://api.mock/api/v1', 'license_key': 'X', 'cashier_pin': '0'},
    })

    # Use real DpsFiscalServerTransport with constructor default endpoint
    # that differs from the seeded profile endpoint
    dps_transport = DpsFiscalServerTransport(endpoint='wrong.default.host:443')

    c = RuntimeContainer(
        cfg,
        transport_handlers={TransportKind.DPS_PRRO_GRPC_ECABINET: dps_transport},
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)),
    )

    captured_endpoints = []

    def _fake_create_channel(endpoint, *, tls_root_certs=None):
        captured_endpoints.append(endpoint)
        return MagicMock()

    with TestClient(create_app(c)) as client:
        with patch('prro_gateway.transports.dps_fiscal_server._create_grpc_channel', side_effect=_fake_create_channel):
            with patch.object(fiscal_server_pb2_grpc, 'ChkIncomeServiceStub', return_value=_MockProbeStub()):
                resp = client.post('/v1/admin/dps-probe', json={'fiscal_number': 'FN-DEV-0001'})

    assert resp.status_code == 200
    # Probe must have used the seeded profile endpoint, not constructor default
    assert len(captured_endpoints) >= 1
    used_endpoint = captured_endpoints[0]
    assert 'prro.tax.gov.ua' in used_endpoint, (
        f'Probe must use profile endpoint (prro.tax.gov.ua), not constructor default. Got: {used_endpoint}'
    )
    assert 'wrong.default' not in used_endpoint
