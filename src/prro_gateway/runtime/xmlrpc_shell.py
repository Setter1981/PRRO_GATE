from __future__ import annotations

from typing import Any

from ..logging import get_logger
from ..runtime.container import RuntimeContainer
from ..services.ingress import AdapterMappingError


class XmlRpcIngressShell:
    def __init__(self, container: RuntimeContainer) -> None:
        self.container = container
        self.logger = get_logger("prro_gateway.runtime.xmlrpc")

    def handle_call(self, *, method: str, params: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
        try:
            with self.container.connect() as conn:
                inbox, command = self.container.ingress_service.accept_webcheck(conn, method=method, params=params, context=context)
        except AdapterMappingError as exc:
            self.logger.warning("xmlrpc_mapping_error", extra={"extra_fields": {"method": method, "code": exc.code}})
            return {"ok": False, "error": {"code": exc.code, "message": exc.message}}
        self.logger.info("xmlrpc_accept_ok", extra={"extra_fields": {"method": method, "request_id": inbox.request_id}})
        return {
            "ok": True,
            "request_id": inbox.request_id,
            "idempotency_key": inbox.idempotency_key,
            "status": inbox.status.value,
            "protocol": command.protocol.value,
        }

    def close(self) -> None:
        self.container.shutdown()


__all__ = ["XmlRpcIngressShell"]
