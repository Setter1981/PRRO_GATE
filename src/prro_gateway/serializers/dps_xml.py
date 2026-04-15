"""
DPS Fiscal Server XML serializer — ФСКО protocol format.

Builds the `<RQ><DAT>...<TS>...</DAT><MAC>...</MAC></RQ>` XML artifact
that gets signed and submitted via gRPC sendChkV2.

Official source: ФСКО protocol v2.1.9
  (Технологія зберігання і збору даних РРО для ДПС)

Document structure:
  <RQ V="1">
    <DAT FN="{fiscal_number}" TN="{tax_number}" DI="{local_number}" ZN="{z_number}" V="1">
      <C T="{check_type}">...</C>  (for checks)
      <Z NO="{z_number}">...</Z>   (for Z-reports)
      <TS>{timestamp}</TS>
    </DAT>
    <MAC>{hash}</MAC>
  </RQ>

Check type values (XML <C T="...">):
  0 = SELL (чек продажу)
  1 = RETURN (чек повернення)
  2 = SERVICE (службовий чек, including shift-open/close in ФСКО)

For gRPC sendChkV2, the proto check_type enum recodes:
  CHK=1, ZREPORT=2, SERVICECHK=3

SHIFT_OPEN uses DI="0" (local_number=0).
SELL/RETURN uses DI="{lnd}".
Z_REPORT uses DI="{lnd}" with <Z> instead of <C>.

Sprint 7 step 1 scope: SHIFT_OPEN, SELL, Z_REPORT only.
"""
from __future__ import annotations

from datetime import datetime, UTC
from typing import Any

from ..enums import OperationType


def build_dps_xml(
    *,
    operation_type: OperationType,
    fiscal_number: str,
    local_number: int,
    business_ts: datetime | str | None = None,
    payload: dict[str, Any] | None = None,
    tax_number: str = '',
    z_number: int = 0,
    previous_hash: str = '',
    tax_groups: dict | None = None,
    device_name: str = 'ПРО_каса',
    device_version: str = '1.1',
) -> str:
    """Build DPS ФСКО XML artifact for the given operation.

    Returns XML string ready for signing.
    Supported: SHIFT_OPEN, SELL, RETURN, SERVICE_IN, SERVICE_OUT, CASH_WITHDRAWAL, Z_REPORT.
    """
    if isinstance(business_ts, str):
        try:
            ts = datetime.fromisoformat(business_ts)
        except (ValueError, TypeError):
            ts = datetime.now(UTC)
    elif business_ts is None:
        ts = datetime.now(UTC)
    else:
        ts = business_ts

    # DPS expects local Kyiv time in TS field (evidence: WebCheck archive uses local time)
    try:
        from zoneinfo import ZoneInfo
        kyiv = ZoneInfo('Europe/Kyiv')
        ts_local = ts.astimezone(kyiv)
    except Exception:
        ts_local = ts
    ts_str = ts_local.strftime('%Y%m%d%H%M%S')

    rq_attrs = {'NDv': device_name, 'PrV': device_version, 'V': '1'}

    if operation_type == OperationType.SHIFT_OPEN:
        return _build_shift_open(fiscal_number=fiscal_number, tax_number=tax_number,
                                 z_number=z_number, ts_str=ts_str, payload=payload,
                                 previous_hash=previous_hash, rq_attrs=rq_attrs)
    elif operation_type == OperationType.SELL:
        return _build_check(fiscal_number=fiscal_number, tax_number=tax_number,
                            local_number=local_number, z_number=z_number,
                            ts_str=ts_str, payload=payload,
                            previous_hash=previous_hash, check_xml_type='0',
                            tax_groups=tax_groups, rq_attrs=rq_attrs)
    elif operation_type == OperationType.RETURN:
        return _build_check(fiscal_number=fiscal_number, tax_number=tax_number,
                            local_number=local_number, z_number=z_number,
                            ts_str=ts_str, payload=payload,
                            previous_hash=previous_hash, check_xml_type='1',
                            tax_groups=tax_groups, rq_attrs=rq_attrs)
    elif operation_type == OperationType.SERVICE_IN:
        return _build_service(fiscal_number=fiscal_number, tax_number=tax_number,
                              local_number=local_number, z_number=z_number,
                              ts_str=ts_str, payload=payload,
                              previous_hash=previous_hash, direction='IN', rq_attrs=rq_attrs)
    elif operation_type == OperationType.SERVICE_OUT:
        return _build_service(fiscal_number=fiscal_number, tax_number=tax_number,
                              local_number=local_number, z_number=z_number,
                              ts_str=ts_str, payload=payload,
                              previous_hash=previous_hash, direction='OUT', rq_attrs=rq_attrs)
    elif operation_type == OperationType.CASH_WITHDRAWAL:
        return _build_cash_withdrawal(fiscal_number=fiscal_number, tax_number=tax_number,
                                      local_number=local_number, z_number=z_number,
                                      ts_str=ts_str, payload=payload,
                                      previous_hash=previous_hash, rq_attrs=rq_attrs)
    elif operation_type == OperationType.Z_REPORT:
        return _build_z_report(fiscal_number=fiscal_number, tax_number=tax_number,
                               local_number=local_number, z_number=z_number,
                               ts_str=ts_str, payload=payload,
                               previous_hash=previous_hash,
                               tax_groups=tax_groups, rq_attrs=rq_attrs)
    else:
        raise ValueError(f'DPS XML serializer does not support {operation_type.value} in this slice')


