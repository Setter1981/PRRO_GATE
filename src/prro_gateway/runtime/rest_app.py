from __future__ import annotations

import hmac
import uuid
from contextlib import asynccontextmanager
from datetime import UTC, datetime
import time

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse

from ..config import AppConfig
from ..logging import get_logger
from ..runtime.container import RuntimeContainer
from ..runtime.alerts import AlertEvent
from ..enums import DocumentState
from ..repositories.fiscal_documents import FiscalDocumentRepository
from ..repositories.audit import AuditRepository
from ..repositories.outbox import OutboxRepository
from ..admin_ui import register_admin_ui
from ..repositories.shifts import ShiftRepository
from ..services.ingress import AdapterMappingError
from ..services.write_path import WritePathWorker
from .maria304_periodic import handle_periodic_report as _handle_periodic_report
from ..utils.json_codec import dumps_json

import json as _json

_DEFAULT_RATE_LIMIT_COOLDOWN = 300


def _read_retry_after(response_json: str | None) -> int:
    """Read retry_after_seconds from persisted response_json, default 300."""
    if response_json:
        try:
            meta = _json.loads(response_json)
            if isinstance(meta, dict) and 'retry_after_seconds' in meta:
                return int(meta['retry_after_seconds'])
        except (ValueError, TypeError):
            pass
    return _DEFAULT_RATE_LIMIT_COOLDOWN


def container_alert(entity_type: str, entity_id: str, event_type: str, payload: dict[str, object]) -> AlertEvent:
    return AlertEvent(entity_type=entity_type, entity_id=entity_id, event_type=event_type, severity="WARNING", payload=payload)


