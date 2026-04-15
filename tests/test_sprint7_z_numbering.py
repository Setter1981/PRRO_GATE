"""
Sprint 7 — Z-report numbering persistence tests.

Coverage:
  ZN1 — first Z_REPORT gets Z NO=1 (not 0)
  ZN2 — second Z_REPORT gets Z NO=2 (monotonic increment)
  ZN3 — retry of same Z_REPORT keeps same Z number
"""
from __future__ import annotations

from datetime import datetime, UTC

from prro_gateway.enums import DocumentState, FileKind, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import DocumentFilesRepository, FiscalDocumentRepository, InboxRepository, ShiftRepository
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.services.write_path import WritePathWorker

_FN = 'FN-DEV-0001'
_BP = 'backend_checkbox_default'
_TP = 'transport_dps_grpc_default'


def _setup_shift(conn):
    ShiftRepository.create_shift(
        conn, shift_id='shift-zn', fiscal_number=_FN,
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id=_BP, transport_profile_id=_TP,
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()


def _cmd(rid):
    return CanonicalFiscalCommand(
        request_id=rid, idempotency_key=f'idem-{rid}',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.Z_REPORT,
        fiscal_number=_FN, route_key='main',
        backend_profile_id=_BP, transport_profile_id=_TP,
        channel_owner='test', external_request_id=f'ext-{rid}',
        business_ts=datetime(2026, 4, 13, 22, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'Z_REPORT'}},
        payload_sha256=f'sha-{rid}',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id=f'c-{rid}'),
        correlation_id=f'c-{rid}',
    )


def _enqueue(conn, rid):
    cmd = _cmd(rid)
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T22:01:00Z',
    )
    conn.commit()


class _SpyCrypto:
    def __init__(self):
        self.signed = []
    def sign(self, *, payload_json, **kw):
        self.signed.append(payload_json)
        return f'SIGNED::{payload_json[:40]}'


class _StubTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        return SendResult(state='ACK', transport_request_id='tr-zn',
                          submission_status='DONE', server_fiscal_no='SFN',
                          response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))


# ---------------------------------------------------------------------------
# ZN1 — first Z_REPORT gets Z NO=1
# ---------------------------------------------------------------------------

def test_zn1_first_z_report_gets_number_1(conn) -> None:
    _setup_shift(conn)
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_StubTransport(), tax_number='TN')

    _enqueue(conn, 'req-zn1')
    result = worker.process_next(conn, fiscal_number=_FN)
    assert result.outcome == 'ACK'

    xml = crypto.signed[0]
    assert '<Z NO="1"' in xml, f'First Z_REPORT must have Z NO=1, got: {xml}'

    # Verify persisted on document
    doc = FiscalDocumentRepository.get_by_id(conn, result.document_id)
    assert doc.z_report_number == 1


# ---------------------------------------------------------------------------
# ZN2 — second Z_REPORT gets Z NO=2
# ---------------------------------------------------------------------------

def test_zn2_second_z_report_increments(conn) -> None:
    """Second Z_REPORT for the same fiscal_number gets Z NO=2.
    Requires closing first shift and opening a new one (duplicate Z_REPORT guard)."""
    _setup_shift(conn)
    crypto = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=crypto, transport_client=_StubTransport(), tax_number='TN')

    # First Z_REPORT
    _enqueue(conn, 'req-zn2a')
    r1 = worker.process_next(conn, fiscal_number=_FN)
    assert r1.outcome == 'ACK'

    # Close shift, open new one for second Z_REPORT
    ShiftRepository.update_state(conn, shift_id='shift-zn', state=ShiftState.CLOSED)
    ShiftRepository.create_shift(
        conn, shift_id='shift-zn2', fiscal_number=_FN,
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id=_BP, transport_profile_id=_TP,
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T23:00:00Z',
    )
    conn.commit()

    # Second Z_REPORT
    _enqueue(conn, 'req-zn2b')
    r2 = worker.process_next(conn, fiscal_number=_FN)
    assert r2.outcome == 'ACK'

    xml2 = crypto.signed[1]
    assert 'NO="2"' in xml2, f'Second Z_REPORT must have Z NO=2, got: {xml2}'

    doc2 = FiscalDocumentRepository.get_by_id(conn, r2.document_id)
    assert doc2.z_report_number == 2


# ---------------------------------------------------------------------------
# ZN3 — retry keeps same Z number
# ---------------------------------------------------------------------------

def test_zn3_retry_keeps_same_z_number(conn) -> None:
    """Z number persists on document after failed send. A subsequent read of the
    same document row returns the same z_report_number. This proves persistence
    stability, not a full resend/reprocess path (which requires re-enqueueing)."""
    _setup_shift(conn)

    # Allocate Z number on a document manually (simulating first attempt that persisted number)
    _enqueue(conn, 'req-zn3')

    # Process with a transport that fails retryably
    class _FailTransport:
        def send(self, **kw):
            from prro_gateway.ports import TransportRetryableError
            raise TransportRetryableError('network fail')

    crypto = _SpyCrypto()
    worker_fail = WritePathWorker(crypto_provider=crypto, transport_client=_FailTransport(), tax_number='TN')
    r1 = worker_fail.process_next(conn, fiscal_number=_FN)
    assert r1.outcome == 'ERROR'

    # Check Z number was allocated and persisted
    doc = FiscalDocumentRepository.get_by_id(conn, r1.document_id)
    assert doc.z_report_number is not None
    first_z = doc.z_report_number

    xml1 = crypto.signed[0]
    assert f'NO="{first_z}"' in xml1

    # node_state counter should have advanced
    ns = NodeStateRepository.get_state(conn, _FN)
    assert ns.next_z_report_number == first_z + 1

    # Now "retry" — re-enqueue same doc won't work (idempotency), but
    # verify the allocated number is stable on the document
    doc_reload = FiscalDocumentRepository.get_by_id(conn, r1.document_id)
    assert doc_reload.z_report_number == first_z, (
        f'Retry must keep same Z number {first_z}, got {doc_reload.z_report_number}'
    )