def _build_shift_open(*, fiscal_number: str, tax_number: str, z_number: int,
                       ts_str: str, payload: dict | None, previous_hash: str = '',
                       rq_attrs: dict | None = None) -> str:
    """SHIFT_OPEN: <C T="108"> (recoded for sendChkV2) with DI="0"."""
    # Extract opening balance from payload if available
    opening_sum = '0'
    if isinstance(payload, dict):
        receipt = payload.get('receipt', {}) if isinstance(payload.get('receipt'), dict) else {}
        # totals.total_sum if present
        totals = receipt.get('totals', {}) if isinstance(receipt.get('totals'), dict) else {}
        if totals.get('total_sum'):
            opening_sum = str(totals['total_sum'])

    return _tag('RQ', rq_attrs or {'V': '1'},
        _tag('DAT', {'DI': '0', 'FN': fiscal_number, 'TN': tax_number, 'V': '1', 'ZN': z_number},
            _tag('C', {'T': '108'},
                _tag('O', {'N': '1', 'SM': opening_sum, 'T': '0'})
                + _tag('E', {'N': '2'})
            ) + _tag('TS', content=ts_str)
        ) + _tag('MAC', content=previous_hash)
    )


def _build_check(*, fiscal_number: str, tax_number: str, local_number: int,
                 z_number: int, ts_str: str, payload: dict | None, previous_hash: str = '',
                 check_xml_type: str = '0', tax_groups: dict | None = None,
                 rq_attrs: dict | None = None) -> str:
    """SELL/RETURN check: <C T="0"> for SELL, <C T="1"> for RETURN.

    When tax_groups is provided, builds full <E> with <TX> sub-elements
    containing calculated TXSM/DTSM per ФСКО protocol.
    """
    header_xml = ''
    items_xml = ''
    discounts_xml = ''
    payments_xml = ''
    footer_xml = ''
    item_no = 1
    total_sum = 0
    p_item_numbers: list[int] = []  # N values of <P> elements — for check-level discount <NI>
    # Aggregate sums per tax group for <E> <TX> blocks
    group_sums: dict[str, int] = {}  # tax_id → sum of item amounts

    if isinstance(payload, dict):
        receipt = payload.get('receipt', {}) if isinstance(payload.get('receipt'), dict) else {}
        goods = receipt.get('goods', []) if isinstance(receipt.get('goods'), list) else []
        payments = receipt.get('payments', []) if isinstance(receipt.get('payments'), list) else []
        totals = receipt.get('totals', {}) if isinstance(receipt.get('totals'), dict) else {}

        # Header text lines → <L> elements before first <P>
        receipt_header = receipt.get('header') or ''
        for line in receipt_header.split('\n'):
            line = line.strip()
            if line:
                header_xml += _tag('L', {'N': item_no, 'NM': line})
                item_no += 1

        # Footer text lines → collected, emitted after payments before <E>
        receipt_footer = receipt.get('footer') or ''
        footer_lines = [ln.strip() for ln in receipt_footer.split('\n') if ln.strip()]

        for g in goods:
            if not isinstance(g, dict):
                continue
            p_attrs: dict[str, object] = {
                'C': g.get('code') or '0',
                'N': item_no,
                'NM': g.get('name', ''),
                'PRC': g.get('price', 0),
                'Q': g.get('quantity', 0),
                'SM': g.get('sum', 0),
            }
            barcode = g.get('barcode', '') or ''
            if barcode:
                p_attrs['CD'] = barcode
            uktzed = g.get('uktzed', '') or ''
            if uktzed:
                p_attrs['CZD'] = uktzed
            if g.get('tax_id') is not None:
                p_attrs['TX'] = g['tax_id']
            if g.get('tax_id_2') is not None:
                p_attrs['TX1'] = g['tax_id_2']
            excise_marks = g.get('excise_barcodes') or []
            ca_xml = ''.join(_tag('CA', {'CA': m}) for m in excise_marks)
            p_item_no = item_no
            p_item_numbers.append(item_no)
            items_xml += _tag('P', p_attrs, ca_xml)
            item_no += 1
            # Accumulate per-group sums
            g_tax_id = g.get('tax_id')
            if g_tax_id is not None:
                group_sums[str(g_tax_id)] = group_sums.get(str(g_tax_id), 0) + int(g.get('sum', 0))

            # Per-item discounts → <D> (знижка) or <S> (націнка)
            for d in (g.get('discounts') or []):
                if not isinstance(d, dict):
                    continue
                d_value = int(d.get('value', 0))
                if d_value == 0:
                    continue
                d_type = d.get('type', 'DISCOUNT')
                d_mode = d.get('mode', 'VALUE')
                xml_tag = 'D' if d_type != 'EXTRA_CHARGE' else 'S'
                d_attrs: dict[str, object] = {
                    'N': item_no,
                    'NI': p_item_no,
                    'TR': 0,
                }
                if d_mode == 'PERCENT':
                    d_attrs['TY'] = 1
                    d_attrs['PR'] = f'{d_value:.2f}'
                    d_attrs['SM'] = round(int(g.get('sum', 0)) * d_value / 100)
                else:
                    d_attrs['TY'] = 0
                    d_attrs['SM'] = d_value
                if d.get('name'):
                    d_attrs['NM'] = d['name']
                if d.get('privilege'):
                    d_attrs['DN'] = d['privilege']
                if d.get('tax_code'):
                    d_attrs['TX'] = d['tax_code']
                discounts_xml += _tag(xml_tag, d_attrs)
                item_no += 1

        # Check-level (receipt) discounts → <D TR="1"> with <NI> sub-elements
        check_discounts = receipt.get('discounts') or []
        if check_discounts and p_item_numbers:
            goods_total = sum(int(g.get('sum', 0)) for g in goods if isinstance(g, dict))
            for d in check_discounts:
                if not isinstance(d, dict):
                    continue
                d_value = int(d.get('value', 0))
                if d_value == 0:
                    continue
                d_type = d.get('type', 'DISCOUNT')
                d_mode = d.get('mode', 'VALUE')
                xml_tag = 'D' if d_type != 'EXTRA_CHARGE' else 'S'
                ni_xml = ''.join(_tag('NI', {'NI': n}) for n in p_item_numbers)
                d_attrs: dict[str, object] = {'N': item_no, 'TR': 1}
                if d_mode == 'PERCENT':
                    d_attrs['TY'] = 1
                    d_attrs['PR'] = f'{d_value:.2f}'
                    d_attrs['SM'] = round(goods_total * d_value / 100)
                else:
                    d_attrs['TY'] = 0
                    d_attrs['SM'] = d_value
                if d.get('name'):
                    d_attrs['NM'] = d['name']
                discounts_xml += _tag(xml_tag, d_attrs, ni_xml)
                item_no += 1

        total_sum = totals.get('total_sum', 0) or 0
        all_payments_sum = sum(int(p.get('amount', 0)) for p in payments if isinstance(p, dict))
        change = all_payments_sum - total_sum if all_payments_sum > total_sum else 0
        rounding = receipt.get('rounding', 0) if isinstance(receipt, dict) else 0
        rm_assigned = False
        smp_assigned = False

        for p in payments:
            if not isinstance(p, dict):
                continue
            ptype = p.get('type', 'CASH')
            amount = p.get('amount', 0)
            m_type = p.get('resolved_t', '0' if ptype == 'CASH' else '2')
            m_nm = p.get('resolved_nm', ptype)
            m_attrs: dict[str, object] = {'N': item_no, 'NM': m_nm, 'SM': amount, 'T': m_type}
            # EPZ attributes for non-cash payments
            if m_type != '0':
                for src, dst in [('rrn', 'RRN'), ('payment_system', 'PSNM'),
                                 ('bank_name', 'PA'), ('terminal', 'PB'),
                                 ('label', 'PC'), ('card_mask', 'PD'),
                                 ('auth_code', 'PE')]:
                    val = p.get(src, '') or ''
                    if val:
                        m_attrs[dst] = val
                commission = p.get('commission', 0) or 0
                if commission > 0:
                    m_attrs['PF'] = commission
            if m_type == '0' and change > 0 and not rm_assigned:
                m_attrs['RM'] = change
                rm_assigned = True
            if m_type == '0' and rounding != 0 and not smp_assigned:
                m_attrs['SMP'] = rounding
                smp_assigned = True
            payments_xml += _tag('M', m_attrs)
            item_no += 1

        # Footer text lines → <L> after all payments, before <E>
        for line in footer_lines:
            footer_xml += _tag('L', {'N': item_no, 'NM': line})
            item_no += 1

    # Build <E> element
    total_xml = _build_e_element(
        item_no=item_no, total_sum=total_sum, fiscal_number=fiscal_number,
        ts_str=ts_str, local_number=local_number, group_sums=group_sums, tax_groups=tax_groups,
    )

    return _tag('RQ', rq_attrs or {'V': '1'},
        _tag('DAT', {'DI': local_number, 'FN': fiscal_number, 'TN': tax_number, 'V': '1', 'ZN': z_number},
            _tag('C', {'T': check_xml_type},
                 header_xml + items_xml + discounts_xml + payments_xml + footer_xml + total_xml)
            + _tag('TS', content=ts_str)
        ) + _tag('MAC', content=previous_hash)
    )


