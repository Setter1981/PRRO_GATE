"""
Sprint 7 / corrective fix — DPS XML serializer + sign-input resolution tests.

Coverage:
  DX1 — SHIFT_OPEN XML has <C T="108"> (recoded) and DI="0"
  DX2 — SELL XML has <C T="0"> and DI="{lnd}"
  DX3 — Z_REPORT XML has <Z> tag and DI="{lnd}"
  DX4 — unsupported op raises ValueError
  DX5 — SHIFT_OPEN transport sends local_number=0 regardless of passed lnd
  DX6 — write-path signs DPS XML for DPS profile (not canonical JSON)
  DX8 — MAC chain: SELL gets SHA-256 of previous ACKed doc's SIGNED_XML from core
  DX9 — SHIFT_OPEN gets MAC from prior doc (not hardcoded empty)
  DX10 — date_time and XML <TS> are derived from the same business_ts
  DX7 — write-path signs canonical JSON for Checkbox profile (unchanged)
"""
from __future__ import annotations

from prro_gateway.enums import OperationType
from prro_gateway.serializers.dps_xml import build_dps_xml

import pytest


# ---------------------------------------------------------------------------
# DX1 — SHIFT_OPEN XML shape
# ---------------------------------------------------------------------------

def test_dx1_shift_open_xml_shape() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SHIFT_OPEN,
        fiscal_number='FN-001',
        local_number=42,  # should be ignored — DI="0"
        business_ts='2026-04-12T12:00:00+00:00',
        tax_number='1234567890',
        previous_hash='abc123',
    )
    assert '<RQ ' in xml and 'V="1"' in xml
    assert 'FN="FN-001"' in xml
    assert 'TN="1234567890"' in xml  # populated TN
    assert 'DI="0"' in xml  # SHIFT_OPEN always 0
    assert '<C T="108">' in xml  # service check type (recoded for sendChkV2)
    assert '<MAC>abc123</MAC>' in xml  # previous hash chain
    assert '<TS>' in xml


# ---------------------------------------------------------------------------
# DX2 — SELL XML shape
# ---------------------------------------------------------------------------

def test_dx2_sell_xml_shape() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001',
        local_number=17,
        business_ts='2026-04-12T12:01:00+00:00',
        tax_number='9876543210',
        previous_hash='prevhash42',
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Coffee', 'price': 5000, 'quantity': 1000, 'sum': 5000, 'uktzed': '0901'}],
                'payments': [{'amount': 5000, 'type': 'CASH'}],
                'totals': {'total_sum': 5000},
            },
        },
    )
    assert '<C T="0">' in xml  # sell check type
    assert 'DI="17"' in xml  # real lnd
    assert 'TN="9876543210"' in xml  # populated TN
    assert '<MAC>prevhash42</MAC>' in xml  # hash chain
    assert 'NM="Coffee"' in xml
    assert 'SM="5000"' in xml
    assert 'CZD="0901"' in xml  # uktzed goes to CZD, not CD


# ---------------------------------------------------------------------------
# DX3 — Z_REPORT XML shape
# ---------------------------------------------------------------------------

def test_dx3_zreport_xml_shape() -> None:
    xml = build_dps_xml(
        operation_type=OperationType.Z_REPORT,
        fiscal_number='FN-001',
        local_number=50,
        business_ts='2026-04-12T22:00:00+00:00',
    )
    assert '<Z NO=' in xml  # Z-report tag, not <C>
    assert 'DI="50"' in xml
    assert '<C ' not in xml  # must NOT have <C> tag


# ---------------------------------------------------------------------------
# DX4 — unsupported op
# ---------------------------------------------------------------------------

def test_dx4_unsupported_op_raises() -> None:
    with pytest.raises(ValueError, match='does not support'):
        build_dps_xml(
            operation_type=OperationType.SHIFT_CLOSE,
            fiscal_number='FN-001',
            local_number=0,
        )


# ---------------------------------------------------------------------------
# DX5 — transport SHIFT_OPEN sends local_number=0
# ---------------------------------------------------------------------------

def test_dx5_transport_shift_open_sends_zero() -> None:
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    class _MockResp:
        id = 'X'; status = 1; error_message = ''
    class _Stub:
        def __init__(self): self.calls = []
        def sendChkV2(self, req):
            self.calls.append({'local_number': req.local_number, 'check_type': req.check_type})
            return _MockResp()

    stub = _Stub()
    t = DpsFiscalServerTransport(grpc_stub=stub)
    t.send(
        document_id='d1', signed_payload='<XML/>', fiscal_number='FN',
        backend_profile_id='bp', transport_profile_id='tp',
        operation_type='SHIFT_OPEN', lnd=99,
    )
    assert stub.calls[0]['local_number'] == 0


