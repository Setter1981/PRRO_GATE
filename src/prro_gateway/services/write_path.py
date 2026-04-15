from __future__ import annotations

import concurrent.futures
import hashlib
import re
import sqlite3
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta

from ..constants import DEFAULT_LEASE_TIMEOUT_SECONDS
import json

from ..enums import BackendType, CanonicalErrorCode, DocumentState, FileKind, NodeMode, OperationType, Protocol, ShiftState, TransportKind
from ..errors import build_canonical_error
from ..logging import get_logger
from ..models.canonical import CanonicalFiscalCommand
from ..models.common import CanonicalError, StrictModel
from ..models.storage import FiscalDocumentRecord, InboxRecord, NodeStateRecord, ShiftRecord, TransportProfileRecord
from ..ports import (
    CryptoProvider,
    CryptoProviderUnavailableError,
    DpsMacRecoveryError,
    SendResult,
    TransportClient,
    TransportRateLimitedError,
    TransportRejectedError,
    TransportRetryableError,
)
from ..repositories import (
    AuditRepository,
    BackendProfileRepository,
    DocumentFilesRepository,
    DuplicateExciseMarkError,
    ExciseRepository,
    FiscalDocumentRepository,
    InboxRepository,
    NodeStateRepository,
    OfflineRepository,
    OutboxRepository,
    PaymentTypeRepository,
    ProtocolTraceRepository,
    ShiftRepository,
    TaxGroupRepository,
    TransportProfileRepository,
    TransportTraceRepository,
)
from ..utils.json_codec import dumps_json


class WorkerProcessResult(StrictModel):
    outcome: str
    request_id: str | None = None
    document_id: str | None = None
    inbox_status: str | None = None
    document_state: str | None = None
    canonical_error: CanonicalError | None = None
    # Sprint 10 Step 10: canonical layer fields — populated for SELL/RETURN ACK only
    cash_balance: int | None = None
    change: int | None = None
    rounded_sum: int | None = None
    rounding: int | None = None


@dataclass
class WorkerContext:
    lease_token: str
    lease_expires_at: str
    fiscal_number: str
    lease_owner: str
    inbox: InboxRecord | None = None
    command: CanonicalFiscalCommand | None = None
    node_state: NodeStateRecord | None = None
    active_shift: ShiftRecord | None = None
    receipt_meta: dict[str, object | None] | None = None
    excise_marks: list[dict] | None = None
    document: FiscalDocumentRecord | None = None
    signed_payload: str | None = None
    send_result: SendResult | None = None
    transport_profile: TransportProfileRecord | None = None