def _build_service(*, fiscal_number: str, tax_number: str, local_number: int,
                    z_number: int, ts_str: str, payload: dict | None,
                    previous_hash: str = '', direction: str = 'IN',
                    rq_attrs: dict | None = None) -> str:
    """SERVICE_IN/SERVICE_OUT: <C T="2"> with <I> for IN or <O> for OUT.

    ФСКО spec: <I> = внесення (service income), <O> = видача (service outcome).
    T="0" = cash. No <P> or <M> elements.
    """
    service_sum = '0'
    header_xml = ''
    item_no = 1
    if isinstance(payload, dict):
        service_sum = str(payload.get('service_sum', 0))
        receipt = payload.get('receipt') if isinstance(payload.get('receipt'), dict) else {}
        receipt_header = (receipt.get('header') or '') if isinstance(receipt, dict) else ''
        for line in receipt_header.split('\n'):
            line = line.strip()
            if line:
                header_xml += _tag('L', {'N': item_no, 'NM': line})
                item_no += 1

    io_tag = 'I' if direction == 'IN' else 'O'
    io_n = item_no
    e_n = item_no + 1
    return _tag('RQ', rq_attrs or {'V': '1'},
        _tag('DAT', {'DI': local_number, 'FN': fiscal_number, 'TN': tax_number, 'V': '1', 'ZN': z_number},
            _tag('C', {'T': '2'},
                header_xml
                + _tag(io_tag, {'N': io_n, 'NM': 'ГОТІВКА', 'SM': service_sum, 'T': '0'})
                + _tag('E', {'N': e_n})
            ) + _tag('TS', content=ts_str)
        ) + _tag('MAC', content=previous_hash)
    )