# ---------------------------------------------------------------------------
# DX6 — write-path signs DPS XML for DPS profile
# ---------------------------------------------------------------------------

def test_dx6_write_path_signs_dps_xml_for_dps_profile(conn) -> None:
    """For a DPS_PRRO_GRPC_ECABINET transport profile, crypto signs DPS XML not canonical JSON."""
    from datetime import datetime, UTC
    from prro_gateway.enums import DocumentState, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import FiscalDocumentRepository, InboxRepository, ShiftRepository, DocumentFilesRepository
    from prro_gateway.services.write_path import WritePathWorker
    from prro_gateway.enums import FileKind

    ShiftRepository.create_shift(
        conn, shift_id='shift-dx6', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-dx6', idempotency_key='idem-dx6',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-dx6',
        business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Item', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                'payments': [{'amount': 1000, 'type': 'CASH'}],
                'totals': {'total_sum': 1000},
            },
        },
        payload_sha256='sha-dx6',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c'),
        correlation_id='c',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-12T12:01:00Z',
    )
    conn.commit()

    # Crypto spy captures what was signed
    signed_inputs = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_inputs.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _StubTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-dx6',
                              submission_status='DONE', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_StubTransport())
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'

    # Verify: signed input was DPS XML, not canonical JSON
    assert len(signed_inputs) == 1
    signed = signed_inputs[0]
    assert signed.startswith('<RQ '), f'Signed input must be DPS XML, got: {signed[:80]}'
    assert '<C T="0">' in signed  # SELL type
    assert '"request_id"' not in signed  # must NOT be canonical JSON

    # Verify: persisted SIGNED_XML is also DPS XML
    content = DocumentFilesRepository.get_content(conn, document_id=result.document_id, file_kind=FileKind.SIGNED_XML)
    assert content is not None
    assert b'<RQ ' in content and b'V="1"' in content


# ---------------------------------------------------------------------------
# DX7 — write-path signs canonical JSON for Checkbox profile (unchanged)
# ---------------------------------------------------------------------------

def test_dx7_write_path_signs_json_for_checkbox_profile(conn) -> None:
    """For CHECKBOX_REST_TRANSPORT profile, crypto still signs canonical JSON."""
    from datetime import datetime, UTC
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-dx7', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-dx7', idempotency_key='idem-dx7',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='test', external_request_id='ext-dx7',
        business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'Item', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                'payments': [{'amount': 1000, 'type': 'CASH'}],
                'totals': {'total_sum': 1000},
            },
        },
        payload_sha256='sha-dx7',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c'),
        correlation_id='c',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-12T12:01:00Z',
    )
    conn.commit()

    signed_inputs = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_inputs.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _StubTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-dx7',
                              submission_status='DONE', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_StubTransport())
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'

    # Verify: signed input was canonical JSON, not DPS XML
    assert len(signed_inputs) == 1
    signed = signed_inputs[0]
    assert '"request_id"' in signed, f'Signed input must be canonical JSON for Checkbox, got: {signed[:80]}'
    assert '<RQ' not in signed

    # Verify: no PAYLOAD_XML persisted for Checkbox path
    from prro_gateway.enums import FileKind as _FK
    from prro_gateway.repositories import DocumentFilesRepository as _DFR
    payload_xml = _DFR.get_content(conn, document_id=result.document_id, file_kind=_FK.PAYLOAD_XML)
    assert payload_xml is None, 'Checkbox path must NOT persist PAYLOAD_XML'


# ---------------------------------------------------------------------------
# DX8 — MAC chain from core: SELL gets SHA-256 of previous doc's SIGNED_XML
# ---------------------------------------------------------------------------

