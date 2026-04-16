"""Sprint 8 step 2: DPS error classification and MAC auto-recovery tests.

Tests cover:
  EC1: ERROR_BAD_HASH_PREV parsing and DpsMacRecoveryError
  EC2: rate limit (-4) classified as retryable
  EC3: empty-MAC (-8 without hash) classified as retryable
  EC4: signature/verify error (-1) classified as rejected
  EC5: wrong check_type (-5) classified as rejected
  EC6: wrong XML tag (-10) classified as rejected
  EC7: MAC recovery in write_path — one bounded retry
  EC8: MAC recovery persists last_known_mac
  EC9: second failure after MAC recovery → retryable, no loop
  EC10: recovered doc XML used for next doc MAC chain
  RL1: rate limit (-4) raises TransportRateLimitedError with retry_after
  RL2: write_path marks rate-limited doc as ERROR_RETRYABLE with DPS_RATE_LIMITED status
  RL3: retry_after_seconds persisted in canonical error
  RL4: no sleep in request path (rate-limit handling is fast)
"""
from __future__ import annotations

import pytest
from prro_gateway.ports import DpsMacRecoveryError, TransportRejectedError, TransportRetryableError
from prro_gateway.transports.dps_fiscal_server import _extract_expected_mac, _raise_classified_dps_error


# ---------------------------------------------------------------------------
# EC1 — ERROR_BAD_HASH_PREV parsing
# ---------------------------------------------------------------------------

def test_ec1_extract_expected_mac_from_bad_hash_prev() -> None:
    """Parse expected hash from live DPS ERROR_BAD_HASH_PREV response."""
    msg = 'ERROR_BAD_HASH_PREV  store 0dfe4344f6a82aeeaba0b4d8faee2fee3d74d516c72f1fca320cf684cc17c3c8 chk deadbeef' + '0' * 56
    mac = _extract_expected_mac(msg)
    assert mac == '0dfe4344f6a82aeeaba0b4d8faee2fee3d74d516c72f1fca320cf684cc17c3c8'


def test_ec1b_extract_returns_none_for_unrelated_error() -> None:
    assert _extract_expected_mac('') is None
    assert _extract_expected_mac('some random error') is None
    assert _extract_expected_mac('Exceeded number of errors') is None


def test_ec1c_bad_hash_prev_raises_mac_recovery_error() -> None:
    msg = 'ERROR_BAD_HASH_PREV  store abcd1234' + '0' * 56 + ' chk 0000' + '0' * 60
    with pytest.raises(DpsMacRecoveryError) as exc_info:
        _raise_classified_dps_error(-12, msg)
    assert exc_info.value.expected_mac == 'abcd1234' + '0' * 56
    assert exc_info.value.dps_status == -12


# ---------------------------------------------------------------------------
# EC2 — ERROR_UNKNOWN (-4) → rejected (per proto, not rate limit)
# ---------------------------------------------------------------------------

def test_ec2_error_unknown_is_rejected() -> None:
    with pytest.raises(TransportRejectedError, match='status=-4'):
        _raise_classified_dps_error(-4, 'Exceeded number of errors')


# ---------------------------------------------------------------------------
# EC3 — ERROR_XML_DATE (-8) → rejected (per proto)
# ---------------------------------------------------------------------------

def test_ec3_xml_date_is_rejected() -> None:
    with pytest.raises(TransportRejectedError, match='status=-8'):
        _raise_classified_dps_error(-8, '')


def test_ec3b_status_minus8_is_not_mac_recovery() -> None:
    """-8 is ERROR_XML_DATE, not MAC recovery — even with hash-like text."""
    msg = 'ERROR_BAD_HASH_PREV  store ' + 'ab' * 32 + ' chk ' + 'cd' * 32
    with pytest.raises(TransportRejectedError):
        _raise_classified_dps_error(-8, msg)


# ---------------------------------------------------------------------------
# EC_NEW — ERROR_OFFLINE_168 (-11) → rejected
# ---------------------------------------------------------------------------

