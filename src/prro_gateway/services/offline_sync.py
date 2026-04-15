"""
OfflineSyncService — DPS submission orchestrator for OFFLINE_LOCAL_ACK documents.

Responsibility:
  Take documents that were locally accepted in OFFLINE mode (state=OFFLINE_LOCAL_ACK,
  submission_status=OFFLINE_LOCAL) and submit them to DPS via the existing transport
  seam, in the deterministic fiscal-sequence order required by DPS.

State transitions performed here:
  OFFLINE_LOCAL_ACK → ACK                           DPS confirmed synchronously
  OFFLINE_LOCAL_ACK → SENT / KVT1 / KVT2           DPS accepted async; reconciliation polls
  OFFLINE_LOCAL_ACK → REJECTED                      DPS business rejection (terminal)
  OFFLINE_LOCAL_ACK → OFFLINE_LOCAL_ACK             retryable failure; recovery_attempts++
  OFFLINE_LOCAL_ACK → REQUIRES_MANUAL_RECONCILIATION terminal failure or max retries exhausted

Invariants preserved:
  #1  Transport call is issued OUTSIDE the SQLite write transaction.
  #2  fiscal_number scope is passed through to the selector for single-writer isolation.
  #4  expected_states guard on every update_state prevents concurrent state corruption.
  #8  No transition is silently skipped; every outcome is persisted with an audit event.
"""
from __future__ import annotations

import logging
import sqlite3
from datetime import UTC, datetime

from ..enums import DocumentState, FileKind, OperationType, Protocol, ShiftState
from ..models.common import StrictModel
from ..ports import SendResult, TransportClient, TransportRejectedError, TransportRetryableError
from ..repositories import AuditRepository, DocumentFilesRepository, FiscalDocumentRepository, InboxRepository, OutboxRepository, ShiftRepository, TransportProfileRepository, TransportTraceRepository
from ..utils.json_codec import dumps_json


class OfflineSyncRunResult(StrictModel):
    checked: int = 0
    synced: int = 0         # OFFLINE_LOCAL_ACK → ACK (DPS confirmed)
    pending_async: int = 0  # OFFLINE_LOCAL_ACK → SENT/KVT1/KVT2 (reconciliation will poll)
    rejected: int = 0       # OFFLINE_LOCAL_ACK → REJECTED (DPS business rejection)
    retryable: int = 0      # stayed OFFLINE_LOCAL_ACK; recovery_attempts incremented
    manual: int = 0         # OFFLINE_LOCAL_ACK → REQUIRES_MANUAL_RECONCILIATION