def _build_cash_withdrawal(*, fiscal_number: str, tax_number: str, local_number: int,
                           z_number: int, ts_str: str, payload: dict | None,
                           previous_hash: str = '', rq_attrs: dict | None = None) -> str:
    """CASH_WITHDRAWAL: <C T="8"> — видача готівки держателям ЕПЗ.

    ФСКО: P (сума) + M (реквізити картки) + E (закриття).
    """
    cash_sum = '0'
    payment_xml = ''
    item_no = 1

    if isinstance(payload, dict):
        cash_sum = str(payload.get('cash_withdrawal_sum', 0))
        receipt = payload.get('receipt', {}) if isinstance(payload.get('receipt'), dict) else {}
        payments = receipt.get('payments', []) if isinstance(receipt.get('payments'), list) else []

        for p in payments:
            if not isinstance(p, dict):
                continue
            item_no += 1
            m_attrs: dict[str, object] = {'N': item_no, 'SM': p.get('amount', cash_sum), 'T': '0'}
            for src, dst in [('bank_name', 'PA'), ('terminal', 'PB'), ('label', 'PC'),
                             ('card_mask', 'PD'), ('auth_code', 'PE'), ('payment_system', 'PSNM'),
                             ('rrn', 'RRN')]:
                val = p.get(src, '') or ''
                if val:
                    m_attrs[dst] = val
            if not m_attrs.get('PC'):
                m_attrs['PC'] = 'ВИДАЧА КОШТІВ'
            commission = p.get('commission', 0) or 0
            if commission:
                m_attrs['PF'] = commission
            payment_xml += _tag('M', m_attrs)

    if not payment_xml:
        item_no += 1
        payment_xml = _tag('M', {'N': item_no, 'NM': 'ГОТІВКА', 'SM': cash_sum, 'T': '0'})

    e_no = item_no + 1

    return _tag('RQ', rq_attrs or {'V': '1'},
        _tag('DAT', {'DI': local_number, 'FN': fiscal_number, 'TN': tax_number, 'V': '1', 'ZN': z_number},
            _tag('C', {'T': '8'},
                _tag('P', {'C': '0', 'N': '1', 'NM': 'Гривня', 'SM': cash_sum})
                + payment_xml
                + _tag('E', {'FN': fiscal_number, 'N': e_no, 'NO': local_number, 'SM': cash_sum, 'TS': ts_str})
            ) + _tag('TS', content=ts_str)
        ) + _tag('MAC', content=previous_hash)
    )


