from __future__ import annotations

from contextlib import asynccontextmanager
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
from ..services.ingress import AdapterMappingError
from ..services.write_path import WritePathWorker
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
            for field in ('cash_balance', 'change', 'rounded_sum', 'rounding'):
                value = getattr(process_result, field, None)
                if value is not None:
                    response[field] = value
        return response

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
            with container.connect() as audit_conn:
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

    return app


__all__ = ["create_app", "RuntimeContainer", "AppConfig"]
