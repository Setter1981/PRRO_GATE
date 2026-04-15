"""
Sprint 6 / step 2 — production transport startup gate tests.

Coverage matrix:
  TG1 — dev mode + default seeded profiles (with stubs): initializes normally
  TG2 — production + default seeded profiles: fails (DPS stubs active)
  TG3 — production + DPS profiles deactivated: initializes (only Checkbox real)
  TG4 — production + injected real handlers replacing stubs: initializes
  TG5 — production + injected CheckboxRestTransportStub via transport_handlers: fails
  TG6 — production + injected WritePathWorker(CheckboxRestTransportStub): fails
  TG7 — production + injected OfflineSyncService(stub transport): fails
  TG8 — production + injected ReconciliationService(stub transport_status_client): fails
"""
from __future__ import annotations

from pathlib import Path

import httpx
import pytest

from prro_gateway.config import AppConfig
from prro_gateway.enums import TransportKind
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.transports.checkbox_rest import CheckboxRestTransport

ROOT = Path(__file__).resolve().parents[1]


def _config(tmp_path: Path, db_name: str, *, environment: str = 'development') -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / db_name),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'tg-test',
        },
        'runtime': {
            'process_immediately': True,
            'environment': environment,
        },
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'TG-LIC',
            'cashier_pin': '0000',
        },
        'crypto': {
            'provider': 'sidecar' if environment == 'production' else 'passthrough',
            'sidecar_url': 'http://sidecar:8080' if environment == 'production' else None,
        },
    })


def _mock_transport(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-tg'})
    return httpx.Response(200, json={'id': 'tg-001', 'status': 'DONE'})


def _sidecar_mock(request: httpx.Request) -> httpx.Response:
    if 'checkbox' in str(request.url):
        return _mock_transport(request)
    return httpx.Response(200, json={'signed_payload': 'signed', 'signature': 'sig'})


# ---------------------------------------------------------------------------
# TG1 — dev mode + default seeded profiles: initializes
# ---------------------------------------------------------------------------

def test_tg1_dev_stubs_initialize(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'tg1.sqlite3', environment='development')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)))
    c.initialize()
    assert c.health.live is True
    assert c.health.phase == 'RUNNING'


# ---------------------------------------------------------------------------
# TG2 — production + default seeded profiles: fails (DPS stubs active)
# ---------------------------------------------------------------------------

def test_tg2_production_stubs_fail(tmp_path: Path) -> None:
    """Production fails because DPS_XML_UNIFIED_WINDOW profile still uses stub handler."""
    cfg = _config(tmp_path, 'tg2.sqlite3', environment='production')
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    with pytest.raises(RuntimeError, match='stub handlers'):
        c.initialize()
    assert c.health.live is False
    assert c.health.phase == 'STARTUP_ERROR'
    assert 'transport_dps_xml_default' in (c.health.last_error or '')


# ---------------------------------------------------------------------------
# TG3 — production + DPS profiles deactivated: initializes
# ---------------------------------------------------------------------------

def test_tg3_production_no_stubs_initializes(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'tg3.sqlite3', environment='production')
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    # Run migrations first, then deactivate DPS profiles before full initialize
    from prro_gateway.migrations.runner import apply_migrations_to_connection
    with c.connect() as conn:
        apply_migrations_to_connection(conn, Path(cfg.database.sql_dir))
        conn.execute("UPDATE transport_profiles SET is_active = 0 WHERE kind IN ('DPS_PRRO_GRPC_ECABINET', 'DPS_PRRO_XML_UNIFIED_WINDOW')")
        conn.commit()

    c.initialize()
    assert c.health.live is True
    assert c.health.phase == 'RUNNING'


# ---------------------------------------------------------------------------
# TG4 — production + injected real handlers replacing stubs: initializes
# ---------------------------------------------------------------------------