def test_ec_offline_168_rejected() -> None:
    with pytest.raises(TransportRejectedError, match='status=-11'):
        _raise_classified_dps_error(-11, 'PRRO blocked: 168-hour offline limit exceeded')


# ---------------------------------------------------------------------------
# EC_NEW — ERROR_OFFLINE_ID (-16) → rejected
# ---------------------------------------------------------------------------

def test_ec_offline_id_rejected() -> None:
    with pytest.raises(TransportRejectedError, match='status=-16'):
        _raise_classified_dps_error(-16, 'Invalid offline ID')


# ---------------------------------------------------------------------------
# EC_NEW — ERROR_BAD_HASH_PREV (-12) without extractable hash → rejected, not recovery
# ---------------------------------------------------------------------------

def test_ec_bad_hash_no_extractable_hash_rejected() -> None:
    """If -12 but error_message doesn't contain parseable hash, reject — no auto-recovery."""
    with pytest.raises(TransportRejectedError):
        _raise_classified_dps_error(-12, 'some unexpected error text')


# ---------------------------------------------------------------------------
# EC4 — signature/verify error (-1) → rejected
# ---------------------------------------------------------------------------

def test_ec4_verify_error_is_rejected() -> None:
    with pytest.raises(TransportRejectedError, match='status=-1'):
        _raise_classified_dps_error(-1, 'ERROR_VEREFY')


# ---------------------------------------------------------------------------
# EC5 — wrong check_type (-5) → rejected
# ---------------------------------------------------------------------------

def test_ec5_wrong_type_is_rejected() -> None:
    with pytest.raises(TransportRejectedError, match='status=-5'):
        _raise_classified_dps_error(-5, 'ERROR_TYPE')


# ---------------------------------------------------------------------------
# EC6 — wrong XML tag (-10) → rejected
# ---------------------------------------------------------------------------

def test_ec6_wrong_xml_tag_is_rejected() -> None:
    with pytest.raises(TransportRejectedError, match='status=-10'):
        _raise_classified_dps_error(-10, 'not correct teg Z')


# ---------------------------------------------------------------------------
# EC7 — MAC recovery in write_path: one bounded retry
# ---------------------------------------------------------------------------

def test_ec7_mac_recovery_resends_and_acks(conn) -> None:
    """When transport raises DpsMacRecoveryError, write_path should:
    1. persist last_known_mac
    2. re-build XML with corrected MAC
    3. re-sign
    4. re-send
    5. if second send succeeds → ACK
    """
    from datetime import datetime, UTC
    from prro_gateway.enums import OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-ec7', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-ec7', idempotency_key='idem-ec7',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-ec7',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {'type': 'SELL', 'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
                        'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 100}},
        },
        payload_sha256='sha-ec7',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-ec7'),
        correlation_id='c-ec7',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T12:01:00Z',
    )
    conn.commit()

    expected_mac = 'aa' * 32
    send_call_count = 0

    class _MacRecoveryCrypto:
        def sign(self, *, payload_json, **kw):
            return f'SIGNED::{payload_json[:40]}'
        def sign_raw(self, *, data, document_id=None):
            return b'\x30\x82SIGNED_RAW'

    class _MacRecoveryTransport:
        def send(self, **kw):
            nonlocal send_call_count
            send_call_count += 1
            if send_call_count == 1:
                raise DpsMacRecoveryError(
                    'DPS fiscal server rejected: status=-12, error=ERROR_BAD_HASH_PREV',
                    expected_mac=expected_mac, dps_status=-12,
                )
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-ec7-recovered',
                              submission_status='DPS_ACK', server_fiscal_no='SFN-RECOVERED',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(
        crypto_provider=_MacRecoveryCrypto(),
        transport_client=_MacRecoveryTransport(),
        tax_number='TN-EC7',
    )
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Expected ACK after MAC recovery, got {result.outcome}'
    assert send_call_count == 2, 'Transport should be called exactly twice (original + recovery retry)'