def _build_z_report(*, fiscal_number: str, tax_number: str, local_number: int,
                     z_number: int, ts_str: str, payload: dict | None,
                     previous_hash: str = '', tax_groups: dict | None = None,
                     rq_attrs: dict | None = None) -> str:
    """Z_REPORT: <Z NO="..."> with TXS, M, IO, NC sections.

    payload may contain z_report_data dict with aggregated shift data:
      tax_sums: {tax_id: {smi: int, smo: int}} — per-group totals
      payment_sums: {type: {smi: int, smo: int}} — per-payment-type totals
      service_sums: {type: {smi: int, smo: int}} — service in/out totals
      check_count: {ni: int, no: int} — sell/return counts
    """
    z_content = ''

    z_data = {}
    if isinstance(payload, dict):
        z_data = payload.get('z_report_data', {}) or {}

    # <TXS> — tax group summaries
    tax_sums = z_data.get('tax_sums', {})
    if isinstance(tax_sums, dict) and tax_groups:
        for tax_id in sorted(tax_sums.keys()):
            ts_data = tax_sums[tax_id]
            if not isinstance(ts_data, dict):
                continue
            smi = ts_data.get('smi', 0)
            smo = ts_data.get('smo', 0)
            group = tax_groups.get(str(tax_id))
            if group:
                tax_rate = getattr(group, 'tax_rate', 0) or 0
                additional_rate = getattr(group, 'additional_rate', 0) or 0
                tax_algorithm = getattr(group, 'tax_algorithm', 0) or 0
                tax_type = getattr(group, 'tax_type', 0) or 0
                txi, _ = _calc_tax(smi, tax_rate, additional_rate, tax_algorithm) if smi else (0, 0)
                txo, _ = _calc_tax(smo, tax_rate, additional_rate, tax_algorithm) if smo else (0, 0)
                z_content += _tag('TXS', {
                    'DTPR': f'{additional_rate:.2f}', 'SMI': smi, 'SMO': smo,
                    'TS': ts_str[:8], 'TX': tax_id, 'TXAL': tax_algorithm,
                    'TXI': txi, 'TXO': txo, 'TXPR': f'{tax_rate:.2f}', 'TXTY': tax_type,
                })
            else:
                z_content += _tag('TXS', {'SMI': smi, 'SMO': smo, 'TX': tax_id})

    # <M> — payment type summaries
    payment_sums = z_data.get('payment_sums', {})
    if isinstance(payment_sums, dict):
        for ptype in sorted(payment_sums.keys()):
            ps = payment_sums[ptype]
            if not isinstance(ps, dict):
                continue
            m_type = '0' if ptype == 'CASH' else '2'
            z_content += _tag('M', {'NM': ptype, 'SMI': ps.get('smi', 0), 'SMO': ps.get('smo', 0), 'T': m_type})

    # <IO> — service in/out summaries
    service_sums = z_data.get('service_sums', {})
    if isinstance(service_sums, dict):
        for stype in sorted(service_sums.keys()):
            ss = service_sums[stype]
            if not isinstance(ss, dict):
                continue
            z_content += _tag('IO', {'NM': stype, 'SMI': ss.get('smi', 0), 'SMO': ss.get('smo', 0), 'T': '0'})

    # <NC> — check counts
    check_count = z_data.get('check_count', {})
    if isinstance(check_count, dict):
        z_content += _tag('NC', {'NI': check_count.get('ni', 0), 'NO': check_count.get('no', 0)})

    # <EPZ> — summary of cash withdrawals via ЕПЗ (electronic payment instruments)
    epz_sums = z_data.get('epz_sums')
    if isinstance(epz_sums, dict) and epz_sums:
        z_content += _tag('EPZ', {
            'EPC': epz_sums.get('epc', 0),
            'EPCS': epz_sums.get('epcs', 0),
            'EPSM': epz_sums.get('epsm', 0),
        })

    return _tag('RQ', rq_attrs or {'V': '1'},
        _tag('DAT', {'DI': local_number, 'FN': fiscal_number, 'TN': tax_number, 'V': '1', 'ZN': z_number},
            _tag('Z', {'NO': z_number}, z_content)
            + _tag('TS', content=ts_str)
        ) + _tag('MAC', content=previous_hash)
    )


