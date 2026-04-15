from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any

import yaml
from typing import Literal

from pydantic import Field

from .models.common import StrictModel


class DatabaseConfig(StrictModel):
    db_path: str = Field(default="./var/prro_gateway.sqlite3")
    sql_dir: str = Field(default="./sql")
    auto_migrate: bool = True


class DefaultsConfig(StrictModel):
    fiscal_number: str = "FN-DEFAULT"
    tax_number: str = ""
    backend_profile_id: str = "backend_checkbox_default"
    transport_profile_id: str = "transport_checkbox_rest_default"
    channel_owner: str = "runtime"


class RuntimeConfig(StrictModel):
    environment: Literal['development', 'test', 'production'] = 'development'
    process_immediately: bool = False
    worker_lease_owner: str = "runtime-worker"
    startup_ready: bool = True
    reconcile_on_startup: bool = True
    startup_phase2_budget_seconds: int = 300
    graceful_shutdown_timeout_seconds: int = 30
    max_recovery_attempts: int = 5


class RestIngressConfig(StrictModel):
    enabled: bool = True
    host: str = "127.0.0.1"
    port: int = 8080
    response_timeout_seconds: int = 60


class XmlRpcIngressConfig(StrictModel):
    enabled: bool = True
    host: str = "127.0.0.1"
    port: int = 8090


class MariaIngressConfig(StrictModel):
    enabled: bool = True
    host: str = "127.0.0.1"
    port: int = 8100
    line_delimited_json: bool = True


class IngressConfig(StrictModel):
    rest: RestIngressConfig = Field(default_factory=RestIngressConfig)
    xmlrpc: XmlRpcIngressConfig = Field(default_factory=XmlRpcIngressConfig)
    maria: MariaIngressConfig = Field(default_factory=MariaIngressConfig)


class LoggingConfig(StrictModel):
    level: str = "INFO"
    json_logs: bool = True


class MetricsConfig(StrictModel):
    enabled: bool = True
    include_process: bool = True


class AlertsConfig(StrictModel):
    enabled: bool = True
    persist_to_audit: bool = True


class CheckboxTransportOverridesConfig(StrictModel):
    endpoint: str | None = None
    license_key: str | None = None
    cashier_pin: str | None = None


class CryptoConfig(StrictModel):
    provider: str = "passthrough"
    timeout_seconds: float | None = 5.0
    sidecar_url: str | None = None
    sidecar_connect_timeout: float = 5.0
    sidecar_read_timeout: float = 10.0
    breaker_threshold: int = 5  # consecutive failures before breaker opens; 0 = disabled


class AppConfig(StrictModel):
    app_name: str = "prro-gateway"
    version: str = "1.4.1"
    database: DatabaseConfig = Field(default_factory=DatabaseConfig)
    defaults: DefaultsConfig = Field(default_factory=DefaultsConfig)
    runtime: RuntimeConfig = Field(default_factory=RuntimeConfig)
    ingress: IngressConfig = Field(default_factory=IngressConfig)
    logging: LoggingConfig = Field(default_factory=LoggingConfig)
    metrics: MetricsConfig = Field(default_factory=MetricsConfig)
    alerts: AlertsConfig = Field(default_factory=AlertsConfig)
    checkbox: CheckboxTransportOverridesConfig = Field(default_factory=CheckboxTransportOverridesConfig)
    crypto: CryptoConfig = Field(default_factory=CryptoConfig)

    @classmethod
    def from_mapping(cls, data: dict[str, Any]) -> "AppConfig":
        return cls.model_validate(data)

    @classmethod
    def from_file(cls, path: str | os.PathLike[str]) -> "AppConfig":
        path_obj = Path(path)
        raw = path_obj.read_text(encoding="utf-8")
        suffix = path_obj.suffix.lower()
        if suffix in {".yaml", ".yml"}:
            data = yaml.safe_load(raw) or {}
        elif suffix == ".json":
            data = json.loads(raw)
        elif suffix == ".toml":
            import tomllib
            data = tomllib.loads(raw)
        else:
            raise ValueError(f"Unsupported config format: {suffix}")
        return cls.from_mapping(data)

    @classmethod
    def from_env(cls) -> "AppConfig":
        cfg_path = os.getenv("PRRO_GATEWAY_CONFIG")
        cfg = cls.from_file(cfg_path) if cfg_path else cls()
        return cls._apply_env_overrides(cfg)

    @classmethod
    def _apply_env_overrides(cls, cfg: "AppConfig") -> "AppConfig":
        data = cfg.model_dump(mode="python")

        def set_if(name: str, path: tuple[str, ...], caster=str) -> None:
            raw = os.getenv(name)
            if raw is None:
                return
            cursor = data
            for key in path[:-1]:
                cursor = cursor[key]
            cursor[path[-1]] = caster(raw)

        set_if("PRRO_DB_PATH", ("database", "db_path"))
        set_if("PRRO_SQL_DIR", ("database", "sql_dir"))
        set_if("PRRO_FISCAL_NUMBER", ("defaults", "fiscal_number"))
        set_if("PRRO_REST_PORT", ("ingress", "rest", "port"), int)
        set_if("PRRO_XMLRPC_PORT", ("ingress", "xmlrpc", "port"), int)
        set_if("PRRO_MARIA_PORT", ("ingress", "maria", "port"), int)
        set_if("PRRO_LOG_LEVEL", ("logging", "level"))
        set_if("PRRO_RUNTIME_ENVIRONMENT", ("runtime", "environment"))
        set_if("PRRO_PROCESS_IMMEDIATELY", ("runtime", "process_immediately"), lambda v: v.lower() in {"1", "true", "yes", "on"})
        set_if("PRRO_CHECKBOX_ENDPOINT", ("checkbox", "endpoint"))
        set_if("PRRO_CHECKBOX_LICENSE_KEY", ("checkbox", "license_key"))
        set_if("PRRO_CHECKBOX_CASHIER_PIN", ("checkbox", "cashier_pin"))
        set_if("PRRO_CRYPTO_PROVIDER", ("crypto", "provider"))
        set_if("PRRO_CRYPTO_SIDECAR_URL", ("crypto", "sidecar_url"))
        set_if("PRRO_CRYPTO_SIDECAR_CONNECT_TIMEOUT", ("crypto", "sidecar_connect_timeout"), float)
        set_if("PRRO_CRYPTO_SIDECAR_READ_TIMEOUT", ("crypto", "sidecar_read_timeout"), float)
        return cls.from_mapping(data)


__all__ = [
    "DatabaseConfig",
    "DefaultsConfig",
    "RuntimeConfig",
    "RestIngressConfig",
    "XmlRpcIngressConfig",
    "MariaIngressConfig",
    "IngressConfig",
    "LoggingConfig",
    "MetricsConfig",
    "AlertsConfig",
    "CheckboxTransportOverridesConfig",
    "CryptoConfig",
    "AppConfig",
]