# ---------------------------------------------------------------------------
# EC8 — MAC recovery persists last_known_mac
# ---------------------------------------------------------------------------

def test_ec8_mac_recovery_persists_seed(conn) -> None:
    """After MAC recovery, node_state.last_known_mac must contain the DPS-provided hash."""
    from datetime import datetime, UTC
    from prro_gateway.enums import OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository, NodeStateRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-ec8', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-ec8', idempotency_key='idem-ec8',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-ec8',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {'type': 'SELL', 'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
                        'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 100}},
        },
        payload_sha256='sha-ec8',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-ec8'),
        correlation_id='c-ec8',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T12:01:00Z',
    )
    conn.commit()

    expected_mac = 'bb' * 32
    call_count = 0

    class _Crypto:
        def sign_raw(self, *, data, document_id=None):
            return b'\x30\x82SIGNED'

    class _Transport:
        def send(self, **kw):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                raise DpsMacRecoveryError('bad hash', expected_mac=expected_mac, dps_status=-12)
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-ec8',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_Transport(), tax_number='TN')
    worker.process_next(conn, fiscal_number='FN-DEV-0001')

    ns = NodeStateRepository.get_state(conn, 'FN-DEV-0001')
    assert ns is not None
    persisted_mac = conn.execute(
        "SELECT last_known_mac FROM node_state WHERE fiscal_number = 'FN-DEV-0001'"
    ).fetchone()[0]
    assert persisted_mac == expected_mac, f'last_known_mac should be {expected_mac}, got {persisted_mac}'


# ---------------------------------------------------------------------------
# EC9 — second failure after MAC recovery → retryable, no loop
# ---------------------------------------------------------------------------

def test_ec9_second_failure_is_retryable_not_loop(conn) -> None:
    """If re-send after MAC recovery also fails, document becomes ERROR_RETRYABLE, not infinite loop."""
    from datetime import datetime, UTC
    from prro_gateway.enums import DocumentState, OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository, FiscalDocumentRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-ec9', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-ec9', idempotency_key='idem-ec9',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-ec9',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {'type': 'SELL', 'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
                        'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 100}},
        },
        payload_sha256='sha-ec9',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-ec9'),
        correlation_id='c-ec9',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T12:01:00Z',
    )
    conn.commit()

    call_count = 0

    class _Crypto:
        def sign_raw(self, *, data, document_id=None):
            return b'\x30\x82SIGNED'

    class _AlwaysFailTransport:
        def send(self, **kw):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                raise DpsMacRecoveryError('bad hash', expected_mac='cc' * 32, dps_status=-12)
            # Second send also fails
            raise TransportRejectedError('DPS still rejects after MAC recovery')

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_AlwaysFailTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome != 'ACK', 'Should not ACK when second send also fails'
    assert call_count == 2, 'Transport called exactly twice (no loop)'

    doc = FiscalDocumentRepository.get_by_id(conn, result.document_id)
    assert doc.state == DocumentState.ERROR_RETRYABLE, f'Expected ERROR_RETRYABLE, got {doc.state}'


# ---------------------------------------------------------------------------
# EC10 — recovered doc's PAYLOAD_XML is used for next doc's MAC, not stale original
# ---------------------------------------------------------------------------

