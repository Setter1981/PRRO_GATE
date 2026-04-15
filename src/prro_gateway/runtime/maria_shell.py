from __future__ import annotations

from typing import Any

from ..logging import get_logger
from ..runtime.container import RuntimeContainer
from ..services.ingress import AdapterMappingError


class MariaIngressShell:
    def __init__(self, container: RuntimeContainer) -> None:
        self.container = container
        self.logger = get_logger("prro_gateway.runtime.maria")

    def handle_envelope(self, *, command_name: str, fields: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
        try:
            with self.container.connect() as conn:
                inbox, command = self.container.ingress_service.accept_maria(conn, command_name=command_name, fields=fields, context=context)
        except AdapterMappingError as exc:
            self.logger.warning("maria_mapping_error", extra={"extra_fields": {"command": command_name, "code": exc.code}})
            return {"ok": False, "error": {"code": exc.code, "message": exc.message}}
        self.logger.info("maria_accept_ok", extra={"extra_fields": {"command": command_name, "request_id": inbox.request_id}})
        return {
            "ok": True,
            "request_id": inbox.request_id,
            "idempotency_key": inbox.idempotency_key,
            "status": inbox.status.value,
            "protocol": command.protocol.value,
        }

    def close(self) -> None:
        self.container.shutdown()


__all__ = ["MariaIngressShell"]
