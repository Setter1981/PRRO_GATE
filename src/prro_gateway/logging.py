from __future__ import annotations

import json
import logging
import os
from datetime import UTC, datetime
from logging.handlers import RotatingFileHandler
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


def configure_logging(
    *,
    level: str = "INFO",
    json_logs: bool = True,
    log_file: str | None = None,
    log_file_max_bytes: int = 10 * 1024 * 1024,
    log_file_backup_count: int = 5,
) -> None:
    global _configured
    root = logging.getLogger()
    for handler in list(root.handlers):
        root.removeHandler(handler)

    numeric_level = getattr(logging, level.upper(), logging.INFO)

    def _make_formatter() -> logging.Formatter:
        if json_logs:
            return JsonLogFormatter()
        return PlainLogFormatter("%(asctime)s %(levelname)s %(name)s %(message)s")

    stream_handler = logging.StreamHandler()
    stream_handler.setFormatter(_make_formatter())
    root.addHandler(stream_handler)

    if log_file:
        os.makedirs(os.path.dirname(os.path.abspath(log_file)), exist_ok=True)
        file_handler = RotatingFileHandler(
            log_file,
            maxBytes=log_file_max_bytes,
            backupCount=log_file_backup_count,
            encoding="utf-8",
        )
        file_handler.setFormatter(_make_formatter())
        root.addHandler(file_handler)

    root.setLevel(numeric_level)
    _configured = True


def get_logger(name: str, **fields: Any) -> BoundLoggerAdapter:
    if not _configured:
        configure_logging()
    return BoundLoggerAdapter(logging.getLogger(name), fields)


__all__ = ["configure_logging", "get_logger", "BoundLoggerAdapter"]
