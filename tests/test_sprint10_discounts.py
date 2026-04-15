"""Sprint 10 Wave 2: discount/surcharge XML serialization.

Tests:
  DIS1: VALUE discount → <D> in XML with correct SM and NI
  DIS2: EXTRA_CHARGE → <S> in XML
  DIS3: PERCENT discount → TY=1, PR attribute, SM computed from item sum
  DIS4: No discounts → no <D>/<S> in XML (backward compat)
  DIS5: Multiple items, discount only on second → NI matches item N
  DIS6: Named discount → NM attribute in <D>
"""
from __future__ import annotations

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml


def _sell_payload(goods: list[dict]) -> dict:
    total = sum(int(g.get('sum', 0)) for g in goods)
    return {
        'receipt': {
            'goods': goods,
            'payments': [{'type': 'CASH', 'amount': total, 'resolved_t': '0', 'resolved_nm': 'ГОТІВКА'}],
            'totals': {'total_sum': total},
        }
    }


# ---------------------------------------------------------------------------
# DIS1 — VALUE discount → <D> with SM and NI
# ---------------------------------------------------------------------------

def test_dis1_value_discount_in_xml() -> None:
    goods = [{
        'code': '1', 'name': 'Товар', 'price': 10000, 'quantity': 1000, 'sum': 10000,
        'discounts': [{'type': 'DISCOUNT', 'mode': 'VALUE', 'value': 500}],
    }]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods),
    )
    assert '<D ' in xml, 'discount element <D> expected'
    assert 'SM="500"' in xml
    assert 'NI="1"' in xml
    assert 'TY="0"' in xml
    assert 'TR="0"' in xml


# ---------------------------------------------------------------------------
# DIS2 — EXTRA_CHARGE → <S> in XML
# ---------------------------------------------------------------------------

def test_dis2_extra_charge_uses_s_tag() -> None:
    goods = [{
        'code': '1', 'name': 'Товар', 'price': 10000, 'quantity': 1000, 'sum': 10000,
        'discounts': [{'type': 'EXTRA_CHARGE', 'mode': 'VALUE', 'value': 200}],
    }]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods),
    )
    assert '<S ' in xml, 'surcharge element <S> expected'
    assert '<D ' not in xml, 'no <D> for EXTRA_CHARGE'
    assert 'SM="200"' in xml


# ---------------------------------------------------------------------------
# DIS3 — PERCENT discount → TY=1, PR attribute, SM computed
# ---------------------------------------------------------------------------

def test_dis3_percent_discount() -> None:
    # 10000 kopecks item, 10% discount → SM = 1000
    goods = [{
        'code': '1', 'name': 'Товар', 'price': 10000, 'quantity': 1000, 'sum': 10000,
        'discounts': [{'type': 'DISCOUNT', 'mode': 'PERCENT', 'value': 10}],
    }]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods),
    )
    assert '<D ' in xml
    assert 'TY="1"' in xml
    assert 'PR="10.00"' in xml
    assert 'SM="1000"' in xml


# ---------------------------------------------------------------------------
# DIS4 — No discounts → no <D>/<S> in XML (backward compat)
# ---------------------------------------------------------------------------

def test_dis4_no_discount_no_d_tag() -> None:
    goods = [{'code': '1', 'name': 'Товар', 'price': 5000, 'quantity': 1000, 'sum': 5000}]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods),
    )
    assert '<D ' not in xml
    assert '<S ' not in xml


# ---------------------------------------------------------------------------
# DIS5 — Multiple items, discount only on second → NI matches item N
# ---------------------------------------------------------------------------

def test_dis5_second_item_discount_has_correct_ni() -> None:
    goods = [
        {'code': '1', 'name': 'Без знижки', 'price': 5000, 'quantity': 1000, 'sum': 5000},
        {
            'code': '2', 'name': 'Зі знижкою', 'price': 8000, 'quantity': 1000, 'sum': 8000,
            'discounts': [{'type': 'DISCOUNT', 'mode': 'VALUE', 'value': 300}],
        },
    ]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods),
    )
    assert '<D ' in xml
    # Second item has N="2", so discount NI must be "2"
    assert 'NI="2"' in xml
    assert 'NI="1"' not in xml


# ---------------------------------------------------------------------------
# DIS6 — Named discount → NM attribute in <D>
# ---------------------------------------------------------------------------

def test_dis6_named_discount_has_nm() -> None:
    goods = [{
        'code': '1', 'name': 'Товар', 'price': 10000, 'quantity': 1000, 'sum': 10000,
        'discounts': [{'type': 'DISCOUNT', 'mode': 'VALUE', 'value': 100, 'name': 'Знижка постійного клієнта'}],
    }]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods),
    )
    assert 'NM="Знижка постійного клієнта"' in xml
