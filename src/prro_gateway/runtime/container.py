from __future__ import annotations

import json
import sqlite3
import threading
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

import httpx

from ..enums import TransportKind

from ..config import AppConfig
from ..logging import configure_logging, get_logger
from ..migrations.runner import apply_migrations_to_connection
from ..runtime.alerts import AlertSink
from ..runtime.health import RuntimeHealthState
from ..runtime.metrics import MetricsCollector
from ..runtime.providers import PassthroughCryptoProvider, SidecarCryptoClient, SidecarCryptoProvider
from ..runtime.supervisor import StartupSupervisor, StartupRunReport
from ..services.cert_provisioning import CertProvisioningService
from ..services.cert_watch import CertWatchService
from ..services.ingress import IngressAcceptService
from ..services.offline_sync import OfflineSyncService
from ..services.backup import BackupService
from ..services.reconciliation import ReconciliationService
from ..services.retention import RetentionService
from ..services.write_path import WritePathWorker
from ..transports import CheckboxRestTransport, DpsFiscalServerTransport, FiscalSidecarTransport, ProfileAwareTransportRouter
from ..transports.stubs import CheckboxRestTransportStub, DpsGrpcEcabinetTransportStub, DpsXmlUnifiedWindowTransportStub


class RuntimeContainer:
    def __init__(self, config: AppConfig, *, command_processor=None, reconciliation_service=None, crypto_provider=None, transport_handlers=None, transport_http_client: httpx.Client | None = None, crypto_http_client: httpx.Client | None = None, cert_watch_service: CertWatchService | None = None, cert_der_provider=None, cert_provisioning_service: CertProvisioningService | None = None) -> None:
        self.config = config
        self.health = RuntimeHealthState(live=True, ready=False, startup_complete=False)
        self.metrics = MetricsCollector()
        self.alerts = AlertSink(enabled=config.alerts.enabled, persist_to_audit=config.alerts.persist_to_audit)
        self.command_processor = command_processor
        self.crypto_provider = crypto_provider
        self.transport_handlers = transport_handlers or {}
        self.transport_http_client = transport_http_client
        self.crypto_http_client = crypto_http_client
        self.transport_router: ProfileAwareTransportRouter | None = None
        self.ingress_service = IngressAcceptService(command_processor=command_processor, worker_lease_owner=config.runtime.worker_lease_owner)
        self.reconciliation_service = reconciliation_service
        self.offline_sync_service: OfflineSyncService | None = None
        # cert_watch: injected or lazy-built in _wire_runtime_services.
        self.cert_watch_service: CertWatchService | None = cert_watch_service
        self._cert_der_provider = cert_der_provider
        # cert_provisioning: injected or lazy-built in _wire_runtime_services.
        self.cert_provisioning_service: CertProvisioningService | None = cert_provisioning_service
        self.last_startup_report: StartupRunReport | None = None
        self.logger = get_logger("prro_gateway.runtime.container", app_name=config.app_name)
        self._ops_loop_stop: threading.Event = threading.Event()
        self._ops_loop_thread: threading.Thread | None = None
        self._cert_watch_stop: threading.Event = threading.Event()
        self._cert_watch_thread: threading.Thread | None = None
        # Sprint 13: dedup sets — reset on container restart, which is correct
        # (shift warning fires once per warning period; auto-GO_ONLINE fires once per offline session)
        self._shift_warned_ids: set[str] = set()
        self._auto_go_online_keys: set[str] = set()
        self._cert_provisioning_stop: threading.Event = threading.Event()
        self._cert_provisioning_thread: threading.Thread | None = None
        # F1/F2: backup + retention services, wired in _wire_runtime_services.
        # _last_*_ts starts at 0.0 intentionally: first ops_tick always triggers a
        # backup and retention pass immediately on startup (fail-fast integrity check).
        self._backup_service: BackupService | None = None
        self._retention_service: RetentionService | None = None
        self._last_backup_ts: float = 0.0
        self._last_retention_ts: float = 0.0
        # H1: half-open probe — tracks last reset per fiscal_number for CRYPTO_DEGRADED recovery.
        self._last_crypto_probe_ts: dict[str, float] = {}

    @property
    def db_path(self) -> Path:
        return Path(self.config.database.db_path)

    @property
    def sql_dir(self) -> Path:
        return Path(self.config.database.sql_dir)

    def initialize(self) -> None:
        configure_logging(
            level=self.config.logging.level,
            json_logs=self.config.logging.json_logs,
            log_file=self.config.logging.log_file,
            log_file_max_bytes=self.config.logging.log_file_max_bytes,
            log_file_backup_count=self.config.logging.log_file_backup_count,
        )
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        try:
            self._ensure_persistent_pragmas()
            if self.config.database.auto_migrate:
                self._migrate()
        except Exception as exc:
            self.health.live = False
            self.health.ready = False
            self.health.startup_complete = False
            self.health.phase = 'STARTUP_ERROR'
            self.health.last_error = str(exc)
            self.logger.error(
                "startup_storage_failure",
                extra={"extra_fields": {"error": str(exc), "db_path": str(self.db_path)}},
            )
            raise
        self._wire_runtime_services()
        self._enforce_production_crypto_gate()
        self._enforce_production_transport_gate()
        self._enforce_production_sign_gate()
        self.metrics.set_gauge("runtime.live", 1)
        supervisor = StartupSupervisor(
            health=self.health,
            connect_factory=self.connect,
            migrate=(lambda: None),
            startup_ready=self.config.runtime.startup_ready,
            reconcile_on_startup=self.config.runtime.reconcile_on_startup,
            phase2_budget_seconds=self.config.runtime.startup_phase2_budget_seconds,
            reconciliation_service=self.reconciliation_service,
        )
        self.last_startup_report = supervisor.run()
        self.metrics.set_gauge("runtime.ready", 1 if self.health.ready else 0)
        self.metrics.set_gauge("runtime.startup_complete", 1 if self.health.startup_complete else 0)
        self.logger.info("runtime_initialized", extra={"extra_fields": {"phase": self.health.phase, "ready": self.health.ready}})
        if self.config.runtime.ops_loop_enabled:
            self._start_ops_loop()
        # cert_watch poller: opt-in via cert_watch_config.enabled (SQL row).
        # Always start the thread; body short-circuits when disabled.
        if self.cert_watch_service is not None:
            self._start_cert_watch_poller()
        # cert_provisioning refresh poller: daily re-fetch of near-expiry certs.
        if self.cert_provisioning_service is not None:
            self._start_cert_provisioning_refresher()


    def _ensure_persistent_pragmas(self) -> None:
        conn = None
        try:
            conn = sqlite3.connect(self.db_path)
            conn.execute("PRAGMA journal_mode = WAL")
            conn.commit()
            row = conn.execute("PRAGMA quick_check").fetchone()
            if row is None or row[0] != 'ok':
                raise RuntimeError(
                    f'Database integrity check failed ({self.db_path}): '
                    f'{row[0] if row else "no result"}'
                )
        except sqlite3.DatabaseError as exc:
            raise RuntimeError(
                f'Database unavailable ({self.db_path}): {exc}'
            ) from exc
        finally:
            if conn is not None:
                conn.close()

    def shutdown(self) -> None:
        self._ops_loop_stop.set()
        self._cert_watch_stop.set()
        self._cert_provisioning_stop.set()
        if self._ops_loop_thread is not None:
            self._ops_loop_thread.join(timeout=10)
            self._ops_loop_thread = None
        if self._cert_watch_thread is not None:
            self._cert_watch_thread.join(timeout=10)
            self._cert_watch_thread = None
        if self._cert_provisioning_thread is not None:
            self._cert_provisioning_thread.join(timeout=10)
            self._cert_provisioning_thread = None
        self.health.live = False
        self.health.ready = False
        self.health.phase = "STOPPING"
        self.metrics.set_gauge("runtime.live", 0)
        self.metrics.set_gauge("runtime.ready", 0)
        deadline = time.monotonic() + self.config.runtime.graceful_shutdown_timeout_seconds
        while self.ingress_service.active_operations > 0 and time.monotonic() < deadline:
            time.sleep(0.01)
        self.logger.info("runtime_shutdown", extra={"extra_fields": {"drained": self.ingress_service.active_operations == 0, "active_operations": self.ingress_service.active_operations}})

    def _start_ops_loop(self) -> None:
        self._ops_loop_stop.clear()
        t = threading.Thread(target=self._ops_loop_body, name="prro-ops-loop", daemon=True)
        t.start()
        self._ops_loop_thread = t
        self.logger.info("ops_loop_started", extra={"extra_fields": {
            "interval_seconds": self.config.runtime.ops_loop_interval_seconds,
        }})

    def _ops_loop_body(self) -> None:
        interval = self.config.runtime.ops_loop_interval_seconds
        while not self._ops_loop_stop.wait(timeout=interval):
            try:
                self._ops_tick()
            except Exception as exc:
                self.logger.error("ops_loop_tick_error", extra={"extra_fields": {"error": str(exc)}})

    def _ops_tick(self) -> None:
        now = time.monotonic()

        # F1: backup (interval-gated, runs once per interval across all FNs)
        if self._backup_service is not None:
            interval_s = self.config.backup.interval_hours * 3600.0
            if now - self._last_backup_ts >= interval_s:
                self._last_backup_ts = now
                try:
                    with self.connect() as conn:
                        rows = conn.execute("SELECT fiscal_number FROM node_state").fetchall()
                        fns = [r[0] for r in rows]
                    with self.connect() as conn:
                        result = self._backup_service.run_backup(conn, fns)
                    if result.success:
                        self.logger.info('backup_completed', extra={'extra_fields': {
                            'path': result.path, 'files_deleted': result.files_deleted,
                        }})
                    else:
                        self.logger.error('backup_failed', extra={'extra_fields': {'error': result.error}})
                except Exception as exc:
                    self.logger.error('backup_tick_error', extra={'extra_fields': {'error': str(exc)}})

        # F2: retention purge (interval-gated)
        if self._retention_service is not None:
            interval_s = self.config.retention.interval_hours * 3600.0
            if now - self._last_retention_ts >= interval_s:
                self._last_retention_ts = now
                try:
                    with self.connect() as conn:
                        result = self._retention_service.run_purge(conn)
                    if result.success:
                        self.logger.info('retention_purge_completed', extra={'extra_fields': {
                            'audit_deleted': result.audit_deleted,
                            'protocol_trace_deleted': result.protocol_trace_deleted,
                            'transport_trace_deleted': result.transport_trace_deleted,
                            'inbox_deleted': result.inbox_deleted,
                        }})
                    else:
                        self.logger.error('retention_purge_failed', extra={'extra_fields': {'error': result.error}})
                except Exception as exc:
                    self.logger.error('retention_tick_error', extra={'extra_fields': {'error': str(exc)}})

        with self.connect() as conn:
            rows = conn.execute("SELECT fiscal_number FROM node_state").fetchall()
            fiscal_numbers = [r[0] for r in rows]
        for fn in fiscal_numbers:
            try:
                self._ops_tick_for_fn(fn)
            except Exception as exc:
                self.logger.error("ops_loop_fn_error", extra={"extra_fields": {
                    "fiscal_number": fn, "error": str(exc),
                }})

    def _ops_tick_for_fn(self, fiscal_number: str) -> None:
        from ..enums import NodeMode
        from ..repositories.node_state import NodeStateRepository
        from ..repositories.fiscal_documents import FiscalDocumentRepository
        with self.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, fiscal_number)
        if node_state is None:
            return
        mode = NodeMode(node_state.mode)
        if mode == NodeMode.OFFLINE:
            self._maybe_ping_and_go_online(fiscal_number, node_state)
            self._maybe_request_offline_codes(fiscal_number, node_state)
            return
        if mode == NodeMode.CRYPTO_DEGRADED:
            # H1: half-open probe — reset breaker counter once per probe interval so that
            # the next process_next call actually tries the sidecar.  If the sidecar has
            # recovered, _stage_sign will succeed and flip mode back to ONLINE automatically.
            # If it still fails, CRYPTO_DEGRADED is re-set and we wait for the next interval.
            probe_interval = self.config.crypto.breaker_probe_interval_seconds
            now = time.monotonic()
            if now - self._last_crypto_probe_ts.get(fiscal_number, 0.0) >= probe_interval:
                self._last_crypto_probe_ts[fiscal_number] = now
                if isinstance(self.command_processor, WritePathWorker):
                    prev = self.command_processor.reset_crypto_breaker()
                    self.logger.info("crypto_breaker_probe_reset", extra={"extra_fields": {
                        "fiscal_number": fiscal_number,
                        "previous_failures": prev,
                        "probe_interval_seconds": probe_interval,
                    }})
            return
        if mode != NodeMode.ONLINE:
            return  # BLOCKED / STOP_MODE — no ops-loop action
        # ONLINE: shift duration warning + reconcile + offline sync
        self._check_shift_duration_warning(fiscal_number)
        if self.reconciliation_service is not None:
            with self.connect() as conn:
                self.reconciliation_service.reconcile_pending(conn, fiscal_number=fiscal_number)
        if self.offline_sync_service is not None:
            with self.connect() as conn:
                pending = FiscalDocumentRepository.count_pending_for_offline_sync(conn, fiscal_number=fiscal_number)
            if pending > 0:
                with self.connect() as conn:
                    self.offline_sync_service.sync_pending(conn, fiscal_number=fiscal_number)

    # --- Sprint 13A: Shift duration warning -----------------------------------

    _SHIFT_WARN_HOURS: float = 20.0

    def _check_shift_duration_warning(self, fiscal_number: str) -> None:
        from datetime import UTC, datetime as _dt
        from ..repositories.shifts import ShiftRepository
        from ..runtime.alerts import AlertEvent
        with self.connect() as conn:
            shift = ShiftRepository.get_active_shift(conn, fiscal_number)
        if shift is None or shift.opened_at is None:
            return
        if shift.shift_id in self._shift_warned_ids:
            return
        try:
            opened = _dt.fromisoformat(shift.opened_at)
            if opened.tzinfo is None:
                opened = opened.replace(tzinfo=UTC)
            age_hours = (_dt.now(UTC) - opened).total_seconds() / 3600.0
        except (ValueError, TypeError):
            return
        if age_hours < self._SHIFT_WARN_HOURS:
            return
        self._shift_warned_ids.add(shift.shift_id)
        with self.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            self.alerts.emit(conn, event=AlertEvent(
                entity_type='shift',
                entity_id=shift.shift_id,
                event_type='SHIFT_DURATION_WARNING',
                severity='WARNING',
                payload={
                    'fiscal_number': fiscal_number,
                    'shift_id': shift.shift_id,
                    'age_hours': round(age_hours, 1),
                },
            ))
            conn.commit()
        self.logger.warning('shift_duration_warning', extra={'extra_fields': {
            'fiscal_number': fiscal_number,
            'shift_id': shift.shift_id,
            'age_hours': round(age_hours, 1),
        }})

    # --- Sprint 13B: DPS ping + auto-GO_ONLINE --------------------------------

    def _maybe_ping_and_go_online(self, fiscal_number: str, node_state) -> None:
        import uuid
        import hashlib
        from ..enums import OperationType, Protocol
        from ..repositories.inbox import InboxRepository
        from ..models.canonical import CanonicalFiscalCommand
        if self.transport_router is None or node_state.current_transport_profile_id is None:
            return
        try:
            handler, profile = self.transport_router._resolve(node_state.current_transport_profile_id)
        except LookupError:
            return
        if not hasattr(handler, 'ping'):
            return
        try:
            alive = handler.ping(fiscal_number=fiscal_number, transport_profile=profile)
        except Exception as exc:
            self.logger.debug('ops_ping_error', extra={'extra_fields': {
                'fiscal_number': fiscal_number, 'error': str(exc),
            }})
            return
        if not alive:
            return
        # DPS reachable — inject synthetic GO_ONLINE into inbox (idempotent per offline session)
        offline_session_id = node_state.current_offline_session_id or 'unknown'
        idem_key = f'ops-auto-go-online-{fiscal_number}-{offline_session_id}'
        if idem_key in self._auto_go_online_keys:
            return
        self._auto_go_online_keys.add(idem_key)
        self.logger.info('ops_auto_go_online', extra={'extra_fields': {
            'fiscal_number': fiscal_number,
            'offline_session_id': offline_session_id,
        }})
        from datetime import UTC, datetime as _dt
        payload: dict = {}
        payload_json = '{}'
        cmd = CanonicalFiscalCommand(
            request_id=str(uuid.uuid4()),
            idempotency_key=idem_key,
            protocol=Protocol.INTERNAL,
            operation_type=OperationType.GO_ONLINE,
            fiscal_number=fiscal_number,
            backend_profile_id=node_state.current_backend_profile_id,
            transport_profile_id=node_state.current_transport_profile_id,
            business_ts=_dt.now(UTC),
            payload=payload,
            payload_sha256=hashlib.sha256(payload_json.encode()).hexdigest(),
        )
        with self.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            InboxRepository.accept_command(
                conn,
                request_id=cmd.request_id,
                idempotency_key=cmd.idempotency_key,
                protocol=cmd.protocol,
                operation_type=cmd.operation_type,
                fiscal_number=cmd.fiscal_number,
                payload_json=cmd.model_dump_json(),
                payload_sha256=cmd.payload_sha256,
                backend_profile_id=cmd.backend_profile_id,
                transport_profile_id=cmd.transport_profile_id,
            )
            conn.commit()
        if self.command_processor is not None:
            with self.connect() as conn:
                self.command_processor.process_next(
                    conn,
                    fiscal_number=fiscal_number,
                    lease_owner=self.config.runtime.worker_lease_owner,
                )

    # --- Sprint 13C: Auto-fetch offline codes (T=112) -------------------------

    def _maybe_request_offline_codes(self, fiscal_number: str, node_state) -> None:
        import uuid
        from datetime import UTC, datetime as _dt
        from ..repositories.fn_config import FiscalNumberConfigRepository
        from ..repositories.offline import OfflineRepository
        if self.transport_router is None or node_state.current_transport_profile_id is None:
            return
        with self.connect() as conn:
            fn_cfg = FiscalNumberConfigRepository.get_or_default(conn, fiscal_number)
        if fn_cfg.min_offline_codes == 0 or fn_cfg.max_offline_codes == 0:
            return
        with self.connect() as conn:
            available = OfflineRepository.count_available(conn, fiscal_number)
        if available >= fn_cfg.min_offline_codes:
            return
        qty_needed = fn_cfg.max_offline_codes - available
        try:
            handler, profile = self.transport_router._resolve(node_state.current_transport_profile_id)
        except LookupError:
            return
        if not hasattr(handler, 'request_offline_codes'):
            return
        crypto = self.crypto_provider or self._resolve_crypto_provider()
        if crypto is None:
            return
        now = _dt.now(UTC)
        # Kyiv local time for <TS> — same convention as _kyiv_local_epoch
        try:
            from zoneinfo import ZoneInfo
            ts = now.astimezone(ZoneInfo('Europe/Kyiv')).strftime('%Y%m%d%H%M%S')
        except Exception:
            ts = now.strftime('%Y%m%d%H%M%S')
        tax_number = (self.config.defaults.tax_number or '').lstrip('ПН').strip()
        xml_payload = (
            f'<?xml version="1.0" encoding="windows-1251"?>'
            f'<RQ V="1"><DAT FN="{fiscal_number}" TN="ПН {tax_number}" ZN="" DI="0" V="1">'
            f'<C T="112"><H SIZE="{qty_needed}"></H></C>'
            f'<TS>{ts}</TS></DAT><MAC></MAC></RQ>'
        )
        try:
            signed = crypto.sign(
                document_id=f'offline-codes-{fiscal_number}-{ts}',
                payload_json=xml_payload,
            )
        except Exception as exc:
            self.logger.warning('ops_offline_codes_sign_error', extra={'extra_fields': {
                'fiscal_number': fiscal_number, 'error': str(exc),
            }})
            return
        try:
            codes = handler.request_offline_codes(
                fiscal_number=fiscal_number,
                qty=qty_needed,
                signed_payload=signed,
                transport_profile=profile,
            )
        except Exception as exc:
            self.logger.warning('ops_offline_codes_request_error', extra={'extra_fields': {
                'fiscal_number': fiscal_number, 'error': str(exc),
            }})
            return
        if not codes:
            return
        first, last = min(codes), max(codes)
        if last - first + 1 != len(codes):
            self.logger.warning('ops_offline_codes_non_contiguous', extra={'extra_fields': {
                'fiscal_number': fiscal_number, 'first': first, 'last': last, 'count': len(codes),
            }})
        with self.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            if not OfflineRepository.has_overlapping_range(
                conn, fiscal_number=fiscal_number, first_fiscal_no=first, last_fiscal_no=last
            ):
                OfflineRepository.create_range(
                    conn,
                    range_id=str(uuid.uuid4()),
                    fiscal_number=fiscal_number,
                    first_fiscal_no=first,
                    last_fiscal_no=last,
                    issued_at=now.isoformat(),
                    source_payload_json=(
                        f'{{"source":"ops-auto","qty":{len(codes)},"from_dps":true}}'
                    ),
                )
                conn.commit()
                self.logger.info('ops_offline_codes_stored', extra={'extra_fields': {
                    'fiscal_number': fiscal_number, 'first': first, 'last': last, 'count': len(codes),
                }})

    def _start_cert_watch_poller(self) -> None:
        """Start the cert_watch background poller.

        The thread respects shutdown via self._cert_watch_stop (set in
        shutdown()), so SIGTERM path stays graceful per invariant FI-9.
        """
        if self.cert_watch_service is None:
            return
        self._cert_watch_stop.clear()
        t = threading.Thread(
            target=self._cert_watch_loop_body,
            name="prro-cert-watch",
            daemon=True,
        )
        t.start()
        self._cert_watch_thread = t
        self.logger.info("cert_watch_started")

    def _cert_watch_loop_body(self) -> None:
        # The actual polling cadence lives in cert_watch_config.
        # We tick at the configured cadence but re-read config each loop
        # so admin edits take effect without a restart. A small minimum
        # wait prevents a misconfigured 0-interval from busy-looping.
        from ..repositories import CertWatchConfigRepository
        while not self._cert_watch_stop.is_set():
            try:
                with self.connect() as conn:
                    cfg = CertWatchConfigRepository.get(conn)
                interval = max(30, int(cfg.polling_interval_seconds))
            except Exception as exc:
                self.logger.error(
                    "cert_watch_config_read_error",
                    extra={"extra_fields": {"error": str(exc)}},
                )
                interval = 60
            if self._cert_watch_stop.wait(timeout=interval):
                return
            try:
                if self.cert_watch_service is not None:
                    self.cert_watch_service.poll_all_active()
            except Exception as exc:
                self.logger.error(
                    "cert_watch_tick_error",
                    extra={"extra_fields": {"error": str(exc)}},
                )

    def _start_cert_provisioning_refresher(self) -> None:
        """Start the daily near-expiry cert refresher.

        Respects shutdown via self._cert_provisioning_stop so SIGTERM
        drains cleanly per invariant FI-9.
        """
        if self.cert_provisioning_service is None:
            return
        self._cert_provisioning_stop.clear()
        t = threading.Thread(
            target=self._cert_provisioning_loop_body,
            name="prro-cert-provisioning",
            daemon=True,
        )
        t.start()
        self._cert_provisioning_thread = t
        self.logger.info("cert_provisioning_refresher_started")

    def _cert_provisioning_loop_body(self) -> None:
        # Daily refresh cadence — near-expiry certs don't need more.
        interval_seconds = 24 * 60 * 60
        while not self._cert_provisioning_stop.is_set():
            if self._cert_provisioning_stop.wait(timeout=interval_seconds):
                return
            try:
                if self.cert_provisioning_service is not None:
                    self.cert_provisioning_service.refresh_expiring_certs()
            except Exception as exc:
                self.logger.error(
                    "cert_provisioning_refresh_error",
                    extra={"extra_fields": {"error": str(exc)}},
                )

    def _migrate(self) -> None:
        with self.connect() as conn:
            apply_migrations_to_connection(conn, self.sql_dir)
            conn.commit()

    def _wire_runtime_services(self) -> None:
        with self.connect() as conn:
            self.transport_router = ProfileAwareTransportRouter.from_connection(conn, handlers=self._build_transport_handlers())
        self._apply_checkbox_config_overrides()
        self._check_checkbox_auth_profiles()
        if self.reconciliation_service is None and self.transport_router is not None:
            self.reconciliation_service = ReconciliationService(
                transport_status_client=self.transport_router,
                max_recovery_attempts=self.config.runtime.max_recovery_attempts,
                crypto_provider=self.crypto_provider or self._resolve_crypto_provider(),
                concurrency=self.config.reconciliation.concurrency,
                cancel_event=self._ops_loop_stop,
            )
        if self.offline_sync_service is None and self.transport_router is not None:
            self.offline_sync_service = OfflineSyncService(
                transport_client=self.transport_router,
                max_recovery_attempts=self.config.runtime.max_recovery_attempts,
            )
        if self.command_processor is None and self.config.runtime.process_immediately and self.transport_router is not None:
            self.command_processor = WritePathWorker(
                crypto_provider=self._resolve_crypto_provider(),
                transport_client=self.transport_router,
                crypto_timeout_seconds=self.config.crypto.timeout_seconds,
                crypto_breaker_threshold=self.config.crypto.breaker_threshold,
                crypto_breaker_recovery_successes=self.config.crypto.breaker_recovery_successes,
                tax_number=self.config.defaults.tax_number,
                require_return_linkage=self.config.runtime.environment != 'development',
                validate_timestamps=self.config.runtime.environment != 'development',
            )
        if self.cert_watch_service is None and self._cert_der_provider is not None:
            # Only build the default service when we have a way to resolve
            # the signing cert. Without it every check returns 'unreachable'
            # — running the poller would be pure noise.
            self.cert_watch_service = CertWatchService(
                connect=self.connect,
                cert_der_provider=self._cert_der_provider,
            )
        if self.cert_provisioning_service is None:
            self.cert_provisioning_service = CertProvisioningService(
                connect=self.connect,
            )
        self.ingress_service.command_processor = self.command_processor
        if self._backup_service is None and self.config.backup.enabled:
            self._backup_service = BackupService(
                db_path=str(self.db_path),
                backup_dir=self.config.backup.backup_dir,
                keep_count=self.config.backup.keep_count,
            )
        if self._retention_service is None and self.config.retention.enabled:
            self._retention_service = RetentionService(
                audit_ttl_days=self.config.retention.audit_ttl_days,
                trace_ttl_days=self.config.retention.trace_ttl_days,
                inbox_ttl_days=self.config.retention.inbox_ttl_days,
            )

    def _apply_checkbox_config_overrides(self) -> None:
        overrides = self.config.checkbox
        if self.transport_router is None:
            return
        if not any([overrides.endpoint, overrides.license_key, overrides.cashier_pin]):
            return
        to_update: list[tuple[str, str, str | None]] = []  # (profile_id, new_config_json, new_endpoint)
        for profile_id, profile in list(self.transport_router.profiles.items()):
            if profile.kind != TransportKind.CHECKBOX_REST_TRANSPORT:
                continue
            try:
                cfg: dict = json.loads(profile.config_json)
            except (ValueError, TypeError):
                cfg = {}
            if not isinstance(cfg, dict):
                cfg = {}
            if overrides.license_key or overrides.cashier_pin:
                auth = cfg.get('auth') if isinstance(cfg.get('auth'), dict) else {}
                if overrides.license_key:
                    auth['license_key'] = overrides.license_key
                if overrides.cashier_pin:
                    auth['cashier_pin'] = overrides.cashier_pin
                cfg['auth'] = auth
            new_config_json = json.dumps(cfg, ensure_ascii=False, separators=(',', ':'))
            new_endpoint = overrides.endpoint if overrides.endpoint else profile.endpoint
            # Update in-memory router profile
            self.transport_router.profiles[profile_id] = profile.model_copy(update={
                'config_json': new_config_json,
                'endpoint': new_endpoint,
            })
            to_update.append((profile_id, new_config_json, new_endpoint))
        # Persist to DB so write-path's direct TransportProfileRepository.get_by_id() picks up overrides
        if to_update:
            with self.connect() as conn:
                for profile_id, new_config_json, new_endpoint in to_update:
                    conn.execute(
                        'UPDATE transport_profiles SET endpoint = ?, config_json = ? WHERE transport_profile_id = ?',
                        (new_endpoint, new_config_json, profile_id),
                    )
                conn.commit()

    def _check_checkbox_auth_profiles(self) -> None:
        if self.transport_router is None:
            return
        for profile_id, profile in self.transport_router.profiles.items():
            if profile.kind != TransportKind.CHECKBOX_REST_TRANSPORT:
                continue
            try:
                cfg = json.loads(profile.config_json)
            except (ValueError, TypeError):
                cfg = {}
            auth = cfg.get('auth') if isinstance(cfg, dict) else {}
            if not isinstance(auth, dict):
                auth = {}
            has_pin_auth = bool(auth.get('license_key') and auth.get('cashier_pin'))
            has_pass_auth = bool(auth.get('cashier_login') and auth.get('cashier_password'))
            if not (has_pin_auth or has_pass_auth):
                self.logger.warning(
                    'checkbox_profile_missing_auth',
                    extra={'extra_fields': {'profile_id': profile_id}},
                )

    def _resolve_crypto_provider(self):
        """Return crypto provider for WritePathWorker.

        Priority:
        1. Constructor injection (self.crypto_provider is not None) — used by tests and operator overrides.
        2. config.crypto.provider value:
           - 'passthrough': PassthroughCryptoProvider (default, signs nothing)
           - 'sidecar':     SidecarCryptoProvider backed by SidecarCryptoClient;
                            requires config.crypto.sidecar_url to be set.
           - anything else: raises ValueError at startup (unknown/unsupported provider)
        """
        if self.crypto_provider is not None:
            return self.crypto_provider
        name = self.config.crypto.provider
        if name == 'passthrough':
            return PassthroughCryptoProvider()
        if name == 'sidecar':
            sidecar_url = self.config.crypto.sidecar_url
            if not sidecar_url:
                raise ValueError(
                    "crypto.provider='sidecar' requires crypto.sidecar_url to be set."
                )
            client = SidecarCryptoClient(
                base_url=sidecar_url,
                http_client=self.crypto_http_client,
                connect_timeout=self.config.crypto.sidecar_connect_timeout,
                read_timeout=self.config.crypto.sidecar_read_timeout,
            )
            return SidecarCryptoProvider(client=client)
        raise ValueError(
            f"Unsupported crypto provider: {name!r}. "
            "Supported values: 'passthrough', 'sidecar'."
        )

    def _enforce_production_crypto_gate(self) -> None:
        """Fail-fast if production runtime uses PassthroughCryptoProvider.

        Resolves the crypto provider explicitly (even if process_immediately=False)
        to catch misconfiguration before runtime becomes ready.  Checks both
        config-driven and constructor-injected providers.
        """
        if self.config.runtime.environment != 'production':
            # M5: Warn at startup so operators see in logs that the gate is inactive.
            # Intentional for dev/test; a reminder to set runtime.environment=production
            # before deploying to a live fiscal node.
            self.logger.warning(
                "production_crypto_gate_inactive",
                extra={"extra_fields": {
                    "environment": self.config.runtime.environment,
                    "note": "PassthroughCryptoProvider is allowed; set runtime.environment=production to enforce gate",
                }},
            )
            return
        # Priority: injected command_processor's provider > injected crypto_provider > config-driven
        if self.command_processor is not None and hasattr(self.command_processor, 'crypto_provider'):
            provider = self.command_processor.crypto_provider
        elif self.crypto_provider is not None:
            provider = self.crypto_provider
        else:
            provider = self._resolve_crypto_provider()
        if isinstance(provider, PassthroughCryptoProvider):
            self.health.live = False
            self.health.ready = False
            self.health.phase = 'STARTUP_ERROR'
            self.health.last_error = 'PassthroughCryptoProvider is not allowed in production'
            # Persist to audit_log — DB is ready after _migrate()
            try:
                from ..repositories import AuditRepository
                from ..utils.json_codec import dumps_json
                with self.connect() as conn:
                    conn.execute('BEGIN IMMEDIATE')
                    AuditRepository.log_event(
                        conn,
                        entity_type='CRYPTO',
                        entity_id='startup-gate',
                        event_type='PRODUCTION_CRYPTO_GATE_BLOCKED',
                        severity='CRITICAL',
                        event_payload_json=dumps_json({
                            'provider_type': type(provider).__name__,
                            'environment': self.config.runtime.environment,
                        }),
                    )
                    conn.commit()
            except Exception:
                pass  # DB may not be ready in edge cases; structured log is the fallback
            raise RuntimeError(
                'Production startup blocked: PassthroughCryptoProvider is not allowed. '
                'Set crypto.provider=sidecar with a valid sidecar_url, '
                'or set runtime.environment to development/test.'
            )

    def _enforce_production_transport_gate(self) -> None:
        """Fail-fast if production runtime has active transport profiles using stub handlers.

        Inspects the effective transport_router: for each active profile, resolves its
        handler by kind and checks if it is a known stub class.
        """
        if self.config.runtime.environment != 'production':
            return
        # Check all injected service transport clients
        _stub_types = (CheckboxRestTransportStub, DpsGrpcEcabinetTransportStub, DpsXmlUnifiedWindowTransportStub)
        _services_to_check = [
            ('command_processor', self.command_processor, 'transport_client'),
            ('offline_sync_service', self.offline_sync_service, 'transport_client'),
            ('reconciliation_service', self.reconciliation_service, 'transport_status_client'),
        ]
        for svc_name, svc, attr in _services_to_check:
            if svc is not None and hasattr(svc, attr):
                tc = getattr(svc, attr)
                if isinstance(tc, _stub_types):
                    self.health.live = False
                    self.health.ready = False
                    self.health.phase = 'STARTUP_ERROR'
                    self.health.last_error = f'Stub transport client on {svc_name}: {type(tc).__name__}'
                    raise RuntimeError(
                        f'Production startup blocked: {svc_name} uses stub transport '
                        f'{type(tc).__name__}. Inject a real transport client or use router-based wiring.'
                    )
        if self.transport_router is None:
            return
        offending: list[str] = []
        for profile_id, profile in self.transport_router.profiles.items():
            handler = self.transport_router.handlers.get(profile.kind)
            if handler is not None and isinstance(handler, _stub_types):
                offending.append(f'{profile_id} (kind={profile.kind.value})')
        if offending:
            self.health.live = False
            self.health.ready = False
            self.health.phase = 'STARTUP_ERROR'
            self.health.last_error = f'Stub transport handlers in production: {", ".join(offending)}'
            raise RuntimeError(
                f'Production startup blocked: active transport profiles use stub handlers: '
                f'{", ".join(offending)}. '
                f'Deactivate stub profiles or replace with real transport handlers.'
            )

    def _enforce_production_sign_gate(self) -> None:
        """Fail-fast if any transport profile in production has require_local_sign=false.

        require_local_sign=false silently bypasses local signing even when crypto.provider=sidecar
        is configured and _enforce_production_crypto_gate passes.  This gate is the second line
        of defence: it catches per-profile sign bypass that the crypto gate cannot see.
        """
        if self.config.runtime.environment != 'production':
            return
        if self.transport_router is None:
            return
        import json as _json
        offending: list[str] = []
        for profile_id, profile in self.transport_router.profiles.items():
            try:
                cfg = _json.loads(profile.config_json or '{}')
            except ValueError:
                cfg = {}
            if isinstance(cfg, dict) and cfg.get('require_local_sign') is False:
                offending.append(profile_id)
        if not offending:
            return
        self.health.live = False
        self.health.ready = False
        self.health.phase = 'STARTUP_ERROR'
        self.health.last_error = f'require_local_sign=false in production profiles: {", ".join(offending)}'
        try:
            from ..repositories import AuditRepository
            from ..utils.json_codec import dumps_json
            with self.connect() as conn:
                conn.execute('BEGIN IMMEDIATE')
                AuditRepository.log_event(
                    conn,
                    entity_type='CRYPTO',
                    entity_id='startup-gate',
                    event_type='PRODUCTION_SIGN_GATE_BLOCKED',
                    severity='CRITICAL',
                    event_payload_json=dumps_json({
                        'offending_profiles': offending,
                        'environment': self.config.runtime.environment,
                    }),
                )
                conn.commit()
        except Exception:
            pass
        raise RuntimeError(
            f'Production startup blocked: transport profiles have require_local_sign=false: '
            f'{", ".join(offending)}. '
            f'Remove require_local_sign=false from production profiles or set '
            f'runtime.environment to development/test.'
        )

    def _build_transport_handlers(self) -> dict[TransportKind, object]:
        handlers: dict[TransportKind, object] = {
            TransportKind.CHECKBOX_REST_TRANSPORT: CheckboxRestTransport(http_client=self.transport_http_client),
            TransportKind.DPS_PRRO_GRPC_ECABINET: DpsFiscalServerTransport() if self.config.runtime.environment != 'development' else DpsGrpcEcabinetTransportStub(),
            TransportKind.DPS_PRRO_XML_UNIFIED_WINDOW: DpsXmlUnifiedWindowTransportStub(),
            TransportKind.DPS_PRRO_FISCAL_SIDECAR_V2: FiscalSidecarTransport(
                sidecar_url=self.config.crypto.sidecar_url or 'http://127.0.0.1:8765',
                http_client=self.transport_http_client,
                crypto_provider=self.config.crypto.provider or 'passthrough',
            ),
        }
        handlers.update(self.transport_handlers)
        return handlers

    @contextmanager
    def connect(self) -> Iterator[sqlite3.Connection]:
        conn = sqlite3.connect(self.db_path)
        try:
            conn.row_factory = sqlite3.Row
            conn.execute("PRAGMA synchronous = FULL")
            conn.execute("PRAGMA foreign_keys = ON")
            conn.execute("PRAGMA busy_timeout = 5000")
            yield conn
        finally:
            conn.close()


__all__ = ["RuntimeContainer"]
