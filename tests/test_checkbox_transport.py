from __future__ import annotations

import json

import httpx

from prro_gateway.repositories.backend_profiles import TransportProfileRepository
from prro_gateway.transports.checkbox_rest import CheckboxRestTransport


def _configure_checkbox_profile(conn) -> None:
    conn.execute(
        """
        UPDATE transport_profiles
           SET endpoint = ?,
               config_json = ?,
               timeout_config_json = ?
         WHERE transport_profile_id = 'transport_checkbox_rest_default'
        """,
        (
            'https://api.checkbox.ua/api/v1',
            json.dumps({
                'headers': {'X-Client-Name': 'test-client', 'X-Client-Version': '1.4.1'},
                'auth': {'license_key': 'lic-001', 'cashier_pin': '1234'},
                'require_local_sign': False,
            }),
            json.dumps({'connect_timeout': 5, 'request_timeout': 30, 'poll_timeout': 10}),
        ),
    )
    conn.commit()


def test_checkbox_transport_signin_send_sell_and_poll_done(conn):
    _configure_checkbox_profile(conn)
    profile = TransportProfileRepository.get_by_id(conn, 'transport_checkbox_rest_default')
    assert profile is not None

    captured: list[tuple[str, str, dict[str, str], dict | None]] = []
    created_ids: list[str] = []

    def handler(request: httpx.Request) -> httpx.Response:
        body = json.loads(request.content.decode()) if request.content else None
        captured.append((request.method, request.url.path, dict(request.headers), body))
        if request.url.path == '/api/v1/cashier/signinPinCode':
            assert request.headers['X-License-Key'] == 'lic-001'
            assert body == {'pin_code': '1234'}
            return httpx.Response(200, json={'access_token': 'token-1'})
        if request.url.path == '/api/v1/receipts/sell':
            assert request.headers['Authorization'] == 'Bearer token-1'
            assert body is not None
            assert body['goods'][0]['good']['code'] == 'SKU-1'
            assert body['goods'][0]['good']['name'] == 'Water'
            assert body['payments'][0]['type'] == 'CASH'
            created_ids.append(body['id'])
            return httpx.Response(200, json={'id': body['id'], 'status': 'CREATED'})
        if request.url.path == f'/api/v1/receipts/{created_ids[0]}':
            return httpx.Response(200, json={'id': created_ids[0], 'status': 'DONE', 'fiscal_code': 'FC-001', 'fiscal_date': '2026-03-28T12:00:01+00:00', 'updated_at': '2026-03-28T12:00:02+00:00'})
        raise AssertionError(f'unexpected request: {request.method} {request.url}')

    client = httpx.Client(transport=httpx.MockTransport(handler))
    transport = CheckboxRestTransport(http_client=client)
    payload = {
        'currency': 'UAH',
        'cashier_id': 'cashier-1',
        'receipt': {
            'type': 'SELL',
            'goods': [{'item_id': 'item-1', 'item_no': 1, 'code': 'SKU-1', 'name': 'Water', 'price': 5000, 'quantity': 1000, 'sum': 5000, 'excise_barcodes': []}],
            'payments': [{'payment_id': 'pay-1', 'payment_type': 'CASH', 'amount': 5000}],
            'related_receipt_id': None,
            'technical_return': False,
            'rounding_enabled': False,
        },
    }

    send = transport.send(
        document_id='doc-1',
        signed_payload='',
        fiscal_number='FN-DEV-0001',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        operation_type='SELL',
        request_payload=payload,
        external_request_id='external-sell-1',
        transport_profile=profile,
    )
    assert send.state == 'SENT'
    assert send.transport_request_id == created_ids[0]

    poll = transport.poll_status(
        document_id='doc-1',
        fiscal_number='FN-DEV-0001',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        transport_request_id=send.transport_request_id,
        operation_type='SELL',
        transport_profile=profile,
    )
    assert poll.state == 'ACK'
    assert poll.server_fiscal_no == 'FC-001'
    assert len(captured) == 3


def test_checkbox_transport_shift_open_uses_license_key_and_polls_opened(conn):
    _configure_checkbox_profile(conn)
    profile = TransportProfileRepository.get_by_id(conn, 'transport_checkbox_rest_default')
    assert profile is not None

    shift_ids: list[str] = []

    def handler(request: httpx.Request) -> httpx.Response:
        body = json.loads(request.content.decode()) if request.content else None
        if request.url.path == '/api/v1/cashier/signinPinCode':
            return httpx.Response(200, json={'access_token': 'token-1'})
        if request.url.path == '/api/v1/shifts':
            assert request.headers['X-License-Key'] == 'lic-001'
            assert request.headers['Authorization'] == 'Bearer token-1'
            assert body is not None and 'id' in body
            shift_ids.append(body['id'])
            return httpx.Response(200, json={'id': body['id'], 'status': 'CREATED'})
        if request.url.path == f'/api/v1/shifts/{shift_ids[0]}':
            return httpx.Response(200, json={'id': shift_ids[0], 'status': 'OPENED', 'updated_at': '2026-03-28T12:00:02+00:00'})
        raise AssertionError(f'unexpected request: {request.method} {request.url}')

    client = httpx.Client(transport=httpx.MockTransport(handler))
    transport = CheckboxRestTransport(http_client=client)
    send = transport.send(
        document_id='doc-shift-open',
        signed_payload='',
        fiscal_number='FN-DEV-0001',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        operation_type='SHIFT_OPEN',
        request_payload={'receipt': {'type': 'SHIFT_OPEN'}},
        external_request_id='shift-open-1',
        transport_profile=profile,
    )
    assert send.state == 'SENT'
    poll = transport.poll_status(
        document_id='doc-shift-open',
        fiscal_number='FN-DEV-0001',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        transport_request_id=send.transport_request_id,
        operation_type='SHIFT_OPEN',
        transport_profile=profile,
    )
    assert poll.state == 'ACK'


def test_checkbox_transport_send_can_resolve_operation_from_request_payload_json(conn):
    _configure_checkbox_profile(conn)
    profile = TransportProfileRepository.get_by_id(conn, 'transport_checkbox_rest_default')
    assert profile is not None

    created_ids: list[str] = []

    def handler(request: httpx.Request) -> httpx.Response:
        body = json.loads(request.content.decode()) if request.content else None
        if request.url.path == '/api/v1/cashier/signinPinCode':
            return httpx.Response(200, json={'access_token': 'token-1'})
        if request.url.path == '/api/v1/receipts/sell':
            assert body is not None
            created_ids.append(body['id'])
            return httpx.Response(200, json={'id': body['id'], 'status': 'CREATED'})
        raise AssertionError(f'unexpected request: {request.method} {request.url}')

    client = httpx.Client(transport=httpx.MockTransport(handler))
    transport = CheckboxRestTransport(http_client=client)
    request_payload_json = json.dumps({
        'operation_type': 'SELL',
        'payload': {
            'receipt': {
                'type': 'SELL',
                'goods': [{'code': 'SKU-1', 'name': 'Water', 'price': 5000, 'quantity': 1000, 'sum': 5000}],
                'payments': [{'payment_type': 'CASH', 'amount': 5000}],
            },
        },
    })

    send = transport.send(
        document_id='doc-fallback-op',
        signed_payload='',
        fiscal_number='FN-DEV-0001',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        request_payload_json=request_payload_json,
        transport_profile=profile,
    )
    assert send.state == 'SENT'
    assert send.transport_request_id == created_ids[0]
