from __future__ import annotations

import json
import sqlite3
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
from ..services.ingress import IngressAcceptService
from ..services.offline_sync import OfflineSyncService
from ..services.reconciliation import ReconciliationService
from ..services.write_path import WritePathWorker
from ..transports import CheckboxRestTransport, DpsFiscalServerTransport, ProfileAwareTransportRouter
from ..transports.stubs import CheckboxRestTransportStub, DpsGrpcEcabinetTransportStub, DpsXmlUnifiedWindowTransportStub


class RuntimeContainer:
    def __init__(self, config: AppConfig, *, command_processor=None, reconciliation_service=None, crypto_provider=None, transport_handlers=None, transport_http_client: httpx.Client | None = None, crypto_http_client: httpx.Client | None = None) -> None:
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
        self.last_startup_report: StartupRunReport | None = None
        self.logger = get_logger("prro_gateway.runtime.container", app_name=config.app_name)

    @property
    def db_path(self) -> Path:
        return Path(self.config.database.db_path)

    @property
    def sql_dir(self) -> Path:
        return Path(self.config.database.sql_dir)

    def initialize(self) -> None:
        configure_logging(level=self.config.logging.level, json_logs=self.config.logging.json_logs)
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
        self.health.live = False
        self.health.ready = False
        self.health.phase = "STOPPING"
        self.metrics.set_gauge("runtime.live", 0)
        self.metrics.set_gauge("runtime.ready", 0)
        deadline = time.monotonic() + self.config.runtime.graceful_shutdown_timeout_seconds
        while self.ingress_service.active_operations > 0 and time.monotonic() < deadline:
            time.sleep(0.01)
        self.logger.info("runtime_shutdown", extra={"extra_fields": {"drained": self.ingress_service.active_operations == 0, "active_operations": self.ingress_service.active_operations}})

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
                tax_number=self.config.defaults.tax_number,
                require_return_linkage=self.config.runtime.environment != 'development',
                validate_timestamps=self.config.runtime.environment != 'development',
            )
        self.ingress_service.command_processor = self.command_processor

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

    def _build_transport_handlers(self) -> dict[TransportKind, object]:
        handlers: dict[TransportKind, object] = {
            TransportKind.CHECKBOX_REST_TRANSPORT: CheckboxRestTransport(http_client=self.transport_http_client),
            TransportKind.DPS_PRRO_GRPC_ECABINET: DpsFiscalServerTransport() if self.config.runtime.environment != 'development' else DpsGrpcEcabinetTransportStub(),
            TransportKind.DPS_PRRO_XML_UNIFIED_WINDOW: DpsXmlUnifiedWindowTransportStub(),
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