def create_app(container: RuntimeContainer) -> FastAPI:
    logger = get_logger("prro_gateway.runtime.rest", app_name=container.config.app_name)

    @asynccontextmanager
    async def lifespan(_: FastAPI):
        container.initialize()
        try:
            yield
        finally:
            container.shutdown()

    app = FastAPI(title=container.config.app_name, version=container.config.version, lifespan=lifespan)

    # Admin UI (optional, enabled via config.admin_ui.enabled).
    register_admin_ui(app, container)

    @app.get("/health/live")
    def live() -> dict[str, str]:
        return {"status": "ok" if container.health.live else "down", "phase": container.health.phase}

    @app.get("/metrics")
    def metrics() -> dict[str, object]:
        return container.metrics.snapshot()

    @app.get("/health/ready")
    def ready() -> JSONResponse:
        status = 200 if container.health.ready else 503
        return JSONResponse(status_code=status, content={"ready": container.health.ready, "phase": container.health.phase, "last_error": container.health.last_error})

    @app.get("/health/startup")
    def startup() -> JSONResponse:
        status = 200 if container.health.startup_complete else 503
        return JSONResponse(status_code=status, content={"startup_complete": container.health.startup_complete, "phase": container.health.phase, "last_error": container.health.last_error})

    @app.get("/v1/ops/summary")
    def ops_summary() -> dict[str, object]:
        with container.connect() as conn:
            manual_count = FiscalDocumentRepository.count_requires_manual_reconciliation(conn)
            outbox_pending = OutboxRepository.count_pending(conn)
            recon_pending = FiscalDocumentRepository.count_pending_for_reconciliation(conn)
        worker = container.command_processor
        if isinstance(worker, WritePathWorker):
            crypto_breaker_open = worker.crypto_breaker_open
            crypto_consecutive_failures = worker._crypto_consecutive_failures
            crypto_breaker_threshold = worker.crypto_breaker_threshold
            crypto_provider_type = type(worker.crypto_provider).__name__
        else:
            crypto_breaker_open = False
            crypto_consecutive_failures = 0
            crypto_breaker_threshold = 0
            # Same resolution priority as startup gate: worker > injected > config-driven
            try:
                resolved = container.crypto_provider or container._resolve_crypto_provider()
                crypto_provider_type = type(resolved).__name__
            except Exception:
                crypto_provider_type = 'unknown'
        runtime_env = container.config.runtime.environment
        crypto_gate_passed = runtime_env == 'production' and container.health.phase != 'STARTUP_ERROR'
        return {
            "health": {
                "live": container.health.live,
                "ready": container.health.ready,
                "startup_complete": container.health.startup_complete,
                "phase": container.health.phase,
                "last_error": container.health.last_error,
            },
            "runtime_environment": runtime_env,
            "crypto_provider_type": crypto_provider_type,
            "crypto_production_gate_passed": crypto_gate_passed if runtime_env == 'production' else None,
            "metrics": container.metrics.snapshot(),
            "startup_report": container.last_startup_report.model_dump(mode="json") if container.last_startup_report else None,
            "manual_reconciliation_count": manual_count,
            "outbox_pending_count": outbox_pending,
            "reconciliation_pending_count": recon_pending,
            "crypto_breaker_open": crypto_breaker_open,
            "crypto_consecutive_failures": crypto_consecutive_failures,
            "crypto_breaker_threshold": crypto_breaker_threshold,
        }

    @app.post("/v1/ingress/checkbox")
    async def ingress_checkbox(request: Request) -> dict[str, object]:
        raw = await request.json()
        started = time.monotonic()
        try:
            with container.connect() as conn:
                inbox, command, process_result, is_replay = container.ingress_service.accept_checkbox(
                    conn,
                    raw_request=raw,
                    response_timeout_seconds=container.config.ingress.rest.response_timeout_seconds,
                )
        except (AdapterMappingError, KeyError, ValueError) as exc:
            container.metrics.inc("ingress.checkbox.error")
            detail = {"code": getattr(exc, "code", "INVALID_REQUEST"), "message": getattr(exc, "message", str(exc))}
            with container.connect() as conn:
                container.alerts.emit(conn, event=container_alert("ingress", "checkbox", "ADAPTER_MAPPING_ERROR", {"code": detail["code"]}))
                conn.commit()
            logger.warning("checkbox_accept_error", extra={"extra_fields": {"code": detail["code"]}})
            raise HTTPException(status_code=400, detail=detail) from exc
        duration_ms = round((time.monotonic() - started) * 1000, 3)
        logger.info(
            "checkbox_accept_ok",
            extra={
                "extra_fields": {
                    "request_id": inbox.request_id,
                    "fiscal_number": inbox.fiscal_number,
                    "operation_type": command.operation_type.value,
                    "duration_ms": duration_ms,
                }
            },
        )
        container.metrics.inc("ingress.checkbox.accepted")
        container.metrics.set_gauge("ingress.last_accept_epoch", time.time())
        response: dict[str, object] = {
            "request_id": inbox.request_id,
            "idempotency_key": inbox.idempotency_key,
            "status": inbox.status.value,
            "existing": is_replay,
            "fiscal_number": inbox.fiscal_number,
        }
        doc_id = getattr(process_result, "document_id", None)
        # F4: on replay the live doc_id is absent — use inbox.result_document_id
        replay_fields_needed = False
        if not doc_id and is_replay:
            doc_id = getattr(inbox, "result_document_id", None)
            replay_fields_needed = bool(doc_id)
        if doc_id:
            with container.connect() as conn:
                doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
                if doc is not None:
                    response["document_id"] = doc.document_id
                    response["document_state"] = doc.state.value
                    response["server_fiscal_no"] = doc.server_fiscal_no
                    response["canonical_error_code"] = doc.canonical_error_code
                    response["error_message"] = doc.error_message
                    if doc.submission_status == 'DPS_RATE_LIMITED':
                        response["retry_after_seconds"] = _read_retry_after(doc.response_json)
                    # F4: replay path — recompute canonical fields from stored document
                    if replay_fields_needed:
                        for field, value in WritePathWorker.compute_canonical_fields_from_document(conn, doc).items():
                            if value is not None:
                                response[field] = value
        elif process_result is not None:
            canonical_error = getattr(process_result, "canonical_error", None)
            if canonical_error is not None:
                response["error_code"] = canonical_error.code.value
                response["error_message"] = canonical_error.message
                if canonical_error.retry_after_seconds is not None:
                    response["retry_after_seconds"] = canonical_error.retry_after_seconds
        # Sprint 10 Step 10: canonical layer fields from live process_result (non-None only)
        if not replay_fields_needed and process_result is not None:
            for field in ('cash_balance', 'change', 'rounded_sum', 'rounding', 'offline_backlog_warning'):
                value = getattr(process_result, field, None)
                if value is not None:
                    response[field] = value
        return response

    # Review finding M3: narrow scope to the two operations that
    # map directly to "sale receipt" semantics.  Real Maria hardware
    # opens shift on first PREP (SELL/RETURN); SERVICE_*/CASH_WITHDRAWAL
    # are cash-management ops that should come AFTER an open shift —
    # auto-opening for them can mask operator error (e.g. accidental
    # service-in before shift start).
    _MARIA304_AUTO_OPEN_OPS: frozenset[str] = frozenset({"SELL", "RETURN"})

    def _maybe_auto_open_shift(container: RuntimeContainer, raw: dict[str, object]) -> None:
        """If SELL/RETURN arrives without an active shift and the
        config allows, synthesise a SHIFT_OPEN canonical request and
        push it through `accept_maria304` BEFORE the original.  The
        single-writer-per-FN lease + UNIQUE idempotency_key together
        serialise concurrent synths into a single row.

        Best-effort: any failure logs and falls through to the original
        request (which will then hit SHIFT_NOT_OPEN naturally and
        surface a clear error to the caller).
        """
        if not container.config.ingress.maria304.auto_open_shift:
            return
        op = raw.get("command_type")
        if op not in _MARIA304_AUTO_OPEN_OPS:
            return
        fiscal_number = raw.get("fiscal_number")
        if not isinstance(fiscal_number, str) or not fiscal_number:
            return
        with container.connect() as conn:
            active = ShiftRepository.get_active_shift(conn, fiscal_number)
        if active is not None:
            return
        # Review finding B1/B2: deterministic idempotency_key keyed on
        # (fiscal_number, UTC date).  Two concurrent synths for the
        # same FN on the same day collapse to a replay on the UNIQUE
        # idempotency_key index — exactly what we want.  The full
        # external_request_id prefix is embedded up-front so the
        # audit row is correct on first commit (no post-hoc UPDATE).
        # UTC date as the partitioning key — one synth row per FN per
        # UTC day.  Local-time operators working across midnight are
        # unaffected: if the previous shift is still OPENED at the
        # boundary, `get_active_shift` returns non-None and no synth
        # happens; if it is CLOSED, day N+1 gets its own key and row.
        today_utc = datetime.now(UTC).strftime("%Y%m%d")
        # Embed the `auto-shift-open` marker directly into the
        # idempotency_key so the audit row carries it on first commit
        # (review finding B2).  Shape still satisfies the adapter-layer
        # regex `^maria304:[A-Za-z0-9_\-:]{3,256}$`.
        synth_key = f"maria304:auto-shift-open:{fiscal_number}:{today_utc}"
        synth = {
            "schema_version": "1.0",
            "fiscal_number": fiscal_number,
            "command_type": "SHIFT_OPEN",
            "idempotency_key": synth_key,
            "cashier_id": raw.get("cashier_id"),
            "department": raw.get("department"),
            "return_check_number": None,
            "payload": {
                "direction": "SALE",
                "goods": [],
                "payments": [],
                "dual_tax_mode": None,
                "totals": {"sale_kopecks": 0, "return_kopecks": 0},
                "raw_frames": [],
            },
        }
        try:
            with container.connect() as conn:
                inbox, _cmd, _pr, is_replay = container.ingress_service.accept_maria304(
                    conn,
                    raw_request=synth,
                    response_timeout_seconds=container.config.ingress.maria304.response_timeout_seconds,
                )
            if is_replay:
                container.metrics.inc("ingress.maria304.auto_shift_open_replay")
            else:
                container.metrics.inc("ingress.maria304.auto_shift_opened")
            logger.info(
                "maria304_auto_shift_opened",
                extra={"extra_fields": {
                    "fiscal_number": fiscal_number,
                    "triggered_by": op,
                    "is_replay": is_replay,
                    "inbox_request_id": inbox.request_id,
                }},
            )
        except Exception as exc:  # noqa: BLE001
            container.metrics.inc("ingress.maria304.auto_shift_open_error")
            logger.warning(
                "maria304_auto_shift_open_failed",
                extra={"extra_fields": {
                    "fiscal_number": fiscal_number,
                    "triggered_by": op,
                    "error": str(exc),
                }},
            )

    def _require_maria304_token(request: Request) -> None:
        """Bearer-token check for the Maria 304 Rust ingress endpoint.

        Raises:
            HTTPException(503) — if shared_token is empty (misconfigured
                gateway); refuse rather than accepting a blank token as
                valid (which would let any client in).
            HTTPException(401) — if Authorization header is missing or
                not `Bearer <token>`.
            HTTPException(403) — if the presented token does not match
                the configured shared_token.  Comparison is
                constant-time to prevent timing oracles.
        """
        configured = container.config.ingress.maria304.shared_token
        if not configured:
            raise HTTPException(
                status_code=503,
                detail={"ok": False, "error_code": "SOFT_MISCONFIGURED",
                        "error_message": "maria304 shared_token is not configured"},
            )
        header = request.headers.get("Authorization", "")
        scheme, _, presented = header.partition(" ")
        if scheme.lower() != "bearer" or not presented:
            raise HTTPException(
                status_code=401,
                detail={"ok": False, "error_code": "SOFT_UNAUTHORIZED",
                        "error_message": "Bearer token required"},
            )
        if not hmac.compare_digest(presented, configured):
            raise HTTPException(
                status_code=403,
                detail={"ok": False, "error_code": "SOFT_FORBIDDEN",
                        "error_message": "invalid bearer token"},
            )

    def _maria304_fiscal_ts(doc: object) -> str:
        """Resolve an ISO8601 timestamp for the Rust CanonicalResponse."""
        for attr in ("server_fiscal_date", "business_ts", "created_at"):
            value = getattr(doc, attr, None)
            if value is not None:
                return value.isoformat() if hasattr(value, "isoformat") else str(value)
        return ""

    def _maria304_totals(doc: object) -> tuple[int, int]:
        total = int(getattr(doc, "total_sum", 0) or 0)
        # RETURN documents carry totals in the return bucket from the
        # Rust driver's perspective; SELL/everything-else in the sale
        # bucket.  Rust's COMP builder reads these two scalars.
        receipt_type = getattr(doc, "receipt_type", None) or getattr(doc, "doc_type", None)
        if receipt_type == "RETURN":
            return 0, total
        return total, 0

    def _maria304_error_code(canonical_code: str) -> str:
        # Rust maps any non-SOFT* prefix to SoftBlock, so we always
        # prefix.  The DPS-side canonical codes already carry enough
        # information; we just shape the wire-compatibility prefix.
        if canonical_code.startswith("SOFT"):
            return canonical_code
        return f"SOFT_{canonical_code}"

    @app.post("/v1/ingress/maria304")
    async def ingress_maria304(request: Request) -> JSONResponse:
        _require_maria304_token(request)
        raw = await request.json()
        # PERIODIC_REPORT is a read-only aggregation over fiscal_documents
        # — no inbox insert, no FiscalDocument, no DPS round-trip.  We
        # intercept here BEFORE accept_maria304 so the canonical write
        # path is not touched.  See plan 2026-04-21-maria304-python-
        # adapter.md §5a (R4-deferred) and §M7-Py-3c.
        if isinstance(raw, dict) and raw.get("command_type") == "PERIODIC_REPORT":
            return _handle_periodic_report(container, raw)
        # Implicit shift-open (M7-Py-3d): 1C/OLE Manager's OpenCheck()
        # does NOT emit `nrep` on wire when shift is closed — real
        # hardware opens shift internally on first PREP.  Mirror this
        # by synthesising SHIFT_OPEN before the incoming SELL/etc.
        # See plan 2026-04-21-maria304-python-adapter.md §5b.
        _maybe_auto_open_shift(container, raw)
        started = time.monotonic()
        try:
            with container.connect() as conn:
                inbox, command, process_result, is_replay = container.ingress_service.accept_maria304(
                    conn,
                    raw_request=raw,
                    response_timeout_seconds=container.config.ingress.maria304.response_timeout_seconds,
                )
        except AdapterMappingError as exc:
            container.metrics.inc("ingress.maria304.error")
            logger.warning(
                "maria304_adapter_error",
                extra={"extra_fields": {"code": getattr(exc, "code", "INVALID_REQUEST")}},
            )
            return JSONResponse(
                status_code=400,
                content={
                    "ok": False,
                    "error_code": "SOFT_UNSUPPORTED",
                    "error_message": getattr(exc, "message", str(exc)),
                },
            )
        except (KeyError, ValueError) as exc:
            container.metrics.inc("ingress.maria304.error")
            return JSONResponse(
                status_code=400,
                content={
                    "ok": False,
                    "error_code": "SOFT_BAD_REQUEST",
                    "error_message": str(exc),
                },
            )

        duration_ms = round((time.monotonic() - started) * 1000, 3)
        logger.info(
            "maria304_accept_ok",
            extra={"extra_fields": {
                "request_id": inbox.request_id,
                "fiscal_number": inbox.fiscal_number,
                "operation_type": command.operation_type.value,
                "is_replay": is_replay,
                "duration_ms": duration_ms,
            }},
        )
        container.metrics.inc("ingress.maria304.accepted")

        doc_id = getattr(process_result, "document_id", None)
        if not doc_id and is_replay:
            doc_id = getattr(inbox, "result_document_id", None)
        if doc_id:
            with container.connect() as conn:
                doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
            if doc is not None:
                sale_k, ret_k = _maria304_totals(doc)
                return JSONResponse(status_code=200, content={
                    "ok": True,
                    "document_id": doc.document_id,
                    "fiscal_id": doc.server_fiscal_no or "",
                    "fiscal_ts": _maria304_fiscal_ts(doc),
                    "document_state": doc.state.value,
                    "sale_total_kopecks": sale_k,
                    "return_total_kopecks": ret_k,
                })

        if process_result is not None:
            canonical_error = getattr(process_result, "canonical_error", None)
            if canonical_error is not None:
                return JSONResponse(status_code=400, content={
                    "ok": False,
                    "error_code": _maria304_error_code(canonical_error.code.value),
                    "error_message": canonical_error.message,
                })

        # Worker has not yet produced a verdict — Rust side retries.
        return JSONResponse(status_code=503, content={
            "ok": False,
            "error_code": "SOFT_PROCESSING",
            "error_message": "worker pending",
        })

    @app.get("/v1/admin/documents/manual")
    def list_manual_documents() -> JSONResponse:
        """List all documents in REQUIRES_MANUAL_RECONCILIATION state."""
        with container.connect() as conn:
            docs = FiscalDocumentRepository.get_requires_manual_reconciliation(conn)
        return JSONResponse(status_code=200, content={
            "count": len(docs),
            "documents": [
                {
                    "document_id": d.document_id,
                    "fiscal_number": d.fiscal_number,
                    "doc_type": d.doc_type,
                    "state": d.state.value,
                    "recovery_attempts": d.recovery_attempts,
                    "submission_status": d.submission_status,
                    "canonical_error_code": d.canonical_error_code,
                    "error_message": d.error_message,
                    "sent_at": d.sent_at,
                    "updated_at": d.updated_at.isoformat() if d.updated_at else None,
                    "created_at": d.created_at.isoformat() if d.created_at else None,
                }
                for d in docs
            ],
        })

    @app.post("/v1/admin/documents/{document_id}/retry")
    def admin_retry_document(document_id: str) -> JSONResponse:
        """Reset a REQUIRES_MANUAL_RECONCILIATION document to ERROR_RETRYABLE for re-processing.

        Resets recovery_attempts to 0 so the document gets a fresh ceiling worth of retries.
        Only valid for documents currently in REQUIRES_MANUAL_RECONCILIATION state.
        Returns 404 if not found, 409 if not in REQUIRES_MANUAL_RECONCILIATION.
        """
        with container.connect() as conn:
            doc = FiscalDocumentRepository.get_by_id(conn, document_id)
            if doc is None:
                return JSONResponse(status_code=404, content={"detail": "document not found"})
            if doc.state != DocumentState.REQUIRES_MANUAL_RECONCILIATION:
                return JSONResponse(status_code=409, content={
                    "detail": "document is not in REQUIRES_MANUAL_RECONCILIATION state",
                    "current_state": doc.state.value,
                })
            conn.execute('BEGIN IMMEDIATE')
            updated = FiscalDocumentRepository.update_state(
                conn,
                document_id=document_id,
                state=DocumentState.ERROR_RETRYABLE,
                expected_states=(DocumentState.REQUIRES_MANUAL_RECONCILIATION,),
            )
            if updated is None:
                conn.rollback()
                return JSONResponse(status_code=409, content={
                    "detail": "state changed concurrently, retry the request",
                })
            conn.execute(
                "UPDATE fiscal_documents SET recovery_attempts = 0, updated_at = CURRENT_TIMESTAMP WHERE document_id = ?",
                (document_id,),
            )
            # Clear old reconcile-poll traces so the post-reset reconciliation cycle
            # can write fresh trace records starting from N=1 without PRIMARY KEY collision.
            # The audit record (DOCUMENT_ADMIN_RETRY_REQUESTED) documents that a reset occurred.
            conn.execute(
                "DELETE FROM transport_trace_log WHERE trace_id LIKE ?",
                (f'{document_id}-reconcile-poll-%',),
            )
            AuditRepository.log_event(
                conn,
                entity_type='DOCUMENT',
                entity_id=document_id,
                event_type='DOCUMENT_ADMIN_RETRY_REQUESTED',
                severity='INFO',
                event_payload_json=dumps_json({
                    'previous_state': 'REQUIRES_MANUAL_RECONCILIATION',
                    'new_state': 'ERROR_RETRYABLE',
                    'recovery_attempts_reset': True,
                }),
            )
            conn.commit()
        logger.info("admin_retry_requested", extra={"extra_fields": {"document_id": document_id}})
        return JSONResponse(status_code=200, content={
            "document_id": document_id,
            "previous_state": "REQUIRES_MANUAL_RECONCILIATION",
            "new_state": "ERROR_RETRYABLE",
            "recovery_attempts": 0,
        })

    @app.post("/v1/admin/crypto/reset-breaker")
    def admin_reset_crypto_breaker() -> JSONResponse:
        """Reset the crypto circuit breaker consecutive-failure counter to 0.

        Use after the crypto sidecar has recovered to allow sign() calls to resume
        without restarting the container. No-op (returns 0→0) if breaker was not open.
        Returns 409 if the command_processor is not a WritePathWorker (e.g., custom injection).
        """
        worker = container.command_processor
        if not isinstance(worker, WritePathWorker):
            return JSONResponse(status_code=409, content={
                "detail": "crypto breaker reset not available: command_processor is not a WritePathWorker",
            })
        previous = worker.reset_crypto_breaker()
        was_open = worker.crypto_breaker_threshold > 0 and previous >= worker.crypto_breaker_threshold
        if was_open:
            from ..enums import NodeMode
            from ..repositories.node_state import NodeStateRepository
            with container.connect() as audit_conn:
                # Reset CRYPTO_DEGRADED → ONLINE for all fiscal_numbers so that
                # the C1 fast-path guard allows sign attempts again.
                rows = audit_conn.execute(
                    "SELECT fiscal_number FROM node_state WHERE mode = 'CRYPTO_DEGRADED'"
                ).fetchall()
                if rows:
                    audit_conn.execute('BEGIN IMMEDIATE')
                    for (fn,) in rows:
                        NodeStateRepository.update_mode(audit_conn, fiscal_number=fn, mode=NodeMode.ONLINE)
                    audit_conn.commit()
                AuditRepository.log_event(
                    audit_conn,
                    entity_type='CRYPTO',
                    entity_id='breaker',
                    event_type='CRYPTO_BREAKER_RESET',
                    severity='INFO',
                    event_payload_json=dumps_json({
                        'previous_consecutive_failures': previous,
                        'threshold': worker.crypto_breaker_threshold,
                    }),
                )
                audit_conn.commit()
        logger.info(
            "crypto_breaker_reset",
            extra={"extra_fields": {
                "previous_consecutive_failures": previous,
                "was_open": was_open,
                "threshold": worker.crypto_breaker_threshold,
            }},
        )
        return JSONResponse(status_code=200, content={
            "previous_consecutive_failures": previous,
            "consecutive_failures": 0,
            "breaker_was_open": was_open,
            "threshold": worker.crypto_breaker_threshold,
        })

    @app.post("/v1/admin/reconciliation/trigger")
    async def admin_reconciliation_trigger(request: Request) -> JSONResponse:
        """Manually trigger reconciliation for one or all fiscal numbers.

        Polls DPS status for PENDING documents and updates their state.
        Body: {"fiscal_number": "FN-..."} for a specific FN, or {} for all.
        """
        if container.reconciliation_service is None:
            return JSONResponse(status_code=503,
                content={"detail": "reconciliation_service not available"})
        try:
            body = await request.json()
        except Exception:
            body = {}
        if not isinstance(body, dict):
            body = {}
        fiscal_number = body.get("fiscal_number") or None  # "" → None
        try:
            with container.connect() as conn:
                result = container.reconciliation_service.reconcile_pending(
                    conn, fiscal_number=fiscal_number
                )
        except Exception as exc:
            logger.error("reconciliation_trigger_error", exc_info=True, extra={"extra_fields": {
                "fiscal_number": fiscal_number, "error": str(exc),
            }})
            return JSONResponse(status_code=500, content={"detail": "reconciliation failed"})
        logger.info("admin_reconciliation_triggered", extra={"extra_fields": {
            "fiscal_number": fiscal_number,
            "checked": result.checked,
            "acked": result.acked,
            "manual": result.manual,
        }})
        return JSONResponse(status_code=200, content={
            "fiscal_number": fiscal_number,
            "checked": result.checked,
            "acked": result.acked,
            "rejected": result.rejected,
            "retryable": result.retryable,
            "still_pending": result.still_pending,
            "manual": result.manual,
        })

    @app.post("/v1/admin/offline-sync")
    async def admin_offline_sync(request: Request) -> JSONResponse:
        """Trigger offline sync for a specific fiscal_number.

        Submits OFFLINE_LOCAL_ACK documents to DPS in fiscal-sequence order.
        Requires fiscal_number in the JSON body — broad batch sync is not
        exposed to prevent accidental large-scale DPS submissions.
        """
        if container.offline_sync_service is None:
            return JSONResponse(status_code=503, content={"detail": "offline sync service not initialized"})
        try:
            body = await request.json()
        except Exception:
            return JSONResponse(status_code=400, content={"detail": "invalid JSON body"})
        if not isinstance(body, dict):
            return JSONResponse(status_code=400, content={"detail": "JSON body must be an object"})
        fiscal_number = body.get("fiscal_number")
        if not fiscal_number or not isinstance(fiscal_number, str):
            return JSONResponse(status_code=400, content={
                "detail": "fiscal_number is required (string)",
            })
        try:
            with container.connect() as conn:
                result = container.offline_sync_service.sync_pending(conn, fiscal_number=fiscal_number)
        except Exception as exc:
            logger.error("offline_sync_error", exc_info=True, extra={"extra_fields": {
                "fiscal_number": fiscal_number, "error": str(exc),
            }})
            return JSONResponse(status_code=500, content={
                "detail": "offline sync failed",
            })
        logger.info("offline_sync_triggered", extra={"extra_fields": {
            "fiscal_number": fiscal_number,
            "checked": result.checked,
            "synced": result.synced,
        }})
        return JSONResponse(status_code=200, content={
            "fiscal_number": fiscal_number,
            "checked": result.checked,
            "synced": result.synced,
            "pending_async": result.pending_async,
            "rejected": result.rejected,
            "retryable": result.retryable,
            "manual": result.manual,
        })

    @app.post("/v1/admin/dps-probe")
    async def admin_dps_probe(request: Request) -> JSONResponse:
        """Explicit fiscal-server probe: statusRro + infoRro for a fiscal_number.

        Calls DPS fiscal server directly via gRPC. NOT automatic — operator-triggered only.
        Requires fiscal_number in JSON body.
        """
        from ..transports.dps_fiscal_server import DpsFiscalServerTransport
        try:
            body = await request.json()
        except Exception:
            return JSONResponse(status_code=400, content={"detail": "invalid JSON body"})
        if not isinstance(body, dict):
            return JSONResponse(status_code=400, content={"detail": "JSON body must be an object"})
        fiscal_number = body.get("fiscal_number")
        if not fiscal_number or not isinstance(fiscal_number, str):
            return JSONResponse(status_code=400, content={"detail": "fiscal_number is required (string)"})

        # Find DPS transport handler + first active profile from router.
        # NOTE: picks first matching profile by kind. If multiple active DPS_PRRO_GRPC_ECABINET
        # profiles exist in the future, add explicit profile_id selection in request body.
        handler = None
        dps_profile = None
        if container.transport_router is not None:
            from ..enums import TransportKind
            handler = container.transport_router.handlers.get(TransportKind.DPS_PRRO_GRPC_ECABINET)
            # Find the active profile for this kind
            for _pid, _prof in container.transport_router.profiles.items():
                if _prof.kind == TransportKind.DPS_PRRO_GRPC_ECABINET:
                    dps_profile = _prof
                    break
        if not isinstance(handler, DpsFiscalServerTransport):
            return JSONResponse(status_code=503, content={"detail": "DPS fiscal-server transport not available"})

        # Resolve crypto for sign_raw
        crypto = None
        if container.command_processor is not None and hasattr(container.command_processor, 'crypto_provider'):
            crypto = container.command_processor.crypto_provider
        elif container.crypto_provider is not None:
            crypto = container.crypto_provider
        else:
            try:
                crypto = container._resolve_crypto_provider()
            except Exception:
                pass
        if crypto is None or not hasattr(crypto, 'sign_raw'):
            return JSONResponse(status_code=503, content={"detail": "crypto provider with sign_raw not available"})

        # Resolve endpoint + TLS from active profile
        probe_endpoint = None
        probe_tls_certs = None
        if dps_profile is not None:
            probe_endpoint = getattr(dps_profile, 'endpoint', None)
            import json as _pjson
            try:
                _pcfg = _pjson.loads(getattr(dps_profile, 'config_json', '{}') or '{}')
            except (ValueError, TypeError):
                _pcfg = {}
            pem_path = _pcfg.get('tls_root_certs_path')
            if pem_path:
                try:
                    with open(pem_path, 'rb') as f:
                        probe_tls_certs = f.read()
                except OSError:
                    pass

        result: dict = {"fiscal_number": fiscal_number}
        try:
            result["status_rro"] = handler.probe_status(
                fiscal_number=fiscal_number, crypto_provider=crypto,
                endpoint=probe_endpoint, tls_root_certs=probe_tls_certs,
            )
        except Exception as exc:
            result["status_rro"] = {"error": str(exc)}

        try:
            result["info_rro"] = handler.probe_info(
                fiscal_number=fiscal_number, crypto_provider=crypto,
                endpoint=probe_endpoint, tls_root_certs=probe_tls_certs,
            )
        except Exception as exc:
            result["info_rro"] = {"error": str(exc)}

        logger.info("dps_probe", extra={"extra_fields": {"fiscal_number": fiscal_number}})
        return JSONResponse(status_code=200, content=result)

    @app.get("/v1/documents/{request_id}")
    def get_document(request_id: str) -> JSONResponse:
        with container.connect() as conn:
            doc = FiscalDocumentRepository.get_by_request_id(conn, request_id)
        if doc is None:
            return JSONResponse(status_code=404, content={"detail": "document not found"})
        content: dict[str, object] = {
            "document_id": doc.document_id,
            "request_id": doc.request_id,
            "fiscal_number": doc.fiscal_number,
            "doc_type": doc.doc_type,
            "state": doc.state.value,
            "fs_mode": doc.fs_mode,
            "lnd": doc.lnd,
            "server_fiscal_no": doc.server_fiscal_no,
            "server_fiscal_date": doc.server_fiscal_date,
            "submission_status": doc.submission_status,
            "canonical_error_code": doc.canonical_error_code,
            "error_message": doc.error_message,
            "sent_at": doc.sent_at,
            "ack_at": doc.ack_at,
        }
        if doc.submission_status == 'DPS_RATE_LIMITED':
            content["retry_after_seconds"] = _read_retry_after(doc.response_json)
        return JSONResponse(status_code=200, content=content)

    @app.get("/v1/shifts/current/x-report")
    def get_x_report(fiscal_number: str) -> JSONResponse:
        from ..enums import ShiftState
        from ..services.shift_aggregation import aggregate_shift_data, aggregate_cash_withdrawals
        from ..services.write_path import WritePathWorker
        from ..repositories.shifts import ShiftRepository
        from datetime import UTC, datetime as dt

        with container.connect() as conn:
            shift = ShiftRepository.get_active_shift(conn, fiscal_number)
            if shift is None or shift.state != ShiftState.OPENED:
                return JSONResponse(status_code=404, content={
                    "error": "NO_ACTIVE_SHIFT",
                    "message": f"No opened shift for fiscal_number '{fiscal_number}'",
                })

            agg = aggregate_shift_data(conn, shift.shift_id)
            balance = WritePathWorker._get_shift_cash_balance(conn, shift.shift_id)
            cw = aggregate_cash_withdrawals(conn, shift.shift_id)

        return JSONResponse(status_code=200, content={
            "fiscal_number": fiscal_number,
            "shift_id": shift.shift_id,
            "shift_opened_at": shift.opened_at,
            "report_ts": dt.now(UTC).isoformat(),
            "tax_groups": agg['tax_sums'],
            "payments": agg['payment_sums'],
            "service": agg['service_sums'],
            "check_count": {
                "sell": agg['check_count']['ni'],
                "return": agg['check_count']['no'],
            },
            "cash_balance": balance,
            "cash_withdrawal": cw,
        })

    @app.get("/v1/admin/node-state")
    def admin_node_state(fiscal_number: str) -> JSONResponse:
        from ..repositories.node_state import NodeStateRepository
        from ..repositories.offline import OfflineRepository
        from ..repositories.fn_config import FiscalNumberConfigRepository
        with container.connect() as conn:
            ns = NodeStateRepository.get_state(conn, fiscal_number)
            if ns is None:
                return JSONResponse(status_code=404, content={"detail": "fiscal_number not found"})
            active_range = OfflineRepository.get_active_range(conn, fiscal_number)
            fn_cfg = FiscalNumberConfigRepository.get_or_default(conn, fiscal_number)
        codes_remaining = None
        if active_range is not None:
            codes_remaining = active_range.last_fiscal_no - active_range.next_fiscal_no + 1
        worker = container.command_processor
        crypto_breaker_open = bool(
            isinstance(worker, WritePathWorker) and
            worker.crypto_breaker_threshold > 0 and
            worker._crypto_consecutive_failures >= worker.crypto_breaker_threshold
        )
        return JSONResponse(status_code=200, content={
            "fiscal_number": fiscal_number,
            "mode": ns.mode.value,
            "shift_state": ns.shift_state.value,
            "current_month_offline_seconds": ns.current_month_offline_seconds,
            "codes_remaining": codes_remaining,
            "active_range_id": active_range.range_id if active_range else None,
            "active_offline_session_id": ns.current_offline_session_id,
            "crypto_breaker_open": crypto_breaker_open,
            "enforce_blocked_mode": bool(fn_cfg.enforce_blocked_mode),
            "min_offline_codes": fn_cfg.min_offline_codes,
            "max_offline_codes": fn_cfg.max_offline_codes,
        })

    @app.get("/v1/admin/offline-ranges")
    def admin_offline_ranges(fiscal_number: str) -> JSONResponse:
        with container.connect() as conn:
            rows = conn.execute(
                "SELECT range_id, first_fiscal_no, last_fiscal_no, next_fiscal_no, status, issued_at "
                "FROM offline_ranges WHERE fiscal_number = ? ORDER BY created_at ASC",
                (fiscal_number,),
            ).fetchall()
        ranges = []
        total_remaining = 0
        for row in rows:
            remaining = max(0, row[2] - row[3] + 1) if row[4] != 'EXHAUSTED' else 0
            total_remaining += remaining
            ranges.append({
                "range_id": row[0],
                "first_fiscal_no": row[1],
                "last_fiscal_no": row[2],
                "next_fiscal_no": row[3],
                "codes_remaining": remaining,
                "status": row[4],
                "issued_at": row[5],
            })
        return JSONResponse(status_code=200, content={
            "fiscal_number": fiscal_number,
            "ranges": ranges,
            "total_remaining": total_remaining,
        })

    @app.get("/v1/admin/offline-sessions")
    def admin_offline_sessions(fiscal_number: str) -> JSONResponse:
        from ..repositories.node_state import NodeStateRepository
        from ..repositories.offline import OfflineRepository
        from datetime import UTC, datetime as dt
        with container.connect() as conn:
            ns = NodeStateRepository.get_state(conn, fiscal_number)
            if ns is None:
                return JSONResponse(status_code=404, content={"detail": "fiscal_number not found"})
            session = OfflineRepository.get_open_session(conn, fiscal_number)
        active_session = None
        if session is not None:
            elapsed = 0
            try:
                started = dt.fromisoformat(session.started_at)
                if started.tzinfo is None:
                    started = started.replace(tzinfo=UTC)
                elapsed = max(0, int((dt.now(UTC) - started).total_seconds()))
            except ValueError:
                pass
            active_session = {
                "offline_session_id": session.offline_session_id,
                "started_at": session.started_at,
                "elapsed_seconds": elapsed,
                "accumulated_month_seconds": session.accumulated_month_seconds,
                "status": session.status.value,
                "reason": session.reason,
            }
        return JSONResponse(status_code=200, content={
            "fiscal_number": fiscal_number,
            "active_session": active_session,
            "current_month_offline_seconds": ns.current_month_offline_seconds,
            "month_limit_seconds": WritePathWorker.MAX_OFFLINE_MONTH_SECONDS,
            "continuous_limit_seconds": WritePathWorker.MAX_OFFLINE_CONTINUOUS_SECONDS,
        })

    return app


__all__ = ["create_app", "RuntimeContainer", "AppConfig"]
