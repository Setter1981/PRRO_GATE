"""MAC recovery semantic equivalence tests.

Proves that after ERROR_BAD_HASH_PREV, the resent document is semantically
identical to what the normal path would produce (same tax_groups, same
related_receipt_id, same Z-report aggregation).

Tests:
  MRS1: recovery resend includes tax_groups in XML (TX blocks in E)
  MRS2: recovery resend preserves related_receipt_id (id_cancel for RETURN)
  MRS3: recovery XML has same canonical structure as normal path XML
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import CanonicalErrorCode, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.ports import DpsMacRecoveryError
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.services.write_path import WritePathWorker


def _setup(conn):
    # Seed tax group
    conn.execute("""
        INSERT OR IGNORE INTO tax_group_definitions
        (fiscal_number, tax_id, name, tax_rate, additional_rate, tax_type, tax_algorithm,
         requires_uktzed, requires_excise_mark)
        VALUES ('FN-DEV-0001', '1', 'А', 20.00, 0, 0, 0, 0, 0)
    """)
    conn.commit()

    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = 'shift-mrs'").fetchone()
    if not existing:
        ShiftRepository.create_shift(
            conn, shift_id='shift-mrs', fiscal_number='FN-DEV-0001',
            state=ShiftState.OPENED, open_mode='ONLINE',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_dps_grpc_default',
            protocol=Protocol.CHECKBOX_REST, integration_owner='test',
            channel_lock_acquired_at='2026-04-14T08:00:00Z',
        )
        conn.commit()


def _enqueue(conn, rid, op, payload):
    cmd = CanonicalFiscalCommand(
        request_id=rid, idempotency_key=f'idem-{rid}',
        protocol=Protocol.CHECKBOX_REST, operation_type=op,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id=f'ext-{rid}',
        business_ts=datetime(2026, 4, 14, 12, 0, 0, tzinfo=UTC),
        payload=payload, payload_sha256=f'sha-{rid}',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id=f'c-{rid}'),
        correlation_id=f'c-{rid}',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-14T12:01:00Z',
    )
    conn.commit()


# ---------------------------------------------------------------------------
# MRS1 — recovery resend includes tax_groups in XML
# ---------------------------------------------------------------------------

def test_mrs1_recovery_has_tax_groups_in_xml(conn) -> None:
    """After MAC recovery, the resent XML must contain <TX> blocks from tax_groups,
    not a minimal <E> without tax info."""
    _setup(conn)
    expected_mac = 'aa' * 32
    send_count = 0
    captured_payloads = []

    class _Crypto:
        def sign_raw(self, *, data):
            captured_payloads.append(data)
            return b'\x30\x82SIGNED'

    class _MacRecoveryTransport:
        def send(self, **kw):
            nonlocal send_count
            send_count += 1
            if send_count == 1:
                raise DpsMacRecoveryError('bad hash', expected_mac=expected_mac, dps_status=-12)
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-mrs1',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    _enqueue(conn, 'req-mrs1', OperationType.SELL, {
        'receipt': {
            'type': 'SELL',
            'goods': [{'name': 'Товар', 'price': 12000, 'quantity': 1000, 'sum': 12000, 'tax_id': '1'}],
            'payments': [{'amount': 12000, 'type': 'CASH'}],
            'totals': {'total_sum': 12000},
        },
    })

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_MacRecoveryTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    assert send_count == 2

    # The recovery-resent XML (second sign call) must have TX blocks
    assert len(captured_payloads) >= 2
    recovery_xml = captured_payloads[1]
    if isinstance(recovery_xml, bytes):
        recovery_xml = recovery_xml.decode('utf-8')

    assert '<TX ' in recovery_xml, f'Recovery XML must have <TX> tax blocks. Got: {recovery_xml}'
    assert 'TXPR="20.00"' in recovery_xml, f'Recovery XML must have TXPR. Got: {recovery_xml}'
    # TXSM = 12000 * 20 / 120 = 2000
    assert 'TXSM="2000"' in recovery_xml, f'Recovery XML must have correct TXSM. Got: {recovery_xml}'


# ---------------------------------------------------------------------------
# MRS2 — recovery resend preserves related_receipt_id (id_cancel)
# ---------------------------------------------------------------------------

def test_mrs2_recovery_preserves_related_receipt_id(conn) -> None:
    """RETURN MAC recovery must pass related_receipt_id to transport as id_cancel."""
    _setup(conn)
    expected_mac = 'bb' * 32
    send_count = 0
    captured_kwargs = []

    class _Crypto:
        def sign_raw(self, *, data):
            return b'\x30\x82SIGNED'

    class _CapturingTransport:
        def send(self, **kw):
            nonlocal send_count
            send_count += 1
            captured_kwargs.append(kw)
            if send_count == 1:
                raise DpsMacRecoveryError('bad hash', expected_mac=expected_mac, dps_status=-12)
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-mrs2',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    _enqueue(conn, 'req-mrs2', OperationType.RETURN, {
        'receipt': {
            'type': 'RETURN',
            'goods': [{'name': 'Повернення', 'price': 5000, 'quantity': 1000, 'sum': 5000, 'tax_id': '1'}],
            'payments': [{'amount': 5000, 'type': 'CASH'}],
            'totals': {'total_sum': 5000},
            'related_receipt_id': 'ORIG-RECEIPT-777',
        },
    })

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_CapturingTransport(),
                             tax_number='TN', require_return_linkage=False)
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    assert send_count == 2

    # Second send (recovery) must have related_receipt_id
    recovery_kw = captured_kwargs[1]
    assert recovery_kw.get('related_receipt_id') == 'ORIG-RECEIPT-777', (
        f'Recovery resend must preserve related_receipt_id. Got: {recovery_kw.get("related_receipt_id")}'
    )


# ---------------------------------------------------------------------------
# MRS3 — recovery XML has same canonical structure as normal path
# ---------------------------------------------------------------------------

def test_mrs3_recovery_xml_matches_normal_structure(conn) -> None:
    """Recovery XML must have the same canonical attributes as normal path:
    NDv, PrV on RQ; alphabetical attr order; proper tag closing."""
    _setup(conn)
    expected_mac = 'cc' * 32
    send_count = 0
    captured_payloads = []

    class _Crypto:
        def sign_raw(self, *, data):
            captured_payloads.append(data)
            return b'\x30\x82SIGNED'

    class _MacRecoveryTransport:
        def send(self, **kw):
            nonlocal send_count
            send_count += 1
            if send_count == 1:
                raise DpsMacRecoveryError('bad hash', expected_mac=expected_mac, dps_status=-12)
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-mrs3',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    _enqueue(conn, 'req-mrs3', OperationType.SELL, {
        'receipt': {
            'type': 'SELL',
            'goods': [{'name': 'X', 'price': 1000, 'quantity': 1000, 'sum': 1000, 'tax_id': '1'}],
            'payments': [{'amount': 1000, 'type': 'CASH'}],
            'totals': {'total_sum': 1000},
        },
    })

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_MacRecoveryTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'

    recovery_xml = captured_payloads[1]
    if isinstance(recovery_xml, bytes):
        recovery_xml = recovery_xml.decode('utf-8')

    # Canonical form checks
    assert recovery_xml.startswith('<RQ '), 'Must start with <RQ>'
    assert 'NDv="' in recovery_xml, 'Recovery XML must have NDv'
    assert 'PrV="' in recovery_xml, 'Recovery XML must have PrV'
    # Alphabetical: DI before FN in DAT
    di_pos = recovery_xml.index('DI="')
    fn_pos = recovery_xml.index('FN="')
    assert di_pos < fn_pos, 'Attributes must be alphabetical: DI before FN'
    # No self-closing tags (canonical form)
    assert '/>' not in recovery_xml, f'No self-closing tags in canonical form. Got: {recovery_xml}'

    # Semantic equivalence: normal XML (first sign) and recovery XML differ ONLY in MAC
    normal_xml = captured_payloads[0]
    if isinstance(normal_xml, bytes):
        normal_xml = normal_xml.decode('utf-8')
    import re
    strip_mac = lambda x: re.sub(r'<MAC>[^<]*</MAC>', '<MAC>STRIPPED</MAC>', x)
    assert strip_mac(normal_xml) == strip_mac(recovery_xml), (
        f'Normal and recovery XML must differ only in MAC.\n'
        f'Normal:   {strip_mac(normal_xml)[:200]}\n'
        f'Recovery: {strip_mac(recovery_xml)[:200]}'
    )


# ---------------------------------------------------------------------------
# MRS4 — Z_REPORT MAC recovery preserves aggregation (TXS, M, IO, NC)
# ---------------------------------------------------------------------------

def test_mrs4_zreport_recovery_preserves_aggregation(conn) -> None:
    """Z_REPORT MAC recovery must include full shift aggregation:
    TXS, M, IO, NC — same as normal path would produce."""
    _setup(conn)
    expected_mac = 'dd' * 32
    send_count = 0
    captured_payloads = []

    class _Crypto:
        def sign_raw(self, *, data):
            captured_payloads.append(data)
            return b'\x30\x82SIGNED'

    class _OkTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id=f'tr-{kw["document_id"]}',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    class _ZRecoveryTransport:
        def send(self, **kw):
            nonlocal send_count
            send_count += 1
            if kw.get('operation_type') == 'Z_REPORT' and send_count == 3:
                raise DpsMacRecoveryError('bad hash', expected_mac=expected_mac, dps_status=-12)
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id=f'tr-{send_count}',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_ZRecoveryTransport(), tax_number='TN')

    # 1. SELL — creates data for Z aggregation
    _enqueue(conn, 'req-mrs4-sell', OperationType.SELL, {
        'receipt': {
            'type': 'SELL',
            'goods': [{'name': 'Bread', 'price': 5000, 'quantity': 2000, 'sum': 10000, 'tax_id': '1'}],
            'payments': [{'amount': 10000, 'type': 'CASH'}],
            'totals': {'total_sum': 10000},
        },
    })
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # 2. SERVICE_IN
    _enqueue(conn, 'req-mrs4-svc', OperationType.SERVICE_IN, {
        'service_sum': 50000,
        'receipt': {'type': 'SERVICE_IN', 'goods': [], 'payments': [], 'totals': {}},
    })
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # 3. Z_REPORT — first send triggers MAC recovery, second succeeds
    captured_payloads.clear()
    _enqueue(conn, 'req-mrs4-z', OperationType.Z_REPORT, {
        'receipt': {'type': 'Z_REPORT', 'goods': [], 'payments': [], 'totals': {}},
    })
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Z_REPORT recovery should ACK. Got: {result.canonical_error}'

    # Recovery XML (second sign for Z_REPORT) must have aggregation
    z_xmls = [p.decode('utf-8') if isinstance(p, bytes) else p
              for p in captured_payloads if '<Z ' in (p.decode('utf-8') if isinstance(p, bytes) else p)]
    assert len(z_xmls) >= 2, f'Expected at least 2 Z XML payloads (normal + recovery). Got {len(z_xmls)}'

    recovery_z = z_xmls[-1]

    # TXS — must be present with sell data
    assert '<TXS ' in recovery_z, f'Recovery Z must have TXS. Got: {recovery_z}'
    assert 'SMI="10000"' in recovery_z, f'TXS SMI must reflect SELL total. Got: {recovery_z}'

    # M — payment summary
    assert '<M ' in recovery_z and 'CASH' in recovery_z, f'Recovery Z must have M/CASH. Got: {recovery_z}'

    # IO — service in/out
    assert '<IO ' in recovery_z, f'Recovery Z must have IO. Got: {recovery_z}'
    assert 'SMI="50000"' in recovery_z, f'IO SMI must reflect SERVICE_IN. Got: {recovery_z}'

    # NC — check counts
    assert 'NI="1"' in recovery_z, f'NC NI must be 1 (one sell). Got: {recovery_z}'

    # Semantic equivalence: normal and recovery Z differ only in MAC
    import re
    strip_mac = lambda x: re.sub(r'<MAC>[^<]*</MAC>', '<MAC>STRIPPED</MAC>', x)
    assert strip_mac(z_xmls[0]) == strip_mac(z_xmls[1]), (
        f'Normal and recovery Z must differ only in MAC.\n'
        f'Normal:   {strip_mac(z_xmls[0])[:300]}\n'
        f'Recovery: {strip_mac(z_xmls[1])[:300]}'
    )