def test_dx8_mac_chain_from_core_persisted_artifact(conn) -> None:
    """SELL DPS XML MAC must be SHA-256 of previous ACKed doc's SIGNED_XML,
    computed by core — NOT from ingress payload passthrough."""
    import hashlib
    from datetime import datetime, UTC
    from prro_gateway.enums import DocumentState, Protocol, ShiftState, FileKind
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import DocumentFilesRepository, FiscalDocumentRepository, InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-dx8', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    def _make_cmd(req_id):
        return CanonicalFiscalCommand(
            request_id=req_id, idempotency_key=f'idem-{req_id}',
            protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
            fiscal_number='FN-DEV-0001', route_key='main',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_dps_grpc_default',
            channel_owner='test', external_request_id=f'ext-{req_id}',
            business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
            payload={
                'receipt': {
                    'type': 'SELL',
                    'goods': [{'name': 'X', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                    'payments': [{'amount': 1000, 'type': 'CASH'}],
                    'totals': {'total_sum': 1000},
                },
            },
            payload_sha256=f'sha-{req_id}',
            trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id=f'c-{req_id}'),
            correlation_id=f'c-{req_id}',
        )

    def _enqueue(c, rid):
        cmd = _make_cmd(rid)
        c.execute('BEGIN IMMEDIATE')
        InboxRepository.accept_command(
            c, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
            protocol=cmd.protocol, operation_type=cmd.operation_type,
            fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
            backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
            channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
            protocol_session_id=None, payload_json=cmd.model_dump_json(),
            payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-12T12:01:00Z',
        )
        c.commit()

    signed_inputs = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_inputs.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _StubTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id=f'tr-dx8',
                              submission_status='DONE', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_StubTransport(), tax_number='TN-TEST')

    # First SELL — no prior doc, MAC should be empty
    _enqueue(conn, 'req-dx8a')
    r1 = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert r1.outcome == 'ACK'
    xml1 = signed_inputs[0]
    assert '<MAC></MAC>' in xml1, f'First doc MAC must be empty, got: {xml1}'

    # Get the persisted PAYLOAD_XML (unsigned DPS XML) of first doc — MAC source
    payload_xml1 = DocumentFilesRepository.get_content(conn, document_id=r1.document_id, file_kind=FileKind.PAYLOAD_XML)
    assert payload_xml1 is not None, 'PAYLOAD_XML must be persisted for DPS documents'
    expected_mac = hashlib.sha256(payload_xml1).hexdigest()

    # Also verify SIGNED_XML is persisted separately
    signed_xml1 = DocumentFilesRepository.get_content(conn, document_id=r1.document_id, file_kind=FileKind.SIGNED_XML)
    assert signed_xml1 is not None, 'SIGNED_XML must also be persisted'

    # Second SELL — MAC must be SHA-256 of first doc's PAYLOAD_XML (not SIGNED_XML)
    _enqueue(conn, 'req-dx8b')
    r2 = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert r2.outcome == 'ACK'
    xml2 = signed_inputs[1]
    assert f'<MAC>{expected_mac}</MAC>' in xml2, (
        f'Second doc MAC must be SHA-256 of first doc PAYLOAD_XML.\n'
        f'Expected: {expected_mac}\n'
        f'Got XML: {xml2}'
    )


# ---------------------------------------------------------------------------
# DX9 — SHIFT_OPEN gets MAC from prior doc (not hardcoded empty)
# ---------------------------------------------------------------------------