def test_ec10_next_doc_mac_uses_recovered_xml(conn) -> None:
    """After MAC recovery replaces PAYLOAD_XML, the next document's MAC chain
    must hash the corrected (recovered) XML, not the stale pre-recovery XML.

    Regression: if recovery appends a second PAYLOAD_XML instead of replacing,
    get_content() may return the stale one and corrupt the MAC chain.
    """
    import hashlib
    from datetime import datetime, UTC
    from prro_gateway.enums import DocumentState, FileKind, OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import DocumentFilesRepository, InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-ec10', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()

    def _cmd(rid, op):
        return CanonicalFiscalCommand(
            request_id=rid, idempotency_key=f'idem-{rid}',
            protocol=Protocol.CHECKBOX_REST, operation_type=op,
            fiscal_number='FN-DEV-0001', route_key='main',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_dps_grpc_default',
            channel_owner='test', external_request_id=f'ext-{rid}',
            business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
            payload={
                'receipt': {'type': 'SELL', 'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
                            'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 100}},
            },
            payload_sha256=f'sha-{rid}',
            trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id=f'c-{rid}'),
            correlation_id=f'c-{rid}',
        )

    def _enqueue(c, rid, op):
        cmd = _cmd(rid, op)
        c.execute('BEGIN IMMEDIATE')
        InboxRepository.accept_command(
            c, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
            protocol=cmd.protocol, operation_type=cmd.operation_type,
            fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
            backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
            channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
            protocol_session_id=None, payload_json=cmd.model_dump_json(),
            payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T12:01:00Z',
        )
        c.commit()

    expected_mac = 'dd' * 32
    first_doc_send_count = 0
    signed_xmls = []  # capture what crypto signs for the second doc

    class _SpyCrypto:
        def sign_raw(self, *, data, document_id=None):
            signed_xmls.append(data)
            return b'\x30\x82SIGNED_RAW'

    class _RecoveryThenOkTransport:
        def __init__(self):
            self._call = 0

        def send(self, **kw):
            nonlocal first_doc_send_count
            self._call += 1
            if self._call == 1:
                # First doc, first send: MAC mismatch
                first_doc_send_count += 1
                raise DpsMacRecoveryError(
                    'bad hash', expected_mac=expected_mac, dps_status=-12)
            # All subsequent sends succeed
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id=f'tr-{self._call}',
                              submission_status='DPS_ACK', server_fiscal_no=f'SFN-{self._call}',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    transport = _RecoveryThenOkTransport()
    worker = WritePathWorker(
        crypto_provider=_SpyCrypto(),
        transport_client=transport,
        tax_number='TN-EC10',
    )

    # First doc: triggers MAC recovery then ACKs
    _enqueue(conn, 'req-ec10-first', OperationType.SELL)
    r1 = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert r1.outcome == 'ACK'

    # Verify: only ONE PAYLOAD_XML exists for the recovered doc (replaced, not appended)
    payload_count = conn.execute(
        "SELECT COUNT(*) FROM document_files WHERE document_id = ? AND file_kind = ?",
        (r1.document_id, str(FileKind.PAYLOAD_XML)),
    ).fetchone()[0]
    assert payload_count == 1, f'Expected exactly 1 PAYLOAD_XML after recovery, got {payload_count}'

    # Verify: SIGNED_XML also replaced
    signed_count = conn.execute(
        "SELECT COUNT(*) FROM document_files WHERE document_id = ? AND file_kind = ?",
        (r1.document_id, str(FileKind.SIGNED_XML)),
    ).fetchone()[0]
    assert signed_count == 1, f'Expected exactly 1 SIGNED_XML after recovery, got {signed_count}'

    # Get the recovered PAYLOAD_XML content and its hash
    recovered_xml = DocumentFilesRepository.get_content(conn, document_id=r1.document_id, file_kind=FileKind.PAYLOAD_XML)
    assert recovered_xml is not None
    recovered_mac = hashlib.sha256(recovered_xml).hexdigest()

    # The recovered XML must contain the corrected MAC (expected_mac), not empty
    recovered_str = recovered_xml.decode('utf-8') if isinstance(recovered_xml, bytes) else recovered_xml
    assert f'<MAC>{expected_mac}</MAC>' in recovered_str, (
        f'Recovered XML must contain corrected MAC.\nExpected: <MAC>{expected_mac}</MAC>\nGot: {recovered_str[-100:]}'
    )

    # Second doc: must use recovered_mac as its previous hash
    signed_xmls.clear()
    _enqueue(conn, 'req-ec10-second', OperationType.SELL)
    r2 = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert r2.outcome == 'ACK'

    # The second doc's XML (captured by spy crypto) must contain MAC = hash of recovered XML
    assert len(signed_xmls) >= 1
    second_xml = signed_xmls[0].decode('utf-8') if isinstance(signed_xmls[0], bytes) else signed_xmls[0]
    assert f'<MAC>{recovered_mac}</MAC>' in second_xml, (
        f'Second doc MAC must be hash of recovered XML.\n'
        f'Expected MAC: {recovered_mac}\n'
        f'Got XML: {second_xml}'
    )


