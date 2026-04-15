from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.migrations.runner import apply_migrations_to_connection
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app
from prro_gateway.repositories import InboxRepository

ROOT = Path(__file__).resolve().parents[1]


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'runtime.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'rest-tests',
        },
    })


def test_rest_health_and_accept_checkbox(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        assert client.get('/health/live').status_code == 200
        assert client.get('/health/ready').status_code == 200
        raw = {
            'context': {
                'request_id': 'req-runtime-1',
                'fiscal_number': 'FN-DEV-0001',
                'backend_profile_id': 'backend_checkbox_default',
                'transport_profile_id': 'transport_checkbox_rest_default',
                'channel_owner': 'rest-tests',
                'business_ts': '2026-03-28T12:00:00Z',
            },
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-1',
                'cashier_id': 'cashier-1',
                'goods': [{'name': 'Water', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        }
        response = client.post('/v1/ingress/checkbox', json=raw)
        assert response.status_code == 200
        body = response.json()
        assert body['status'] == 'NEW'
    with container.connect() as conn:
        record = InboxRepository.get_by_request_id(conn, body['request_id'])
        assert record is not None
        assert record.fiscal_number == 'FN-DEV-0001'



def test_rest_process_immediately_with_real_checkbox_transport(tmp_path: Path) -> None:
    cfg = AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'runtime-real-checkbox.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'rest-tests',
        },
        'runtime': {
            'process_immediately': True,
        },
    })

    def handler(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'token-1'})
        if path.endswith('/shifts'):
            return httpx.Response(200, json={'id': 'remote-shift-1', 'status': 'OPENED', 'fiscal_code': 'SHIFT-FN-1', 'updated_at': '2026-03-29T10:00:00+00:00'})
        if path.endswith('/receipts/sell'):
            return httpx.Response(200, json={'id': 'remote-receipt-1', 'status': 'DONE', 'fiscal_code': 'RCPT-1', 'updated_at': '2026-03-29T10:01:00+00:00'})
        raise AssertionError(f'unexpected request: {request.method} {request.url}')

    db_path = Path(cfg.database.db_path)
    db_path.parent.mkdir(parents=True, exist_ok=True)
    import sqlite3
    seed_conn = sqlite3.connect(db_path)
    try:
        apply_migrations_to_connection(seed_conn, ROOT / 'sql')
        seed_conn.execute(
            "UPDATE transport_profiles SET endpoint = ?, config_json = ? WHERE transport_profile_id = 'transport_checkbox_rest_default'",
            ('https://api.checkbox.test/api/v1', '{"require_local_sign": false, "auth": {"license_key": "LIC-1", "cashier_pin": "1234"}, "headers": {"X-Client-Name": "tests", "X-Client-Version": "1.4.1"}}'),
        )
        seed_conn.commit()
    finally:
        seed_conn.close()

    transport_client = httpx.Client(transport=httpx.MockTransport(handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)
    with TestClient(create_app(container)) as client:
        open_raw = {
            'context': {
                'request_id': 'req-runtime-open-1',
                'fiscal_number': 'FN-DEV-0001',
                'backend_profile_id': 'backend_checkbox_default',
                'transport_profile_id': 'transport_checkbox_rest_default',
                'channel_owner': 'rest-tests',
                'business_ts': '2026-03-29T10:00:00Z',
            },
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'ext-open-1',
                'cashier_id': 'cashier-1',
            },
        }
        sell_raw = {
            'context': {
                'request_id': 'req-runtime-sell-1',
                'fiscal_number': 'FN-DEV-0001',
                'backend_profile_id': 'backend_checkbox_default',
                'transport_profile_id': 'transport_checkbox_rest_default',
                'channel_owner': 'rest-tests',
                'business_ts': '2026-03-29T10:01:00Z',
            },
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-1',
                'cashier_id': 'cashier-1',
                'goods': [{'name': 'Water', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        }
        open_response = client.post('/v1/ingress/checkbox', json=open_raw)
        assert open_response.status_code == 200
        sell_response = client.post('/v1/ingress/checkbox', json=sell_raw)
        assert sell_response.status_code == 200
    with container.connect() as conn:
        docs = conn.execute("SELECT doc_type, state FROM fiscal_documents ORDER BY created_at").fetchall()
        assert [tuple(row) for row in docs] == [('SHIFT_OPEN', 'ACK'), ('SELL', 'ACK')]
        shift = conn.execute("SELECT state FROM shifts ORDER BY created_at DESC LIMIT 1").fetchone()
        assert tuple(shift) == ('OPENED',)


def test_rest_document_status_endpoint(tmp_path: Path) -> None:
    cfg = AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'runtime-docstatus.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'rest-tests',
        },
        'runtime': {'process_immediately': True},
    })

    def handler(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'tok'})
        if path.endswith('/shifts'):
            return httpx.Response(200, json={'id': 'sh-1', 'status': 'OPENED', 'fiscal_code': 'SH-FC-1', 'updated_at': '2026-03-29T10:00:00+00:00'})
        if path.endswith('/receipts/sell'):
            return httpx.Response(200, json={'id': 'rc-1', 'status': 'DONE', 'fiscal_code': 'RC-FC-1', 'updated_at': '2026-03-29T10:01:00+00:00'})
        raise AssertionError(f'unexpected: {request.method} {request.url}')

    db_path = Path(cfg.database.db_path)
    db_path.parent.mkdir(parents=True, exist_ok=True)
    import sqlite3
    seed_conn = sqlite3.connect(db_path)
    try:
        apply_migrations_to_connection(seed_conn, ROOT / 'sql')
        seed_conn.execute(
            "UPDATE transport_profiles SET endpoint = ?, config_json = ? WHERE transport_profile_id = 'transport_checkbox_rest_default'",
            ('https://api.checkbox.test/api/v1', '{"require_local_sign": false, "auth": {"license_key": "LIC-1", "cashier_pin": "1234"}, "headers": {"X-Client-Name": "tests", "X-Client-Version": "1.4.1"}}'),
        )
        seed_conn.commit()
    finally:
        seed_conn.close()

    transport_client = httpx.Client(transport=httpx.MockTransport(handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)
    with TestClient(create_app(container)) as client:
        # 404 for unknown request_id
        assert client.get('/v1/documents/nonexistent').status_code == 404

        # open shift then sell
        client.post('/v1/ingress/checkbox', json={
            'context': {'request_id': 'req-open-ds', 'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default', 'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'rest-tests', 'business_ts': '2026-03-29T10:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-ds', 'cashier_id': 'c1'},
        })
        sell_resp = client.post('/v1/ingress/checkbox', json={
            'context': {'request_id': 'req-sell-ds', 'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default', 'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'rest-tests', 'business_ts': '2026-03-29T10:01:00Z'},
            'operation': 'SELL',
            'request': {'external_request_id': 'ext-sell-ds', 'cashier_id': 'c1', 'goods': [{'name': 'Tea', 'price': 500, 'quantity': 1000}], 'payments': [{'type': 'CASH', 'amount': 500}]},
        })
        assert sell_resp.status_code == 200
        sell_request_id = sell_resp.json()['request_id']

        doc_resp = client.get(f'/v1/documents/{sell_request_id}')
        assert doc_resp.status_code == 200
        body = doc_resp.json()
        assert body['state'] == 'ACK'
        assert body['doc_type'] == 'SELL'
        assert body['fiscal_number'] == 'FN-DEV-0001'
        assert body['server_fiscal_no'] == 'RC-FC-1'


def _make_process_immediately_config(tmp_path: Path, db_name: str) -> AppConfig:
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
            'channel_owner': 'rest-tests',
        },
        'runtime': {'process_immediately': True},
    })


