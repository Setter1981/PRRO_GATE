"""
Sprint 6 / step 3 — crypto audit events and readiness surface tests.

Coverage matrix:
  OS1 — ops/summary shows crypto_provider_type
  OS2 — ops/summary shows runtime_environment
  OS3 — ops/summary shows crypto_production_gate_passed (None for dev)
  OS4 — existing breaker fields remain intact
  OS5 — breaker reset writes audit event when breaker was open
  OS6 — breaker open writes audit event at threshold
  OS7 — production + process_immediately=False + sidecar: crypto_provider_type is SidecarCryptoProvider
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]


def _config(tmp_path: Path, db_name: str) -> AppConfig:
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
            'channel_owner': 'os-test',
        },
        'runtime': {'process_immediately': True, 'environment': 'development'},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'OS-LIC',
            'cashier_pin': '0000',
        },
    })


def _mock(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-os'})
    return httpx.Response(200, json={'id': 'os-001', 'status': 'DONE'})


# ---------------------------------------------------------------------------
# OS1 — crypto_provider_type in ops/summary
# ---------------------------------------------------------------------------

def test_os1_ops_summary_shows_crypto_provider_type(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'os1.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)))
    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/ops/summary')
    assert resp.status_code == 200
    body = resp.json()
    assert body['crypto_provider_type'] == 'PassthroughCryptoProvider', (
        f'Expected PassthroughCryptoProvider, got {body.get("crypto_provider_type")}'
    )


# ---------------------------------------------------------------------------
# OS2 — runtime_environment in ops/summary
# ---------------------------------------------------------------------------

def test_os2_ops_summary_shows_runtime_environment(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'os2.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)))
    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/ops/summary')
    body = resp.json()
    assert body['runtime_environment'] == 'development'


# ---------------------------------------------------------------------------
# OS3 — crypto_production_gate_passed is None for dev
# ---------------------------------------------------------------------------

def test_os3_crypto_gate_passed_none_for_dev(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'os3.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)))
    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/ops/summary')
    body = resp.json()
    assert body['crypto_production_gate_passed'] is None, (
        f'Dev mode gate should be None, got {body["crypto_production_gate_passed"]}'
    )


# ---------------------------------------------------------------------------
# OS4 — existing breaker fields remain
# ---------------------------------------------------------------------------

def test_os4_existing_breaker_fields_intact(tmp_path: Path) -> None:
    cfg = _config(tmp_path, 'os4.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)))
    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/ops/summary')
    body = resp.json()
    assert 'crypto_breaker_open' in body
    assert 'crypto_consecutive_failures' in body
    assert 'crypto_breaker_threshold' in body
    assert body['crypto_breaker_open'] is False
    assert body['crypto_consecutive_failures'] == 0


# ---------------------------------------------------------------------------
# OS5 — breaker reset writes audit when was_open
# ---------------------------------------------------------------------------

def test_os5_breaker_reset_audit_event(tmp_path: Path) -> None:
    """When breaker was open and reset is called, audit event is persisted."""
    cfg = _config(tmp_path, 'os5.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)))
    with TestClient(create_app(c)) as client:
        # Force breaker open by setting failures >= threshold
        from prro_gateway.services.write_path import WritePathWorker
        worker = c.command_processor
        if isinstance(worker, WritePathWorker):
            worker._crypto_consecutive_failures = worker.crypto_breaker_threshold

        # Reset
        resp = client.post('/v1/admin/crypto/reset-breaker')
        assert resp.status_code == 200
        assert resp.json()['breaker_was_open'] is True

    # Check audit event
    with c.connect() as conn:
        row = conn.execute(
            "SELECT event_type, severity FROM audit_log WHERE entity_type = 'CRYPTO' AND event_type = 'CRYPTO_BREAKER_RESET' ORDER BY rowid DESC LIMIT 1"
        ).fetchone()
    assert row is not None, 'CRYPTO_BREAKER_RESET audit event must be persisted'
    assert row[0] == 'CRYPTO_BREAKER_RESET'
    assert row[1] == 'INFO'


# ---------------------------------------------------------------------------
# OS6 — breaker open writes audit at threshold
# ---------------------------------------------------------------------------

def test_os6_breaker_open_audit_event(tmp_path: Path) -> None:
    """When breaker threshold is reached, CRYPTO_BREAKER_BLOCKED audit event is persisted."""
    from datetime import datetime, UTC
    from prro_gateway.enums import OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker
    from prro_gateway.runtime.providers import PassthroughCryptoProvider

    cfg = _config(tmp_path, 'os6.sqlite3')
    c = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)))
    with TestClient(create_app(c)):
        pass  # initialize via lifespan

    # Setup shift + command
    with c.connect() as conn:
        ShiftRepository.create_shift(
            conn, shift_id='shift-os6', fiscal_number='FN-DEV-0001',
            state=ShiftState.OPENED, open_mode='ONLINE',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            protocol=Protocol.CHECKBOX_REST, integration_owner='test',
            channel_lock_acquired_at='2026-04-12T12:00:00Z',
        )
        conn.commit()
        cmd = CanonicalFiscalCommand(
            request_id='req-os6', idempotency_key='idem-os6',
            protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
            fiscal_number='FN-DEV-0001', route_key='main',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            channel_owner='test', external_request_id='ext-os6',
            business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
            payload={
                'receipt': {
                    'type': 'SELL',
                    'goods': [{'name': 'X', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                    'payments': [{'amount': 1000, 'type': 'CASH'}],
                    'totals': {'total_sum': 1000},
                },
            },
            payload_sha256='sha-os6',
            trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c'),
            correlation_id='c',
        )
        conn.execute('BEGIN IMMEDIATE')
        InboxRepository.accept_command(
            conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
            protocol=cmd.protocol, operation_type=cmd.operation_type,
            fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
            backend_profile_id=cmd.backend_profile_id,
            transport_profile_id=cmd.transport_profile_id,
            channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
            protocol_session_id=None, payload_json=cmd.model_dump_json(),
            payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-12T12:01:00Z',
        )
        conn.commit()

    # Create worker with failing crypto and threshold=1 so first failure opens breaker
    from prro_gateway.ports import CryptoProviderUnavailableError

    class _FailCrypto:
        def sign(self, **kw):
            raise CryptoProviderUnavailableError('test failure')

    class _NoTransport:
        def send(self, **kw): pass

    worker = WritePathWorker(crypto_provider=_FailCrypto(), transport_client=_NoTransport(), crypto_breaker_threshold=1)

    # First process: crypto fails, breaker increments to 1 (== threshold)
    with c.connect() as conn1:
        worker.process_next(conn1, fiscal_number='FN-DEV-0001')

    # Enqueue second command
    with c.connect() as conn2:
        conn2.execute('BEGIN IMMEDIATE')
        InboxRepository.accept_command(
            conn2, request_id='req-os6b', idempotency_key='idem-os6b',
            protocol='CHECKBOX_REST', operation_type='SELL',
            fiscal_number='FN-DEV-0001', route_key='main',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            channel_owner='test', external_request_id='ext-os6b',
            protocol_session_id=None, payload_json=cmd.model_dump_json(),
            payload_sha256='sha-os6b', response_deadline_at='2026-04-12T12:01:00Z',
        )
        conn2.commit()

    # Second process triggers breaker guard → audit event
    with c.connect() as conn3:
        worker.process_next(conn3, fiscal_number='FN-DEV-0001')

    # Check audit
    with c.connect() as conn4:
        row = conn4.execute(
            "SELECT event_type, severity FROM audit_log WHERE event_type = 'CRYPTO_BREAKER_BLOCKED' ORDER BY rowid DESC LIMIT 1"
        ).fetchone()
    assert row is not None, 'CRYPTO_BREAKER_BLOCKED audit event must be persisted'
    assert row[0] == 'CRYPTO_BREAKER_BLOCKED'
    assert row[1] == 'ERROR'


# ---------------------------------------------------------------------------
# OS7 — production + process_immediately=False + sidecar: crypto_provider_type correct
# ---------------------------------------------------------------------------

def test_os7_production_deferred_sidecar_shows_correct_provider(tmp_path: Path) -> None:
    """When process_immediately=False, ops/summary must still resolve crypto_provider_type
    via _resolve_crypto_provider(), not return 'none'."""
    from prro_gateway.migrations.runner import apply_migrations_to_connection

    cfg = AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'os7.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'os-test',
        },
        'runtime': {'process_immediately': False, 'environment': 'production'},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'OS7-LIC',
            'cashier_pin': '0000',
        },
        'crypto': {'provider': 'sidecar', 'sidecar_url': 'http://sidecar:8080'},
    })

    def _sidecar_mock(request: httpx.Request) -> httpx.Response:
        if 'checkbox' in str(request.url):
            return _mock(request)
        return httpx.Response(200, json={'signed_payload': 'signed', 'signature': 'sig'})

    c = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock)),
        crypto_http_client=httpx.Client(transport=httpx.MockTransport(_sidecar_mock)),
    )
    # Deactivate DPS stubs for production transport gate
    with c.connect() as conn:
        apply_migrations_to_connection(conn, Path(ROOT / 'sql'))
        conn.execute("UPDATE transport_profiles SET is_active = 0 WHERE kind != 'CHECKBOX_REST_TRANSPORT'")
        conn.commit()

    with TestClient(create_app(c)) as client:
        resp = client.get('/v1/ops/summary')

    assert resp.status_code == 200
    body = resp.json()
    assert body['runtime_environment'] == 'production'
    assert body['crypto_provider_type'] == 'SidecarCryptoProvider', (
        f'Deferred worker must still show resolved sidecar, got {body["crypto_provider_type"]}'
    )
    assert body['crypto_production_gate_passed'] is True
