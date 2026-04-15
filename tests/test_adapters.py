from __future__ import annotations

import json
from pathlib import Path

import pytest

from prro_gateway.adapters import AdapterMappingError, CheckboxRestAdapter, MariaTcpAdapter, WebCheckXmlRpcAdapter

ROOT = Path(__file__).resolve().parents[1]

ADAPTERS = {
    'CHECKBOX_REST': CheckboxRestAdapter(),
    'WEBCHECK_XMLRPC': WebCheckXmlRpcAdapter(),
    'MARIA_TCP': MariaTcpAdapter(),
}


def _load_json(path: Path):
    return json.loads(path.read_text(encoding='utf-8'))


@pytest.mark.parametrize('golden_entry', _load_json(ROOT / 'golden' / 'manifest.json')['golden'], ids=lambda item: item['name'])
def test_adapter_golden_contracts(golden_entry):
    adapter = ADAPTERS[golden_entry['adapter']]
    raw = _load_json(ROOT / golden_entry['raw_request'])
    expected = _load_json(ROOT / golden_entry['expected_canonical'])
    actual = adapter.map_command(raw).model_dump(mode='json')
    assert actual == expected


def test_webcheck_unsupported_method_raises():
    adapter = WebCheckXmlRpcAdapter()
    raw = {
        'context': {
            'request_id': 'cmd-unsupported',
            'fiscal_number': 'FN300000001',
            'business_ts': '2026-03-28T12:00:00Z',
        },
        'method': 'UnknownMethod',
        'params': {},
    }
    with pytest.raises(AdapterMappingError) as exc:
        adapter.map_command(raw)
    assert exc.value.code == 'UNSUPPORTED_METHOD'



def test_maria_unsupported_command_raises():
    adapter = MariaTcpAdapter()
    raw = {
        'context': {
            'request_id': 'cmd-unsupported',
            'fiscal_number': 'FN300000001',
            'business_ts': '2026-03-28T12:00:00Z',
        },
        'command': 'DISPLAY_STATUS',
        'fields': {},
    }
    with pytest.raises(AdapterMappingError) as exc:
        adapter.map_command(raw)
    assert exc.value.code == 'UNSUPPORTED_METHOD'


def test_checkbox_rest_payload_contains_full_receipt_block():
    adapter = CheckboxRestAdapter()
    raw = _load_json(ROOT / 'golden' / 'checkbox_rest_sell_online' / 'raw_request.json')
    cmd = adapter.map_command(raw)
    receipt = cmd.payload['receipt']
    assert receipt['goods'][0]['code'] == 'PARKING'
    assert receipt['payments'][0]['payment_type'] == 'CASHLESS'


def test_checkbox_rest_return_contains_related_receipt_and_delivery():
    adapter = CheckboxRestAdapter()
    raw = _load_json(ROOT / 'golden' / 'checkbox_rest_return' / 'raw_request.json')
    cmd = adapter.map_command(raw)
    receipt = cmd.payload['receipt']
    assert receipt['related_receipt_id'] == 'rcpt-0001'
    assert receipt['delivery']['email'] == 'buyer@example.com'


def test_write_path_extractors_see_adapter_payload():
    from prro_gateway.services.write_path import WritePathWorker
    adapter = CheckboxRestAdapter()
    raw = {
        'context': {
            'request_id': 'cmd-excise',
            'fiscal_number': 'FN300000001',
            'business_ts': '2026-03-28T12:00:00Z',
        },
        'operation': 'SELL',
        'request': {
            'goods': [
                {
                    'good': {'code': 'ALC001', 'name': 'Wine', 'excise_barcodes': ['AB-123']},
                    'price': 10000,
                    'quantity': 1000
                }
            ],
            'payments': [{'type': 'CASHLESS', 'value': 10000, 'label': 'CARD'}]
        }
    }
    cmd = adapter.map_command(raw)
    marks = WritePathWorker._extract_excise_marks(cmd)
    meta = WritePathWorker._extract_receipt_metadata(cmd)
    assert marks and marks[0]['normalized_mark_code'] == 'AB123'
    assert meta['total_sum'] == 10000
