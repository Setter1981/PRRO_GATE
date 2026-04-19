from __future__ import annotations

import json
import time
from datetime import UTC, datetime
from typing import Any
from uuid import NAMESPACE_URL, uuid5

import httpx

from ..enums import DocumentState, OperationType
from ..ports import PollResult, SendResult, TransportRejectedError, TransportRetryableError
from ..models.storage import TransportProfileRecord


def _make_default_http_client() -> httpx.Client:
    """Default outbound HTTP client with HTTP/2 when available.

    HTTP/2 multiplexes requests over one TCP/TLS connection — verified to give
    ~6× lower per-request overhead vs HTTP/1.1 in scripts/bench_transport.py
    (B9 vs B10). Requires the optional `h2` package; falls back to HTTP/1.1
    if absent so this module stays importable in minimal prod environments.
    """
    try:
        import h2  # noqa: F401
        return httpx.Client(http2=True)
    except ImportError:
        return httpx.Client()


class CheckboxAuthSession:
    def __init__(
        self,
        *,
        base_url: str,
        client_name: str,
        client_version: str,
        license_key: str | None = None,
        cashier_pin: str | None = None,
        cashier_login: str | None = None,
        cashier_password: str | None = None,
        verify_tls: bool = True,
        connect_timeout: float = 5.0,
        request_timeout: float = 30.0,
        http_client: httpx.Client | None = None,
    ) -> None:
        self.base_url = base_url.rstrip('/')
        self.client_name = client_name
        self.client_version = client_version
        self.license_key = license_key
        self.cashier_pin = cashier_pin
        self.cashier_login = cashier_login
        self.cashier_password = cashier_password
        self.connect_timeout = connect_timeout
        self.request_timeout = request_timeout
        self.http_client = http_client or _make_default_http_client()
        self.verify_tls = verify_tls
        self._access_token: str | None = None

    def authorized_request(self, method: str, path: str, *, json_body: dict[str, Any] | None = None, include_license_key: bool = False) -> httpx.Response:
        if self._access_token is None:
            self._signin()
        response = self._request(method, path, json_body=json_body, include_auth=True, include_license_key=include_license_key)
        if response.status_code == 401:
            self._signin(force_refresh=True)
            response = self._request(method, path, json_body=json_body, include_auth=True, include_license_key=include_license_key)
        self._raise_for_status(response)
        return response

    def _signin(self, *, force_refresh: bool = False) -> None:
        if self._access_token is not None and not force_refresh:
            return
        if self.license_key and self.cashier_pin:
            response = self._request(
                'POST',
                '/cashier/signinPinCode',
                json_body={'pin_code': self.cashier_pin},
                include_license_key=True,
                include_auth=False,
            )
        elif self.cashier_login and self.cashier_password:
            response = self._request(
                'POST',
                '/cashier/signin',
                json_body={'login': self.cashier_login, 'password': self.cashier_password},
                include_auth=False,
            )
        else:
            raise TransportRejectedError('Checkbox auth config is missing. Provide either auth.license_key+cashier_pin or auth.cashier_login+cashier_password.')
        self._raise_for_status(response)
        payload = self._decode_json(response)
        token = payload.get('access_token')
        if not token:
            raise TransportRejectedError('Checkbox auth succeeded without access_token in response.')
        self._access_token = str(token)

    def _request(
        self,
        method: str,
        path: str,
        *,
        json_body: dict[str, Any] | None = None,
        include_auth: bool = True,
        include_license_key: bool = False,
    ) -> httpx.Response:
        headers = {
            'accept': 'application/json',
            'X-Client-Name': self.client_name,
            'X-Client-Version': self.client_version,
        }
        if json_body is not None:
            headers['Content-Type'] = 'application/json'
        if include_license_key and self.license_key:
            headers['X-License-Key'] = self.license_key
        if include_auth and self._access_token:
            headers['Authorization'] = f'Bearer {self._access_token}'
        timeout = httpx.Timeout(connect=self.connect_timeout, read=self.request_timeout, write=self.request_timeout, pool=self.request_timeout)
        try:
            return self.http_client.request(method, f'{self.base_url}{path}', headers=headers, json=json_body, timeout=timeout)
        except (httpx.TimeoutException, httpx.NetworkError) as exc:
            raise TransportRetryableError(f'Checkbox transport network error: {exc}') from exc

    @staticmethod
    def _decode_json(response: httpx.Response) -> dict[str, Any]:
        try:
            data = response.json()
        except ValueError as exc:
            raise TransportRejectedError(f'Checkbox returned invalid JSON: {response.text[:200]}') from exc
        if not isinstance(data, dict):
            raise TransportRejectedError('Checkbox returned non-object JSON response.')
        return data

    @classmethod
    def _raise_for_status(cls, response: httpx.Response) -> None:
        if response.status_code < 400:
            return
        message = cls._extract_message(response)
        if response.status_code >= 500:
            raise TransportRetryableError(message)
        raise TransportRejectedError(message)

    @classmethod
    def _extract_message(cls, response: httpx.Response) -> str:
        try:
            payload = response.json()
        except ValueError:
            payload = None
        if isinstance(payload, dict):
            for key in ('message', 'detail', 'error', 'response_error_message'):
                value = payload.get(key)
                if value:
                    return f'Checkbox HTTP {response.status_code}: {value}'
        snippet = response.text.strip()[:200]
        return f'Checkbox HTTP {response.status_code}: {snippet or "request failed"}'