class WritePathWorker:
    MAX_OFFLINE_CONTINUOUS_SECONDS = 36 * 60 * 60
    MAX_OFFLINE_MONTH_SECONDS = 168 * 60 * 60

    def __init__(self, *, crypto_provider: CryptoProvider, transport_client: TransportClient, lease_timeout_seconds: int = DEFAULT_LEASE_TIMEOUT_SECONDS, crypto_timeout_seconds: float | None = None, crypto_breaker_threshold: int = 0, tax_number: str = '', require_return_linkage: bool = True, validate_timestamps: bool = False) -> None:
        self.crypto_provider = crypto_provider
        self.transport_client = transport_client
        self.tax_number = tax_number
        self.lease_timeout_seconds = lease_timeout_seconds
        self.crypto_timeout_seconds = crypto_timeout_seconds
        self.crypto_breaker_threshold = crypto_breaker_threshold
        self.require_return_linkage = require_return_linkage
        self.validate_timestamps = validate_timestamps
        self._crypto_consecutive_failures: int = 0
        self.logger = get_logger("prro_gateway.write_path")

    @property
    def crypto_breaker_open(self) -> bool:
        return self.crypto_breaker_threshold > 0 and self._crypto_consecutive_failures >= self.crypto_breaker_threshold

    def reset_crypto_breaker(self) -> int:
        """Reset consecutive failure counter to 0. Returns the previous value."""
        previous = self._crypto_consecutive_failures
        self._crypto_consecutive_failures = 0
        return previous

    def process_next(self, conn: sqlite3.Connection, *, fiscal_number: str, lease_owner: str = 'worker-1') -> WorkerProcessResult:
        ctx = WorkerContext(
            lease_token=f'lease-{fiscal_number}-{int(datetime.now(UTC).timestamp() * 1000000)}',
            lease_expires_at=(datetime.now(UTC) + timedelta(seconds=self.lease_timeout_seconds)).isoformat(),
            fiscal_number=fiscal_number,
            lease_owner=lease_owner,
        )

        ctx, result = self._stage_acquire_and_validate(conn, ctx)
        if result is not None:
            return result

        assert ctx.inbox is not None and ctx.command is not None and ctx.document is not None
        if self._requires_local_sign(conn, ctx):
            ctx, result = self._stage_sign(conn, ctx)
            if result is not None:
                return result
        else:
            self.logger.info("stage_sign_skipped", extra={"extra_fields": {"document_id": ctx.document.document_id, "transport_profile_id": ctx.document.transport_profile_id}})
        ctx, result = self._stage_send_or_offline(conn, ctx)
        if result is not None:
            return result

        return self._stage_finalize_ack(conn, ctx)

    def _stage_acquire_and_validate(self, conn: sqlite3.Connection, ctx: WorkerContext) -> tuple[WorkerContext, WorkerProcessResult | None]:
        conn.execute('BEGIN IMMEDIATE')
        inbox = InboxRepository.acquire_lease(
            conn,
            fiscal_number=ctx.fiscal_number,
            lease_owner=ctx.lease_owner,
            lease_token=ctx.lease_token,
            lease_expires_at=ctx.lease_expires_at,
        )
        if inbox is None:
            conn.commit()
            return ctx, WorkerProcessResult(outcome='NOOP')
        ctx.inbox = inbox

        try:
            command = CanonicalFiscalCommand.model_validate_json(inbox.payload_json)
        except Exception as exc:
            err = build_canonical_error(CanonicalErrorCode.UNKNOWN_METHOD, message=f'Invalid canonical command payload: {exc}')
            result = self._mark_error_locked(conn, ctx=ctx, error=err, document_id=None, state=None)
            return ctx, result
        ctx.command = command

        guard_error = self._guard_preconditions(conn, inbox=inbox, command=command)
        if guard_error is not None:
            result = self._mark_error_locked(conn, ctx=ctx, error=guard_error, document_id=None, state=None)
            return ctx, result

        node_state = NodeStateRepository.get_state(conn, ctx.fiscal_number)
        active_shift = ShiftRepository.get_active_shift(conn, ctx.fiscal_number)
        if node_state is None:
            result = self._mark_error_locked(
                conn,
                ctx=ctx,
                error=build_canonical_error(CanonicalErrorCode.BACKEND_TEMPORARY_UNAVAILABLE, message='Node state not initialized.'),
                document_id=None,
                state=None,
            )
            return ctx, result
        ctx.node_state = node_state
        ctx.active_shift = active_shift
        ctx.receipt_meta = self._extract_receipt_metadata(command)
        ctx.excise_marks = self._extract_excise_marks(command)

        # Receipt validation — before offline allocation and document creation.
        violations: list[str] = []
        if command.operation_type in {OperationType.SELL, OperationType.RETURN}:
            from ..validators.ua_receipt import validate_sell_return_receipt
            violations = validate_sell_return_receipt(command.payload)
        if command.operation_type in {OperationType.SELL, OperationType.RETURN} and not violations:
            from ..validators.ua_receipt import validate_excise_marks
            violations.extend(validate_excise_marks(command.payload))
        elif command.operation_type in {OperationType.SERVICE_IN, OperationType.SERVICE_OUT}:
            from ..validators.ua_receipt import validate_service_receipt
            violations = validate_service_receipt(command.payload)
        elif command.operation_type == OperationType.CASH_WITHDRAWAL:
            from ..validators.ua_receipt import validate_cash_withdrawal_receipt
            violations = validate_cash_withdrawal_receipt(command.payload)
        if violations:
            err = build_canonical_error(
                CanonicalErrorCode.INVALID_RECEIPT_DATA,
                message=f'Receipt validation failed: {"; ".join(violations)}',
            )
            result = self._mark_error_locked(conn, ctx=ctx, error=err, document_id=None, state=None)
            return ctx, result

        fs_mode = 'ONLINE'
        offline_session_id: str | None = None
        offline_fiscal_no: int | None = None
        offline_fiscal_date: str | None = None
        if self._operation_supports_offline(command.operation_type) and node_state.mode == NodeMode.OFFLINE:
            offline_session = OfflineRepository.get_open_session(conn, ctx.fiscal_number)
            if offline_session is None:
                result = self._mark_error_locked(conn, ctx=ctx, error=build_canonical_error(CanonicalErrorCode.OFFLINE_MODE_REQUIRED, message='Node is OFFLINE but no open offline session exists.'), document_id=None, state=None)
                return ctx, result
            offline_limit_error = self._check_offline_limits(node_state=node_state, offline_session=offline_session)
            if offline_limit_error is not None:
                result = self._mark_error_locked(conn, ctx=ctx, error=offline_limit_error, document_id=None, state=None)
                return ctx, result
            try:
                offline_fiscal_no = OfflineRepository.allocate_offline_code(conn, ctx.fiscal_number)
            except LookupError:
                result = self._mark_error_locked(conn, ctx=ctx, error=build_canonical_error(CanonicalErrorCode.OFFLINE_CODES_EXHAUSTED), document_id=None, state=None)
                return ctx, result
            fs_mode = 'OFFLINE'
            offline_session_id = offline_session.offline_session_id
            offline_fiscal_date = command.business_ts.isoformat()

        lnd = NodeStateRepository.increment_lnd(conn, fiscal_number=ctx.fiscal_number)
        document_id = f'doc-{ctx.fiscal_number}-{lnd}'
        document = FiscalDocumentRepository.create_prepared(
            conn,
            document_id=document_id,
            request_id=inbox.request_id,
            fiscal_number=ctx.fiscal_number,
            lnd=lnd,
            doc_type=command.operation_type.value,
            backend_profile_id=command.backend_profile_id or inbox.backend_profile_id or 'backend_checkbox_default',
            transport_profile_id=command.transport_profile_id or inbox.transport_profile_id or 'transport_checkbox_rest_default',
            fs_mode=fs_mode,
            business_ts=command.business_ts.isoformat(),
            payload_json=inbox.payload_json,
            payload_sha256=inbox.payload_sha256,
            shift_id=active_shift.shift_id if active_shift else None,
            offline_session_id=offline_session_id,
            receipt_type=ctx.receipt_meta['receipt_type'],
            previous_hash=ctx.receipt_meta['previous_hash'],
            channel_lock_ref=active_shift.shift_id if active_shift else None,
            offline_fiscal_no=offline_fiscal_no,
            offline_fiscal_date=offline_fiscal_date,
            related_receipt_id=ctx.receipt_meta['related_receipt_id'],
            previous_receipt_id=ctx.receipt_meta['previous_receipt_id'],
            technical_return=ctx.receipt_meta['technical_return'],
            delivery_json=ctx.receipt_meta['delivery_json'],
            rounding_enabled=ctx.receipt_meta['rounding_enabled'],
            total_sum=ctx.receipt_meta['total_sum'],
            round_sum=ctx.receipt_meta['round_sum'],
            discounts_sum=ctx.receipt_meta['discounts_sum'],
            extra_charge_sum=ctx.receipt_meta['extra_charge_sum'],
        )
        ctx.document = document
        try:
            if ctx.excise_marks and command.operation_type == OperationType.SELL:
                ExciseRepository.reserve_marks(conn, document_id=document_id, marks=ctx.excise_marks)
        except DuplicateExciseMarkError as exc:
            result = self._mark_error_locked(
                conn,
                ctx=ctx,
                error=build_canonical_error(CanonicalErrorCode.DUPLICATE_EXCISE_MARK, message=str(exc), details={'mark': exc.normalized_mark_code}),
                document_id=document_id,
                state=DocumentState.REJECTED,
            )
            return ctx, result

        self._record_prepared_locked(conn, ctx)
        conn.commit()
        self.logger.info("stage_prepare_complete", extra={"extra_fields": {"request_id": inbox.request_id, "document_id": document_id, "fiscal_number": ctx.fiscal_number}})
        return ctx, None

    def _requires_local_sign(self, conn: sqlite3.Connection, ctx: WorkerContext) -> bool:
        assert ctx.document is not None
        profile = TransportProfileRepository.get_by_id(conn, ctx.document.transport_profile_id)
        if profile is not None:
            try:
                config = json.loads(profile.config_json or '{}')
            except ValueError:
                config = {}
            if isinstance(config, dict) and 'require_local_sign' in config:
                return bool(config['require_local_sign'])
        backend_profile = BackendProfileRepository.get_by_id(conn, ctx.document.backend_profile_id)
        if backend_profile is not None and backend_profile.backend_type == BackendType.CHECKBOX_CLOUD_COMPAT:
            return True
        return True

    def _resolve_dps_mac(self, conn: sqlite3.Connection, ctx: WorkerContext) -> str:
        """Compute authoritative MAC for DPS XML: SHA-256 of previous document's unsigned XML payload.

        Source: core-persisted PAYLOAD_XML (unsigned DPS XML) from the most recent ACKed
        document for this fiscal_number. Falls back to SIGNED_XML for older docs.
        NOT from ingress payload passthrough.

        Empty only when no prior ACKed doc exists AND no bootstrap seed exists
        (true first-ever document for this fiscal number on this DPS server).
        SHIFT_OPEN is NOT exempt — evidence shows shift-open docs carry MAC
        when they are not the first-ever document for the fiscal number.

        Bootstrap: when a fresh DB connects to a DPS FN with existing history,
        node_state.last_known_mac provides the seed (populated from DPS
        ERROR_BAD_HASH_PREV response or operator-provided seed).
        """
        assert ctx.document is not None and ctx.command is not None
        # Find the most recent ACKed DPS document for this fiscal_number
        row = conn.execute(
            """SELECT d.document_id FROM fiscal_documents d
               WHERE d.fiscal_number = ? AND d.state = 'ACK'
                 AND d.transport_profile_id = ?
                 AND d.document_id != ?
               ORDER BY d.lnd DESC LIMIT 1""",
            (ctx.fiscal_number, ctx.document.transport_profile_id, ctx.document.document_id),
        ).fetchone()
        if row is not None:
            prev_doc_id = row[0]
            # MAC = hash of previous unsigned DPS XML payload, not signed artifact.
            # Fall back to SIGNED_XML for older docs that may not have PAYLOAD_XML yet.
            content = DocumentFilesRepository.get_content(conn, document_id=prev_doc_id, file_kind=FileKind.PAYLOAD_XML)
            if content is None:
                content = DocumentFilesRepository.get_content(conn, document_id=prev_doc_id, file_kind=FileKind.SIGNED_XML)
            if content is not None:
                return hashlib.sha256(content).hexdigest()
        # Fallback: bootstrap seed from node_state (for fresh DB against DPS FN with history)
        seed_row = conn.execute(
            "SELECT last_known_mac FROM node_state WHERE fiscal_number = ?",
            (ctx.fiscal_number,),
        ).fetchone()
        if seed_row is not None and seed_row[0]:
            return seed_row[0]
        return ''

    def _resolve_sign_input(self, conn: sqlite3.Connection, ctx: WorkerContext) -> str:
        """Determine what artifact the crypto provider should sign.

        For DPS fiscal-server profiles: build DPS ФСКО XML artifact.
        For everything else: use canonical command JSON (existing behavior).
        """
        assert ctx.document is not None and ctx.command is not None and ctx.inbox is not None
        profile = TransportProfileRepository.get_by_id(conn, ctx.document.transport_profile_id)
        if profile is not None and profile.kind == TransportKind.DPS_PRRO_GRPC_ECABINET:
            from ..serializers.dps_xml import build_dps_xml
            local_number = 0 if ctx.command.operation_type == OperationType.SHIFT_OPEN else ctx.document.lnd
            mac_hash = self._resolve_dps_mac(conn, ctx)
            # Allocate Z number for Z_REPORT, persist on document for retry stability
            z_number = 0
            if ctx.command.operation_type == OperationType.Z_REPORT:
                if ctx.document.z_report_number is not None:
                    z_number = ctx.document.z_report_number  # retry: reuse allocated
                else:
                    conn.execute('BEGIN IMMEDIATE')
                    z_number = NodeStateRepository.allocate_z_report_number(conn, fiscal_number=ctx.fiscal_number)
                    conn.execute(
                        "UPDATE fiscal_documents SET z_report_number = ? WHERE document_id = ?",
                        (z_number, ctx.document.document_id),
                    )
                    conn.commit()
            tax_groups = TaxGroupRepository.get_for_fiscal_number(conn, ctx.fiscal_number) or None
            payload = self._enrich_payload_for_dps(conn, ctx)
            return build_dps_xml(
                operation_type=ctx.command.operation_type,
                fiscal_number=ctx.fiscal_number,
                local_number=local_number,
                business_ts=ctx.command.business_ts,
                payload=payload,
                tax_number=self.tax_number,
                previous_hash=mac_hash,
                z_number=z_number,
                tax_groups=tax_groups,
            )
        return ctx.inbox.payload_json

    def _stage_sign(self, conn: sqlite3.Connection, ctx: WorkerContext) -> tuple[WorkerContext, WorkerProcessResult | None]:
        assert ctx.document is not None and ctx.inbox is not None and ctx.command is not None
        if self.crypto_breaker_threshold > 0 and self._crypto_consecutive_failures >= self.crypto_breaker_threshold:
            result = self._mark_document_and_inbox_error(
                conn,
                ctx=ctx,
                document_id=ctx.document.document_id,
                state=DocumentState.ERROR_RETRYABLE,
                error=build_canonical_error(
                    CanonicalErrorCode.CRYPTO_PROVIDER_UNAVAILABLE,
                    message=f'crypto circuit breaker open after {self._crypto_consecutive_failures} consecutive failures',
                ),
            )
            # Audit every blocked doc so operator sees full impact (after error tx committed)
            conn.execute('BEGIN IMMEDIATE')
            AuditRepository.log_event(
                conn,
                entity_type='CRYPTO',
                entity_id='breaker',
                event_type='CRYPTO_BREAKER_BLOCKED',
                severity='ERROR',
                event_payload_json=dumps_json({
                    'consecutive_failures': self._crypto_consecutive_failures,
                    'threshold': self.crypto_breaker_threshold,
                    'fiscal_number': ctx.fiscal_number,
                    'document_id': ctx.document.document_id if ctx.document else None,
                }),
            )
            conn.commit()
            return ctx, result
        # Determine what artifact to sign: DPS XML for fiscal-server profiles, canonical JSON otherwise
        sign_input = self._resolve_sign_input(conn, ctx)
        is_dps_xml = sign_input.startswith('<RQ')

        try:
            if is_dps_xml and hasattr(self.crypto_provider, 'sign_raw'):
                # DPS path: use sign_raw() → returns bytes (CMS/PKCS#7 DER)
                sign_data = sign_input.encode('utf-8')
                if self.crypto_timeout_seconds is not None:
                    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as _executor:
                        _future = _executor.submit(self.crypto_provider.sign_raw, data=sign_data)
                        try:
                            signed_payload = _future.result(timeout=self.crypto_timeout_seconds)
                        except concurrent.futures.TimeoutError:
                            raise CryptoProviderUnavailableError(
                                f'crypto sign_raw() timed out after {self.crypto_timeout_seconds}s'
                            )
                else:
                    signed_payload = self.crypto_provider.sign_raw(data=sign_data)
            else:
                # Non-DPS path: use sign() → returns str
                if self.crypto_timeout_seconds is not None:
                    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as _executor:
                        _future = _executor.submit(
                            self.crypto_provider.sign,
                            document_id=ctx.document.document_id,
                            payload_json=sign_input,
                        )
                        try:
                            signed_payload = _future.result(timeout=self.crypto_timeout_seconds)
                        except concurrent.futures.TimeoutError:
                            raise CryptoProviderUnavailableError(
                                f'crypto sign() timed out after {self.crypto_timeout_seconds}s'
                            )
                else:
                    signed_payload = self.crypto_provider.sign(document_id=ctx.document.document_id, payload_json=sign_input)
        except CryptoProviderUnavailableError as exc:
            self._crypto_consecutive_failures += 1
            result = self._mark_document_and_inbox_error(
                conn,
                ctx=ctx,
                document_id=ctx.document.document_id,
                state=DocumentState.ERROR_RETRYABLE,
                error=build_canonical_error(CanonicalErrorCode.CRYPTO_PROVIDER_UNAVAILABLE, message=str(exc)),
            )
            return ctx, result
        except Exception as exc:
            self._crypto_consecutive_failures += 1
            result = self._mark_document_and_inbox_error(
                conn,
                ctx=ctx,
                document_id=ctx.document.document_id,
                state=DocumentState.ERROR_RETRYABLE,
                error=build_canonical_error(CanonicalErrorCode.CRYPTO_PROVIDER_UNAVAILABLE, message=f'{type(exc).__name__}: {exc}'),
            )
            return ctx, result

        self._crypto_consecutive_failures = 0
        ctx.signed_payload = signed_payload
        conn.execute('BEGIN IMMEDIATE')
        ctx.document = FiscalDocumentRepository.update_state(conn, document_id=ctx.document.document_id, state=DocumentState.SIGNED)
        # Persist unsigned DPS XML payload for MAC chain (only for DPS profiles)
        if sign_input != (ctx.inbox.payload_json if ctx.inbox else ''):
            DocumentFilesRepository.add_file(
                conn,
                file_id=f'{ctx.document.document_id}-payload-xml',
                document_id=ctx.document.document_id,
                file_kind=FileKind.PAYLOAD_XML,
                path=f'/archive/{ctx.fiscal_number}/{ctx.document.document_id}/payload.xml',
                content=sign_input,
            )
        DocumentFilesRepository.add_file(
            conn,
            file_id=f'{ctx.document.document_id}-signed',
            document_id=ctx.document.document_id,
            file_kind=FileKind.SIGNED_XML,
            path=f'/archive/{ctx.fiscal_number}/{ctx.document.document_id}/signed.xml',
            content=signed_payload,
        )
        AuditRepository.log_events(
            conn,
            events=[('DOCUMENT', ctx.document.document_id, 'DOCUMENT_SIGNED', 'INFO', dumps_json({'state': 'SIGNED'}))],
        )
        conn.commit()
        self.logger.info("stage_sign_complete", extra={"extra_fields": {"document_id": ctx.document.document_id}})
        return ctx, None

    def _stage_send_or_offline(self, conn: sqlite3.Connection, ctx: WorkerContext) -> tuple[WorkerContext, WorkerProcessResult | None]:
        assert ctx.document is not None and ctx.command is not None
        if ctx.document.fs_mode == 'OFFLINE':
            return ctx, None

        transport_profile = TransportProfileRepository.get_by_id(conn, ctx.document.transport_profile_id)
        ctx.transport_profile = transport_profile
        try:
            send_result = self.transport_client.send(
                document_id=ctx.document.document_id,
                signed_payload=ctx.signed_payload or '',
                fiscal_number=ctx.fiscal_number,
                backend_profile_id=ctx.document.backend_profile_id,
                transport_profile_id=ctx.document.transport_profile_id,
                operation_type=ctx.command.operation_type.value,
                request_payload=ctx.command.payload if isinstance(ctx.command.payload, dict) else None,
                request_payload_json=ctx.inbox.payload_json if ctx.inbox is not None else None,
                external_request_id=ctx.command.external_request_id,
                transport_profile=transport_profile,
                lnd=ctx.document.lnd,
                business_ts=ctx.command.business_ts,
                related_receipt_id=ctx.document.related_receipt_id,
            )
        except DpsMacRecoveryError as exc:
            return self._try_mac_recovery_and_resend(conn, ctx=ctx, mac_error=exc)
        except TransportRateLimitedError as exc:
            result = self._mark_document_and_inbox_error(
                conn,
                ctx=ctx,
                document_id=ctx.document.document_id,
                state=DocumentState.ERROR_RETRYABLE,
                error=build_canonical_error(
                    CanonicalErrorCode.TRANSPORT_RETRYABLE_ERROR,
                    message=str(exc),
                    retry_after_seconds=exc.retry_after_seconds,
                ),
                technical_status='DPS_RATE_LIMITED',
                submission_status='DPS_RATE_LIMITED',
                response_json=dumps_json({'retry_after_seconds': exc.retry_after_seconds}),
            )
            return ctx, result
        except TransportRetryableError as exc:
            result = self._mark_document_and_inbox_error(
                conn,
                ctx=ctx,
                document_id=ctx.document.document_id,
                state=DocumentState.ERROR_RETRYABLE,
                error=build_canonical_error(CanonicalErrorCode.TRANSPORT_RETRYABLE_ERROR, message=str(exc)),
                technical_status='RETRYABLE_ERROR',
            )
            return ctx, result
        except TransportRejectedError as exc:
            result = self._mark_document_and_inbox_error(
                conn,
                ctx=ctx,
                document_id=ctx.document.document_id,
                state=DocumentState.REJECTED,
                error=build_canonical_error(CanonicalErrorCode.BACKEND_TEMPORARY_UNAVAILABLE, message=str(exc)),
                technical_status='REJECTED',
            )
            return ctx, result

        ctx.send_result = send_result
        conn.execute('BEGIN IMMEDIATE')
        ctx.document = FiscalDocumentRepository.update_state(
            conn,
            document_id=ctx.document.document_id,
            state=DocumentState.SENT,
            submission_status=send_result.submission_status,
            transport_request_id=send_result.transport_request_id,
            sent_at=send_result.sent_at.isoformat() if send_result.sent_at else datetime.now(UTC).isoformat(),
        )
        TransportTraceRepository.log_event(
            conn,
            trace_id=f'{ctx.document.document_id}-transport-out',
            fiscal_number=ctx.fiscal_number,
            backend_profile_id=ctx.document.backend_profile_id,
            transport_profile_id=ctx.document.transport_profile_id,
            endpoint=transport_profile.endpoint if transport_profile else None,
            direction='OUT',
            request_envelope=ctx.signed_payload if isinstance(ctx.signed_payload, str) else '<CMS_DER_BYTES>',
            technical_status='SENT',
            business_status=send_result.submission_status,
            correlation_id=ctx.command.correlation_id,
        )
        conn.commit()
        self.logger.info("stage_send_complete", extra={"extra_fields": {"document_id": ctx.document.document_id, "transport_request_id": send_result.transport_request_id}})
        return ctx, None

    def _try_mac_recovery_and_resend(
        self, conn: sqlite3.Connection, *, ctx: WorkerContext, mac_error: DpsMacRecoveryError,
    ) -> tuple[WorkerContext, WorkerProcessResult | None]:
        """One bounded MAC-recovery retry when DPS tells us the expected previous hash.

        Steps (all outside any open SQLite transaction):
        1. Persist expected MAC into node_state.last_known_mac
        2. Re-build DPS XML with corrected MAC
        3. Re-sign via crypto provider
        4. Re-send to DPS
        5. On second failure: mark as retryable (no further auto-retry)
        """
        assert ctx.document is not None and ctx.command is not None
        expected_mac = mac_error.expected_mac
        self.logger.info('mac_recovery_attempt', extra={'extra_fields': {
            'document_id': ctx.document.document_id,
            'expected_mac': expected_mac,
            'dps_status': mac_error.dps_status,
        }})

        # 1. Persist MAC seed (short DB write, no network)
        conn.execute('BEGIN IMMEDIATE')
        NodeStateRepository.update_last_known_mac(conn, fiscal_number=ctx.fiscal_number, mac=expected_mac)
        AuditRepository.log_events(conn, events=[(
            'DOCUMENT', ctx.document.document_id, 'DPS_MAC_RECOVERY',
            'WARNING', dumps_json({'expected_mac': expected_mac, 'dps_status': mac_error.dps_status}),
        )])
        conn.commit()

        # 2. Re-build XML with corrected MAC — uses shared enrichment helper
        try:
            from ..serializers.dps_xml import build_dps_xml
            local_number = 0 if ctx.command.operation_type == OperationType.SHIFT_OPEN else ctx.document.lnd
            z_number = ctx.document.z_report_number or 0
            tax_groups = TaxGroupRepository.get_for_fiscal_number(conn, ctx.fiscal_number) or None
            payload = self._enrich_payload_for_dps(conn, ctx)
            sign_input = build_dps_xml(
                operation_type=ctx.command.operation_type,
                fiscal_number=ctx.fiscal_number,
                local_number=local_number,
                business_ts=ctx.command.business_ts,
                payload=payload,
                tax_number=self.tax_number,
                previous_hash=expected_mac,
                z_number=z_number,
                tax_groups=tax_groups,
            )
        except Exception as exc:
            result = self._mark_document_and_inbox_error(
                conn, ctx=ctx, document_id=ctx.document.document_id,
                state=DocumentState.ERROR_RETRYABLE,
                error=build_canonical_error(CanonicalErrorCode.TRANSPORT_RETRYABLE_ERROR,
                                            message=f'MAC recovery XML rebuild failed: {exc}'),
            )
            return ctx, result

        # 3. Re-sign (outside DB transaction)
        try:
            if hasattr(self.crypto_provider, 'sign_raw'):
                signed_payload = self.crypto_provider.sign_raw(data=sign_input.encode('utf-8'))
            else:
                signed_payload = self.crypto_provider.sign(document_id=ctx.document.document_id, payload_json=sign_input)
        except Exception as exc:
            result = self._mark_document_and_inbox_error(
                conn, ctx=ctx, document_id=ctx.document.document_id,
                state=DocumentState.ERROR_RETRYABLE,
                error=build_canonical_error(CanonicalErrorCode.CRYPTO_PROVIDER_UNAVAILABLE,
                                            message=f'MAC recovery re-sign failed: {exc}'),
            )
            return ctx, result

        # 4. Re-send (outside DB transaction)
        transport_profile = ctx.transport_profile
        try:
            send_result = self.transport_client.send(
                document_id=ctx.document.document_id,
                signed_payload=signed_payload,
                fiscal_number=ctx.fiscal_number,
                backend_profile_id=ctx.document.backend_profile_id,
                transport_profile_id=ctx.document.transport_profile_id,
                operation_type=ctx.command.operation_type.value,
                request_payload=ctx.command.payload if isinstance(ctx.command.payload, dict) else None,
                request_payload_json=ctx.inbox.payload_json if ctx.inbox is not None else None,
                external_request_id=ctx.command.external_request_id,
                transport_profile=transport_profile,
                lnd=ctx.document.lnd,
                business_ts=ctx.command.business_ts,
                related_receipt_id=ctx.document.related_receipt_id,
            )
        except Exception as exc:
            # Second failure — no further auto-retry, mark retryable
            result = self._mark_document_and_inbox_error(
                conn, ctx=ctx, document_id=ctx.document.document_id,
                state=DocumentState.ERROR_RETRYABLE,
                error=build_canonical_error(CanonicalErrorCode.TRANSPORT_RETRYABLE_ERROR,
                                            message=f'MAC recovery re-send failed: {exc}'),
                technical_status='MAC_RECOVERY_FAILED',
            )
            return ctx, result

        # 5. Success — replace persisted artifacts with corrected versions
        ctx.signed_payload = signed_payload
        ctx.send_result = send_result
        conn.execute('BEGIN IMMEDIATE')
        # Remove stale pre-recovery artifacts so get_content returns the corrected ones
        conn.execute(
            "DELETE FROM document_files WHERE document_id = ? AND file_kind IN (?, ?)",
            (ctx.document.document_id, str(FileKind.PAYLOAD_XML), str(FileKind.SIGNED_XML)),
        )
        DocumentFilesRepository.add_file(
            conn,
            file_id=f'{ctx.document.document_id}-payload-xml',
            document_id=ctx.document.document_id,
            file_kind=FileKind.PAYLOAD_XML,
            path=f'/archive/{ctx.fiscal_number}/{ctx.document.document_id}/payload.xml',
            content=sign_input,
        )
        DocumentFilesRepository.add_file(
            conn,
            file_id=f'{ctx.document.document_id}-signed-recovered',
            document_id=ctx.document.document_id,
            file_kind=FileKind.SIGNED_XML,
            path=f'/archive/{ctx.fiscal_number}/{ctx.document.document_id}/signed.xml',
            content=signed_payload,
        )
        conn.commit()

        self.logger.info('mac_recovery_success', extra={'extra_fields': {
            'document_id': ctx.document.document_id,
            'server_id': send_result.transport_request_id,
        }})
        return ctx, None

    def _stage_finalize_ack(self, conn: sqlite3.Connection, ctx: WorkerContext) -> WorkerProcessResult:
        assert ctx.document is not None and ctx.inbox is not None and ctx.command is not None
        conn.execute('BEGIN IMMEDIATE')
        if ctx.document.fs_mode == 'OFFLINE':
            ack_at = datetime.now(UTC).isoformat()
            ctx.document = FiscalDocumentRepository.update_state(
                conn,
                document_id=ctx.document.document_id,
                state=DocumentState.OFFLINE_LOCAL_ACK,
                response_json=dumps_json({'mode': 'OFFLINE_LOCAL', 'offline_fiscal_no': ctx.document.offline_fiscal_no}),
                submission_status='OFFLINE_LOCAL',
                ack_at=ack_at,
            )
            if ctx.excise_marks and ctx.command.operation_type == OperationType.SELL:
                ExciseRepository.mark_document_sold(conn, document_id=ctx.document.document_id, sold_at=ack_at)
            elif ctx.excise_marks and ctx.command.operation_type == OperationType.RETURN:
                codes = [m['normalized_mark_code'] for m in ctx.excise_marks]
                ExciseRepository.mark_returned(conn, mark_codes=codes, returned_at=ack_at)
            self._apply_shift_side_effects_locked(conn, ctx=ctx, target_state=DocumentState.OFFLINE_LOCAL_ACK)
            outcome = 'OFFLINE_LOCAL_ACK'
        else:
            assert ctx.send_result is not None
            send_state = DocumentState(ctx.send_result.state) if ctx.send_result.state else DocumentState.ACK
            if send_state == DocumentState.ACK:
                ctx.document = FiscalDocumentRepository.update_state(
                    conn,
                    document_id=ctx.document.document_id,
                    state=DocumentState.ACK,
                    response_json=ctx.send_result.response_json,
                    submission_status=ctx.send_result.submission_status,
                    transport_request_id=ctx.send_result.transport_request_id,
                    server_fiscal_no=ctx.send_result.server_fiscal_no,
                    server_fiscal_date=ctx.send_result.server_fiscal_date,
                    sent_at=ctx.send_result.sent_at.isoformat() if ctx.send_result.sent_at else datetime.now(UTC).isoformat(),
                    ack_at=ctx.send_result.ack_at.isoformat() if ctx.send_result.ack_at else datetime.now(UTC).isoformat(),
                )
                if ctx.excise_marks and ctx.command.operation_type == OperationType.SELL:
                    ExciseRepository.mark_document_sold(conn, document_id=ctx.document.document_id, sold_at=ctx.send_result.ack_at.isoformat() if ctx.send_result.ack_at else None)
                elif ctx.excise_marks and ctx.command.operation_type == OperationType.RETURN:
                    codes = [m['normalized_mark_code'] for m in ctx.excise_marks]
                    ExciseRepository.mark_returned(conn, mark_codes=codes, returned_at=ctx.send_result.ack_at.isoformat() if ctx.send_result.ack_at else None)
                self._apply_shift_side_effects_locked(conn, ctx=ctx, target_state=DocumentState.ACK)
                outcome = 'ACK'
                audit_event = 'DOCUMENT_ACK'
                protocol_result = 'ACK'
                transport_status = 'ACK'
            else:
                ctx.document = FiscalDocumentRepository.update_state(
                    conn,
                    document_id=ctx.document.document_id,
                    state=send_state,
                    response_json=ctx.send_result.response_json,
                    submission_status=ctx.send_result.submission_status,
                    transport_request_id=ctx.send_result.transport_request_id,
                    server_fiscal_no=ctx.send_result.server_fiscal_no,
                    server_fiscal_date=ctx.send_result.server_fiscal_date,
                    sent_at=ctx.send_result.sent_at.isoformat() if ctx.send_result.sent_at else datetime.now(UTC).isoformat(),
                )
                self._apply_shift_side_effects_locked(conn, ctx=ctx, target_state=send_state)
                outcome = send_state.value
                audit_event = 'DOCUMENT_PENDING'
                protocol_result = send_state.value
                transport_status = send_state.value
            if ctx.send_result.response_json is not None:
                DocumentFilesRepository.add_file(
                    conn,
                    file_id=f'{ctx.document.document_id}-response',
                    document_id=ctx.document.document_id,
                    file_kind=FileKind.PROTO_RESPONSE,
                    path=f'/archive/{ctx.fiscal_number}/{ctx.document.document_id}/response.json',
                    content=ctx.send_result.response_json,
                )
            if ctx.transport_profile is not None:
                TransportTraceRepository.log_event(
                    conn,
                    trace_id=f'{ctx.document.document_id}-transport-in',
                    fiscal_number=ctx.fiscal_number,
                    backend_profile_id=ctx.document.backend_profile_id,
                    transport_profile_id=ctx.document.transport_profile_id,
                    endpoint=ctx.transport_profile.endpoint,
                    direction='IN',
                    response_envelope=ctx.send_result.response_json,
                    technical_status=transport_status,
                    business_status=ctx.send_result.submission_status,
                    correlation_id=ctx.command.correlation_id,
                )
            if send_state == DocumentState.ACK:
                OutboxRepository.enqueue_document(
                    conn,
                    outbox_id=f'{ctx.document.document_id}-outbox',
                    aggregate_id=ctx.document.document_id,
                    fiscal_number=ctx.fiscal_number,
                    payload_path=f'/archive/{ctx.fiscal_number}/{ctx.document.document_id}.json',
                    payload_sha256=ctx.document.payload_sha256,
                    sequence_no=ctx.document.lnd,
                )
            AuditRepository.log_events(
                conn,
                events=[('DOCUMENT', ctx.document.document_id, audit_event, 'INFO', dumps_json({'state': ctx.document.state.value, 'fs_mode': ctx.document.fs_mode}))],
            )
            self._log_protocol_event(conn, inbox=ctx.inbox, command=ctx.command, result_code=protocol_result)
        updated = InboxRepository.mark_done(conn, request_id=ctx.inbox.request_id, lease_token=ctx.lease_token, result_document_id=ctx.document.document_id)
        conn.commit()
        self.logger.info("stage_finalize_ack", extra={"extra_fields": {"request_id": ctx.inbox.request_id, "document_id": ctx.document.document_id, "state": ctx.document.state.value}})

        # Sprint 10 Step 10: canonical layer fields — read-after-ACK, no writes
        cash_balance = None
        change = None
        rounded_sum = None
        rounding = None

        if outcome in ('ACK', 'OFFLINE_LOCAL_ACK') and ctx.active_shift:
            cash_balance = self._get_shift_cash_balance(conn, ctx.active_shift.shift_id)

        if ctx.command.operation_type in (OperationType.SELL, OperationType.RETURN):
            receipt = ctx.command.payload.get('receipt', {}) if isinstance(ctx.command.payload, dict) else {}
            totals = receipt.get('totals', {}) if isinstance(receipt, dict) else {}
            payments = receipt.get('payments', []) if isinstance(receipt, dict) else []
            total_sum = int(totals.get('total_sum', 0) or 0) if isinstance(totals, dict) else 0
            all_payments_sum = sum(int(p.get('amount', 0)) for p in payments if isinstance(p, dict))
            raw_change = all_payments_sum - total_sum if all_payments_sum > total_sum else 0
            # Suppress change=0 unless node_state.print_zero_change is enabled
            if raw_change > 0 or (ctx.node_state and ctx.node_state.print_zero_change):
                change = raw_change

            enriched = self._enrich_payload_for_dps(conn, ctx)
            enriched_receipt = enriched.get('receipt', {}) if isinstance(enriched, dict) else {}
            enriched_rounding = enriched_receipt.get('rounding', 0) if isinstance(enriched_receipt, dict) else 0
            if enriched_rounding != 0:
                rounding = enriched_rounding
                rounded_sum = total_sum + enriched_rounding

        return WorkerProcessResult(
            outcome=outcome,
            request_id=ctx.inbox.request_id,
            document_id=ctx.document.document_id,
            inbox_status=updated.status.value if updated else 'DONE',
            document_state=ctx.document.state.value,
            cash_balance=cash_balance,
            change=change,
            rounded_sum=rounded_sum,
            rounding=rounding,
        )

    def _apply_shift_side_effects_locked(self, conn: sqlite3.Connection, *, ctx: WorkerContext, target_state: DocumentState) -> None:
        assert ctx.document is not None and ctx.command is not None
        op = ctx.command.operation_type
        if op == OperationType.SHIFT_OPEN:
            self._sync_shift_open_locked(conn, ctx=ctx, target_state=target_state)
        elif op == OperationType.SHIFT_CLOSE:
            self._sync_shift_close_locked(conn, ctx=ctx, target_state=target_state)
        elif op == OperationType.Z_REPORT and target_state in {DocumentState.ACK, DocumentState.OFFLINE_LOCAL_ACK, DocumentState.SENT, DocumentState.KVT1, DocumentState.KVT2}:
            active_shift = ShiftRepository.get_active_shift(conn, ctx.fiscal_number)
            if active_shift is not None:
                ShiftRepository.link_document(conn, shift_id=active_shift.shift_id, z_report_document_id=ctx.document.document_id)
                # Cash balance carry-over: write snapshot for next SHIFT_OPEN
                final_balance = max(0, self._get_shift_cash_balance(conn, active_shift.shift_id))
                node = NodeStateRepository.get_state(conn, ctx.fiscal_number)
                if node and node.cash_balance_mode == 'preserve':
                    NodeStateRepository.update_last_cash_balance(conn, fiscal_number=ctx.fiscal_number, balance=final_balance)
                else:
                    NodeStateRepository.update_last_cash_balance(conn, fiscal_number=ctx.fiscal_number, balance=0)
                # Offline Z_REPORT signals day-close intent: transition shift to CLOSING
                # to prevent further fiscal operations until sync + close completes.
                # Online Z_REPORT does not close the shift — SHIFT_CLOSE handles that.
                if target_state == DocumentState.OFFLINE_LOCAL_ACK and active_shift.state != ShiftState.CLOSING:
                    ShiftRepository.update_state(conn, shift_id=active_shift.shift_id, state=ShiftState.CLOSING)

    def _sync_shift_open_locked(self, conn: sqlite3.Connection, *, ctx: WorkerContext, target_state: DocumentState) -> None:
        assert ctx.document is not None and ctx.command is not None
        active_shift = ShiftRepository.get_active_shift(conn, ctx.fiscal_number)
        if target_state in {DocumentState.ACK, DocumentState.OFFLINE_LOCAL_ACK}:
            if active_shift is None:
                ShiftRepository.create_shift(
                    conn,
                    shift_id=f'shift-{ctx.document.document_id}',
                    fiscal_number=ctx.fiscal_number,
                    state=ShiftState.OPENED,
                    open_mode=ctx.document.fs_mode,
                    backend_profile_id=ctx.document.backend_profile_id,
                    transport_profile_id=ctx.document.transport_profile_id,
                    protocol=ctx.command.protocol,
                    integration_owner=ctx.command.channel_owner,
                    channel_lock_acquired_at=ctx.send_result.ack_at.isoformat() if ctx.send_result and ctx.send_result.ack_at else datetime.now(UTC).isoformat(),  # send_result is None for OFFLINE path; guard falls to now()
                    open_document_id=ctx.document.document_id,
                )
            elif active_shift.state != ShiftState.OPENED:
                ShiftRepository.update_state(conn, shift_id=active_shift.shift_id, state=ShiftState.OPENED)
                ShiftRepository.link_document(conn, shift_id=active_shift.shift_id, open_document_id=ctx.document.document_id)
            elif active_shift.open_document_id is None:
                ShiftRepository.link_document(conn, shift_id=active_shift.shift_id, open_document_id=ctx.document.document_id)
            return
        if target_state in {DocumentState.SENT, DocumentState.KVT1, DocumentState.KVT2} and active_shift is None:
            ShiftRepository.create_shift(
                conn,
                shift_id=f'shift-{ctx.document.document_id}',
                fiscal_number=ctx.fiscal_number,
                state=ShiftState.OPENING,
                open_mode=ctx.document.fs_mode,
                backend_profile_id=ctx.document.backend_profile_id,
                transport_profile_id=ctx.document.transport_profile_id,
                protocol=ctx.command.protocol,
                integration_owner=ctx.command.channel_owner,
                channel_lock_acquired_at=ctx.send_result.sent_at.isoformat() if ctx.send_result and ctx.send_result.sent_at else datetime.now(UTC).isoformat(),
                open_document_id=ctx.document.document_id,
            )

    def _sync_shift_close_locked(self, conn: sqlite3.Connection, *, ctx: WorkerContext, target_state: DocumentState) -> None:
        active_shift = ShiftRepository.get_active_shift(conn, ctx.fiscal_number)
        if active_shift is None:
            return
        if target_state in {DocumentState.ACK, DocumentState.OFFLINE_LOCAL_ACK}:
            ShiftRepository.update_state(conn, shift_id=active_shift.shift_id, state=ShiftState.CLOSED)
            ShiftRepository.link_document(conn, shift_id=active_shift.shift_id, close_document_id=ctx.document.document_id)
            return
        if target_state in {DocumentState.SENT, DocumentState.KVT1, DocumentState.KVT2}:
            if active_shift.state != ShiftState.CLOSING:
                ShiftRepository.update_state(conn, shift_id=active_shift.shift_id, state=ShiftState.CLOSING)
            if active_shift.close_document_id is None:
                ShiftRepository.link_document(conn, shift_id=active_shift.shift_id, close_document_id=ctx.document.document_id)

    def _record_prepared_locked(self, conn: sqlite3.Connection, ctx: WorkerContext) -> None:
        assert ctx.inbox is not None and ctx.command is not None and ctx.document is not None
        DocumentFilesRepository.add_file(
            conn,
            file_id=f'{ctx.document.document_id}-request',
            document_id=ctx.document.document_id,
            file_kind=FileKind.REQUEST_JSON,
            path=f'/archive/{ctx.fiscal_number}/{ctx.document.document_id}/request.json',
            content=ctx.inbox.payload_json,
        )
        AuditRepository.log_events(
            conn,
            events=[('DOCUMENT', ctx.document.document_id, 'DOCUMENT_PREPARED', 'INFO', dumps_json({'state': 'PREPARED', 'fs_mode': ctx.document.fs_mode, 'offline_fiscal_no': ctx.document.offline_fiscal_no}))],
        )
        self._log_protocol_event(conn, inbox=ctx.inbox, command=ctx.command, result_code='PREPARED')

    def _mark_error_locked(
        self,
        conn: sqlite3.Connection,
        *,
        ctx: WorkerContext,
        error: CanonicalError,
        document_id: str | None,
        state: DocumentState | None,
    ) -> WorkerProcessResult:
        assert ctx.inbox is not None
        if document_id and state is not None:
            FiscalDocumentRepository.update_state(
                conn,
                document_id=document_id,
                state=state,
                canonical_error_code=error.code.value,
                error_message=error.message,
            )
        updated = InboxRepository.mark_error(
            conn,
            request_id=ctx.inbox.request_id,
            lease_token=ctx.lease_token,
            canonical_error_code=error.code.value,
            error_message=error.message,
        )
        AuditRepository.log_events(
            conn,
            events=[(
                'INBOX' if document_id is None else 'DOCUMENT',
                ctx.inbox.request_id if document_id is None else document_id,
                'COMMAND_ERROR' if document_id is None else 'DOCUMENT_ERROR',
                'ERROR',
                dumps_json({'code': error.code.value, 'message': error.message, 'state': state.value if state else None}),
            )],
        )
        self._log_protocol_event(conn, inbox=ctx.inbox, command=ctx.command, result_code=error.code.value, error=error)
        conn.commit()
        return WorkerProcessResult(outcome='ERROR', request_id=ctx.inbox.request_id, inbox_status=updated.status.value if updated else 'ERROR', canonical_error=error, document_id=document_id, document_state=state.value if state else None)

    def _mark_document_and_inbox_error(
        self,
        conn: sqlite3.Connection,
        *,
        ctx: WorkerContext,
        document_id: str,
        state: DocumentState,
        error: CanonicalError,
        technical_status: str | None = None,
        submission_status: str | None = None,
        response_json: str | None = None,
    ) -> WorkerProcessResult:
        assert ctx.inbox is not None and ctx.command is not None
        conn.execute('BEGIN IMMEDIATE')
        document = FiscalDocumentRepository.update_state(
            conn,
            document_id=document_id,
            state=state,
            canonical_error_code=error.code.value,
            error_message=error.message,
            submission_status=submission_status,
            response_json=response_json,
        )
        updated = InboxRepository.mark_error(
            conn,
            request_id=ctx.inbox.request_id,
            lease_token=ctx.lease_token,
            canonical_error_code=error.code.value,
            error_message=error.message,
        )
        AuditRepository.log_events(
            conn,
            events=[('DOCUMENT', document_id, 'DOCUMENT_ERROR', 'ERROR', dumps_json({'state': state.value, 'code': error.code.value, 'message': error.message}))],
        )
        self._log_protocol_event(conn, inbox=ctx.inbox, command=ctx.command, result_code=error.code.value, error=error)
        if document is not None:
            transport_profile = TransportProfileRepository.get_by_id(conn, document.transport_profile_id)
            TransportTraceRepository.log_event(
                conn,
                trace_id=f'{document.document_id}-transport-error',
                fiscal_number=ctx.fiscal_number,
                backend_profile_id=document.backend_profile_id,
                transport_profile_id=document.transport_profile_id,
                endpoint=transport_profile.endpoint if transport_profile else None,
                direction='EVENT',
                technical_status=technical_status or state.value,
                business_status=error.code.value,
                correlation_id=ctx.command.correlation_id,
            )
        conn.commit()
        return WorkerProcessResult(
            outcome='ERROR',
            request_id=ctx.inbox.request_id,
            document_id=document.document_id if document else document_id,
            inbox_status=updated.status.value if updated else 'ERROR',
            document_state=document.state.value if document else state.value,
            canonical_error=error,
        )

    def _guard_preconditions(self, conn: sqlite3.Connection, *, inbox: InboxRecord, command: CanonicalFiscalCommand) -> CanonicalError | None:
        backend_profile_id = command.backend_profile_id or inbox.backend_profile_id
        if backend_profile_id is None:
            return build_canonical_error(CanonicalErrorCode.UNSUPPORTED_BY_BACKEND, message='backend_profile_id is required for worker processing.')
        capabilities = BackendProfileRepository.get_capabilities(conn, backend_profile_id)
        op = command.operation_type
        if op == OperationType.CASH_WITHDRAWAL and not capabilities.get('supports_cash_withdrawal', False):
            return build_canonical_error(CanonicalErrorCode.UNSUPPORTED_BY_BACKEND, message='CASH_WITHDRAWAL is not supported by current backend.')
        if op in {OperationType.SERVICE_IN, OperationType.SERVICE_OUT} and not capabilities.get('supports_service_receipts', False):
            return build_canonical_error(CanonicalErrorCode.UNSUPPORTED_BY_BACKEND, message='Service receipts are not supported by current backend.')
        if op == OperationType.X_REPORT and not capabilities.get('supports_x_report', False):
            return build_canonical_error(CanonicalErrorCode.UNSUPPORTED_BY_BACKEND, message='X-report is not supported by current backend.')

        # RETURN linkage gate: production requires related_receipt_id
        if op == OperationType.RETURN and self.require_return_linkage:
            receipt = command.payload.get('receipt') if isinstance(command.payload, dict) else None
            related = receipt.get('related_receipt_id') if isinstance(receipt, dict) else None
            if not related:
                return build_canonical_error(CanonicalErrorCode.RETURN_LINKAGE_REQUIRED)

        # Timestamp sanity: not in future, not older than 24h
        if self.validate_timestamps and command.business_ts is not None:
            now = datetime.now(UTC)
            bts = command.business_ts
            if bts.tzinfo is None:
                bts = bts.replace(tzinfo=UTC)
            delta_seconds = (bts - now).total_seconds()
            if delta_seconds > 300:  # >5 min in future
                return build_canonical_error(
                    CanonicalErrorCode.INVALID_FISCAL_DATE,
                    message=f'business_ts is {delta_seconds / 60:.0f} minutes in the future. Check device clock.',
                )
            if delta_seconds < -86400:  # >24h in past
                return build_canonical_error(
                    CanonicalErrorCode.INVALID_FISCAL_DATE,
                    message=f'business_ts is {abs(delta_seconds) / 3600:.1f} hours in the past. Backdating not allowed.',
                )

        # Receipt sanity checks
        if op in {OperationType.SELL, OperationType.RETURN}:
            sanity_error = self._guard_receipt_sanity(command)
            if sanity_error is not None:
                return sanity_error

        # Cash transaction limit: 50 000 UAH per receipt (law requirement)
        if op in {OperationType.SELL, OperationType.RETURN}:
            cash_limit_error = self._guard_cash_transaction_limit(command)
            if cash_limit_error is not None:
                return cash_limit_error

        # Excise license master switch: block excise goods on unlicensed PRRO
        if op in {OperationType.SELL, OperationType.RETURN}:
            excise_license_error = self._guard_excise_license(conn, fiscal_number=inbox.fiscal_number, command=command)
            if excise_license_error is not None:
                return excise_license_error

        # Tax group compliance: check UKTZED / excise mark requirements per group
        if op in {OperationType.SELL, OperationType.RETURN}:
            tax_guard_error = self._guard_tax_group_compliance(conn, fiscal_number=inbox.fiscal_number, command=command)
            if tax_guard_error is not None:
                return tax_guard_error

        # Payment type validation: reject unknown types before sign
        if op in {OperationType.SELL, OperationType.RETURN, OperationType.CASH_WITHDRAWAL}:
            pay_type_error = self._guard_payment_types(conn, fiscal_number=inbox.fiscal_number, command=command)
            if pay_type_error is not None:
                return pay_type_error

        active_shift = ShiftRepository.get_active_shift(conn, inbox.fiscal_number)
        if op == OperationType.SHIFT_OPEN and active_shift is not None:
            return build_canonical_error(CanonicalErrorCode.SHIFT_ALREADY_OPEN)
        if op == OperationType.SHIFT_CLOSE and active_shift is None:
            return build_canonical_error(CanonicalErrorCode.SHIFT_NOT_OPEN)
        if op in {OperationType.SELL, OperationType.RETURN, OperationType.SERVICE_IN, OperationType.SERVICE_OUT, OperationType.CASH_WITHDRAWAL, OperationType.X_REPORT, OperationType.Z_REPORT} and active_shift is None:
            return build_canonical_error(CanonicalErrorCode.SHIFT_NOT_OPEN)

        # Shift duration > 24h: block fiscal operations (legal requirement)
        if active_shift is not None and op not in {OperationType.SHIFT_OPEN, OperationType.Z_REPORT, OperationType.SHIFT_CLOSE}:
            shift_opened = active_shift.opened_at or active_shift.created_at
            if shift_opened:
                try:
                    opened_dt = datetime.fromisoformat(str(shift_opened))
                    if opened_dt.tzinfo is None:
                        opened_dt = opened_dt.replace(tzinfo=UTC)
                    hours = (datetime.now(UTC) - opened_dt).total_seconds() / 3600
                    if hours >= 24:
                        return build_canonical_error(
                            CanonicalErrorCode.INVALID_RECEIPT_SEQUENCE,
                            message=f'Shift open for {hours:.1f} hours (limit 24h). Close shift with Z_REPORT first.',
                        )
                except (ValueError, TypeError):
                    pass

        # Shift in CLOSING (offline day-close initiated): block further fiscal ops.
        # Only SHIFT_CLOSE is allowed to finalize the close sequence.
        if active_shift is not None and active_shift.state == ShiftState.CLOSING:
            if op in {OperationType.SELL, OperationType.RETURN, OperationType.SERVICE_IN, OperationType.SERVICE_OUT, OperationType.CASH_WITHDRAWAL, OperationType.X_REPORT}:
                return build_canonical_error(
                    CanonicalErrorCode.INVALID_RECEIPT_SEQUENCE,
                    message=f'{op.value} blocked: shift is in CLOSING state (day-close initiated)',
                )
            # SHIFT_CLOSE finalization guard: linked Z_REPORT must be DPS-confirmed (ACK)
            # before the shift can be closed.  If z_report_document_id is set but the
            # document is still OFFLINE_LOCAL_ACK or SENT, the operator must sync first.
            if op == OperationType.SHIFT_CLOSE and active_shift.z_report_document_id is not None:
                zr_doc = FiscalDocumentRepository.get_by_id(conn, active_shift.z_report_document_id)
                if zr_doc is not None and zr_doc.state != DocumentState.ACK:
                    return build_canonical_error(
                        CanonicalErrorCode.OFFLINE_BACKLOG_NOT_SYNCED,
                        message=f'SHIFT_CLOSE blocked: linked Z_REPORT ({active_shift.z_report_document_id}) is {zr_doc.state.value}, must be ACK before closing shift',
                    )

        if active_shift is not None:
            lock = ShiftRepository.get_channel_lock(conn, inbox.fiscal_number)
            if lock is not None and not self._channel_matches(lock=lock, inbox=inbox, command=command):
                return build_canonical_error(CanonicalErrorCode.SHIFT_CHANNEL_SWITCH_FORBIDDEN)

        # Legal blocker: SHIFT_CLOSE and Z_REPORT are forbidden while
        # there are unsynced offline documents for this fiscal_number.
        # The operator must drain the offline backlog via /v1/admin/offline-sync first.
        # Exception: in OFFLINE mode, Z_REPORT is allowed as the closing operation —
        # it joins the backlog and will be synced to DPS together with other offline docs.
        # SHIFT_CLOSE is always blocked with pending backlog regardless of mode.
        # Placed AFTER channel-lock check to preserve SHIFT_CHANNEL_SWITCH_FORBIDDEN precedence.
        if op == OperationType.Z_REPORT and active_shift is not None and active_shift.z_report_document_id is not None:
            return build_canonical_error(
                CanonicalErrorCode.INVALID_RECEIPT_SEQUENCE,
                message=f'Z_REPORT already submitted for this shift (document {active_shift.z_report_document_id})',
            )

        if op in {OperationType.SHIFT_CLOSE, OperationType.Z_REPORT}:
            pending = FiscalDocumentRepository.count_pending_for_offline_sync(conn, fiscal_number=inbox.fiscal_number)
            if pending > 0:
                if op == OperationType.Z_REPORT:
                    node_state = NodeStateRepository.get_state(conn, inbox.fiscal_number)
                    if node_state is not None and node_state.mode == NodeMode.OFFLINE:
                        pass  # allow offline Z_REPORT through
                    else:
                        return build_canonical_error(
                            CanonicalErrorCode.OFFLINE_BACKLOG_NOT_SYNCED,
                            message=f'{op.value} blocked: {pending} offline document(s) pending DPS sync for {inbox.fiscal_number}',
                        )
                else:
                    return build_canonical_error(
                        CanonicalErrorCode.OFFLINE_BACKLOG_NOT_SYNCED,
                        message=f'{op.value} blocked: {pending} offline document(s) pending DPS sync for {inbox.fiscal_number}',
                    )

        # Cash balance guards: SERVICE_OUT and CASH_WITHDRAWAL cannot exceed available cash
        if active_shift is not None and op == OperationType.SERVICE_OUT:
            try:
                service_sum = int(command.payload.get('service_sum', 0) if isinstance(command.payload, dict) else 0)
            except (ValueError, TypeError):
                service_sum = 0
            current_balance = self._get_shift_cash_balance(conn, active_shift.shift_id)
            if service_sum > current_balance:
                return build_canonical_error(
                    CanonicalErrorCode.INVALID_RECEIPT_DATA,
                    message=f'SERVICE_OUT sum {service_sum} exceeds current cash balance {current_balance}',
                )

        if active_shift is not None and op == OperationType.CASH_WITHDRAWAL:
            try:
                cash_sum = int(command.payload.get('cash_withdrawal_sum', 0) if isinstance(command.payload, dict) else 0)
            except (ValueError, TypeError):
                cash_sum = 0
            current_balance = self._get_shift_cash_balance(conn, active_shift.shift_id)
            if cash_sum > current_balance:
                return build_canonical_error(
                    CanonicalErrorCode.INVALID_RECEIPT_DATA,
                    message=f'CASH_WITHDRAWAL sum {cash_sum} exceeds current cash balance {current_balance}',
                )

        return None

    @staticmethod
    def _channel_matches(*, lock, inbox: InboxRecord, command: CanonicalFiscalCommand) -> bool:
        backend_profile_id = command.backend_profile_id or inbox.backend_profile_id
        transport_profile_id = command.transport_profile_id or inbox.transport_profile_id
        channel_owner = command.channel_owner or inbox.channel_owner
        return (
            backend_profile_id == lock.backend_profile_id
            and transport_profile_id == lock.transport_profile_id
            and command.protocol == lock.protocol
            and channel_owner == lock.integration_owner
        )

    @classmethod
    def _check_offline_limits(cls, *, node_state, offline_session) -> CanonicalError | None:
        now = datetime.now(UTC)
        try:
            started_at = datetime.fromisoformat(offline_session.started_at)
        except ValueError:
            return build_canonical_error(CanonicalErrorCode.OFFLINE_LIMIT_REACHED, message='Offline session has invalid started_at timestamp.')
        if started_at.tzinfo is None:
            started_at = started_at.replace(tzinfo=UTC)
        continuous_seconds = max(0, int((now - started_at).total_seconds()))
        current_month_bucket = now.strftime('%Y-%m')
        node_month_seconds = node_state.current_month_offline_seconds if node_state.current_month_bucket == current_month_bucket else 0
        session_month_seconds = offline_session.accumulated_month_seconds
        if started_at.strftime('%Y-%m') == current_month_bucket:
            session_month_seconds += continuous_seconds
        monthly_seconds = max(node_month_seconds, session_month_seconds)

        if continuous_seconds >= cls.MAX_OFFLINE_CONTINUOUS_SECONDS:
            return build_canonical_error(
                CanonicalErrorCode.OFFLINE_LIMIT_REACHED,
                message='Continuous offline time limit exceeded.',
                details={'continuous_seconds': continuous_seconds, 'limit_seconds': cls.MAX_OFFLINE_CONTINUOUS_SECONDS},
            )
        if monthly_seconds >= cls.MAX_OFFLINE_MONTH_SECONDS:
            return build_canonical_error(
                CanonicalErrorCode.OFFLINE_LIMIT_REACHED,
                message='Monthly offline time limit exceeded.',
                details={'monthly_seconds': monthly_seconds, 'limit_seconds': cls.MAX_OFFLINE_MONTH_SECONDS},
            )
        return None

    @staticmethod
    def _normalize_excise_mark(raw: object | None) -> str:
        from ..repositories.excise import normalize_excise_mark
        return normalize_excise_mark(raw)

    @staticmethod
    def _operation_supports_offline(operation_type: OperationType) -> bool:
        return operation_type in {
            OperationType.SELL,
            OperationType.RETURN,
            OperationType.SERVICE_IN,
            OperationType.SERVICE_OUT,
            OperationType.CASH_WITHDRAWAL,
            OperationType.X_REPORT,
            OperationType.Z_REPORT,
        }

    # 50 000 UAH in kopecks (legal cash transaction limit per receipt)
    CASH_TRANSACTION_LIMIT = 50_000_00

    # Excise mark format: 4 Latin letters + 6 digits (e.g. "FRTG456985")
    _EXCISE_MARK_RE = re.compile(r'^[A-Z]{4}[0-9]{6}$')

    def _enrich_payload_for_dps(self, conn: sqlite3.Connection, ctx: WorkerContext) -> dict | None:
        """Shared payload enrichment for DPS XML — called by both _resolve_sign_input and MAC recovery.

        Handles: SHIFT_OPEN carry-over, Z_REPORT aggregation, SELL/RETURN rounding.
        Single source of truth to prevent recurring divergence between normal and recovery paths.
        """
        payload = ctx.command.payload if isinstance(ctx.command.payload, dict) else None

        # SHIFT_OPEN: inject carry-over cash balance
        if ctx.command.operation_type == OperationType.SHIFT_OPEN:
            node = NodeStateRepository.get_state(conn, ctx.fiscal_number)
            opening_sum = 0
            if node and node.cash_balance_mode == 'preserve':
                opening_sum = node.last_cash_balance or 0
            payload = dict(payload) if payload else {}
            payload.setdefault('receipt', {})
            if isinstance(payload['receipt'], dict):
                payload['receipt'].setdefault('totals', {})
                if isinstance(payload['receipt']['totals'], dict):
                    payload['receipt']['totals']['total_sum'] = opening_sum

        # Z_REPORT: aggregate shift data
        elif ctx.command.operation_type == OperationType.Z_REPORT:
            z_data = self._aggregate_shift_for_z_report(conn, ctx)
            payload = dict(payload) if payload else {}
            payload['z_report_data'] = z_data

        # SELL/RETURN: apply rounding if enabled and has cash payment
        if ctx.command.operation_type in (OperationType.SELL, OperationType.RETURN):
            node = NodeStateRepository.get_state(conn, ctx.fiscal_number)
            if node and node.rounding_enabled:
                from ..utils.rounding import round_to_50_kopecks
                p_receipt = (payload or {}).get('receipt', {})
                p_payments = p_receipt.get('payments', []) if isinstance(p_receipt, dict) else []
                has_cash = any(
                    p.get('payment_type', p.get('type', '')) == 'CASH'
                    for p in p_payments if isinstance(p, dict)
                )
                if has_cash:
                    p_totals = p_receipt.get('totals', {}) if isinstance(p_receipt, dict) else {}
                    ts = int(p_totals.get('total_sum', 0) or 0) if isinstance(p_totals, dict) else 0
                    rounded = round_to_50_kopecks(ts)
                    rounding_delta = rounded - ts
                    if rounding_delta != 0:
                        payload = dict(payload) if payload else {}
                        payload.setdefault('receipt', {})
                        payload['receipt'] = dict(payload['receipt']) if isinstance(payload['receipt'], dict) else {}
                        payload['receipt']['rounding'] = rounding_delta

        # Payment type resolution: resolve T and NM from payment_type_definitions
        # Skip CASH_WITHDRAWAL — has its own _build_cash_withdrawal serializer
        if ctx.command.operation_type in (OperationType.SELL, OperationType.RETURN):
            p_receipt = (payload or {}).get('receipt', {})
            p_payments = p_receipt.get('payments', []) if isinstance(p_receipt, dict) else []
            if p_payments:
                payload = dict(payload) if payload else {}
                payload.setdefault('receipt', {})
                payload['receipt'] = dict(payload['receipt']) if isinstance(payload['receipt'], dict) else {}
                resolved_payments = []
                for p in p_payments:
                    if not isinstance(p, dict):
                        resolved_payments.append(p)
                        continue
                    p = dict(p)
                    ptype = p.get('payment_type', p.get('type', 'CASH'))
                    if ptype == 'CASH':
                        p['resolved_t'] = '0'
                        p['resolved_nm'] = 'Готівка'
                    else:
                        pt_def = PaymentTypeRepository.get_type(conn, ctx.fiscal_number, ptype)
                        if pt_def:
                            p['resolved_t'] = str(pt_def.type_group)
                            p['resolved_nm'] = pt_def.name
                        else:
                            p['resolved_t'] = '2'
                            p['resolved_nm'] = ptype
                    resolved_payments.append(p)
                payload['receipt']['payments'] = resolved_payments

        return payload

    @staticmethod
    def _get_shift_cash_balance(conn: sqlite3.Connection, shift_id: str) -> int:
        """Derive current cash balance from acknowledged documents in shift.

        Not a persisted counter — always computed on the fly.
        Includes OFFLINE_LOCAL_ACK for Sprint 11 compatibility.
        """
        rows = conn.execute(
            """SELECT doc_type, payload_json FROM fiscal_documents
               WHERE shift_id = ? AND state IN ('ACK', 'OFFLINE_LOCAL_ACK')""",
            (shift_id,),
        ).fetchall()

        balance = 0
        for doc_type, payload_json in rows:
            try:
                payload = json.loads(payload_json) if payload_json else {}
            except (ValueError, TypeError):
                payload = {}
            cmd_payload = payload.get('payload', payload)
            if not isinstance(cmd_payload, dict):
                continue
            receipt = cmd_payload.get('receipt', {})
            if not isinstance(receipt, dict):
                receipt = {}

            if doc_type == 'SERVICE_IN':
                balance += int(cmd_payload.get('service_sum', 0))
            elif doc_type == 'SERVICE_OUT':
                balance -= int(cmd_payload.get('service_sum', 0))
            elif doc_type == 'CASH_WITHDRAWAL':
                balance -= int(cmd_payload.get('cash_withdrawal_sum', 0))
            elif doc_type in ('SELL', 'RETURN'):
                for p in (receipt.get('payments') or []):
                    if not isinstance(p, dict):
                        continue
                    ptype = p.get('payment_type', p.get('type', 'CASH'))
                    if ptype == 'CASH':
                        amt = int(p.get('amount', 0))
                        if doc_type == 'SELL':
                            balance += amt
                        else:
                            balance -= amt
        return balance

    @staticmethod
    def compute_canonical_fields_from_document(conn: sqlite3.Connection, doc: object) -> dict:
        """Recompute canonical layer fields from a stored FiscalDocumentRecord.

        Used for replay responses where WorkerProcessResult is NOOP.
        Returns dict with keys: cash_balance, change, rounded_sum, rounding (all nullable).
        """
        import json as _json
        from ..repositories import NodeStateRepository
        from ..utils.rounding import round_to_50_kopecks

        cash_balance = None
        change = None
        rounded_sum = None
        rounding = None

        shift_id = getattr(doc, 'shift_id', None)
        if shift_id:
            cash_balance = WritePathWorker._get_shift_cash_balance(conn, shift_id)

        doc_type = getattr(doc, 'doc_type', None)
        doc_state = getattr(doc, 'state', None)
        doc_state_val = doc_state.value if hasattr(doc_state, 'value') else str(doc_state)

        if doc_type in ('SELL', 'RETURN') and doc_state_val in ('ACK', 'OFFLINE_LOCAL_ACK'):
            fiscal_number = getattr(doc, 'fiscal_number', None)
            payload_json = getattr(doc, 'payload_json', None)
            try:
                raw = _json.loads(payload_json) if payload_json else {}
            except (ValueError, TypeError):
                raw = {}
            cmd_payload = raw.get('payload', {}) if isinstance(raw, dict) else {}
            receipt = cmd_payload.get('receipt', {}) if isinstance(cmd_payload, dict) else {}
            totals = receipt.get('totals', {}) if isinstance(receipt, dict) else {}
            payments = receipt.get('payments', []) if isinstance(receipt, dict) else []

            total_sum = int(totals.get('total_sum', 0) or 0) if isinstance(totals, dict) else 0
            payments_sum = sum(int(p.get('amount', 0)) for p in payments if isinstance(p, dict))

            raw_change = payments_sum - total_sum if payments_sum > total_sum else 0
            node = NodeStateRepository.get_state(conn, fiscal_number) if fiscal_number else None
            if raw_change > 0 or (node and node.print_zero_change):
                change = raw_change

            if node and node.rounding_enabled:
                has_cash = any(
                    p.get('payment_type', p.get('type', '')) == 'CASH'
                    for p in payments if isinstance(p, dict)
                )
                if has_cash and total_sum > 0:
                    rounded = round_to_50_kopecks(total_sum)
                    rounding_delta = rounded - total_sum
                    if rounding_delta != 0:
                        rounding = rounding_delta
                        rounded_sum = total_sum + rounding_delta

        return {'cash_balance': cash_balance, 'change': change, 'rounded_sum': rounded_sum, 'rounding': rounding}

    @staticmethod
    def _aggregate_shift_for_z_report(conn, ctx) -> dict:
        """Aggregate fiscal_documents for current shift into Z-report data.

        Delegates to shared aggregate_shift_data(), excluding the Z_REPORT document itself.
        """
        from .shift_aggregation import aggregate_shift_data
        shift_id = ctx.document.shift_id if ctx.document else None
        if not shift_id:
            return {}
        return aggregate_shift_data(conn, shift_id, exclude_document_id=ctx.document.document_id)

    @staticmethod
    def _guard_receipt_sanity(command: CanonicalFiscalCommand) -> CanonicalError | None:
        """Basic sanity: no empty receipts, no negative amounts, payments present."""
        receipt = command.payload.get('receipt') if isinstance(command.payload, dict) else None
        if not isinstance(receipt, dict):
            return build_canonical_error(CanonicalErrorCode.INVALID_RECEIPT_DATA, message='receipt object is missing')
        goods = receipt.get('goods')
        if not isinstance(goods, list) or len(goods) == 0:
            return build_canonical_error(CanonicalErrorCode.INVALID_RECEIPT_DATA, message='receipt has no goods')
        payments = receipt.get('payments')
        if not isinstance(payments, list) or len(payments) == 0:
            return build_canonical_error(CanonicalErrorCode.INVALID_RECEIPT_DATA, message='receipt has no payments')

        errors = []
        for i, g in enumerate(goods):
            if not isinstance(g, dict):
                continue
            name = g.get('name', '')
            if not name or not str(name).strip():
                errors.append(f'item #{i+1}: empty product name')
                name = f'item #{i+1}'
            price = g.get('price', 0)
            qty = g.get('quantity', 0)
            sm = g.get('sum', 0)
            if not isinstance(price, (int, float)) or price < 0:
                errors.append(f'{name}: negative or invalid price ({price})')
            if not isinstance(qty, (int, float)) or qty <= 0:
                errors.append(f'{name}: quantity must be > 0 ({qty})')
            if not isinstance(sm, (int, float)) or sm < 0:
                errors.append(f'{name}: negative or invalid sum ({sm})')

            # Excise mark format: 4 Latin letters + 6 digits (e.g. FRTG456985)
            for mark in (g.get('excise_barcodes') or []):
                if not isinstance(mark, str) or not WritePathWorker._EXCISE_MARK_RE.match(mark.strip()):
                    errors.append(f'{name}: invalid excise mark format: "{mark}" (expected XXXX999999)')

        for i, p in enumerate(payments):
            if not isinstance(p, dict):
                continue
            amt = p.get('amount', 0)
            if not isinstance(amt, (int, float)) or amt < 0:
                errors.append(f'payment #{i+1}: negative or invalid amount ({amt})')

        if errors:
            return build_canonical_error(CanonicalErrorCode.INVALID_RECEIPT_DATA, message='; '.join(errors))
        return None

    @staticmethod
    def _guard_cash_transaction_limit(command: CanonicalFiscalCommand) -> CanonicalError | None:
        """Block cash payments exceeding 50 000 UAH per receipt (legal requirement)."""
        receipt = command.payload.get('receipt') if isinstance(command.payload, dict) else None
        if not isinstance(receipt, dict):
            return None
        payments = receipt.get('payments')
        if not isinstance(payments, list):
            return None
        cash_total = sum(
            int(p.get('amount', 0))
            for p in payments
            if isinstance(p, dict) and p.get('payment_type', p.get('type', '')) == 'CASH'
        )
        if cash_total >= WritePathWorker.CASH_TRANSACTION_LIMIT:
            return build_canonical_error(
                CanonicalErrorCode.INVALID_RECEIPT_DATA,
                message=f'Cash payment {cash_total / 100:.2f} UAH exceeds legal limit of 50 000.00 UAH per transaction',
            )
        return None

    @staticmethod
    def _guard_excise_license(conn, *, fiscal_number: str, command: CanonicalFiscalCommand) -> CanonicalError | None:
        """Block excise goods on PRRO without excise license (node_state.excise_allowed=0)."""
        row = conn.execute(
            "SELECT excise_allowed FROM node_state WHERE fiscal_number = ?",
            (fiscal_number,),
        ).fetchone()
        if row is None or row[0]:
            return None  # allowed or no node_state (skip)

        # excise_allowed=0 — check if any goods use excise tax groups
        receipt = command.payload.get('receipt') if isinstance(command.payload, dict) else None
        if not isinstance(receipt, dict):
            return None
        goods = receipt.get('goods')
        if not isinstance(goods, list):
            return None

        tax_groups = TaxGroupRepository.get_for_fiscal_number(conn, fiscal_number)
        for g in goods:
            if not isinstance(g, dict):
                continue
            tax_id = g.get('tax_id')
            if tax_id is None:
                continue
            group = tax_groups.get(str(tax_id))
            if group and (group.requires_excise_mark or group.additional_rate > 0):
                return build_canonical_error(
                    CanonicalErrorCode.INVALID_RECEIPT_DATA,
                    message=f'Excise operations not allowed on this PRRO (fiscal number {fiscal_number}). '
                            f'Item "{g.get("name", "?")}" uses excise tax group {group.name} (TX={tax_id}). '
                            f'Enable excise_allowed in PRRO settings or remove excise goods from receipt.',
                )
        return None

    @staticmethod
    def _guard_payment_types(conn, *, fiscal_number: str, command: CanonicalFiscalCommand) -> CanonicalError | None:
        """Reject receipts with payment types not configured for this PRRO.

        CASH is hardcoded canonical (always allowed). All other types must exist
        in payment_type_definitions for the fiscal number. No silent fallback.
        """
        receipt = command.payload.get('receipt') if isinstance(command.payload, dict) else None
        if not isinstance(receipt, dict):
            return None
        payments = receipt.get('payments')
        if not isinstance(payments, list):
            return None
        for p in payments:
            if not isinstance(p, dict):
                continue
            ptype = p.get('payment_type', p.get('type', ''))
            if ptype == 'CASH':
                continue
            if not ptype:
                continue
            resolved = PaymentTypeRepository.get_type(conn, fiscal_number, ptype)
            if resolved is None:
                return build_canonical_error(
                    CanonicalErrorCode.INVALID_RECEIPT_DATA,
                    message=f"Unknown payment type '{ptype}' for fiscal number '{fiscal_number}'. "
                            f"Configure it in payment_type_definitions or use 'CASH'.",
                )
        return None

    @staticmethod
    def _guard_tax_group_compliance(conn, *, fiscal_number: str, command: CanonicalFiscalCommand) -> CanonicalError | None:
        """Check each good's tax group for UKTZED / excise mark requirements."""
        receipt = command.payload.get('receipt') if isinstance(command.payload, dict) else None
        if not isinstance(receipt, dict):
            return None
        goods = receipt.get('goods')
        if not isinstance(goods, list) or not goods:
            return None

        tax_groups = TaxGroupRepository.get_for_fiscal_number(conn, fiscal_number)
        if not tax_groups:
            return None  # no groups configured — skip validation

        errors = []
        for i, g in enumerate(goods):
            if not isinstance(g, dict):
                continue
            tax_id = g.get('tax_id')
            if tax_id is None:
                continue
            item_name = g.get('name', f'item #{i+1}')
            group = tax_groups.get(str(tax_id))
            if group is None:
                errors.append(f'{item_name}: tax group TX={tax_id} is not configured for this PRRO')
                continue
            if group.tax_algorithm == 3:
                errors.append(
                    f'{item_name}: tax group {group.name} (TX={tax_id}) uses TXAL=3 '
                    f'(absolute excise per quantity) which is not supported yet'
                )
                continue
            if group.requires_uktzed and not g.get('uktzed'):
                errors.append(f'{item_name}: tax group {group.name} (TX={tax_id}) requires UKTZED code')
            excise_barcodes = g.get('excise_barcodes') or []
            if group.requires_excise_mark:
                qty_raw = g.get('quantity', 1000)
                if isinstance(qty_raw, int) and qty_raw % 1000 != 0:
                    errors.append(
                        f'{item_name}: fractional quantity ({qty_raw / 1000:.3f}) not allowed '
                        f'for tax group {group.name} (TX={tax_id}) with mandatory excise marks'
                    )
            if group.requires_excise_mark and not excise_barcodes:
                errors.append(f'{item_name}: tax group {group.name} (TX={tax_id}) requires excise mark')
            elif group.requires_excise_mark and excise_barcodes:
                qty_raw = g.get('quantity', 1000)
                qty_units = qty_raw // 1000 if isinstance(qty_raw, int) else int(qty_raw) // 1000
                if qty_units < 1:
                    qty_units = 1
                mark_count = len(excise_barcodes)
                if mark_count != qty_units:
                    errors.append(
                        f'{item_name}: quantity={qty_units} units but {mark_count} excise marks '
                        f'(must be equal)'
                    )

        if errors:
            return build_canonical_error(
                CanonicalErrorCode.INVALID_RECEIPT_DATA,
                message='; '.join(errors),
            )
        return None

    @staticmethod
    def _extract_receipt_metadata(command: CanonicalFiscalCommand) -> dict[str, object | None]:
        receipt = command.payload.get('receipt') if isinstance(command.payload, dict) else None
        if not isinstance(receipt, dict):
            return {
                'receipt_type': None,
                'previous_hash': None,
                'related_receipt_id': None,
                'previous_receipt_id': None,
                'technical_return': None,
                'delivery_json': None,
                'rounding_enabled': None,
                'total_sum': None,
                'round_sum': None,
                'discounts_sum': None,
                'extra_charge_sum': None,
            }
        totals = receipt.get('totals') if isinstance(receipt.get('totals'), dict) else {}
        delivery = receipt.get('delivery')
        return {
            'receipt_type': receipt.get('type'),
            'previous_hash': receipt.get('previous_hash'),
            'related_receipt_id': receipt.get('related_receipt_id'),
            'previous_receipt_id': receipt.get('previous_receipt_id'),
            'technical_return': receipt.get('technical_return'),
            'delivery_json': dumps_json(delivery) if delivery is not None else None,
            'rounding_enabled': receipt.get('rounding_enabled'),
            'total_sum': totals.get('total_sum'),
            'round_sum': totals.get('round_sum'),
            'discounts_sum': totals.get('discounts_sum'),
            'extra_charge_sum': totals.get('extra_charge_sum'),
        }

    @staticmethod
    def _extract_excise_marks(command: CanonicalFiscalCommand) -> list[dict]:
        if command.operation_type not in {OperationType.SELL, OperationType.RETURN}:
            return []
        receipt = command.payload.get('receipt') if isinstance(command.payload, dict) else None
        if not isinstance(receipt, dict):
            return []
        goods = receipt.get('goods') or []
        marks: list[dict] = []
        for idx, item in enumerate(goods, start=1):
            if not isinstance(item, dict):
                continue
            item_no = item.get('item_no', idx)
            product_code = item.get('code')
            for raw in item.get('excise_barcodes', []) or []:
                normalized = WritePathWorker._normalize_excise_mark(raw)
                if not normalized:
                    continue
                marks.append({
                    'normalized_mark_code': normalized,
                    'raw_mark_code': str(raw),
                    'receipt_item_no': item_no,
                    'product_code': product_code,
                })
        return marks

    @staticmethod
    def _log_protocol_event(
        conn: sqlite3.Connection,
        *,
        inbox: InboxRecord,
        command: CanonicalFiscalCommand | None,
        result_code: str,
        error: CanonicalError | None = None,
    ) -> None:
        trace_context = command.trace_context if command is not None else None
        source_ip = None
        source_port = None
        session_id = None
        if isinstance(trace_context, dict):
            source_ip = trace_context.get('source_ip')
            source_port = trace_context.get('source_port')
            session_id = trace_context.get('session_id')
        elif trace_context is not None:
            source_ip = trace_context.source_ip
            source_port = trace_context.source_port
            session_id = trace_context.session_id
        ProtocolTraceRepository.log_event(
            conn,
            trace_id=f'{inbox.request_id}-protocol-{result_code}-{int(datetime.now(UTC).timestamp() * 1000000)}',
            protocol=inbox.protocol.value if isinstance(inbox.protocol, Protocol) else str(inbox.protocol),
            fiscal_number=inbox.fiscal_number,
            route_key=inbox.route_key,
            connection_id=inbox.protocol_session_id,
            session_id=session_id or inbox.protocol_session_id,
            direction='EVENT',
            trace_level='SAFE',
            mapped_operation=command.operation_type.value if command is not None else None,
            canonical_request_id=inbox.request_id,
            parsed_payload_json=inbox.payload_json,
            result_code=result_code,
            error_class=error.code.value if error is not None else None,
            error_message=error.message if error is not None else None,
            source_ip=source_ip,
            source_port=source_port,
        )


__all__ = ['WritePathWorker', 'WorkerProcessResult', 'WorkerContext']