def _seed_checkbox_auth(db_path: Path) -> None:
    import sqlite3
    db_path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(db_path)
    try:
        apply_migrations_to_connection(conn, ROOT / 'sql')
        conn.execute(
            "UPDATE transport_profiles SET endpoint = ?, config_json = ? WHERE transport_profile_id = 'transport_checkbox_rest_default'",
            ('https://api.checkbox.test/api/v1', '{"require_local_sign": false, "auth": {"license_key": "LIC-1", "cashier_pin": "1234"}, "headers": {"X-Client-Name": "tests", "X-Client-Version": "1.4.1"}}'),
        )
        conn.commit()
    finally:
        conn.close()


def test_ingress_checkbox_response_enriched_on_sync_ack(tmp_path: Path) -> None:
    """process_immediately=True: ingress response directly contains document_state=ACK and server_fiscal_no."""
    cfg = _make_process_immediately_config(tmp_path, 'enrich-ack.sqlite3')
    _seed_checkbox_auth(Path(cfg.database.db_path))

    def handler(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'tok'})
        if path.endswith('/shifts'):
            return httpx.Response(200, json={'id': 'sh-1', 'status': 'OPENED', 'fiscal_code': 'SH-FC-1', 'updated_at': '2026-03-29T10:00:00+00:00'})
        if path.endswith('/receipts/sell'):
            return httpx.Response(200, json={'id': 'rc-1', 'status': 'DONE', 'fiscal_code': 'SELL-FC-1', 'updated_at': '2026-03-29T10:01:00+00:00'})
        raise AssertionError(f'unexpected: {request.method} {request.url}')

    container = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(handler)))
    with TestClient(create_app(container)) as client:
        client.post('/v1/ingress/checkbox', json={
            'context': {'request_id': 'req-open-ack', 'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default', 'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'rest-tests', 'business_ts': '2026-03-29T10:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-ack', 'cashier_id': 'c1'},
        })
        sell_resp = client.post('/v1/ingress/checkbox', json={
            'context': {'request_id': 'req-sell-ack', 'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default', 'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'rest-tests', 'business_ts': '2026-03-29T10:01:00Z'},
            'operation': 'SELL',
            'request': {'external_request_id': 'ext-sell-ack', 'cashier_id': 'c1', 'goods': [{'name': 'Tea', 'price': 500, 'quantity': 1000}], 'payments': [{'type': 'CASH', 'amount': 500}]},
        })
    assert sell_resp.status_code == 200
    body = sell_resp.json()
    assert body['document_state'] == 'ACK'
    assert body['server_fiscal_no'] == 'SELL-FC-1'
    assert body['document_id'] is not None
    assert body['error_message'] is None


def test_ingress_checkbox_response_not_enriched_when_async(tmp_path: Path) -> None:
    """process_immediately=False (default): ingress response must NOT include document enrichment fields."""
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        resp = client.post('/v1/ingress/checkbox', json={
            'context': {'request_id': 'req-async-1', 'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default', 'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'rest-tests', 'business_ts': '2026-03-29T10:00:00Z'},
            'operation': 'SELL',
            'request': {'external_request_id': 'ext-async-1', 'cashier_id': 'c1', 'goods': [{'name': 'Tea', 'price': 500, 'quantity': 1000}], 'payments': [{'type': 'CASH', 'amount': 500}]},
        })
    assert resp.status_code == 200
    body = resp.json()
    assert 'document_id' not in body
    assert 'document_state' not in body
    assert 'server_fiscal_no' not in body


def test_ingress_checkbox_response_enriched_on_sync_transport_rejection(tmp_path: Path) -> None:
    """process_immediately=True: transport rejection → response includes document_state=REJECTED and error_message."""
    cfg = _make_process_immediately_config(tmp_path, 'enrich-reject.sqlite3')
    _seed_checkbox_auth(Path(cfg.database.db_path))

    def handler(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'tok'})
        if path.endswith('/shifts'):
            return httpx.Response(200, json={'id': 'sh-2', 'status': 'OPENED', 'fiscal_code': 'SH-FC-2', 'updated_at': '2026-03-29T10:00:00+00:00'})
        if path.endswith('/receipts/sell'):
            return httpx.Response(422, json={'message': 'receipt validation failed'})
        raise AssertionError(f'unexpected: {request.method} {request.url}')

    container = RuntimeContainer(cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(handler)))
    with TestClient(create_app(container)) as client:
        client.post('/v1/ingress/checkbox', json={
            'context': {'request_id': 'req-open-rej', 'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default', 'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'rest-tests', 'business_ts': '2026-03-29T10:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-rej', 'cashier_id': 'c1'},
        })
        sell_resp = client.post('/v1/ingress/checkbox', json={
            'context': {'request_id': 'req-sell-rej', 'fiscal_number': 'FN-DEV-0001', 'backend_profile_id': 'backend_checkbox_default', 'transport_profile_id': 'transport_checkbox_rest_default', 'channel_owner': 'rest-tests', 'business_ts': '2026-03-29T10:01:00Z'},
            'operation': 'SELL',
            'request': {'external_request_id': 'ext-sell-rej', 'cashier_id': 'c1', 'goods': [{'name': 'Tea', 'price': 500, 'quantity': 1000}], 'payments': [{'type': 'CASH', 'amount': 500}]},
        })
    assert sell_resp.status_code == 200
    body = sell_resp.json()
    assert body['document_state'] == 'REJECTED'
    assert body['document_id'] is not None
    assert body['error_message'] is not None


def test_ingress_checkbox_guard_error_surfaced_in_sync_response(tmp_path: Path) -> None:
    """process_immediately=True + SELL without open shift → response contains error_code/error_message, no document_id."""
    cfg = _make_process_immediately_config(tmp_path, 'guard-error.sqlite3')
    # No Checkbox auth seeding needed: guard fires before transport is called
    container = RuntimeContainer(cfg)
    with TestClient(create_app(container)) as client:
        sell_resp = client.post('/v1/ingress/checkbox', json={
            'context': {
                'request_id': 'req-guard-1',
                'fiscal_number': 'FN-DEV-0001',
                'backend_profile_id': 'backend_checkbox_default',
                'transport_profile_id': 'transport_checkbox_rest_default',
                'channel_owner': 'rest-tests',
                'business_ts': '2026-03-29T10:00:00Z',
            },
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-guard-1',
                'cashier_id': 'c1',
                'goods': [{'name': 'Tea', 'price': 500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 500}],
            },
        })
    assert sell_resp.status_code == 200
    body = sell_resp.json()
    assert body['error_code'] == 'SHIFT_NOT_OPEN'
    assert body['error_message'] is not None
    assert 'document_id' not in body
    assert 'document_state' not in body


def test_ingress_checkbox_fresh_request_existing_is_false(tmp_path: Path) -> None:
    """First-time accept must have existing=false (not a replay)."""
    container = RuntimeContainer(_config(tmp_path))
    with TestClient(create_app(container)) as client:
        resp = client.post('/v1/ingress/checkbox', json={
            'context': {
                'request_id': 'req-fresh-existing-1',
                'fiscal_number': 'FN-DEV-0001',
                'backend_profile_id': 'backend_checkbox_default',
                'transport_profile_id': 'transport_checkbox_rest_default',
                'channel_owner': 'rest-tests',
                'business_ts': '2026-03-29T12:00:00Z',
            },
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-fresh-1',
                'cashier_id': 'c1',
                'goods': [{'name': 'Water', 'price': 1000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1000}],
            },
        })
    assert resp.status_code == 200
    assert resp.json()['existing'] is False


def test_ingress_checkbox_same_request_id_replay_existing_is_true(tmp_path: Path) -> None:
    """Replay with same request_id must have existing=true."""
    container = RuntimeContainer(_config(tmp_path))
    payload = {
        'context': {
            'request_id': 'req-replay-signal-1',
            'fiscal_number': 'FN-DEV-0001',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'rest-tests',
            'business_ts': '2026-03-29T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'external_request_id': 'ext-replay-1',
            'cashier_id': 'c1',
            'goods': [{'name': 'Water', 'price': 1000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1000}],
        },
    }
    with TestClient(create_app(container)) as client:
        first = client.post('/v1/ingress/checkbox', json=payload)
        second = client.post('/v1/ingress/checkbox', json=payload)
    assert first.status_code == 200
    assert second.status_code == 200
    assert first.json()['existing'] is False, 'first accept must not be marked existing'
    assert second.json()['existing'] is True, 'same-request_id replay must be marked existing'
    # Both responses return the same request_id
    assert first.json()['request_id'] == second.json()['request_id'] == 'req-replay-signal-1'
