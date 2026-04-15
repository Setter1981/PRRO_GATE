"""DPS offline replay transport field tests (PRRO_GATE-4wi).

Proves that offline sync passes all protocol-critical fields to DPS transport:
  OFR1: transport receives business_ts from document (not now())
  OFR2: transport receives offline_fiscal_no → proto id_offline
  OFR3: transport receives related_receipt_id for offline RETURN → id_cancel
  OFR4: transport receives transport_profile for endpoint/TLS resolution
  OFR5: proto Check.id_offline is set for offline documents
"""
from __future__ import annotations

from datetime import datetime, UTC

import pytest
from prro_gateway.enums import DocumentState, FileKind, OperationType, Protocol, ShiftState
from prro_gateway.ports import SendResult
from prro_gateway.repositories import (
    DocumentFilesRepository, FiscalDocumentRepository, InboxRepository,
    ShiftRepository, TransportProfileRepository,
)
from prro_gateway.services.offline_sync import OfflineSyncService


def _seed_offline_doc(conn, *, doc_id, doc_type='SELL', fiscal_number='FN-DEV-0001',
                      business_ts='2026-04-14T10:00:00+00:00', offline_fiscal_no=100001,
                      related_receipt_id=None, lnd=1):
    """Insert an OFFLINE_LOCAL_ACK document ready for sync."""
    conn.execute('BEGIN IMMEDIATE')
    req_id = f'req-{doc_id}'
    # Inbox record needed for FK
    conn.execute("""
        INSERT OR IGNORE INTO ingress_inbox (
            request_id, idempotency_key, protocol, operation_type, fiscal_number,
            backend_profile_id, transport_profile_id, channel_owner,
            payload_json, payload_sha256, status, response_deadline_at
        ) VALUES (?, ?, 'CHECKBOX_REST', ?, ?, 'backend_checkbox_default',
            'transport_dps_grpc_default', 'test', '{}', 'sha', 'DONE', '2026-04-14T23:00:00Z')
    """, (req_id, f'idem-{doc_id}', doc_type, fiscal_number))
    conn.execute("""
        INSERT INTO fiscal_documents (
            document_id, request_id, fiscal_number, doc_type, state, fs_mode,
            lnd, backend_profile_id, transport_profile_id,
            submission_status, payload_json, payload_sha256, business_ts,
            offline_fiscal_no, offline_fiscal_date, related_receipt_id
        ) VALUES (?, ?, ?, ?, 'OFFLINE_LOCAL_ACK', 'OFFLINE',
            ?, 'backend_checkbox_default', 'transport_dps_grpc_default',
            'OFFLINE_LOCAL', '{}', 'sha', ?, ?, ?, ?)
    """, (doc_id, req_id, fiscal_number, doc_type,
          lnd, business_ts, offline_fiscal_no,
          business_ts, related_receipt_id))
    # Add a dummy signed payload so sync doesn't skip
    DocumentFilesRepository.add_file(
        conn, file_id=f'{doc_id}-signed', document_id=doc_id,
        file_kind=FileKind.SIGNED_XML,
        path=f'/archive/{fiscal_number}/{doc_id}/signed.xml',
        content=b'OFFLINE_SIGNED_STUB',
    )
    conn.commit()


# ---------------------------------------------------------------------------
# OFR1 — business_ts passed to transport
# ---------------------------------------------------------------------------