def test_tg4_production_injected_real_handlers(tmp_path: Path) -> None:
    """If caller injects real handlers for DPS kinds, production gate passes."""

    class _RealDpsGrpcHandler:
        """Placeholder non-stub handler for DPS gRPC."""
        def send(self, **kw): pass
        def poll_status(self, **kw): pass

    class _RealDpsXmlHandler:
        """Placeholder non-stub handler for DPS XML."""
        def send(self, **kw): pass
        def poll_status(self, **kw): pass

    cfg = _config(tmp_path, 'tg4.sqlite3', environment='production')
    c = RuntimeContainer(
        cfg,
        transport_handlers={
            TransportKind.DPS_PRRO_GRPC_ECABINET: _RealDpsGrpcHandler(),
            TransportKind.DPS_PRRO_XML_UNIFIED_WINDOW: _RealDpsXmlHandler(),
        },
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    c.initialize()
    assert c.health.live is True
    assert c.health.phase == 'RUNNING'


# ---------------------------------------------------------------------------
# TG5 — production + injected CheckboxRestTransportStub: fails
# ---------------------------------------------------------------------------

def test_tg5_production_injected_checkbox_stub_fails(tmp_path: Path) -> None:
    """CheckboxRestTransportStub injected via transport_handlers must be caught."""
    from prro_gateway.transports.stubs import CheckboxRestTransportStub
    from prro_gateway.migrations.runner import apply_migrations_to_connection

    cfg = _config(tmp_path, 'tg5.sqlite3', environment='production')
    c = RuntimeContainer(
        cfg,
        transport_handlers={
            TransportKind.CHECKBOX_REST_TRANSPORT: CheckboxRestTransportStub(),
        },
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    # Deactivate DPS profiles so only checkbox stub triggers
    with c.connect() as conn:
        apply_migrations_to_connection(conn, Path(ROOT / 'sql'))
        conn.execute("UPDATE transport_profiles SET is_active = 0 WHERE kind IN ('DPS_PRRO_GRPC_ECABINET', 'DPS_PRRO_XML_UNIFIED_WINDOW')")
        conn.commit()

    with pytest.raises(RuntimeError, match='stub handlers'):
        c.initialize()
    assert c.health.live is False


# ---------------------------------------------------------------------------
# TG6 — production + injected WritePathWorker(CheckboxRestTransportStub): fails
# ---------------------------------------------------------------------------

def test_tg6_production_injected_worker_stub_transport_fails(tmp_path: Path) -> None:
    """Prebuilt WritePathWorker carrying a stub transport_client must be caught."""
    from prro_gateway.transports.stubs import CheckboxRestTransportStub as _CbStub
    from prro_gateway.runtime.providers import SidecarCryptoProvider, SidecarCryptoClient
    from prro_gateway.services.write_path import WritePathWorker
    from prro_gateway.migrations.runner import apply_migrations_to_connection

    cfg = _config(tmp_path, 'tg6.sqlite3', environment='production')
    sidecar_client = SidecarCryptoClient(
        base_url='http://sidecar:8080',
        http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    worker = WritePathWorker(
        crypto_provider=SidecarCryptoProvider(client=sidecar_client),
        transport_client=_CbStub(),
    )
    c = RuntimeContainer(
        cfg,
        command_processor=worker,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    # Deactivate DPS stubs so router gate passes — worker gate must still catch
    with c.connect() as conn:
        apply_migrations_to_connection(conn, Path(ROOT / 'sql'))
        conn.execute("UPDATE transport_profiles SET is_active = 0 WHERE kind != 'CHECKBOX_REST_TRANSPORT'")
        conn.commit()

    with pytest.raises(RuntimeError, match='stub transport'):
        c.initialize()
    assert c.health.live is False


# ---------------------------------------------------------------------------
# TG7 — production + injected OfflineSyncService(stub transport): fails
# ---------------------------------------------------------------------------

def test_tg7_production_injected_offline_sync_stub_fails(tmp_path: Path) -> None:
    """Injected OfflineSyncService carrying a stub transport_client must be caught."""
    from prro_gateway.transports.stubs import DpsGrpcEcabinetTransportStub
    from prro_gateway.services.offline_sync import OfflineSyncService
    from prro_gateway.migrations.runner import apply_migrations_to_connection

    cfg = _config(tmp_path, 'tg7.sqlite3', environment='production')
    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    # Deactivate DPS stubs so router gate passes
    with c.connect() as conn:
        apply_migrations_to_connection(conn, Path(ROOT / 'sql'))
        conn.execute("UPDATE transport_profiles SET is_active = 0 WHERE kind != 'CHECKBOX_REST_TRANSPORT'")
        conn.commit()
    # Inject stub offline_sync_service
    c.offline_sync_service = OfflineSyncService(transport_client=DpsGrpcEcabinetTransportStub())

    with pytest.raises(RuntimeError, match='offline_sync_service.*stub transport'):
        c.initialize()
    assert c.health.live is False


# ---------------------------------------------------------------------------
# TG8 — production + injected ReconciliationService(stub): fails
# ---------------------------------------------------------------------------

def test_tg8_production_injected_reconciliation_stub_fails(tmp_path: Path) -> None:
    """Injected ReconciliationService carrying a stub transport_status_client must be caught."""
    from prro_gateway.transports.stubs import DpsXmlUnifiedWindowTransportStub
    from prro_gateway.services.reconciliation import ReconciliationService
    from prro_gateway.migrations.runner import apply_migrations_to_connection

    cfg = _config(tmp_path, 'tg8.sqlite3', environment='production')
    c = RuntimeContainer(
        cfg,
        reconciliation_service=ReconciliationService(transport_status_client=DpsXmlUnifiedWindowTransportStub()),
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_transport)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    # Deactivate DPS stubs so router gate passes
    with c.connect() as conn:
        apply_migrations_to_connection(conn, Path(ROOT / 'sql'))
        conn.execute("UPDATE transport_profiles SET is_active = 0 WHERE kind != 'CHECKBOX_REST_TRANSPORT'")
        conn.commit()

    with pytest.raises(RuntimeError, match='reconciliation_service.*stub transport'):
        c.initialize()
    assert c.health.live is False
