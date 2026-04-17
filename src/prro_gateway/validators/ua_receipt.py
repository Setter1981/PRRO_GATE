"""
Ukrainian fiscal receipt validator — SELL/RETURN/SERVICE/CASH/excise consistency checks.

Scope:
  SELL/RETURN (Sprint 3 step 1):
    - goods non-empty, payments non-empty
    - line sum, total, payment consistency

  SERVICE_IN/SERVICE_OUT (Sprint 3 step 2):
    - service_sum present and > 0
    - if payments present: sum(payments.amount) == service_sum

  CASH_WITHDRAWAL (Sprint 3 step 2):
    - cash_withdrawal_sum present and > 0
    - payments non-empty
    - sum(payments.amount) == cash_withdrawal_sum
    - if totals present: totals.total_sum == cash_withdrawal_sum

  Excise marks — SELL only (Sprint 3 step 3):
    - if excise_barcodes present on a goods item, uktzed must be non-empty
    - each barcode must normalize to non-empty alphanumeric string
    - no intra-receipt duplicate marks

Returns a list of violation strings. Empty list = valid.
"""
from __future__ import annotations

from typing import Any


def validate_sell_return_receipt(payload: dict[str, Any]) -> list[str]:
    """Validate SELL/RETURN receipt payload for Ukrainian fiscal consistency.

    Works against the raw payload dict from CanonicalFiscalCommand.payload.
    Returns list of human-readable violation strings (empty = valid).
    """
    errors: list[str] = []
    receipt = payload.get('receipt')
    if not isinstance(receipt, dict):
        errors.append('receipt object is missing')
        return errors

    goods = receipt.get('goods')
    payments = receipt.get('payments')

    # --- goods non-empty ---
    if not isinstance(goods, list) or len(goods) == 0:
        errors.append('goods must be non-empty')
    else:
        # --- line sum consistency ---
        for i, item in enumerate(goods):
            if not isinstance(item, dict):
                continue
            price = item.get('price')
            quantity = item.get('quantity')
            line_sum = item.get('sum')
            if price is not None and quantity is not None and line_sum is not None:
                expected = price * quantity // 1000
                if line_sum != expected:
                    name = item.get('name', f'item #{i+1}')
                    errors.append(
                        f'good "{name}": sum={line_sum} != price({price}) * quantity({quantity}) / 1000 = {expected}'
                    )

    # --- payments non-empty ---
    if not isinstance(payments, list) or len(payments) == 0:
        errors.append('payments must be non-empty')

    # --- total / payment consistency ---
    totals = receipt.get('totals')
    has_totals = isinstance(totals, dict) and totals.get('total_sum') is not None
    has_goods = isinstance(goods, list) and len(goods) > 0
    has_payments = isinstance(payments, list) and len(payments) > 0

    if has_goods:
        goods_sum = sum(int(item.get('sum', 0)) for item in goods if isinstance(item, dict))
        total_discounts = 0
        total_extra_charges = 0
        
        # Line-level discounts
        for item in goods:
            if not isinstance(item, dict):
                continue
            line_sum = int(item.get('sum', 0))
            for d in (item.get('discounts') or []):
                if not isinstance(d, dict):
                    continue
                v = int(d.get('value', 0))
                d_mode = d.get('mode', 'VALUE')
                d_sm = round(line_sum * v / 100) if d_mode == 'PERCENT' else v
                if d.get('type', 'DISCOUNT') == 'EXTRA_CHARGE':
                    total_extra_charges += d_sm
                else:
                    total_discounts += d_sm
                    
        # Check-level discounts
        check_discounts = receipt.get('discounts') or []
        for d in check_discounts:
            if not isinstance(d, dict):
                continue
            v = int(d.get('value', 0))
            d_mode = d.get('mode', 'VALUE')
            d_sm = round(goods_sum * v / 100) if d_mode == 'PERCENT' else v
            if d.get('type', 'DISCOUNT') == 'EXTRA_CHARGE':
                total_extra_charges += d_sm
            else:
                total_discounts += d_sm
                
        expected_total_sum = goods_sum - total_discounts + total_extra_charges
    else:
        expected_total_sum = 0

    if has_totals:
        total_sum = totals['total_sum']

        if has_goods:
            if expected_total_sum != total_sum:
                errors.append(
                    f'totals.total_sum={total_sum} != goods_sum({goods_sum}) - discounts({total_discounts}) + extra({total_extra_charges}) = {expected_total_sum}'
                )

        if has_payments:
            payments_sum = sum(int(item.get('amount', 0)) for item in payments if isinstance(item, dict))
            if payments_sum < total_sum:
                # Underpayment is invalid. Overpayment (payments > total) is allowed —
                # the difference is change (RM) returned to the customer.
                errors.append(
                    f'underpayment: totals.total_sum={total_sum} > sum(payments.amount)={payments_sum}'
                )
    elif has_goods and has_payments:
        # Fallback: no totals — payments must not be less than expected_total_sum
        payments_sum = sum(int(item.get('amount', 0)) for item in payments if isinstance(item, dict))
        if payments_sum < expected_total_sum:
            errors.append(
                f'underpayment: sum(payments.amount)={payments_sum} < expected_total={expected_total_sum} (no totals provided)'
            )

    return errors


