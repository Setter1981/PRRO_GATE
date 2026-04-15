from __future__ import annotations

import json
import logging
from datetime import UTC, datetime
from typing import Any


class JsonLogFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        payload: dict[str, Any] = {
            "ts": datetime.now(UTC).isoformat(),
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
        }
        if hasattr(record, "extra_fields") and isinstance(record.extra_fields, dict):
            payload.update(record.extra_fields)
        if record.exc_info:
            payload["exc_info"] = self.formatException(record.exc_info)
        return json.dumps(payload, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


class PlainLogFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        base = super().format(record)
        if hasattr(record, "extra_fields") and isinstance(record.extra_fields, dict) and record.extra_fields:
            extras = " ".join(f"{k}={v}" for k, v in sorted(record.extra_fields.items()))
            return f"{base} {extras}"
        return base


class BoundLoggerAdapter(logging.LoggerAdapter):
    def process(self, msg: str, kwargs: dict[str, Any]) -> tuple[str, dict[str, Any]]:
        extra = kwargs.setdefault("extra", {})
        existing = extra.get("extra_fields", {})
        merged = {**self.extra, **existing}
        extra["extra_fields"] = merged
        return msg, kwargs


_configured = False


def configure_logging(*, level: str = "INFO", json_logs: bool = True) -> None:
    global _configured
    root = logging.getLogger()
    for handler in list(root.handlers):
        root.removeHandler(handler)
    handler = logging.StreamHandler()
    if json_logs:
        handler.setFormatter(JsonLogFormatter())
    else:
        handler.setFormatter(PlainLogFormatter("%(asctime)s %(levelname)s %(name)s %(message)s"))
    root.addHandler(handler)
    root.setLevel(getattr(logging, level.upper(), logging.INFO))
    _configured = True


def get_logger(name: str, **fields: Any) -> BoundLoggerAdapter:
    if not _configured:
        configure_logging()
    return BoundLoggerAdapter(logging.getLogger(name), fields)


__all__ = ["configure_logging", "get_logger", "BoundLoggerAdapter"]