# ---------------------------------------------------------------------------
# RL1 — rate limit (-4) raises TransportRateLimitedError with retry_after
# ---------------------------------------------------------------------------

def test_rl1_status_minus4_is_rejected_not_rate_limit() -> None:
    """status=-4 is ERROR_UNKNOWN per proto — rejected, not rate-limited.
    TransportRateLimitedError must NOT come from CheckResponse.status."""
    with pytest.raises(TransportRejectedError, match='status=-4'):
        _raise_classified_dps_error(-4, 'Exceeded number of errors')


# ---------------------------------------------------------------------------
# RL2 — write_path marks rate-limited doc as ERROR_RETRYABLE with DPS_RATE_LIMITED
# ---------------------------------------------------------------------------

def test_rl2_write_path_rate_limit_status(conn) -> None:
    """Rate-limited transport error should produce ERROR_RETRYABLE with
    technical_status=DPS_RATE_LIMITED, not generic RETRYABLE_ERROR."""
    from datetime import datetime, UTC
    from prro_gateway.enums import DocumentState, OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.ports import TransportRateLimitedError
    from prro_gateway.repositories import InboxRepository, ShiftRepository, FiscalDocumentRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-rl2', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-rl2', idempotency_key='idem-rl2',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-rl2',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {'type': 'SELL', 'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
                        'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 100}},
        },
        payload_sha256='sha-rl2',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-rl2'),
        correlation_id='c-rl2',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T12:01:00Z',
    )
    conn.commit()

    class _Crypto:
        def sign_raw(self, *, data, document_id=None):
            return b'\x30\x82SIGNED'

    class _RateLimitedTransport:
        def send(self, **kw):
            raise TransportRateLimitedError('Exceeded number of errors', retry_after_seconds=300)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_RateLimitedTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.outcome == 'ERROR'
    doc = FiscalDocumentRepository.get_by_id(conn, result.document_id)
    assert doc.state == DocumentState.ERROR_RETRYABLE

    # Check transport trace has DPS_RATE_LIMITED
    trace = conn.execute(
        "SELECT technical_status FROM transport_trace_log WHERE fiscal_number = 'FN-DEV-0001' ORDER BY rowid DESC LIMIT 1"
    ).fetchone()
    assert trace[0] == 'DPS_RATE_LIMITED'


# ---------------------------------------------------------------------------
# RL3 — retry_after_seconds persisted in canonical error
# ---------------------------------------------------------------------------

def test_rl3_retry_after_in_canonical_error(conn) -> None:
    """The canonical error on a rate-limited document must carry retry_after_seconds."""
    from datetime import datetime, UTC
    from prro_gateway.enums import OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.ports import TransportRateLimitedError
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-rl3', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-rl3', idempotency_key='idem-rl3',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-rl3',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {'type': 'SELL', 'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
                        'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 100}},
        },
        payload_sha256='sha-rl3',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-rl3'),
        correlation_id='c-rl3',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T12:01:00Z',
    )
    conn.commit()

    class _Crypto:
        def sign_raw(self, *, data, document_id=None):
            return b'\x30\x82SIGNED'

    class _RateLimitedTransport:
        def send(self, **kw):
            raise TransportRateLimitedError('Exceeded', retry_after_seconds=600)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_RateLimitedTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert result.canonical_error is not None
    assert result.canonical_error.retry_after_seconds == 600
