"""Shift aggregation — shared by Z-report and X-report.

Pure read-only functions that aggregate fiscal_documents for a shift.
No side effects, no state changes.
"""
from __future__ import annotations

import json
import sqlite3


def aggregate_shift_data(conn: sqlite3.Connection, shift_id: str,
                         exclude_document_id: str | None = None) -> dict:
    """Aggregate fiscal documents for a shift.

    Used by Z-report (with exclude_document_id) and X-report (without).
    Returns: {tax_sums, payment_sums, service_sums, check_count}
    """
    if exclude_document_id:
        rows = conn.execute(
            """SELECT doc_type, payload_json, state FROM fiscal_documents
               WHERE shift_id = ? AND state IN ('ACK', 'OFFLINE_LOCAL_ACK') AND document_id != ?""",
            (shift_id, exclude_document_id),
        ).fetchall()
    else:
        rows = conn.execute(
            """SELECT doc_type, payload_json, state FROM fiscal_documents
               WHERE shift_id = ? AND state IN ('ACK', 'OFFLINE_LOCAL_ACK')""",
            (shift_id,),
        ).fetchall()

    tax_sums: dict[str, dict[str, int]] = {}
    payment_sums: dict[str, dict[str, int]] = {}
    service_sums: dict[str, dict[str, int]] = {}
    sell_count = 0
    return_count = 0

    for doc_type, payload_json, state in rows:
        try:
            payload = json.loads(payload_json) if payload_json else {}
        except (ValueError, TypeError):
            payload = {}
        cmd_payload = payload.get('payload', payload)
        if not isinstance(cmd_payload, dict):
            continue
        receipt = cmd_payload.get('receipt', {})
        if not isinstance(receipt, dict):
            receipt = {}

        is_sell = doc_type == 'SELL'
        is_return = doc_type == 'RETURN'
        is_service_in = doc_type == 'SERVICE_IN'
        is_service_out = doc_type == 'SERVICE_OUT'

        if is_sell:
            sell_count += 1
        elif is_return:
            return_count += 1

        if is_sell or is_return:
            for g in (receipt.get('goods') or []):
                if not isinstance(g, dict):
                    continue
                tax_id = g.get('tax_id')
                if tax_id is None:
                    continue
                sm = int(g.get('sum', 0))
                tid = str(tax_id)
                if tid not in tax_sums:
                    tax_sums[tid] = {'smi': 0, 'smo': 0}
                if is_sell:
                    tax_sums[tid]['smi'] += sm
                else:
                    tax_sums[tid]['smo'] += sm

        if is_sell or is_return:
            for p in (receipt.get('payments') or []):
                if not isinstance(p, dict):
                    continue
                ptype = p.get('payment_type', p.get('type', 'CASH'))
                amt = int(p.get('amount', 0))
                if ptype not in payment_sums:
                    payment_sums[ptype] = {'smi': 0, 'smo': 0}
                if is_sell:
                    payment_sums[ptype]['smi'] += amt
                else:
                    payment_sums[ptype]['smo'] += amt

        if is_service_in or is_service_out:
            svc_sum = int(cmd_payload.get('service_sum', 0))
            nm = 'ГОТІВКА'
            if nm not in service_sums:
                service_sums[nm] = {'smi': 0, 'smo': 0}
            if is_service_in:
                service_sums[nm]['smi'] += svc_sum
            else:
                service_sums[nm]['smo'] += svc_sum

    return {
        'tax_sums': tax_sums,
        'payment_sums': payment_sums,
        'service_sums': service_sums,
        'check_count': {'ni': sell_count, 'no': return_count},
    }


def aggregate_cash_withdrawals(conn: sqlite3.Connection, shift_id: str) -> dict:
    """Count and sum CASH_WITHDRAWAL docs in shift."""
    rows = conn.execute(
        """SELECT payload_json FROM fiscal_documents
           WHERE shift_id = ? AND doc_type = 'CASH_WITHDRAWAL'
             AND state IN ('ACK', 'OFFLINE_LOCAL_ACK')""",
        (shift_id,),
    ).fetchall()

    count = len(rows)
    total_sum = 0
    total_commission = 0
    for (payload_json,) in rows:
        try:
            payload = json.loads(payload_json) if payload_json else {}
        except (ValueError, TypeError):
            payload = {}
        cmd_payload = payload.get('payload', payload)
        if isinstance(cmd_payload, dict):
            total_sum += int(cmd_payload.get('cash_withdrawal_sum', 0))
            receipt = cmd_payload.get('receipt', {})
            if isinstance(receipt, dict):
                for p in (receipt.get('payments') or []):
                    if isinstance(p, dict):
                        total_commission += int(p.get('commission', 0) or 0)

    return {'count': count, 'sum': total_sum, 'commission': total_commission}


__all__ = ['aggregate_shift_data', 'aggregate_cash_withdrawals']
