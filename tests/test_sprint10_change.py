"""Sprint 10 step 6: Change (RM) on cash payment.

Tests:
  RM1: change in XML — RM on cash M
  RM2: mixed payment — RM only on cash M, not on card
  RM3: card-only — no RM
  RM4: exact payment — no RM (change == 0)
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml


def _sell_xml(payments, total_sum=5000):
    return build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=1,
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Item', 'price': total_sum, 'quantity': 1000, 'sum': total_sum}],
                'payments': payments,
                'totals': {'total_sum': total_sum},
            },
        },
        tax_number='TN',
    )


# ---------------------------------------------------------------------------
# RM1 — change in XML
# ---------------------------------------------------------------------------

def test_rm1_change_in_xml() -> None:
    xml = _sell_xml([{'amount': 18000, 'type': 'CASH'}], total_sum=5000)
    assert 'RM="13000"' in xml, f'RM must be 13000 (18000-5000). Got: {xml}'
    # RM between NM and SM in alphabetical order
    rm_pos = xml.index('RM="13000"')
    nm_pos = xml.index('NM="CASH"')
    sm_pos = xml.index('SM="18000"')
    assert nm_pos < rm_pos < sm_pos, f'RM must be between NM and SM (alphabetical). Got: {xml}'


# ---------------------------------------------------------------------------
# RM2 — mixed payment: RM only on cash
# ---------------------------------------------------------------------------

def test_rm2_mixed_payment_rm_on_cash_only() -> None:
    xml = _sell_xml([
        {'amount': 7000, 'type': 'CARD'},
        {'amount': 5000, 'type': 'CASH'},
    ], total_sum=10000)
    # total payments = 12000, change = 2000
    assert 'RM="2000"' in xml, f'RM must be 2000. Got: {xml}'

    # RM only on cash M (T="0"), not on card M (T="2")
    import re
    m_tags = re.findall(r'<M [^>]+>', xml)
    assert len(m_tags) == 2
    cash_m = [m for m in m_tags if 'T="0"' in m]
    card_m = [m for m in m_tags if 'T="2"' in m]
    assert len(cash_m) == 1 and 'RM=' in cash_m[0], f'Cash M must have RM. Got: {cash_m}'
    assert len(card_m) == 1 and 'RM=' not in card_m[0], f'Card M must NOT have RM. Got: {card_m}'


# ---------------------------------------------------------------------------
# RM3 — card only: no RM
# ---------------------------------------------------------------------------

def test_rm3_card_only_no_rm() -> None:
    xml = _sell_xml([{'amount': 5000, 'type': 'CARD'}], total_sum=5000)
    assert 'RM=' not in xml, f'Card-only must not have RM. Got: {xml}'


# ---------------------------------------------------------------------------
# RM4 — exact payment: no RM
# ---------------------------------------------------------------------------

def test_rm4_exact_payment_no_rm() -> None:
    xml = _sell_xml([{'amount': 5000, 'type': 'CASH'}], total_sum=5000)
    assert 'RM=' not in xml, f'Exact payment must not have RM (not RM="0"). Got: {xml}'


# ---------------------------------------------------------------------------
# RM5 — multiple cash payments: RM only on first
# ---------------------------------------------------------------------------

def test_rm5_multiple_cash_rm_on_first_only() -> None:
    xml = _sell_xml([
        {'amount': 3000, 'type': 'CASH'},
        {'amount': 4000, 'type': 'CASH'},
    ], total_sum=5000)
    # total payments = 7000, change = 2000
    import re
    m_tags = re.findall(r'<M [^>]+>', xml)
    cash_ms = [m for m in m_tags if 'T="0"' in m]
    assert len(cash_ms) == 2, f'Expected 2 cash M tags. Got: {cash_ms}'
    assert 'RM="2000"' in cash_ms[0], f'First cash M must have RM. Got: {cash_ms[0]}'
    assert 'RM=' not in cash_ms[1], f'Second cash M must NOT have RM. Got: {cash_ms[1]}'
