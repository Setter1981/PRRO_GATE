# Maria 304 Driver — Installation

Covers a single-host Linux install where the driver and the Python
PRRO Gateway run on the same machine (typical for a retail point
with 1–5 cash registers).  For a fleet / shop-server topology see
`docs/maria304/1C_SETUP.md § Centralised`.

## 1. Build the binary

```bash
cd rust
cargo build --release -p maria304_driver --bin maria304_driver
```

The binary lands at `target/release/maria304_driver`.  Copy it to
`/opt/maria304/maria304_driver` and `chmod +x` it.

## 2. Create the service user

```bash
sudo useradd --system --home-dir /opt/maria304 --shell /usr/sbin/nologin maria304
sudo mkdir -p /opt/maria304 /etc/maria304_driver /var/log/maria304
sudo chown maria304:maria304 /var/log/maria304
```

## 3. Drop the config

```bash
sudo cp deployment/maria304/config.example.yaml /etc/maria304_driver/config.yaml
sudoedit /etc/maria304_driver/config.yaml
```

Required edits:

* `bridge.gateway_url` — usually `http://127.0.0.1:8000/v1/ingress/maria304`.
* `bridge.shared_token` — shared secret with the Python gateway (env
  substitution `${MARIA304_BRIDGE_TOKEN}` works).
* `listeners[].fiscal_number` + `bind` — one entry per cash register.

## 4. Install the systemd unit

```bash
sudo cp deployment/maria304/maria304_driver.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now maria304_driver
```

## 5. Verify

```bash
# Liveness.
curl -sS http://127.0.0.1:9202/admin/health
# → OK

# Listener list (token required if configured).
curl -sS -H "Authorization: Bearer $MARIA304_ADMIN_TOKEN" \
     http://127.0.0.1:9202/admin/fns

# Journal.
journalctl -u maria304_driver -f
```

## 6. Docker (optional)

```bash
docker build -t maria304_driver -f deployment/maria304/Dockerfile .
docker run --rm --network host \
  -v /etc/maria304_driver:/etc/maria304_driver:ro \
  -e MARIA304_BRIDGE_TOKEN=... \
  -e MARIA304_ADMIN_TOKEN=... \
  maria304_driver
```

Alpine image is ~15 MB.