class OfflineSyncService:
    def __init__(
        self,
        *,
        transport_client: TransportClient,
        max_recovery_attempts: int = 5,
    ) -> None:
        self.transport_client = transport_client
        self.max_recovery_attempts = max_recovery_attempts

    def sync_pending(
        self,
        conn: sqlite3.Connection,
        *,
        fiscal_number: str | None = None,
    ) -> OfflineSyncRunResult:
        """Submit OFFLINE_LOCAL_ACK documents to DPS in fiscal-sequence order.

        Transport calls are issued OUTSIDE the write transaction (INV-1).
        Each outcome is committed in a short BEGIN IMMEDIATE...COMMIT block
        with an expected_states=(OFFLINE_LOCAL_ACK,) guard (INV-4).

        Pass fiscal_number to restrict to a single write-path scope (INV-2).
        """
        docs = FiscalDocumentRepository.get_pending_for_offline_sync(conn, fiscal_number=fiscal_number)
        result = OfflineSyncRunResult(checked=len(docs))

        for doc in docs:
            # --- Transport call OUTSIDE transaction (INV-1) ---
            send_result: SendResult | None = None
            transport_error: Exception | None = None
            try:
                # Read persisted signed payload; warn if missing (pre-migration docs or unsigned).
                stored_content = DocumentFilesRepository.get_content(
                    conn, document_id=doc.document_id, file_kind=FileKind.SIGNED_XML,
                )
                if stored_content is not None:
                    signed_payload = stored_content.decode('utf-8') if isinstance(stored_content, bytes) else stored_content
                else:
                    signed_payload = ''
                    logging.getLogger('prro_gateway.offline_sync').warning(
                        'offline_sync_missing_signed_payload',
                        extra={'extra_fields': {'document_id': doc.document_id}},
                    )
                transport_profile = TransportProfileRepository.get_by_id(conn, doc.transport_profile_id)
                business_ts = None
                if doc.business_ts:
                    try:
                        business_ts = datetime.fromisoformat(doc.business_ts)
                    except (ValueError, TypeError):
                        pass
                send_result = self.transport_client.send(
                    document_id=doc.document_id,
                    signed_payload=signed_payload,
                    fiscal_number=doc.fiscal_number,
                    backend_profile_id=doc.backend_profile_id,
                    transport_profile_id=doc.transport_profile_id,
                    operation_type=doc.doc_type,
                    request_payload_json=doc.payload_json,
                    lnd=doc.lnd,
                    business_ts=business_ts,
                    related_receipt_id=doc.related_receipt_id,
                    transport_profile=transport_profile,
                    offline_fiscal_no=doc.offline_fiscal_no,
                )
            except TransportRetryableError as exc:
                transport_error = exc
            except TransportRejectedError as exc:
                transport_error = exc

            # Parse DPS state BEFORE opening the transaction to prevent ValueError
            # from DocumentState() landing inside an open BEGIN IMMEDIATE and leaving
            # the connection in an un-committable state for subsequent iterations.
            send_state: DocumentState | None = None
            if send_result is not None:
                try:
                    send_state = DocumentState(send_result.state) if send_result.state else DocumentState.SENT
                except ValueError:
                    # DPS returned an unrecognised status string.  Treat as retryable so we
                    # don't advance to an unknown state; next sync run will re-attempt.
                    transport_error = TransportRetryableError(
                        f'Unrecognised DPS state: {send_result.state!r}'
                    )
                    send_result = None

            # --- Short write transaction: commit exactly one outcome ---
            # The outer try/except ensures any unexpected exception (DB constraint,
            # assertion, etc.) triggers a rollback so the connection stays clean for
            # the next document in the batch.
            conn.execute('BEGIN IMMEDIATE')
            try:
                if send_result is not None:
                    assert send_state is not None  # set above when send_result is set

                    if send_state == DocumentState.ACK:
                        ack_at = (send_result.ack_at or datetime.now(UTC)).isoformat()
                        updated = FiscalDocumentRepository.update_state(
                            conn,
                            document_id=doc.document_id,
                            state=DocumentState.ACK,
                            response_json=send_result.response_json,
                            submission_status=send_result.submission_status or 'ACK',
                            transport_request_id=send_result.transport_request_id,
                            server_fiscal_no=send_result.server_fiscal_no,
                            server_fiscal_date=send_result.server_fiscal_date,
                            sent_at=(send_result.sent_at or datetime.now(UTC)).isoformat(),
                            ack_at=ack_at,
                            expected_states=(DocumentState.OFFLINE_LOCAL_ACK,),
                        )
                        if updated is None:
                            conn.rollback()
                            continue
                        self._apply_shift_side_effects_locked(conn, doc=doc, target_state=DocumentState.ACK)
                        OutboxRepository.enqueue_document(
                            conn,
                            outbox_id=f'{doc.document_id}-offline-sync-outbox',
                            aggregate_id=doc.document_id,
                            fiscal_number=doc.fiscal_number,
                            destination='CLOUD_HUB',
                            payload_path=f'/archive/{doc.fiscal_number}/{doc.document_id}/response.json',
                            payload_sha256='offline-sync',
                            sequence_no=doc.lnd,
                        )
                        AuditRepository.log_event(
                            conn,
                            entity_type='DOCUMENT',
                            entity_id=doc.document_id,
                            event_type='OFFLINE_SYNC_ACK',
                            severity='INFO',
                            event_payload_json=dumps_json({
                                'state': 'ACK',
                                'offline_fiscal_no': doc.offline_fiscal_no,
                                'server_fiscal_no': send_result.server_fiscal_no,
                            }),
                        )
                        TransportTraceRepository.log_event(
                            conn,
                            trace_id=f'{doc.document_id}-offline-sync',
                            fiscal_number=doc.fiscal_number,
                            backend_profile_id=doc.backend_profile_id,
                            transport_profile_id=doc.transport_profile_id,
                            direction='OUT',
                            response_envelope=send_result.response_json,
                            technical_status='OFFLINE_SYNC',
                            business_status=send_result.submission_status,
                        )
                        conn.commit()
                        result.synced += 1

                    elif send_state == DocumentState.REJECTED:
                        # DPS business rejection: terminal, consistent with
                        # write_path and reconciliation REJECTED handling.
                        updated = FiscalDocumentRepository.update_state(
                            conn,
                            document_id=doc.document_id,
                            state=DocumentState.REJECTED,
                            response_json=send_result.response_json,
                            submission_status=send_result.submission_status or 'REJECTED',
                            transport_request_id=send_result.transport_request_id,
                            expected_states=(DocumentState.OFFLINE_LOCAL_ACK,),
                        )
                        if updated is None:
                            conn.rollback()
                            continue
                        self._apply_shift_side_effects_locked(conn, doc=doc, target_state=DocumentState.REJECTED)
                        AuditRepository.log_event(
                            conn,
                            entity_type='DOCUMENT',
                            entity_id=doc.document_id,
                            event_type='OFFLINE_SYNC_REJECTED',
                            severity='WARNING',
                            event_payload_json=dumps_json({
                                'state': 'REJECTED',
                                'offline_fiscal_no': doc.offline_fiscal_no,
                                'transport_request_id': send_result.transport_request_id,
                            }),
                        )
                        TransportTraceRepository.log_event(
                            conn,
                            trace_id=f'{doc.document_id}-offline-sync-rejected',
                            fiscal_number=doc.fiscal_number,
                            backend_profile_id=doc.backend_profile_id,
                            transport_profile_id=doc.transport_profile_id,
                            direction='OUT',
                            response_envelope=send_result.response_json,
                            technical_status='OFFLINE_SYNC_REJECTED',
                            business_status=send_result.submission_status,
                        )
                        conn.commit()
                        result.rejected += 1

                    elif send_state in (DocumentState.SENT, DocumentState.KVT1, DocumentState.KVT2):
                        # DPS accepted async: reconciliation will poll
                        updated = FiscalDocumentRepository.update_state(
                            conn,
                            document_id=doc.document_id,
                            state=send_state,
                            response_json=send_result.response_json,
                            submission_status=send_result.submission_status,
                            transport_request_id=send_result.transport_request_id,
                            server_fiscal_no=send_result.server_fiscal_no,
                            server_fiscal_date=send_result.server_fiscal_date,
                            sent_at=(send_result.sent_at or datetime.now(UTC)).isoformat(),
                            expected_states=(DocumentState.OFFLINE_LOCAL_ACK,),
                        )
                        if updated is None:
                            conn.rollback()
                            continue
                        AuditRepository.log_event(
                            conn,
                            entity_type='DOCUMENT',
                            entity_id=doc.document_id,
                            event_type='OFFLINE_SYNC_SENT',
                            severity='INFO',
                            event_payload_json=dumps_json({
                                'state': send_state.value,
                                'offline_fiscal_no': doc.offline_fiscal_no,
                                'transport_request_id': send_result.transport_request_id,
                            }),
                        )
                        TransportTraceRepository.log_event(
                            conn,
                            trace_id=f'{doc.document_id}-offline-sync-sent',
                            fiscal_number=doc.fiscal_number,
                            backend_profile_id=doc.backend_profile_id,
                            transport_profile_id=doc.transport_profile_id,
                            direction='OUT',
                            response_envelope=send_result.response_json,
                            technical_status='OFFLINE_SYNC_SENT',
                            business_status=send_result.submission_status,
                        )
                        conn.commit()
                        result.pending_async += 1

                    else:
                        # Unexpected but valid DocumentState (e.g. ERROR_RETRYABLE,
                        # PREPARED).  Treat as retryable — don't advance to a state
                        # that offline sync doesn't understand.
                        conn.rollback()
                        transport_error = TransportRetryableError(
                            f'Unexpected DPS state for offline sync: {send_state.value!r}'
                        )
                        send_result = None
                        # Handle inline: same ceiling logic as the main error path.
                        current = FiscalDocumentRepository.get_by_id(conn, doc.document_id)
                        attempts = (current.recovery_attempts if current else 0) + 1
                        is_terminal = attempts >= self.max_recovery_attempts

                        conn.execute('BEGIN IMMEDIATE')
                        if is_terminal:
                            updated = FiscalDocumentRepository.update_state(
                                conn,
                                document_id=doc.document_id,
                                state=DocumentState.REQUIRES_MANUAL_RECONCILIATION,
                                error_message=str(transport_error),
                                expected_states=(DocumentState.OFFLINE_LOCAL_ACK,),
                            )
                            if updated is None:
                                conn.rollback()
                                continue
                            event_type = 'OFFLINE_SYNC_FAILED_TERMINAL'
                            severity = 'ERROR'
                        else:
                            event_type = 'OFFLINE_SYNC_FAILED_RETRYABLE'
                            severity = 'WARNING'

                        FiscalDocumentRepository.set_recovery_attempts(
                            conn, document_id=doc.document_id, recovery_attempts=attempts,
                        )
                        AuditRepository.log_event(
                            conn,
                            entity_type='DOCUMENT',
                            entity_id=doc.document_id,
                            event_type=event_type,
                            severity=severity,
                            event_payload_json=dumps_json({
                                'error': str(transport_error),
                                'recovery_attempts': attempts,
                                'offline_fiscal_no': doc.offline_fiscal_no,
                            }),
                        )
                        TransportTraceRepository.log_event(
                            conn,
                            trace_id=f'{doc.document_id}-offline-sync-unexpected-{attempts}',
                            fiscal_number=doc.fiscal_number,
                            backend_profile_id=doc.backend_profile_id,
                            transport_profile_id=doc.transport_profile_id,
                            direction='OUT',
                            technical_status='OFFLINE_SYNC_UNEXPECTED_STATE',
                            business_status=str(transport_error),
                        )
                        conn.commit()
                        if is_terminal:
                            result.manual += 1
                        else:
                            result.retryable += 1
                        continue

                else:
                    # Transport error path (TransportRetryableError, TransportRejectedError,
                    # or ValueError-converted-to-retryable from unknown DPS state above)
                    current = FiscalDocumentRepository.get_by_id(conn, doc.document_id)
                    attempts = (current.recovery_attempts if current else 0) + 1
                    is_terminal = (
                        isinstance(transport_error, TransportRejectedError)
                        or attempts >= self.max_recovery_attempts
                    )

                    if is_terminal:
                        updated = FiscalDocumentRepository.update_state(
                            conn,
                            document_id=doc.document_id,
                            state=DocumentState.REQUIRES_MANUAL_RECONCILIATION,
                            error_message=str(transport_error),
                            expected_states=(DocumentState.OFFLINE_LOCAL_ACK,),
                        )
                        if updated is None:
                            conn.rollback()
                            continue
                        event_type = 'OFFLINE_SYNC_FAILED_TERMINAL'
                        severity = 'ERROR'
                        result.manual += 1
                    else:
                        # Stay in OFFLINE_LOCAL_ACK; only recovery_attempts advances
                        event_type = 'OFFLINE_SYNC_FAILED_RETRYABLE'
                        severity = 'WARNING'
                        result.retryable += 1

                    FiscalDocumentRepository.set_recovery_attempts(
                        conn,
                        document_id=doc.document_id,
                        recovery_attempts=attempts,
                    )
                    AuditRepository.log_event(
                        conn,
                        entity_type='DOCUMENT',
                        entity_id=doc.document_id,
                        event_type=event_type,
                        severity=severity,
                        event_payload_json=dumps_json({
                            'error': str(transport_error),
                            'recovery_attempts': attempts,
                            'offline_fiscal_no': doc.offline_fiscal_no,
                        }),
                    )
                    TransportTraceRepository.log_event(
                        conn,
                        trace_id=f'{doc.document_id}-offline-sync-error-{attempts}',
                        fiscal_number=doc.fiscal_number,
                        backend_profile_id=doc.backend_profile_id,
                        transport_profile_id=doc.transport_profile_id,
                        direction='OUT',
                        technical_status='OFFLINE_SYNC_ERROR',
                        business_status=event_type,
                    )
                    conn.commit()

            except Exception:
                conn.rollback()
                raise

        return result

    @staticmethod
    def _apply_shift_side_effects_locked(conn: sqlite3.Connection, *, doc, target_state: DocumentState) -> None:
        """Mirror of ReconciliationService._apply_shift_side_effects_locked.

        Called with target_state == ACK or REJECTED.  Only ACK triggers shift
        transitions; REJECTED is a no-op (shift was not confirmed by DPS).
        For offline SHIFT_OPEN the shift was already set OPENED at write-path time,
        so the ACK call is idempotent; for regular SELL/RETURN/etc. this is a no-op.
        """
        try:
            op = OperationType(doc.doc_type)
        except ValueError:
            return
        if op == OperationType.SHIFT_OPEN and target_state == DocumentState.ACK:
            active_shift = ShiftRepository.get_active_shift(conn, doc.fiscal_number)
            if active_shift is None:
                # Recover original ingress protocol from inbox, not hardcoded
                ingress_protocol = InboxRepository.get_protocol(conn, doc.request_id)
                if ingress_protocol is None:
                    ingress_protocol = 'CHECKBOX_REST'
                    AuditRepository.log_event(
                        conn, entity_type='DOCUMENT', entity_id=doc.document_id,
                        event_type='PROTOCOL_RECOVERY_FALLBACK', severity='WARNING',
                        event_payload_json=dumps_json({'request_id': doc.request_id, 'fallback': 'CHECKBOX_REST'}),
                    )
                ShiftRepository.create_shift(
                    conn,
                    shift_id=f'shift-{doc.document_id}',
                    fiscal_number=doc.fiscal_number,
                    state=ShiftState.OPENED,
                    open_mode=doc.fs_mode,
                    backend_profile_id=doc.backend_profile_id,
                    transport_profile_id=doc.transport_profile_id,
                    protocol=Protocol(ingress_protocol),
                    integration_owner='offline_sync',
                    channel_lock_acquired_at=doc.ack_at or datetime.now(UTC).isoformat(),
                    open_document_id=doc.document_id,
                )
            elif active_shift.state != ShiftState.OPENED:
                ShiftRepository.update_state(conn, shift_id=active_shift.shift_id, state=ShiftState.OPENED)
                ShiftRepository.link_document(conn, shift_id=active_shift.shift_id, open_document_id=doc.document_id)
            elif active_shift.open_document_id is None:
                ShiftRepository.link_document(conn, shift_id=active_shift.shift_id, open_document_id=doc.document_id)
        elif op == OperationType.SHIFT_CLOSE and target_state == DocumentState.ACK:
            active_shift = ShiftRepository.get_active_shift(conn, doc.fiscal_number)
            if active_shift is not None:
                ShiftRepository.update_state(conn, shift_id=active_shift.shift_id, state=ShiftState.CLOSED)
                ShiftRepository.link_document(conn, shift_id=active_shift.shift_id, close_document_id=doc.document_id)


__all__ = ['OfflineSyncRunResult', 'OfflineSyncService']
