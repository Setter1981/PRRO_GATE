from __future__ import annotations

from xmlrpc.server import SimpleXMLRPCServer

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.xmlrpc_shell import XmlRpcIngressShell
from prro_gateway.services.ingress import IngressAcceptService


def main() -> None:
    config = AppConfig.from_env()
    container = RuntimeContainer(config)
    container.initialize()
    shell = XmlRpcIngressShell(container)

    server = SimpleXMLRPCServer((config.ingress.xmlrpc.host, config.ingress.xmlrpc.port), allow_none=True, logRequests=False)

    def handle(method: str, params: dict, context: dict | None = None):
        return shell.handle_call(method=method, params=params, context=context or IngressAcceptService.build_context(
            fiscal_number=config.defaults.fiscal_number,
            backend_profile_id=config.defaults.backend_profile_id,
            transport_profile_id=config.defaults.transport_profile_id,
            channel_owner=config.defaults.channel_owner,
        ))

    server.register_function(handle, "handle")
    server.serve_forever()


if __name__ == "__main__":
    main()
