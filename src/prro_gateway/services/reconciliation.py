from __future__ import annotations

from datetime import UTC, datetime

from ..enums import DocumentState, OperationType, Protocol, ShiftState
from ..models.common import StrictModel
from ..ports import PollResult, TransportStatusClient
import json

from ..repositories import AuditRepository, ExciseRepository, FiscalDocumentRepository, InboxRepository, OutboxRepository, ShiftRepository, TransportTraceRepository
from ..repositories.excise import normalize_excise_mark
from ..utils.json_codec import dumps_json


class ReconciliationRunResult(StrictModel):
    checked: int = 0
    acked: int = 0
    rejected: int = 0
    retryable: int = 0
    still_pending: int = 0
    manual: int = 0


class ReconciliationService:
    def __init__(self, *, transport_status_client: TransportStatusClient, max_recovery_attempts: int = 5, crypto_provider=None) -> None:
        self.transport_status_client = transport_status_client
        self.max_recovery_attempts = max_recovery_attempts
        self.crypto_provider = crypto_provider

    def reconcile_pending(self, conn, fiscal_number: str | None = None) -> ReconciliationRunResult:
        docs = FiscalDocumentRepository.get_pending_for_reconciliation(conn, fiscal_number=fiscal_number)
        result = ReconciliationRunResult(checked=len(docs))
        for doc in docs:
            # Skip rate-limited docs whose cooldown has not expired
            if self._is_rate_limit_cooldown_active(doc):
                conn.execute('BEGIN IMMEDIATE')
                AuditRepository.log_event(
                    conn,
                    entity_type='DOCUMENT',
                    entity_id=doc.document_id,
                    event_type='RECONCILE_COOLDOWN_SKIP',
                    severity='INFO',
                    event_payload_json=dumps_json({
                        'submission_status': doc.submission_status,
                        'updated_at': str(doc.updated_at),
                    }),
                )
                conn.commit()
                result.still_pending += 1
                continue
            poll = self.transport_status_client.poll_status(
                document_id=doc.document_id,
                fiscal_number=doc.fiscal_number,
                backend_profile_id=doc.backend_profile_id,
                transport_profile_id=doc.transport_profile_id,
                transport_request_id=doc.transport_request_id,
                operation_type=doc.doc_type,
                crypto_provider=self.crypto_provider,
            )
            if poll.state == DocumentState.ACK.value:
                conn.execute('BEGIN IMMEDIATE')
                ack_at = (poll.ack_at or datetime.now(UTC)).isoformat()
                updated = FiscalDocumentRepository.update_state(
                    conn,
                    document_id=doc.document_id,
                    state=DocumentState.ACK,
                    response_json=poll.response_json,
                    submission_status=poll.submission_status or 'ACK',
                    server_fiscal_no=poll.server_fiscal_no,
                    server_fiscal_date=poll.server_fiscal_date,
                    ack_at=ack_at,
                    expected_states=(DocumentState.SENT, DocumentState.KVT1, DocumentState.KVT2, DocumentState.ERROR_RETRYABLE, DocumentState.REQUIRES_MANUAL_RECONCILIATION),
                )
                if updated is None:
                    conn.rollback()
                    result.still_pending += 1
                    continue
                self._apply_shift_side_effects_locked(conn, doc=doc, target_state=DocumentState.ACK)
                self._apply_excise_side_effects_locked(conn, doc=doc, ack_at=ack_at)
                OutboxRepository.enqueue_document(
                    conn,
                    outbox_id=f'{doc.document_id}-reconcile-outbox',
                    aggregate_id=doc.document_id,
                    fiscal_number=doc.fiscal_number,
                    destination='CLOUD_HUB',
                    payload_path=f'/archive/{doc.fiscal_number}/{doc.document_id}/response.json',
                    payload_sha256='reconcile',
                    sequence_no=doc.lnd,
                )
                AuditRepository.log_event(
                    conn,
                    entity_type='DOCUMENT',
                    entity_id=doc.document_id,
                    event_type='DOCUMENT_RECONCILED_ACK',
                    severity='INFO',
                    event_payload_json=dumps_json({'state': 'ACK'}),
                )
                TransportTraceRepository.log_event(
                    conn,
                    trace_id=f'{doc.document_id}-reconcile-poll',
                    fiscal_number=doc.fiscal_number,
                    backend_profile_id=doc.backend_profile_id,
                    transport_profile_id=doc.transport_profile_id,
                    direction='EVENT',
                    response_envelope=poll.response_json,
                    technical_status='RECONCILE_POLL',
                    business_status=poll.submission_status,
                )
                conn.commit()
                result.acked += 1
            elif poll.state == DocumentState.REJECTED.value:
                conn.execute('BEGIN IMMEDIATE')
                updated = FiscalDocumentRepository.update_state(
                    conn,
                    document_id=doc.document_id,
                    state=DocumentState.REJECTED,
                    response_json=poll.response_json,
                    submission_status=poll.submission_status or 'REJECTED',
                    expected_states=(DocumentState.SENT, DocumentState.KVT1, DocumentState.KVT2, DocumentState.ERROR_RETRYABLE, DocumentState.REQUIRES_MANUAL_RECONCILIATION),
                )
                if updated is None:
                    conn.rollback()
                    result.still_pending += 1
                    continue
                self._apply_shift_side_effects_locked(conn, doc=doc, target_state=DocumentState.REJECTED)
                AuditRepository.log_event(
                    conn,
                    entity_type='DOCUMENT',
                    entity_id=doc.document_id,
                    event_type='DOCUMENT_RECONCILED_REJECTED',
                    severity='WARNING',
                    event_payload_json=dumps_json({'state': 'REJECTED'}),
                )
                TransportTraceRepository.log_event(
                    conn,
                    trace_id=f'{doc.document_id}-reconcile-poll',
                    fiscal_number=doc.fiscal_number,
                    backend_profile_id=doc.backend_profile_id,
                    transport_profile_id=doc.transport_profile_id,
                    direction='EVENT',
                    response_envelope=poll.response_json,
                    technical_status='RECONCILE_POLL',
                    business_status=poll.submission_status,
                )
                conn.commit()
                result.rejected += 1
            elif poll.retryable or poll.state == DocumentState.ERROR_RETRYABLE.value:
                # REQUIRES_MANUAL_RECONCILIATION + DPS still not ready: keep state as-is.
                # Auto-close happens only on ACK/REJECTED (above). Retryable response means
                # DPS is still processing — operator is already aware, no state change needed.
                if doc.state == DocumentState.REQUIRES_MANUAL_RECONCILIATION:
                    result.still_pending += 1
                    continue
                conn.execute('BEGIN IMMEDIATE')
                current = FiscalDocumentRepository.get_by_id(conn, doc.document_id)
                attempts = (current.recovery_attempts if current else 0) + 1
                if attempts >= self.max_recovery_attempts:
                    target_state = DocumentState.REQUIRES_MANUAL_RECONCILIATION
                    audit_event_type = 'DOCUMENT_REQUIRES_MANUAL_RECONCILIATION'
                    audit_severity = 'ERROR'
                    audit_payload = {'state': 'REQUIRES_MANUAL_RECONCILIATION', 'recovery_attempts': attempts}
                else:
                    target_state = DocumentState.ERROR_RETRYABLE
                    audit_event_type = 'DOCUMENT_RECONCILED_RETRYABLE'
                    audit_severity = 'WARNING'
                    audit_payload = {'state': 'ERROR_RETRYABLE', 'recovery_attempts': attempts}
                updated = FiscalDocumentRepository.update_state(
                    conn,
                    document_id=doc.document_id,
                    state=target_state,
                    response_json=poll.response_json,
                    submission_status=poll.submission_status or 'RETRYABLE',
                    expected_states=(DocumentState.SENT, DocumentState.KVT1, DocumentState.KVT2, DocumentState.ERROR_RETRYABLE),
                )
                if updated is None:
                    conn.rollback()
                    result.still_pending += 1
                    continue
                FiscalDocumentRepository.set_recovery_attempts(conn, document_id=doc.document_id, recovery_attempts=attempts)
                AuditRepository.log_event(
                    conn,
                    entity_type='DOCUMENT',
                    entity_id=doc.document_id,
                    event_type=audit_event_type,
                    severity=audit_severity,
                    event_payload_json=dumps_json(audit_payload),
                )
                TransportTraceRepository.log_event(
                    conn,
                    trace_id=f'{doc.document_id}-reconcile-poll-{attempts}',
                    fiscal_number=doc.fiscal_number,
                    backend_profile_id=doc.backend_profile_id,
                    transport_profile_id=doc.transport_profile_id,
                    direction='EVENT',
                    response_envelope=poll.response_json,
                    technical_status='RECONCILE_POLL',
                    business_status=poll.submission_status,
                )
                conn.commit()
                if target_state == DocumentState.REQUIRES_MANUAL_RECONCILIATION:
                    result.manual += 1
                else:
                    result.retryable += 1
            else:
                result.still_pending += 1
        return result

    _DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS = 300  # conservative default

    @staticmethod
    def _is_rate_limit_cooldown_active(doc) -> bool:
        """Check if a rate-limited document is still within its cooldown window.

        Rate-limited docs have submission_status='DPS_RATE_LIMITED'.
        Cooldown duration is read from doc.response_json['retry_after_seconds'],
        falling back to 300s default if not persisted.
        """
        if doc.submission_status != 'DPS_RATE_LIMITED':
            return False
        if doc.updated_at is None:
            return False
        if isinstance(doc.updated_at, str):
            try:
                updated = datetime.fromisoformat(doc.updated_at)
            except (ValueError, TypeError):
                return False
        else:
            updated = doc.updated_at
        if updated.tzinfo is None:
            updated = updated.replace(tzinfo=UTC)
        cooldown = ReconciliationService._DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS
        if doc.response_json:
            try:
                meta = json.loads(doc.response_json)
                if isinstance(meta, dict) and 'retry_after_seconds' in meta:
                    cooldown = int(meta['retry_after_seconds'])
            except (ValueError, TypeError):
                pass
        cooldown_expires = updated.timestamp() + cooldown
        return datetime.now(UTC).timestamp() < cooldown_expires

    @staticmethod
    def _apply_shift_side_effects_locked(conn, *, doc, target_state: DocumentState) -> None:
        try:
            op = OperationType(doc.doc_type)
        except ValueError:
            return
        if op == OperationType.SHIFT_OPEN:
            active_shift = ShiftRepository.get_active_shift(conn, doc.fiscal_number)
            if target_state == DocumentState.ACK:
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
                        integration_owner='reconciliation',
                        channel_lock_acquired_at=doc.ack_at or doc.sent_at or datetime.now(UTC).isoformat(),
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
        elif op == OperationType.Z_REPORT and target_state == DocumentState.ACK:
            active_shift = ShiftRepository.get_active_shift(conn, doc.fiscal_number)
            if active_shift is not None:
                ShiftRepository.link_document(conn, shift_id=active_shift.shift_id, z_report_document_id=doc.document_id)

    @staticmethod
    def _apply_excise_side_effects_locked(conn, *, doc, ack_at: str) -> None:
        """Handle excise mark transitions on reconciliation ACK.

        SELL: RESERVED → SOLD (marks were reserved at write-path time).
        RETURN: SOLD → RETURNED (releases marks for re-use).
        """
        try:
            op = OperationType(doc.doc_type)
        except ValueError:
            return
        if op == OperationType.SELL:
            ExciseRepository.mark_document_sold(conn, document_id=doc.document_id, sold_at=ack_at)
        elif op == OperationType.RETURN:
            try:
                parsed = json.loads(doc.payload_json) if doc.payload_json else {}
            except (json.JSONDecodeError, TypeError):
                return
            # payload_json stores full CanonicalFiscalCommand; receipt is at payload.receipt
            inner = parsed.get('payload', parsed) if isinstance(parsed, dict) else {}
            receipt = inner.get('receipt') if isinstance(inner, dict) else None
            if not isinstance(receipt, dict):
                return
            codes: list[str] = []
            for item in receipt.get('goods', []):
                if not isinstance(item, dict):
                    continue
                for raw in item.get('excise_barcodes', []) or []:
                    normalized = normalize_excise_mark(raw)
                    if normalized:
                        codes.append(normalized)
            if codes:
                ExciseRepository.mark_returned(conn, mark_codes=codes, returned_at=ack_at)