def test_ofr1_business_ts_from_document(conn) -> None:
    _seed_offline_doc(conn, doc_id='doc-ofr1', business_ts='2026-04-14T10:30:00+00:00')

    captured = {}

    class _CapTransport:
        def send(self, **kw):
            captured.update(kw)
            return SendResult(state='ACK', transport_request_id='tr-ofr1',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    svc = OfflineSyncService(transport_client=_CapTransport())
    svc.sync_pending(conn, fiscal_number='FN-DEV-0001')

    assert 'business_ts' in captured, f'business_ts must be passed. Keys: {list(captured.keys())}'
    bts = captured['business_ts']
    assert bts is not None, 'business_ts must not be None'
    assert '2026' in str(bts), f'business_ts must be from document, not now(). Got: {bts}'


# ---------------------------------------------------------------------------
# OFR2 — offline_fiscal_no passed to transport
# ---------------------------------------------------------------------------

def test_ofr2_offline_fiscal_no_passed(conn) -> None:
    _seed_offline_doc(conn, doc_id='doc-ofr2', offline_fiscal_no=200042, lnd=2)

    captured = {}

    class _CapTransport:
        def send(self, **kw):
            captured.update(kw)
            return SendResult(state='ACK', transport_request_id='tr-ofr2',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    svc = OfflineSyncService(transport_client=_CapTransport())
    svc.sync_pending(conn, fiscal_number='FN-DEV-0001')

    assert captured.get('offline_fiscal_no') == 200042, (
        f'offline_fiscal_no must be 200042. Got: {captured.get("offline_fiscal_no")}'
    )


# ---------------------------------------------------------------------------
# OFR3 — related_receipt_id for offline RETURN
# ---------------------------------------------------------------------------

def test_ofr3_related_receipt_id_for_return(conn) -> None:
    _seed_offline_doc(conn, doc_id='doc-ofr3', doc_type='RETURN',
                      related_receipt_id='ORIG-SELL-555', lnd=3)

    captured = {}

    class _CapTransport:
        def send(self, **kw):
            captured.update(kw)
            return SendResult(state='ACK', transport_request_id='tr-ofr3',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    svc = OfflineSyncService(transport_client=_CapTransport())
    svc.sync_pending(conn, fiscal_number='FN-DEV-0001')

    assert captured.get('related_receipt_id') == 'ORIG-SELL-555', (
        f'related_receipt_id must be preserved. Got: {captured.get("related_receipt_id")}'
    )


# ---------------------------------------------------------------------------
# OFR4 — transport_profile passed for endpoint/TLS
# ---------------------------------------------------------------------------

def test_ofr4_transport_profile_passed(conn) -> None:
    _seed_offline_doc(conn, doc_id='doc-ofr4', lnd=4)

    captured = {}

    class _CapTransport:
        def send(self, **kw):
            captured.update(kw)
            return SendResult(state='ACK', transport_request_id='tr-ofr4',
                              submission_status='DPS_ACK', server_fiscal_no='SFN',
                              response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC))

    svc = OfflineSyncService(transport_client=_CapTransport())
    svc.sync_pending(conn, fiscal_number='FN-DEV-0001')

    tp = captured.get('transport_profile')
    assert tp is not None, 'transport_profile must be passed for endpoint/TLS resolution'
    assert tp.transport_profile_id == 'transport_dps_grpc_default'


# ---------------------------------------------------------------------------
# OFR5 — proto Check.id_offline set for offline docs
# ---------------------------------------------------------------------------

def test_ofr5_proto_id_offline_set() -> None:
    """DPS transport must write offline_fiscal_no to proto Check.id_offline."""
    from prro_gateway.transports.dps_fiscal_server import DpsFiscalServerTransport

    captured = {}

    class _CapStub:
        def sendChkV2(self, req):
            captured['id_offline'] = req.id_offline
            captured['id_cancel'] = req.id_cancel

            class _R:
                id = 'X'; status = 1; error_message = ''
            return _R()

    transport = DpsFiscalServerTransport(grpc_stub=_CapStub())
    transport.send(
        document_id='doc-ofr5', signed_payload=b'\x30\x82test',
        fiscal_number='FN-001', backend_profile_id='bp',
        transport_profile_id='tp', operation_type='SELL',
        lnd=5, offline_fiscal_no=300099,
        related_receipt_id='ORIG-123',
    )
    assert captured['id_offline'] == '300099', f'id_offline must be set. Got: {captured["id_offline"]}'
    assert captured['id_cancel'] == 'ORIG-123', f'id_cancel must be set. Got: {captured["id_cancel"]}'