def test_dx9_shift_open_mac_from_prior_doc(conn) -> None:
    """SHIFT_OPEN is NOT exempt from MAC chain. When a prior ACKed doc exists
    for the same fiscal_number+transport, SHIFT_OPEN must carry its hash.
    Empty MAC only for true first-ever document (no prior exists).

    Evidence: WebCheck 6651a.xml — T=108 (shift-open recoded) with non-empty MAC.
    """
    import hashlib
    from datetime import datetime, UTC
    from prro_gateway.enums import DocumentState, Protocol, ShiftState, FileKind
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import DocumentFilesRepository, FiscalDocumentRepository, InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    # Create shift + first SELL to establish a prior doc
    ShiftRepository.create_shift(
        conn, shift_id='shift-dx9', fiscal_number='FN-DEV-0001',
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
                'receipt': {
                    'type': op.value,
                    'goods': [{'name': 'X', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                    'payments': [{'amount': 1000, 'type': 'CASH'}],
                    'totals': {'total_sum': 1000},
                },
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

    signed_inputs = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_inputs.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _StubTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-dx9',
                              submission_status='DONE', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_StubTransport(), tax_number='TN-DX9')

    # First: SELL → ACK (establishes a prior doc with persisted SIGNED_XML)
    _enqueue(conn, 'req-dx9-sell', OperationType.SELL)
    r_sell = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert r_sell.outcome == 'ACK'

    # Close shift so we can open a new one
    ShiftRepository.update_state(conn, shift_id='shift-dx9', state=ShiftState.CLOSED)
    conn.commit()

    # Get hash of SELL's PAYLOAD_XML (unsigned DPS XML) — correct MAC source
    content_sell = DocumentFilesRepository.get_content(conn, document_id=r_sell.document_id, file_kind=FileKind.PAYLOAD_XML)
    assert content_sell is not None, 'PAYLOAD_XML must be persisted for DPS SELL'
    expected_mac = hashlib.sha256(content_sell).hexdigest()

    # Now SHIFT_OPEN — must carry MAC of prior doc, NOT empty
    _enqueue(conn, 'req-dx9-open', OperationType.SHIFT_OPEN)
    r_open = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert r_open.outcome == 'ACK'

    xml_open = signed_inputs[1]
    assert f'<MAC>{expected_mac}</MAC>' in xml_open, (
        f'SHIFT_OPEN MAC must be SHA-256 of prior doc, not empty.\n'
        f'Expected: {expected_mac}\n'
        f'Got XML: {xml_open}'
    )


# ---------------------------------------------------------------------------
# DX10 — date_time and XML <TS> derived from the same business_ts
# ---------------------------------------------------------------------------

def test_dx10_date_time_matches_xml_ts() -> None:
    """Transport date_time must be derived from the same business_ts as XML <TS>,
    not from send-time now(). This prevents ERROR_XML_DATE on delayed send/retry."""
    import re
    from datetime import datetime, timezone
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport, _kyiv_local_epoch
    from prro_gateway.serializers.dps_xml import build_dps_xml

    # Use a specific business_ts (not now) to prove transport uses it
    business_ts = datetime(2026, 6, 15, 10, 30, 0, tzinfo=timezone.utc)

    # Build XML — extracts <TS> from business_ts in Kyiv time
    xml = build_dps_xml(
        operation_type=OperationType.SELL,
        fiscal_number='FN-001', local_number=5,
        business_ts=business_ts, tax_number='TN',
    )
    ts_match = re.search(r'<TS>(\d{14})</TS>', xml)
    assert ts_match, f'XML must contain <TS>, got: {xml}'
    ts_from_xml = ts_match.group(1)

    # Compute expected date_time the same way transport should
    expected_dt = _kyiv_local_epoch(business_ts)
    # Verify: parsing TS digits as UTC epoch should equal expected_dt
    from datetime import datetime as dt
    ts_as_utc = dt.strptime(ts_from_xml, '%Y%m%d%H%M%S').replace(tzinfo=timezone.utc)
    assert int(ts_as_utc.timestamp()) == expected_dt, (
        f'date_time ({expected_dt}) must match TS "{ts_from_xml}" interpreted as epoch ({int(ts_as_utc.timestamp())})'
    )

    # Now verify transport actually uses business_ts kwarg
    class _MockResp:
        id = 'X'; status = 1; error_message = ''
    class _CapStub:
        def __init__(self): self.captured_dt = None
        def sendChkV2(self, req):
            self.captured_dt = req.date_time
            return _MockResp()

    stub = _CapStub()
    transport = DpsFiscalServerTransport(grpc_stub=stub)
    transport.send(
        document_id='dx10', signed_payload=b'\x30\x82test',
        fiscal_number='FN-001', backend_profile_id='bp', transport_profile_id='tp',
        operation_type='SELL', business_ts=business_ts,
    )
    assert stub.captured_dt == expected_dt, (
        f'Transport date_time must use business_ts. Expected {expected_dt}, got {stub.captured_dt}'
    )


# ---------------------------------------------------------------------------
# DX11 — MAC seed fallback from node_state.last_known_mac
# ---------------------------------------------------------------------------

def test_dx11_mac_seed_from_node_state(conn) -> None:
    """When no prior ACKed document exists but node_state.last_known_mac is set,
    _resolve_dps_mac must use the seed MAC instead of returning empty.

    This is the bootstrap scenario: fresh DB against a DPS FN with existing history.
    """
    from datetime import datetime, UTC
    from prro_gateway.enums import Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    seed_mac = 'abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789'

    # Set last_known_mac on the node
    conn.execute("UPDATE node_state SET last_known_mac = ? WHERE fiscal_number = 'FN-DEV-0001'", (seed_mac,))
    conn.commit()

    # Create shift
    ShiftRepository.create_shift(
        conn, shift_id='shift-dx11', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-dx11', idempotency_key='idem-dx11',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-dx11',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'X', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                'payments': [{'amount': 1000, 'type': 'CASH'}],
                'totals': {'total_sum': 1000},
            },
        },
        payload_sha256='sha-dx11',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-dx11'),
        correlation_id='c-dx11',
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

    signed_inputs = []

    class _SpyCrypto:
        def sign(self, *, payload_json, **kw):
            signed_inputs.append(payload_json)
            return f'SIGNED::{payload_json[:40]}'

    class _StubTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-dx11',
                              submission_status='DONE', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_StubTransport(), tax_number='TN-DX11')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK'
    assert len(signed_inputs) == 1
    assert f'<MAC>{seed_mac}</MAC>' in signed_inputs[0], (
        f'MAC must fall back to node_state.last_known_mac when no prior ACKed doc exists.\n'
        f'Expected MAC: {seed_mac}\n'
        f'Got XML: {signed_inputs[0]}'
    )
