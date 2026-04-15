"""Sprint 10 Wave 2: <L> text comment and <EPZ> Z-report serialization.

Tests:
  L1: Receipt header → <L> before first <P>
  L2: Receipt footer → <L> after payments, before <E>
  L3: Multiline header → multiple consecutive <L> elements
  L4: No header/footer → no <L> elements (backward compat)
  L5: SERVICE_IN with header text → <L> in service check
  EPZ1: Z-report with epz_sums → <EPZ> element present
  EPZ2: Z-report without epz_sums → no <EPZ>
  EPZ3: <EPZ> attributes EPC, EPCS, EPSM are correct
"""
from __future__ import annotations

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml


def _sell_payload(goods: list[dict], header: str | None = None, footer: str | None = None) -> dict:
    total = sum(int(g.get('sum', 0)) for g in goods)
    receipt: dict = {
        'goods': goods,
        'payments': [{'type': 'CASH', 'amount': total, 'resolved_t': '0', 'resolved_nm': 'ГОТІВКА'}],
        'totals': {'total_sum': total},
    }
    if header is not None:
        receipt['header'] = header
    if footer is not None:
        receipt['footer'] = footer
    return {'receipt': receipt}


# ---------------------------------------------------------------------------
# L1 — Receipt header → <L> before first <P>
# ---------------------------------------------------------------------------

def test_l1_header_before_items() -> None:
    goods = [{'code': '1', 'name': 'Товар', 'price': 5000, 'quantity': 1000, 'sum': 5000}]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods, header='Дякуємо за покупку!'),
    )
    assert '<L ' in xml, '<L> element expected'
    # L must appear before first <P>
    l_pos = xml.index('<L ')
    p_pos = xml.index('<P ')
    assert l_pos < p_pos, '<L> must precede first <P>'
    assert 'Дякуємо за покупку!' in xml


# ---------------------------------------------------------------------------
# L2 — Receipt footer → <L> after payments, before <E>
# ---------------------------------------------------------------------------

def test_l2_footer_after_payments() -> None:
    goods = [{'code': '1', 'name': 'Товар', 'price': 5000, 'quantity': 1000, 'sum': 5000}]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods, footer='Приходьте ще!'),
    )
    assert '<L ' in xml
    # L must appear after last <M> and before <E>
    m_pos = xml.rindex('</M>')
    l_pos = xml.index('<L ')
    e_pos = xml.index('<E ')
    assert m_pos < l_pos, '<L> footer must appear after </M>'
    assert l_pos < e_pos, '<L> footer must appear before <E>'
    assert 'Приходьте ще!' in xml


# ---------------------------------------------------------------------------
# L3 — Multiline header → multiple <L> elements
# ---------------------------------------------------------------------------

def test_l3_multiline_header_multiple_l_tags() -> None:
    goods = [{'code': '1', 'name': 'Товар', 'price': 1000, 'quantity': 1000, 'sum': 1000}]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods, header='Рядок 1\nРядок 2\nРядок 3'),
    )
    assert xml.count('<L ') == 3, 'three <L> elements expected for three-line header'
    assert 'Рядок 1' in xml
    assert 'Рядок 2' in xml
    assert 'Рядок 3' in xml


# ---------------------------------------------------------------------------
# L4 — No header/footer → no <L> elements (backward compat)
# ---------------------------------------------------------------------------

def test_l4_no_header_footer_no_l_tag() -> None:
    goods = [{'code': '1', 'name': 'Товар', 'price': 5000, 'quantity': 1000, 'sum': 5000}]
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=1,
        payload=_sell_payload(goods),
    )
    assert '<L ' not in xml, 'no <L> without header/footer'


# ---------------------------------------------------------------------------
# L5 — SERVICE_IN with header → <L> in service check
# ---------------------------------------------------------------------------

def test_l5_service_in_with_header() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SERVICE_IN,
        fiscal_number='FN-001',
        local_number=5,
        payload={'service_sum': 10000, 'receipt': {'header': 'Поповнення каси'}},
    )
    assert '<L ' in xml
    assert 'Поповнення каси' in xml


# ---------------------------------------------------------------------------
# EPZ1 — Z-report with epz_sums → <EPZ> present
# ---------------------------------------------------------------------------

def test_epz1_z_report_with_epz_sums() -> None:
    payload = {
        'z_report_data': {
            'epz_sums': {'epc': 3, 'epcs': 150, 'epsm': 45000},
            'check_count': {'ni': 10, 'no': 2},
        }
    }
    xml = build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-001',
        local_number=10,
        z_number=5,
        payload=payload,
    )
    assert '<EPZ ' in xml, '<EPZ> expected when epz_sums present'


# ---------------------------------------------------------------------------
# EPZ2 — Z-report without epz_sums → no <EPZ>
# ---------------------------------------------------------------------------

def test_epz2_z_report_without_epz_no_tag() -> None:
    payload = {
        'z_report_data': {
            'check_count': {'ni': 5, 'no': 1},
        }
    }
    xml = build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-001',
        local_number=8,
        z_number=4,
        payload=payload,
    )
    assert '<EPZ' not in xml


# ---------------------------------------------------------------------------
# EPZ3 — <EPZ> attributes EPC, EPCS, EPSM are correct
# ---------------------------------------------------------------------------

def test_epz3_epz_attributes_correct() -> None:
    payload = {
        'z_report_data': {
            'epz_sums': {'epc': 7, 'epcs': 300, 'epsm': 120000},
            'check_count': {'ni': 15, 'no': 0},
        }
    }
    xml = build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-001',
        local_number=12,
        z_number=6,
        payload=payload,
    )
    assert 'EPC="7"' in xml
    assert 'EPCS="300"' in xml
    assert 'EPSM="120000"' in xml
