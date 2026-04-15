from __future__ import annotations

import uvicorn

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app


def main() -> None:
    config = AppConfig.from_env()
    container = RuntimeContainer(config)
    app = create_app(container)
    uvicorn.run(app, host=config.ingress.rest.host, port=config.ingress.rest.port)


if __name__ == "__main__":
    main()