class CheckboxRestTransport:
    def __init__(self, *, http_client: httpx.Client | None = None, sleep_func = time.sleep) -> None:
        self.http_client = http_client or _make_default_http_client()
        self.sleep_func = sleep_func
        self._sessions: dict[str, CheckboxAuthSession] = {}

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
        transport_profile: TransportProfileRecord | None = None,
        **kwargs,
    ) -> SendResult:
        profile = self._require_profile(transport_profile, transport_profile_id)
        decoded_request = self._decode_request_json(request_payload_json)
        payload = request_payload or self._extract_request_payload(decoded_request)
        op = self._resolve_operation(operation_type=operation_type, request_payload=request_payload, decoded_request=decoded_request)
        session = self._session_for_profile(profile)
        checkbox_id = self._stable_uuid(document_id=document_id, external_request_id=external_request_id)
        path, body, include_license_key = self._build_send_request(op=op, checkbox_id=checkbox_id, payload=payload)
        response = session.authorized_request('POST', path, json_body=body, include_license_key=include_license_key)
        response_payload = session._decode_json(response)
        state = self._map_checkbox_state(op=op, response_payload=response_payload)
        now = datetime.now(UTC)
        response_json = json.dumps(response_payload, ensure_ascii=False, separators=(',', ':'))
        return SendResult(
            state=state.value,
            transport_request_id=str(response_payload.get('id') or checkbox_id),
            submission_status=str(response_payload.get('status') or state.value),
            server_fiscal_no=self._extract_server_fiscal_no(response_payload),
            server_fiscal_date=self._extract_server_fiscal_date(response_payload),
            response_json=response_json,
            sent_at=now,
            ack_at=now if state == DocumentState.ACK else None,
        )

    def poll_status(
        self,
        *,
        document_id: str,
        fiscal_number: str,
        backend_profile_id: str,
        transport_profile_id: str,
        transport_request_id: str | None,
        operation_type: str | None = None,
        transport_profile: TransportProfileRecord | None = None,
        **kwargs,
    ) -> PollResult:
        if not transport_request_id:
            return PollResult(state=DocumentState.ERROR_RETRYABLE.value, submission_status='MISSING_TRANSPORT_REQUEST_ID', retryable=True)
        profile = self._require_profile(transport_profile, transport_profile_id)
        op = self._resolve_operation(operation_type=operation_type)
        session = self._session_for_profile(profile)
        if op in {OperationType.SHIFT_OPEN, OperationType.SHIFT_CLOSE}:
            path = f'/shifts/{transport_request_id}'
        elif op == OperationType.Z_REPORT:
            path = f'/reports/{transport_request_id}'
        else:
            path = f'/receipts/{transport_request_id}'
        response = session.authorized_request('GET', path)
        response_payload = session._decode_json(response)
        state = self._map_checkbox_state(op=op, response_payload=response_payload)
        response_json = json.dumps(response_payload, ensure_ascii=False, separators=(',', ':'))
        ack_at = None
        if state == DocumentState.ACK:
            ack_at = self._parse_dt(response_payload.get('updated_at')) or datetime.now(UTC)
        return PollResult(
            state=state.value,
            submission_status=str(response_payload.get('status') or state.value),
            server_fiscal_no=self._extract_server_fiscal_no(response_payload),
            server_fiscal_date=self._extract_server_fiscal_date(response_payload),
            response_json=response_json,
            ack_at=ack_at,
            retryable=False,
        )

    def _session_for_profile(self, profile: TransportProfileRecord) -> CheckboxAuthSession:
        cached = self._sessions.get(profile.transport_profile_id)
        if cached is not None:
            return cached
        config = self._build_profile_config(profile)
        headers = config.get('headers') if isinstance(config.get('headers'), dict) else {}
        auth = config.get('auth') if isinstance(config.get('auth'), dict) else {}
        timeouts = self._decode_json(profile.timeout_config_json)
        session = CheckboxAuthSession(
            base_url=profile.endpoint or 'https://api.checkbox.ua/api/v1',
            client_name=str(headers.get('X-Client-Name') or 'prro-gateway'),
            client_version=str(headers.get('X-Client-Version') or '1.4.1'),
            license_key=auth.get('license_key') or headers.get('X-License-Key'),
            cashier_pin=auth.get('cashier_pin'),
            cashier_login=auth.get('cashier_login'),
            cashier_password=auth.get('cashier_password'),
            verify_tls=bool(config.get('verify_tls', True)),
            connect_timeout=float(timeouts.get('connect_timeout', 5)),
            request_timeout=float(timeouts.get('request_timeout', 30)),
            http_client=self.http_client,
        )
        self._sessions[profile.transport_profile_id] = session
        return session

    @staticmethod
    def _decode_json(raw: str | None) -> dict[str, Any]:
        if not raw:
            return {}
        try:
            data = json.loads(raw)
        except ValueError:
            return {}
        return data if isinstance(data, dict) else {}

    @classmethod
    def _build_profile_config(cls, profile: TransportProfileRecord) -> dict[str, Any]:
        return cls._decode_json(profile.config_json)

    @classmethod
    def _require_profile(cls, transport_profile: TransportProfileRecord | None, transport_profile_id: str) -> TransportProfileRecord:
        if transport_profile is None:
            raise LookupError(f'Transport profile details are required for Checkbox transport: {transport_profile_id}')
        return transport_profile

    @staticmethod
    def _decode_request_json(raw: str | None) -> dict[str, Any] | None:
        if not raw:
            return None
        try:
            data = json.loads(raw)
        except ValueError:
            return None
        return data if isinstance(data, dict) else None

    @staticmethod
    def _extract_request_payload(decoded_request: dict[str, Any] | None) -> dict[str, Any] | None:
        if not decoded_request:
            return None
        payload = decoded_request.get('payload')
        if isinstance(payload, dict):
            return payload
        return decoded_request

    @classmethod
    def _resolve_operation(
        cls,
        *,
        operation_type: str | None,
        request_payload: dict[str, Any] | None = None,
        decoded_request: dict[str, Any] | None = None,
    ) -> OperationType:
        candidate = operation_type
        if not candidate and isinstance(request_payload, dict):
            payload_op = request_payload.get('operation_type') or request_payload.get('type')
            if isinstance(payload_op, str):
                candidate = payload_op
        if not candidate and isinstance(decoded_request, dict):
            decoded_op = decoded_request.get('operation_type')
            if isinstance(decoded_op, str):
                candidate = decoded_op
            else:
                payload = decoded_request.get('payload')
                if isinstance(payload, dict):
                    payload_op = payload.get('operation_type') or payload.get('type')
                    if isinstance(payload_op, str):
                        candidate = payload_op
        if not candidate:
            raise TransportRejectedError('Checkbox transport requires operation_type.')
        return OperationType(candidate)

    @staticmethod
    def _stable_uuid(*, document_id: str, external_request_id: str | None) -> str:
        candidate = (external_request_id or '').strip()
        if candidate:
            try:
                return str(__import__('uuid').UUID(candidate))
            except Exception:
                pass
        return str(uuid5(NAMESPACE_URL, f'prro-gateway:{document_id}'))

    @staticmethod
    def _compact(data: dict[str, Any]) -> dict[str, Any]:
        return {key: value for key, value in data.items() if value is not None}

    def _build_send_request(self, *, op: OperationType, checkbox_id: str, payload: dict | None) -> tuple[str, dict[str, Any] | None, bool]:
        payload = payload or {}
        receipt = payload.get('receipt') if isinstance(payload.get('receipt'), dict) else {}
        if op == OperationType.SHIFT_OPEN:
            body = {'id': checkbox_id}
            if 'fiscal_code' in payload:
                body['fiscal_code'] = payload['fiscal_code']
            if 'fiscal_date' in payload:
                body['fiscal_date'] = payload['fiscal_date']
            return '/shifts', body, True
        if op == OperationType.SHIFT_CLOSE:
            return '/shifts/close', {}, False
        if op in {OperationType.SERVICE_IN, OperationType.SERVICE_OUT}:
            value = int(payload.get('service_sum', 0))
            if op == OperationType.SERVICE_OUT and value > 0:
                value = -value
            body = self._compact({
                'id': checkbox_id,
                'payment': {'type': 'CASH', 'value': value},
                'custom': payload.get('custom'),
            })
            return '/receipts/service', body, False
        if op in {OperationType.SELL, OperationType.RETURN}:
            body = {
                'id': checkbox_id,
                'goods': [self._map_good(item, op=op) for item in (receipt.get('goods') or []) if isinstance(item, dict)],
                'payments': [self._map_payment(item) for item in (receipt.get('payments') or []) if isinstance(item, dict)],
                'delivery': receipt.get('delivery'),
                'related_receipt_id': receipt.get('related_receipt_id'),
                'technical_return': receipt.get('technical_return'),
                'rounding': receipt.get('rounding_enabled'),
                'header': receipt.get('header'),
                'footer': receipt.get('footer'),
                'barcode': receipt.get('barcode'),
                'cashier_name': payload.get('cashier_name'),
                'departament': payload.get('department') or payload.get('departament'),
            }
            return '/receipts/sell', self._compact(body), False
        if op == OperationType.Z_REPORT:
            return '/reports', {}, False
        raise TransportRejectedError(f'Checkbox transport does not yet support operation {op.value}.')

    @staticmethod
    def _map_good(item: dict[str, Any], *, op: OperationType) -> dict[str, Any]:
        good_block = CheckboxRestTransport._compact({
            'code': item.get('code'),
            'name': item.get('name'),
            'price': item.get('price'),
            'barcode': item.get('barcode'),
            'excise_barcodes': item.get('excise_barcodes') or None,
            'header': item.get('header'),
            'footer': item.get('footer'),
            'uktzed': item.get('uktzed'),
        })
        mapped = CheckboxRestTransport._compact({
            'good': good_block,
            'good_id': item.get('good_id'),
            'quantity': item.get('quantity'),
            'is_return': True if op == OperationType.RETURN else item.get('is_return'),
            'discounts': item.get('discounts') or None,
            'total_sum': item.get('sum'),
        })
        return mapped

    @staticmethod
    def _map_payment(item: dict[str, Any]) -> dict[str, Any]:
        return CheckboxRestTransport._compact({
            'type': item.get('payment_type') or item.get('type'),
            'label': item.get('label'),
            'value': item.get('amount') or item.get('value'),
            'code': item.get('payment_code') or item.get('code'),
            'provider_type': item.get('provider_type'),
            'commission': item.get('commission'),
            'card_mask': item.get('card_mask'),
            'bank_name': item.get('bank_name'),
            'auth_code': item.get('auth_code'),
            'rrn': item.get('rrn'),
            'payment_system': item.get('payment_system'),
            'owner_name': item.get('owner_name'),
            'terminal': item.get('terminal'),
            'acquirer_and_seller': item.get('acquirer_and_seller'),
            'receipt_no': item.get('receipt_no'),
            'signature_required': item.get('signature_required'),
            'tapxphone_terminal': item.get('tapxphone_terminal'),
        })

    @staticmethod
    def _map_checkbox_state(*, op: OperationType, response_payload: dict[str, Any]) -> DocumentState:
        status = str(response_payload.get('status') or '').upper()
        if op in {OperationType.SHIFT_OPEN, OperationType.SHIFT_CLOSE}:
            if status in {'OPENED', 'CLOSED'}:
                return DocumentState.ACK
            if status == 'ERROR':
                return DocumentState.REJECTED
            return DocumentState.SENT
        if status == 'DONE':
            return DocumentState.ACK
        if status == 'ERROR':
            return DocumentState.REJECTED
        return DocumentState.SENT

    @staticmethod
    def _extract_server_fiscal_no(response_payload: dict[str, Any]) -> str | None:
        value = response_payload.get('fiscal_code') or response_payload.get('serial')
        return str(value) if value is not None else None

    @staticmethod
    def _extract_server_fiscal_date(response_payload: dict[str, Any]) -> str | None:
        for key in ('fiscal_date', 'created_at', 'updated_at'):
            value = response_payload.get(key)
            if value:
                return str(value)
        return None

    @staticmethod
    def _parse_dt(value: Any) -> datetime | None:
        if not value or not isinstance(value, str):
            return None
        try:
            dt = datetime.fromisoformat(value)
        except ValueError:
            return None
        if dt.tzinfo is None:
            return dt.replace(tzinfo=UTC)
        return dt


__all__ = ['CheckboxAuthSession', 'CheckboxRestTransport']
