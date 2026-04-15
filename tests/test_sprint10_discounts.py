"""Sprint 10 Wave 2: discount/surcharge XML serialization.

Tests:
  DIS1: VALUE discount → <D> in XML with correct SM and NI
  DIS2: EXTRA_CHARGE → <S> in XML
  DIS3: PERCENT discount → TY=1, PR attribute, SM computed from item sum
  DIS4: No discounts → no <D>/<S> in XML (backward compat)
  DIS5: Multiple items, discount only on second → NI matches item N
  DIS6: Named discount → NM attribute in <D>
  DIS7: Check-level discount → <D TR="1"> with <NI> for all items
  DIS8: Check-level PERCENT discount → TY=1, SM computed from goods total
  DIS9: Check-level surcharge → <S TR="1">
  DIS10: Per-item and check-level discounts coexist in same check
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


# ---------------------------------------------------------------------------
# DIS7 — Check-level VALUE discount → <D TR="1"> with <NI> for all items
# ---------------------------------------------------------------------------

def test_dis7_check_level_value_discount() -> None:
    goods = [
        {'code': '1', 'name': 'Молоко', 'price': 3000, 'quantity': 1000, 'sum': 3000},
        {'code': '2', 'name': 'Хліб', 'price': 1500, 'quantity': 1000, 'sum': 1500},
    ]
    payload = _sell_payload(goods)
    payload['receipt']['discounts'] = [{'type': 'DISCOUNT', 'mode': 'VALUE', 'value': 450}]

    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=payload,
    )
    assert '<D ' in xml
    assert 'TR="1"' in xml
    assert 'SM="450"' in xml
    assert 'TY="0"' in xml
    # Both items referenced via <NI>
    assert '<NI NI="1">' in xml
    assert '<NI NI="2">' in xml


# ---------------------------------------------------------------------------
# DIS8 — Check-level PERCENT discount → TY=1, SM from goods total
# ---------------------------------------------------------------------------

def test_dis8_check_level_percent_discount() -> None:
    # goods total = 10000, 10% → SM = 1000
    goods = [
        {'code': '1', 'name': 'Товар А', 'price': 6000, 'quantity': 1000, 'sum': 6000},
        {'code': '2', 'name': 'Товар Б', 'price': 4000, 'quantity': 1000, 'sum': 4000},
    ]
    payload = _sell_payload(goods)
    payload['receipt']['discounts'] = [{'type': 'DISCOUNT', 'mode': 'PERCENT', 'value': 10}]

    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=payload,
    )
    assert 'TR="1"' in xml
    assert 'TY="1"' in xml
    assert 'PR="10.00"' in xml
    assert 'SM="1000"' in xml


# ---------------------------------------------------------------------------
# DIS9 — Check-level surcharge → <S TR="1">
# ---------------------------------------------------------------------------

def test_dis9_check_level_surcharge() -> None:
    goods = [{'code': '1', 'name': 'Товар', 'price': 5000, 'quantity': 1000, 'sum': 5000}]
    payload = _sell_payload(goods)
    payload['receipt']['discounts'] = [{'type': 'EXTRA_CHARGE', 'mode': 'VALUE', 'value': 200}]

    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=payload,
    )
    assert '<S ' in xml
    assert 'TR="1"' in xml
    assert 'SM="200"' in xml
    assert '<NI NI="1">' in xml


# ---------------------------------------------------------------------------
# DIS10 — Per-item and check-level discounts coexist
# ---------------------------------------------------------------------------

def test_dis10_per_item_and_check_level_coexist() -> None:
    goods = [
        {
            'code': '1', 'name': 'Товар', 'price': 10000, 'quantity': 1000, 'sum': 10000,
            'discounts': [{'type': 'DISCOUNT', 'mode': 'VALUE', 'value': 500}],
        },
        {'code': '2', 'name': 'Інший', 'price': 5000, 'quantity': 1000, 'sum': 5000},
    ]
    payload = _sell_payload(goods)
    payload['receipt']['discounts'] = [{'type': 'DISCOUNT', 'mode': 'VALUE', 'value': 300}]

    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=payload,
    )
    # Per-item discount: TR="0", NI attr (not sub-element)
    assert 'TR="0"' in xml
    assert 'NI="1"' in xml
    # Check-level discount: TR="1", <NI> sub-elements
    # Item 1 → N=1, per-item discount → N=2, Item 2 → N=3
    assert 'TR="1"' in xml
    assert '<NI NI="1">' in xml
    assert '<NI NI="3">' in xml