def _build_e_element(*, item_no: int, total_sum: int, fiscal_number: str,
                     ts_str: str, local_number: int = 0, group_sums: dict[str, int],
                     tax_groups: dict | None) -> str:
    """Build <E> closing element with optional <TX> sub-elements.

    Always emits FN, N, NO, SM, TS per ФСКО spec Table 23.
    Without tax_groups (or no matching groups): full attributes, no <TX> children.
    With tax_groups: full attributes + <TX> sub-elements per group.
    """
    if not tax_groups or not group_sums:
        return _tag('E', {'FN': fiscal_number, 'N': item_no, 'NO': local_number,
                          'SM': total_sum, 'TS': ts_str})

    tx_xml = ''
    for tax_id, group_sum in sorted(group_sums.items()):
        group = tax_groups.get(tax_id)
        if group is None:
            continue
        tax_rate = getattr(group, 'tax_rate', 0) or 0
        additional_rate = getattr(group, 'additional_rate', 0) or 0
        tax_algorithm = getattr(group, 'tax_algorithm', 0) or 0
        tax_type = getattr(group, 'tax_type', 0) or 0

        txsm, dtsm = _calc_tax(group_sum, tax_rate, additional_rate, tax_algorithm)

        tx_xml += _tag('TX', {
            'DTPR': f'{additional_rate:.2f}', 'DTSM': dtsm,
            'TX': tax_id, 'TXAL': tax_algorithm,
            'TXPR': f'{tax_rate:.2f}', 'TXSM': txsm, 'TXTY': tax_type,
        })

    return _tag('E', {'FN': fiscal_number, 'N': item_no, 'NO': local_number,
                      'SM': total_sum, 'TS': ts_str}, tx_xml)


