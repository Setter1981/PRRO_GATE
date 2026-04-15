# INSTALL

## Local virtualenv

```bash
python -m venv .venv
source .venv/bin/activate
pip install -r requirements-dev.txt
pip install -e .
cp ops/config.example.yaml ./config.yaml
PRRO_GATEWAY_CONFIG=./config.yaml python scripts/run_rest.py
```

## Docker

```bash
docker build -t prro-gateway:rc1 .
docker compose up --build
```

## systemd

Copy `ops/systemd/prro-gateway-rest.service` to `/etc/systemd/system/` and adjust `WorkingDirectory`, `Environment=PRRO_GATEWAY_CONFIG=...`, and the Python executable path.