def validate_excise_marks(payload: dict[str, Any]) -> list[str]:
    """Validate excise mark fields for SELL receipts.

    For each goods item that carries excise_barcodes:
      - uktzed must be present and non-empty (Ukrainian commodity code required for excise goods)
      - each barcode must normalize to a non-empty alphanumeric string
      - no duplicate marks within the same receipt
    """
    errors: list[str] = []
    receipt = payload.get('receipt')
    if not isinstance(receipt, dict):
        return errors
    goods = receipt.get('goods')
    if not isinstance(goods, list):
        return errors

    seen_marks: set[str] = set()
    for i, item in enumerate(goods):
        if not isinstance(item, dict):
            continue
        barcodes = item.get('excise_barcodes')
        if not isinstance(barcodes, list) or len(barcodes) == 0:
            continue

        name = item.get('name', f'item #{i+1}')

        # uktzed required for excise goods
        uktzed = item.get('uktzed')
        if not uktzed or not isinstance(uktzed, str) or not uktzed.strip():
            errors.append(f'good "{name}": uktzed is required when excise_barcodes are present')

        # validate each barcode
        for j, raw in enumerate(barcodes):
            if raw is None or not isinstance(raw, (str, int, float)):
                errors.append(f'good "{name}": excise_barcodes[{j}] is invalid (null or wrong type)')
                continue
            normalized = ''.join(ch for ch in str(raw).strip().upper() if ch.isalnum())
            if not normalized:
                errors.append(f'good "{name}": excise_barcodes[{j}] ("{raw}") normalizes to empty string')
                continue
            if normalized in seen_marks:
                errors.append(f'good "{name}": duplicate excise mark "{normalized}" within receipt')
            seen_marks.add(normalized)

    return errors


def validate_service_receipt(payload: dict[str, Any]) -> list[str]:
    """Validate SERVICE_IN/SERVICE_OUT payload.

    service_sum must be present and > 0 (direction is determined by operation_type,
    not by sign — adapter always stores positive).  If receipt.payments is present
    and non-empty, sum(payments.amount) must equal service_sum.

    Note: for Checkbox REST, receipt.payments is typically empty — the transport
    builds its own CASH payment from service_sum.  Payments are not required.
    """
    errors: list[str] = []
    service_sum = payload.get('service_sum')
    if service_sum is None or not isinstance(service_sum, (int, float)) or service_sum <= 0:
        errors.append('service_sum must be present and > 0')
        return errors

    receipt = payload.get('receipt')
    if not isinstance(receipt, dict):
        errors.append('receipt object is missing')
        return errors

    payments = receipt.get('payments')
    if isinstance(payments, list) and len(payments) > 0:
        payments_sum = sum(int(item.get('amount', 0)) for item in payments if isinstance(item, dict))
        if payments_sum != service_sum:
            errors.append(
                f'sum(payments.amount)={payments_sum} != service_sum={service_sum}'
            )

    return errors


def validate_cash_withdrawal_receipt(payload: dict[str, Any]) -> list[str]:
    """Validate CASH_WITHDRAWAL payload.

    cash_withdrawal_sum must be present and > 0.
    payments must be non-empty and sum(payments.amount) must equal cash_withdrawal_sum.
    If totals present, totals.total_sum must equal cash_withdrawal_sum.
    """
    errors: list[str] = []
    cw_sum = payload.get('cash_withdrawal_sum')
    if cw_sum is None or not isinstance(cw_sum, (int, float)) or cw_sum <= 0:
        errors.append('cash_withdrawal_sum must be present and > 0')

    receipt = payload.get('receipt')
    if not isinstance(receipt, dict):
        errors.append('receipt object is missing')
        return errors

    payments = receipt.get('payments')
    if not isinstance(payments, list) or len(payments) == 0:
        errors.append('payments must be non-empty for CASH_WITHDRAWAL')
    elif cw_sum is not None and isinstance(cw_sum, (int, float)):
        payments_sum = sum(int(item.get('amount', 0)) for item in payments if isinstance(item, dict))
        if payments_sum != cw_sum:
            errors.append(
                f'sum(payments.amount)={payments_sum} != cash_withdrawal_sum={cw_sum}'
            )

    totals = receipt.get('totals')
    if isinstance(totals, dict) and totals.get('total_sum') is not None and cw_sum is not None:
        if totals['total_sum'] != cw_sum:
            errors.append(
                f'totals.total_sum={totals["total_sum"]} != cash_withdrawal_sum={cw_sum}'
            )

    return errors