def _calc_tax(group_sum: int, tax_rate: float, additional_rate: float,
              tax_algorithm: int) -> tuple[int, int]:
    """Calculate TXSM (tax sum) and DTSM (additional charge sum) per ФСКО formulas.

    TXAL=0: TXSM = SM * TXPR / (100 + TXPR)
    TXAL=1: DTSM first, then TXSM = (SM + DTSM) * TXPR / (100 + TXPR)
    TXAL=2: DTSM = SM * DTPR / (100 + DTPR), then TXSM = (SM - DTSM) * TXPR / (100 + TXPR)
    TXAL=3: DTSM = absolute (not percentage-based, needs per-unit rate — not supported yet)
    """
    dtsm = 0
    txsm = 0

    if tax_algorithm == 0:
        if tax_rate > 0:
            txsm = round(group_sum * tax_rate / (100 + tax_rate))
    elif tax_algorithm == 1:
        if additional_rate > 0:
            dtsm = round(group_sum * additional_rate / 100)
        if tax_rate > 0:
            txsm = round((group_sum + dtsm) * tax_rate / (100 + tax_rate))
    elif tax_algorithm == 2:
        if additional_rate > 0:
            dtsm = round(group_sum * additional_rate / (100 + additional_rate))
        if tax_rate > 0:
            txsm = round((group_sum - dtsm) * tax_rate / (100 + tax_rate))
    # TXAL=3 (absolute excise per quantity) — requires per-unit rate, not supported in this slice

    return txsm, dtsm


def _xml_escape(s: str) -> str:
    """Minimal XML attribute value escaping."""
    return s.replace('&', '&amp;').replace('"', '&quot;').replace('<', '&lt;').replace('>', '&gt;')


def _tag(name: str, attrs: dict[str, object] | None = None, content: str = '') -> str:
    """Build canonical XML tag: attributes in alphabetical order, proper closing.

    ФСКО canonical form rules:
    - All attributes sorted alphabetically
    - Self-closing <tag/> becomes <tag></tag>
    - No whitespace between tags
    """
    if attrs:
        # Filter None values, sort alphabetically
        sorted_attrs = ' '.join(
            f'{k}="{_xml_escape(str(v))}"'
            for k, v in sorted(attrs.items())
            if v is not None
        )
        open_tag = f'<{name} {sorted_attrs}>' if sorted_attrs else f'<{name}>'
    else:
        open_tag = f'<{name}>'
    return f'{open_tag}{content}</{name}>'


__all__ = ['build_dps_xml']
