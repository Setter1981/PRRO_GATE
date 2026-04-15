"""
Gate 1i — Checkbox adapter schema smoke tests.

Verifies that CheckboxRestAdapter.map_command() produces a *full* canonical
envelope with all mandatory schema fields — not a summary-only shortcut.

Invariants covered:
  #6 (adapters must build full canonical payloads): verified by checking every
      required block (receipt, goods, payments, totals, top-level meta).
  #7 (all canonical envelopes must carry schema_version): directly asserted.

Tests are intentionally additive to the existing golden-contract and field-spot
tests in test_adapters.py.  They focus on the structural invariants that would
be violated by a summary-only shortcut:
  - schema_version present and equal to the SCHEMA_VERSION constant
  - top-level payload meta fields (currency, cashier_id, goods_count, payments_count)
  - receipt block: type, goods, payments, totals all non-empty / structurally valid
  - per-good fields used downstream: name, price, quantity, sum, excise_barcodes list
  - per-payment fields used downstream: payment_type, amount, payment_id
  - requires_shift flag correct for SELL and RETURN
  - RETURN-specific: receipt.type == 'RETURN', related_receipt_id preserved
  - idempotency_key derivation: ext_id path and sha256 fallback
"""
from __future__ import annotations

from prro_gateway.adapters import CheckboxRestAdapter
from prro_gateway.constants import SCHEMA_VERSION
from prro_gateway.enums import OperationType

_ADAPTER = CheckboxRestAdapter()

FISCAL = 'FN-GATE1I-001'
_BASE_CTX = {
    'request_id': 'req-gate1i',
    'fiscal_number': FISCAL,
    'business_ts': '2026-03-29T18:00:00Z',
}


def _raw_sell(*, ext_id: str | None = 'ext-sell-gate1i', extra: dict | None = None) -> dict:
    req: dict = {
        'external_request_id': ext_id,
        'cashier_id': 'cashier-gate1i',
        'goods': [
            {
                'name': 'Coffee',
                'code': 'COFFEE-001',
                'price': 5000,
                'quantity': 2000,
                'sum': 10000,
                'excise_barcodes': ['EX-BAR-001'],
            }
        ],
        'payments': [
            {'type': 'CASH', 'amount': 10000}
        ],
    }
    if extra:
        req.update(extra)
    return {'context': _BASE_CTX, 'operation': 'SELL', 'request': req}


def _raw_return(*, related_receipt_id: str = 'rcpt-orig-001') -> dict:
    return {
        'context': _BASE_CTX,
        'operation': 'RETURN',
        'request': {
            'external_request_id': 'ext-return-gate1i',
            'cashier_id': 'cashier-gate1i',
            'related_receipt_id': related_receipt_id,
            'goods': [
                {
                    'name': 'Coffee',
                    'price': 5000,
                    'quantity': 1000,
                    'sum': 5000,
                    'is_return': True,
                }
            ],
            'payments': [
                {'type': 'CASH', 'amount': 5000}
            ],
        },
    }


# ---------------------------------------------------------------------------
# schema_version — Invariant #7
# ---------------------------------------------------------------------------

def test_gate1i_sell_has_schema_version() -> None:
    """schema_version must be present on the canonical envelope and match the constant."""
    cmd = _ADAPTER.map_command(_raw_sell())
    assert cmd.schema_version == SCHEMA_VERSION, (
        f'Expected schema_version={SCHEMA_VERSION!r}, got {cmd.schema_version!r}'
    )


def test_gate1i_return_has_schema_version() -> None:
    cmd = _ADAPTER.map_command(_raw_return())
    assert cmd.schema_version == SCHEMA_VERSION, (
        f'RETURN: expected schema_version={SCHEMA_VERSION!r}, got {cmd.schema_version!r}'
    )


# ---------------------------------------------------------------------------
# Full payload structure — Invariant #6
# ---------------------------------------------------------------------------

def test_gate1i_sell_top_level_payload_fields() -> None:
    """
    Top-level payload fields must all be present (not a summary-only shortcut).
    Downstream write-path reads currency, cashier_id, goods_count, payments_count.
    """
    cmd = _ADAPTER.map_command(_raw_sell())
    p = cmd.payload
    assert p.get('currency') == 'UAH', f'currency missing or wrong: {p.get("currency")!r}'
    assert p.get('cashier_id') == 'cashier-gate1i', f'cashier_id: {p.get("cashier_id")!r}'
    assert p.get('goods_count') == 1, f'goods_count: {p.get("goods_count")!r}'
    assert p.get('payments_count') == 1, f'payments_count: {p.get("payments_count")!r}'


def test_gate1i_sell_receipt_block_complete() -> None:
    """receipt block must contain type, goods, payments, totals — no omitted sections."""
    cmd = _ADAPTER.map_command(_raw_sell())
    receipt = cmd.payload['receipt']
    assert receipt.get('type') == 'SELL', f'receipt.type: {receipt.get("type")!r}'
    assert isinstance(receipt.get('goods'), list) and len(receipt['goods']) == 1, (
        f'receipt.goods: {receipt.get("goods")!r}'
    )
    assert isinstance(receipt.get('payments'), list) and len(receipt['payments']) == 1, (
        f'receipt.payments: {receipt.get("payments")!r}'
    )
    totals = receipt.get('totals')
    assert isinstance(totals, dict) and totals.get('total_sum') is not None, (
        f'receipt.totals missing or empty: {totals!r}'
    )


