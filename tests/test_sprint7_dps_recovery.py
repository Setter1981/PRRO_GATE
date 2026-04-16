"""
Sprint 7 / step 3 — DPS fiscal-server lastChk recovery integration test.

Coverage:
  DR1 — SENT DPS doc → reconciliation with lastChk match → ACK
"""
from __future__ import annotations

import itertools
import uuid
from datetime import UTC, datetime

from prro_gateway.enums import DocumentState, OperationType, Protocol
from prro_gateway.ports import PollResult
from prro_gateway.repositories import FiscalDocumentRepository, InboxRepository
from prro_gateway.services.reconciliation import ReconciliationService

_lnd = itertools.count(55000)
_FN = 'FN-DEV-0001'
_BP = 'backend_checkbox_default'
_TP = 'transport_dps_grpc_default'


def test_dr1_reconciliation_lastchk_acks_sent_dps_doc(conn) -> None:
    """A SENT DPS document with transport_request_id must be ACKed by reconciliation
    when lastChk returns a matching response.id."""

    # Seed inbox + document in SENT state with a transport_request_id
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
        transport_profile_id=_TP, fs_mode='ONLINE',
        business_ts='2026-04-13T12:00:00Z', payload_json='{}',
        payload_sha256=f'sha-{doc_id[:8]}',
    )
    # Move to SENT with a transport_request_id (as if sendChkV2 returned OK but response was lost)
    FiscalDocumentRepository.update_state(
        conn, document_id=doc_id, state=DocumentState.SENT,
        transport_request_id='DPS-FISCAL-42',
        sent_at=datetime.now(UTC).isoformat(),
    )
    conn.commit()

    # Verify doc is pending for reconciliation
    pending = FiscalDocumentRepository.get_pending_for_reconciliation(conn)
    assert any(d.document_id == doc_id for d in pending)

    # Mock transport that simulates lastChk returning a match
    class _MockDpsTransport:
        def poll_status(self, *, document_id, fiscal_number, backend_profile_id,
                        transport_profile_id, transport_request_id, operation_type=None,
                        transport_profile=None, **kwargs):
            crypto = kwargs.get('crypto_provider')
            # Verify crypto_provider was passed through
            assert crypto is not None, 'crypto_provider must be passed to poll_status'
            assert hasattr(crypto, 'sign_raw'), 'crypto must have sign_raw'

            # Simulate lastChk: response.id matches our transport_request_id
            if transport_request_id == 'DPS-FISCAL-42':
                return PollResult(
                    state=DocumentState.ACK.value,
                    submission_status='DPS_ACK',
                    server_fiscal_no='DPS-FISCAL-42',
                    server_fiscal_date=datetime.now(UTC).isoformat(),
                    response_json='{"id":"DPS-FISCAL-42","status":1}',
                    ack_at=datetime.now(UTC),
                )
            return PollResult(state=DocumentState.ERROR_RETRYABLE.value,
                              submission_status='DPS_LASTCHK_MISMATCH', retryable=True)

    class _MockCrypto:
        def sign(self, *, document_id, payload_json):
            return payload_json
        def sign_raw(self, *, data, document_id=None):
            return b'SIGNED::' + data

    svc = ReconciliationService(
        transport_status_client=_MockDpsTransport(),
        crypto_provider=_MockCrypto(),
    )
    result = svc.reconcile_pending(conn)

    assert result.acked == 1, f'Expected 1 ACKed doc, got {result}'

    # Verify document state in DB
    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc.state == DocumentState.ACK, f'Document must be ACK, got {doc.state}'
    assert doc.server_fiscal_no == 'DPS-FISCAL-42'
