"""
Architecture cleanup step A — protocol preservation tests.

Verifies that reconciliation and offline sync recover the original ingress
protocol from ingress_inbox, not hardcode Protocol.CHECKBOX_REST.

Coverage:
  P1 — reconciliation SHIFT_OPEN preserves non-Checkbox ingress protocol
  P2 — offline sync SHIFT_OPEN preserves non-Checkbox ingress protocol
  P3 — Checkbox-origin case still works (no regression)
"""
from __future__ import annotations

import itertools
import uuid
from datetime import UTC, datetime

from prro_gateway.enums import DocumentState, OperationType, Protocol, ShiftState
from prro_gateway.ports import SendResult
from prro_gateway.repositories import FiscalDocumentRepository, InboxRepository, ShiftRepository
from prro_gateway.services.offline_sync import OfflineSyncService
from prro_gateway.services.reconciliation import ReconciliationService

_lnd_counter = itertools.count(70000)
_FN = 'FN-DEV-0001'
_BP = 'backend_checkbox_default'
_TP = 'transport_checkbox_rest_default'


def _seed_shift_open_doc(conn, *, protocol: str, state: DocumentState, submission_status: str | None = None) -> str:
    """Seed an ingress_inbox + fiscal_document for SHIFT_OPEN with given protocol."""
    doc_id = str(uuid.uuid4())
    req_id = str(uuid.uuid4())
    lnd = next(_lnd_counter)

    conn.execute(
        """INSERT INTO ingress_inbox
            (request_id, idempotency_key, protocol, operation_type,
             fiscal_number, payload_json, payload_sha256, status)
        VALUES (?, ?, ?, 'SHIFT_OPEN', ?, '{}', ?, 'DONE')""",
        (req_id, f'shift_open:{_FN}:{req_id}', protocol, _FN, f'sha-{req_id[:8]}'),
    )
    FiscalDocumentRepository.create_prepared(
        conn,
        document_id=doc_id, request_id=req_id, fiscal_number=_FN,
        lnd=lnd, doc_type='SHIFT_OPEN',
        backend_profile_id=_BP, transport_profile_id=_TP,
        fs_mode='ONLINE' if state != DocumentState.OFFLINE_LOCAL_ACK else 'OFFLINE',
        business_ts='2026-04-12T12:00:00Z', payload_json='{}',
        payload_sha256=f'sha-{doc_id[:8]}',
    )
    FiscalDocumentRepository.update_state(
        conn, document_id=doc_id, state=state,
        submission_status=submission_status,
    )
    conn.commit()
    return doc_id


# ---------------------------------------------------------------------------
# P1 — reconciliation preserves non-Checkbox protocol
# ---------------------------------------------------------------------------

def test_p1_reconciliation_preserves_webcheck_protocol(conn) -> None:
    """SHIFT_OPEN from WEBCHECK_XMLRPC ingress → reconciliation ACK →
    shift.opened_via_protocol must be WEBCHECK_XMLRPC, not CHECKBOX_REST."""

    doc_id = _seed_shift_open_doc(conn, protocol='WEBCHECK_XMLRPC', state=DocumentState.SENT)

    # Simulate reconciliation ACK for this SHIFT_OPEN
    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)

    class _MockDoc:
        document_id = doc.document_id
        request_id = doc.request_id
        fiscal_number = doc.fiscal_number
        doc_type = doc.doc_type
        fs_mode = doc.fs_mode
        backend_profile_id = doc.backend_profile_id
        transport_profile_id = doc.transport_profile_id
        ack_at = '2026-04-12T12:05:00Z'
        sent_at = '2026-04-12T12:01:00Z'
        payload_json = doc.payload_json

    ReconciliationService._apply_shift_side_effects_locked(
        conn, doc=_MockDoc(), target_state=DocumentState.ACK,
    )
    conn.commit()

    shift = ShiftRepository.get_active_shift(conn, _FN)
    assert shift is not None, 'Shift must be created'
    assert shift.state == ShiftState.OPENED
    assert shift.opened_via_protocol == 'WEBCHECK_XMLRPC', (
        f'Protocol must be WEBCHECK_XMLRPC, got {shift.opened_via_protocol}'
    )


# ---------------------------------------------------------------------------
# P2 — offline sync preserves non-Checkbox protocol
# ---------------------------------------------------------------------------

def test_p2_offline_sync_preserves_maria_protocol(conn) -> None:
    """SHIFT_OPEN from MARIA_TCP ingress → offline sync ACK →
    shift.opened_via_protocol must be MARIA_TCP, not CHECKBOX_REST."""

    doc_id = _seed_shift_open_doc(
        conn, protocol='MARIA_TCP',
        state=DocumentState.OFFLINE_LOCAL_ACK,
        submission_status='OFFLINE_LOCAL',
    )

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)

    class _MockDoc:
        document_id = doc.document_id
        request_id = doc.request_id
        fiscal_number = doc.fiscal_number
        doc_type = doc.doc_type
        fs_mode = doc.fs_mode
        backend_profile_id = doc.backend_profile_id
        transport_profile_id = doc.transport_profile_id
        ack_at = '2026-04-12T12:05:00Z'
        payload_json = doc.payload_json

    OfflineSyncService._apply_shift_side_effects_locked(
        conn, doc=_MockDoc(), target_state=DocumentState.ACK,
    )
    conn.commit()

    shift = ShiftRepository.get_active_shift(conn, _FN)
    assert shift is not None, 'Shift must be created'
    assert shift.state == ShiftState.OPENED
    assert shift.opened_via_protocol == 'MARIA_TCP', (
        f'Protocol must be MARIA_TCP, got {shift.opened_via_protocol}'
    )


# ---------------------------------------------------------------------------
# P3 — Checkbox-origin case still works
# ---------------------------------------------------------------------------

def test_p3_checkbox_origin_still_works(conn) -> None:
    """SHIFT_OPEN from CHECKBOX_REST ingress → reconciliation ACK →
    shift.opened_via_protocol must be CHECKBOX_REST (no regression)."""

    doc_id = _seed_shift_open_doc(conn, protocol='CHECKBOX_REST', state=DocumentState.SENT)

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)

    class _MockDoc:
        document_id = doc.document_id
        request_id = doc.request_id
        fiscal_number = doc.fiscal_number
        doc_type = doc.doc_type
        fs_mode = doc.fs_mode
        backend_profile_id = doc.backend_profile_id
        transport_profile_id = doc.transport_profile_id
        ack_at = '2026-04-12T12:05:00Z'
        sent_at = '2026-04-12T12:01:00Z'
        payload_json = doc.payload_json

    ReconciliationService._apply_shift_side_effects_locked(
        conn, doc=_MockDoc(), target_state=DocumentState.ACK,
    )
    conn.commit()

    shift = ShiftRepository.get_active_shift(conn, _FN)
    assert shift is not None
    assert shift.opened_via_protocol == 'CHECKBOX_REST'