def test_gate1i_sell_good_fields_complete() -> None:
    """Per-good canonical fields used by write-path/transport must all be present."""
    cmd = _ADAPTER.map_command(_raw_sell())
    good = cmd.payload['receipt']['goods'][0]
    # Fields used downstream
    assert good.get('name') == 'Coffee', f'good.name: {good.get("name")!r}'
    assert good.get('price') == 5000, f'good.price: {good.get("price")!r}'
    assert good.get('quantity') == 2000, f'good.quantity: {good.get("quantity")!r}'
    assert good.get('sum') == 10000, f'good.sum: {good.get("sum")!r}'
    assert isinstance(good.get('excise_barcodes'), list), (
        f'excise_barcodes must be a list: {good.get("excise_barcodes")!r}'
    )
    assert good['excise_barcodes'] == ['EX-BAR-001'], (
        f'excise_barcodes not preserved: {good["excise_barcodes"]!r}'
    )
    # item_id and item_no must be auto-populated when not supplied
    assert good.get('item_id') is not None, 'item_id must be auto-populated'
    assert good.get('item_no') is not None, 'item_no must be auto-populated'


def test_gate1i_sell_payment_fields_complete() -> None:
    """Per-payment canonical fields used by write-path/transport must all be present."""
    cmd = _ADAPTER.map_command(_raw_sell())
    payment = cmd.payload['receipt']['payments'][0]
    assert payment.get('payment_type') == 'CASH', (
        f'payment_type: {payment.get("payment_type")!r}'
    )
    assert payment.get('amount') == 10000, f'amount: {payment.get("amount")!r}'
    assert payment.get('payment_id') is not None, 'payment_id must be auto-populated'


def test_gate1i_sell_totals_auto_computed_when_absent() -> None:
    """
    When request.totals is not supplied, totals must be derived from goods/payments.
    A summary-only shortcut might omit this computation.
    """
    cmd = _ADAPTER.map_command(_raw_sell())
    totals = cmd.payload['receipt']['totals']
    assert totals['total_sum'] == 10000, (
        f'total_sum should equal goods sum (5000*2), got {totals["total_sum"]!r}'
    )


def test_gate1i_sell_requires_shift_true() -> None:
    """SELL requires an open shift — requires_shift must be True."""
    cmd = _ADAPTER.map_command(_raw_sell())
    assert cmd.requires_shift is True, (
        f'SELL must have requires_shift=True, got {cmd.requires_shift!r}'
    )


def test_gate1i_sell_operation_type() -> None:
    cmd = _ADAPTER.map_command(_raw_sell())
    assert cmd.operation_type == OperationType.SELL, (
        f'operation_type: {cmd.operation_type!r}'
    )


# ---------------------------------------------------------------------------
# RETURN-specific semantics
# ---------------------------------------------------------------------------

def test_gate1i_return_receipt_type_is_return() -> None:
    """receipt.type must be 'RETURN', not 'SELL'."""
    cmd = _ADAPTER.map_command(_raw_return())
    receipt = cmd.payload['receipt']
    assert receipt.get('type') == 'RETURN', (
        f'RETURN: receipt.type must be RETURN, got {receipt.get("type")!r}'
    )


def test_gate1i_return_related_receipt_id_preserved() -> None:
    """related_receipt_id must survive the adapter mapping (not dropped)."""
    cmd = _ADAPTER.map_command(_raw_return(related_receipt_id='rcpt-orig-gate1i'))
    receipt = cmd.payload['receipt']
    assert receipt.get('related_receipt_id') == 'rcpt-orig-gate1i', (
        f'RETURN: related_receipt_id not preserved: {receipt.get("related_receipt_id")!r}'
    )


def test_gate1i_return_is_return_flag_on_good() -> None:
    """Return goods should carry is_return=True through the mapping."""
    cmd = _ADAPTER.map_command(_raw_return())
    good = cmd.payload['receipt']['goods'][0]
    assert good.get('is_return') is True, (
        f'RETURN good: is_return must be True, got {good.get("is_return")!r}'
    )


def test_gate1i_return_requires_shift_true() -> None:
    cmd = _ADAPTER.map_command(_raw_return())
    assert cmd.requires_shift is True, (
        f'RETURN must have requires_shift=True, got {cmd.requires_shift!r}'
    )


def test_gate1i_return_operation_type() -> None:
    cmd = _ADAPTER.map_command(_raw_return())
    assert cmd.operation_type == OperationType.RETURN, (
        f'operation_type: {cmd.operation_type!r}'
    )


# ---------------------------------------------------------------------------
# Idempotency key derivation
# ---------------------------------------------------------------------------

def test_gate1i_idempotency_key_uses_external_request_id_when_present() -> None:
    """When external_request_id is provided, idempotency_key must use it (not sha256)."""
    cmd = _ADAPTER.map_command(_raw_sell(ext_id='ext-idem-gate1i'))
    expected = f'sell:{FISCAL.lower()}:ext-idem-gate1i'
    assert cmd.idempotency_key == expected, (
        f'Expected idempotency_key={expected!r}, got {cmd.idempotency_key!r}'
    )


def test_gate1i_idempotency_key_falls_back_to_sha256_when_no_ext_id() -> None:
    """When external_request_id is absent, idempotency_key must use payload sha256."""
    cmd = _ADAPTER.map_command(_raw_sell(ext_id=None))
    # Key must follow pattern op:fiscal:<sha256> (not contain a literal 'None')
    parts = cmd.idempotency_key.split(':')
    assert len(parts) == 3, f'Expected op:fiscal:hash format, got {cmd.idempotency_key!r}'
    assert parts[0] == 'sell'
    assert parts[1] == FISCAL.lower()
    assert len(parts[2]) == 64, (
        f'Third segment should be a sha256 hex (64 chars), got {parts[2]!r}'
    )
