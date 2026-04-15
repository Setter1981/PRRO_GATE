from __future__ import annotations

import asyncio
import json

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.maria_shell import MariaIngressShell
from prro_gateway.services.ingress import IngressAcceptService


async def main() -> None:
    config = AppConfig.from_env()
    container = RuntimeContainer(config)
    container.initialize()
    shell = MariaIngressShell(container)

    async def handle(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        while not reader.at_eof():
            line = await reader.readline()
            if not line:
                break
            envelope = json.loads(line.decode("utf-8"))
            response = shell.handle_envelope(
                command_name=envelope["command"],
                fields=envelope.get("fields", {}),
                context=envelope.get("context") or IngressAcceptService.build_context(
                    fiscal_number=config.defaults.fiscal_number,
                    backend_profile_id=config.defaults.backend_profile_id,
                    transport_profile_id=config.defaults.transport_profile_id,
                    channel_owner=config.defaults.channel_owner,
                ),
            )
            writer.write((json.dumps(response) + "\n").encode("utf-8"))
            await writer.drain()
        writer.close()
        await writer.wait_closed()

    server = await asyncio.start_server(handle, config.ingress.maria.host, config.ingress.maria.port)
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    asyncio.run(main())
