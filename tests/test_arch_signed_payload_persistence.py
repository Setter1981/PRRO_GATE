"""
Architecture — signed payload persistence tests.

Verifies that signed payload content is now persisted in document_files
and can be recovered by offline sync for transport-neutral egress.

Coverage:
  SP1 — write-path persists signed payload content in document_files
  SP2 — DocumentFilesRepository.get_content reads it back
  SP3 — offline sync sends persisted signed payload (not empty string)
"""
from __future__ import annotations

import itertools
import uuid
from datetime import UTC, datetime

from prro_gateway.enums import DocumentState, FileKind, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import DocumentFilesRepository, FiscalDocumentRepository, InboxRepository, ShiftRepository
from prro_gateway.services.offline_sync import OfflineSyncService
from prro_gateway.services.write_path import WritePathWorker

_lnd = itertools.count(60000)
_FN = 'FN-DEV-0001'
_BP = 'backend_checkbox_default'
_TP = 'transport_checkbox_rest_default'


# ---------------------------------------------------------------------------
# SP1 — write-path persists signed payload content
# ---------------------------------------------------------------------------

def test_sp1_write_path_persists_signed_content(conn) -> None:
    """After _stage_sign, document_files SIGNED_XML row must contain actual content."""
    ShiftRepository.create_shift(
        conn, shift_id='shift-sp1', fiscal_number=_FN,
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id=_BP, transport_profile_id=_TP,
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-12T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-sp1', idempotency_key='idem-sp1',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number=_FN, route_key='main',
        backend_profile_id=_BP, transport_profile_id=_TP,
        channel_owner='test', external_request_id='ext-sp1',
        business_ts=datetime(2026, 4, 12, 12, 0, 0, tzinfo=UTC),
        payload={
            'receipt': {
                'type': 'SELL',
                'goods': [{'name': 'X', 'price': 1000, 'quantity': 1000, 'sum': 1000}],
                'payments': [{'amount': 1000, 'type': 'CASH'}],
                'totals': {'total_sum': 1000},
            },
        },
        payload_sha256='sha-sp1',
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

    class _StubCrypto:
        def sign(self, *, payload_json, **kw):
            return f'SIGNED::{payload_json[:50]}'

    class _StubTransport:
        def send(self, **kw):
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-sp1',
                              submission_status='DONE', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    worker = WritePathWorker(crypto_provider=_StubCrypto(), transport_client=_StubTransport())
    result = worker.process_next(conn, fiscal_number=_FN)
    assert result.outcome == 'ACK'

    # Verify content is persisted
    content = DocumentFilesRepository.get_content(conn, document_id=result.document_id, file_kind=FileKind.SIGNED_XML)
    assert content is not None, 'SIGNED_XML content must be persisted'
    assert content.startswith(b'SIGNED::'), f'Content must be the signed payload, got: {content[:30]}'


# ---------------------------------------------------------------------------
# SP2 — get_content reads back correctly
# ---------------------------------------------------------------------------

def test_sp2_get_content_reads_back(conn) -> None:
    """DocumentFilesRepository.get_content returns exact persisted bytes."""
    # Seed a document for FK
    req_id = str(uuid.uuid4())
    doc_id = str(uuid.uuid4())
    conn.execute(
        """INSERT INTO ingress_inbox
            (request_id, idempotency_key, protocol, operation_type,
             fiscal_number, payload_json, payload_sha256, status)
        VALUES (?, ?, 'CHECKBOX_REST', 'SELL', ?, '{}', ?, 'DONE')""",
        (req_id, f'idem-{req_id}', _FN, f'sha-{req_id[:8]}'),
    )
    FiscalDocumentRepository.create_prepared(
        conn, document_id=doc_id, request_id=req_id, fiscal_number=_FN,
        lnd=next(_lnd), doc_type='SELL', backend_profile_id=_BP,
        transport_profile_id=_TP, fs_mode='ONLINE',
        business_ts='2026-04-12T12:00:00Z', payload_json='{}', payload_sha256='sha-x',
    )
    conn.commit()

    test_content = b'<signed-xml>test-content-here</signed-xml>'
    conn.execute('BEGIN IMMEDIATE')
    DocumentFilesRepository.add_file(
        conn, file_id=f'{doc_id}-signed', document_id=doc_id,
        file_kind=FileKind.SIGNED_XML,
        path=f'/archive/{_FN}/{doc_id}/signed.xml',
        content=test_content,
    )
    conn.commit()

    retrieved = DocumentFilesRepository.get_content(conn, document_id=doc_id, file_kind=FileKind.SIGNED_XML)
    assert retrieved == test_content

    # Non-existent returns None
    none_result = DocumentFilesRepository.get_content(conn, document_id=doc_id, file_kind=FileKind.PROTO_RESPONSE)
    assert none_result is None


# ---------------------------------------------------------------------------
# SP3 — offline sync sends persisted signed payload
# ---------------------------------------------------------------------------

def test_sp3_offline_sync_sends_persisted_signed_payload(conn) -> None:
    """Offline sync must read SIGNED_XML content and pass it as signed_payload."""
    req_id = str(uuid.uuid4())
    doc_id = str(uuid.uuid4())
    lnd = next(_lnd)

    conn.execute(
        """INSERT INTO ingress_inbox
            (request_id, idempotency_key, protocol, operation_type,
             fiscal_number, payload_json, payload_sha256, status)
        VALUES (?, ?, 'CHECKBOX_REST', 'SELL', ?, '{}', ?, 'DONE')""",
        (req_id, f'idem-{req_id}', _FN, f'sha-{req_id[:8]}'),
    )
    FiscalDocumentRepository.create_prepared(
        conn, document_id=doc_id, request_id=req_id, fiscal_number=_FN,
        lnd=lnd, doc_type='SELL', backend_profile_id=_BP,
        transport_profile_id=_TP, fs_mode='OFFLINE',
        business_ts='2026-04-12T12:00:00Z', payload_json='{}',
        payload_sha256='sha-sp3', offline_fiscal_no=9001,
    )
    FiscalDocumentRepository.update_state(
        conn, document_id=doc_id, state=DocumentState.OFFLINE_LOCAL_ACK,
        submission_status='OFFLINE_LOCAL',
    )
    # Persist a signed payload as if write-path had stored it
    DocumentFilesRepository.add_file(
        conn, file_id=f'{doc_id}-signed', document_id=doc_id,
        file_kind=FileKind.SIGNED_XML,
        path=f'/archive/{_FN}/{doc_id}/signed.xml',
        content=b'<SignedXML>real-signed-content</SignedXML>',
    )
    conn.commit()

    # Track what signed_payload the transport receives
    captured = {}

    class _CapturingTransport:
        def send(self, *, signed_payload, **kw):
            captured['signed_payload'] = signed_payload
            from prro_gateway.ports import SendResult
            return SendResult(state='ACK', transport_request_id='tr-sp3',
                              submission_status='DONE', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    svc = OfflineSyncService(transport_client=_CapturingTransport())
    result = svc.sync_pending(conn, fiscal_number=_FN)
    assert result.synced == 1

    assert captured.get('signed_payload') == '<SignedXML>real-signed-content</SignedXML>', (
        f'Offline sync must pass persisted signed payload, got: {captured.get("signed_payload")!r}'
    )
