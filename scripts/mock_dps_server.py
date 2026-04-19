"""
Mock DPS server for benchmarking PRRO gateway transport layer.

Starts two servers in one process:
- HTTPS REST on :8443 (aiohttp)
- gRPC on :9443 (grpcio)

Configurable via env vars:
  MOCK_DPS_LATENCY_MS=100
  MOCK_DPS_JITTER_PCT=10

Or via CLI flags (override env vars):
  --latency-ms 100 --jitter 0.1 --rest-port 8443 --grpc-port 9443
"""

from __future__ import annotations

import argparse
import asyncio
import json
import logging
import math
import os
import random
import statistics
import sys
import tempfile
import time
import uuid
from collections import deque
from concurrent import futures
from datetime import datetime, timezone
from typing import Any

import aiohttp
from aiohttp import web

# ---------------------------------------------------------------------------
# Structured logger
# ---------------------------------------------------------------------------
logging.basicConfig(
    format='{"ts":"%(asctime)s","level":"%(levelname)s","msg":%(message)s}',
    datefmt="%Y-%m-%dT%H:%M:%S",
    level=logging.INFO,
)
log = logging.getLogger("mock_dps")


# ---------------------------------------------------------------------------
# Shared mutable state (protected by asyncio event loop for REST,
# thread-lock for gRPC)
# ---------------------------------------------------------------------------
class ServerState:
    def __init__(self, latency_ms: float, jitter_pct: float) -> None:
        self.latency_ms = latency_ms
        self.jitter_pct = jitter_pct
        self.total_requests = 0
        # Rolling window of handler durations in ms (last 10000 requests)
        self._durations: deque[float] = deque(maxlen=10000)
        self._lock = asyncio.Lock()

    async def record(self, duration_ms: float) -> None:
        async with self._lock:
            self.total_requests += 1
            self._durations.append(duration_ms)

    def record_sync(self, duration_ms: float) -> None:
        """Thread-safe record for gRPC (called from threadpool)."""
        self.total_requests += 1
        self._durations.append(duration_ms)

    def stats(self) -> dict[str, Any]:
        if not self._durations:
            return {"total_requests": self.total_requests, "p50_ms": 0, "p95_ms": 0}
        sorted_d = sorted(self._durations)
        n = len(sorted_d)

        def percentile(p: float) -> float:
            idx = max(0, min(n - 1, int(math.ceil(p / 100.0 * n)) - 1))
            return round(sorted_d[idx], 3)

        return {
            "total_requests": self.total_requests,
            "p50_ms": percentile(50),
            "p95_ms": percentile(95),
            "p99_ms": percentile(99),
            "mean_ms": round(statistics.mean(sorted_d), 3),
            "min_ms": round(sorted_d[0], 3),
            "max_ms": round(sorted_d[-1], 3),
        }

    def effective_latency_ms(self) -> float:
        """Return the jittered latency in ms for a single request."""
        jitter_fraction = self.jitter_pct / 100.0
        factor = 1.0 + random.uniform(-jitter_fraction, jitter_fraction)
        return max(0.0, self.latency_ms * factor)


# ---------------------------------------------------------------------------
# TLS certificate generation
# ---------------------------------------------------------------------------
def _generate_tls_cert(tmpdir: str) -> tuple[str, str]:
    """
    Generate a self-signed cert+key pair.
    Tries trustme first, falls back to openssl subprocess.
    Returns (cert_path, key_path).
    """
    cert_path = os.path.join(tmpdir, "server.pem")
    key_path = os.path.join(tmpdir, "server.key")
    ca_path = os.path.join(tmpdir, "ca.pem")

    try:
        import trustme  # type: ignore[import-not-found]
        ca = trustme.CA()
        server_cert = ca.issue_cert("localhost", "127.0.0.1")
        ca.cert_pem.write_to_path(ca_path)
        with open(cert_path, "wb") as f:
            for blob in server_cert.cert_chain_pems:
                f.write(blob.bytes())
        server_cert.private_key_pem.write_to_path(key_path)
        log.info('"Generated TLS cert via trustme"')
        return cert_path, key_path
    except ImportError:
        pass

    # Fallback: openssl
    import subprocess
    result = subprocess.run(
        [
            "openssl", "req", "-x509", "-newkey", "rsa:2048",
            "-keyout", key_path, "-out", cert_path,
            "-days", "1", "-nodes",
            "-subj", "/CN=localhost",
        ],
        capture_output=True, timeout=30,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"openssl cert generation failed: {result.stderr.decode()}"
        )
    log.info('"Generated TLS cert via openssl"')
    return cert_path, key_path


# ---------------------------------------------------------------------------
# gRPC servicer — generated inline to avoid needing grpc_tools on import
# ---------------------------------------------------------------------------
def _build_grpc_servicer(state: ServerState):
    """Dynamically compile proto and return (servicer_class, pb2, pb2_grpc)."""
    import os
    import sys
    import importlib.util

    proto_path = os.path.join(os.path.dirname(__file__), "mock_dps.proto")
    out_dir = tempfile.mkdtemp(prefix="mock_dps_grpc_")

    try:
        from grpc_tools import protoc  # type: ignore[import-not-found]
        ret = protoc.main([
            "grpc_tools.protoc",
            f"--proto_path={os.path.dirname(proto_path)}",
            f"--python_out={out_dir}",
            f"--grpc_python_out={out_dir}",
            proto_path,
        ])
        if ret != 0:
            raise RuntimeError(f"protoc compilation failed with code {ret}")
    except ImportError:
        raise RuntimeError(
            "grpcio-tools is required for gRPC mock. Install with: pip install grpcio-tools"
        )

    sys.path.insert(0, out_dir)
    pb2 = importlib.import_module("mock_dps_pb2")
    pb2_grpc = importlib.import_module("mock_dps_pb2_grpc")

    class FiscalServiceServicer(pb2_grpc.FiscalServiceServicer):  # type: ignore[misc]
        def Submit(self, request, context):
            t_start = time.perf_counter()
            latency = state.effective_latency_ms()
            if latency > 0:
                time.sleep(latency / 1000.0)

            fiscal_code = f"DPS-{uuid.uuid4().hex[:12].upper()}"
            ts_now = datetime.now(timezone.utc).isoformat()

            t_total_ms = (time.perf_counter() - t_start) * 1000.0
            state.record_sync(t_total_ms)

            log.info(
                json.dumps({
                    "proto": "grpc",
                    "latency_applied_ms": round(latency, 3),
                    "total_handler_ms": round(t_total_ms, 3),
                })
            )
            return pb2.ReceiptAck(
                status="ok",
                fiscal_code=fiscal_code,
                timestamp=ts_now,
                error_message="",
            )

    return FiscalServiceServicer, pb2, pb2_grpc


# ---------------------------------------------------------------------------
# REST handlers (aiohttp)
# ---------------------------------------------------------------------------
async def handle_post_receipts(request: web.Request) -> web.Response:
    state: ServerState = request.app["state"]
    t_start = time.perf_counter()

    latency = state.effective_latency_ms()
    if latency > 0:
        await asyncio.sleep(latency / 1000.0)

    fiscal_code = f"DPS-{uuid.uuid4().hex[:12].upper()}"
    ts_now = datetime.now(timezone.utc).isoformat()

    t_total_ms = (time.perf_counter() - t_start) * 1000.0
    await state.record(t_total_ms)

    log.info(
        json.dumps({
            "proto": "rest",
            "latency_applied_ms": round(latency, 3),
            "total_handler_ms": round(t_total_ms, 3),
        })
    )
    return web.json_response({
        "status": "ok",
        "fiscal_code": fiscal_code,
        "timestamp": ts_now,
    })


async def handle_get_stats(request: web.Request) -> web.Response:
    state: ServerState = request.app["state"]
    return web.json_response(state.stats())


async def handle_post_config(request: web.Request) -> web.Response:
    """Runtime config endpoint: POST /config {"latency_ms": 200, "jitter_pct": 5}"""
    state: ServerState = request.app["state"]
    try:
        body = await request.json()
    except Exception:
        return web.json_response({"error": "invalid JSON"}, status=400)

    if "latency_ms" in body:
        state.latency_ms = float(body["latency_ms"])
    if "jitter_pct" in body:
        state.jitter_pct = float(body["jitter_pct"])

    log.info(
        json.dumps({
            "proto": "rest",
            "config_update": {
                "latency_ms": state.latency_ms,
                "jitter_pct": state.jitter_pct,
            },
        })
    )
    return web.json_response({
        "latency_ms": state.latency_ms,
        "jitter_pct": state.jitter_pct,
    })


# ---------------------------------------------------------------------------
# Server startup
# ---------------------------------------------------------------------------
def build_rest_app(state: ServerState) -> web.Application:
    app = web.Application()
    app["state"] = state
    app.router.add_post("/api/v2/receipts", handle_post_receipts)
    app.router.add_get("/stats", handle_get_stats)
    app.router.add_post("/config", handle_post_config)
    return app


async def run_rest_server(
    state: ServerState,
    cert_path: str,
    key_path: str,
    host: str = "0.0.0.0",
    port: int = 8443,
) -> None:
    import ssl
    ssl_ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ssl_ctx.load_cert_chain(cert_path, key_path)

    app = build_rest_app(state)
    runner = web.AppRunner(app)
    await runner.setup()
    site = web.TCPSite(runner, host, port, ssl_context=ssl_ctx)
    await site.start()
    log.info(json.dumps({"proto": "rest", "event": "listening", "host": host, "port": port}))
    return runner


def run_grpc_server(
    state: ServerState,
    port: int = 9443,
) -> Any:
    import grpc  # type: ignore[import-not-found]

    try:
        FiscalServiceServicer, pb2, pb2_grpc = _build_grpc_servicer(state)
    except RuntimeError as exc:
        log.warning(json.dumps({"proto": "grpc", "event": "disabled", "reason": str(exc)}))
        return None

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=10))
    pb2_grpc.add_FiscalServiceServicer_to_server(FiscalServiceServicer(), server)
    server.add_insecure_port(f"[::]:{port}")
    server.start()
    log.info(json.dumps({"proto": "grpc", "event": "listening", "port": port}))
    return server


async def main(args: argparse.Namespace) -> None:
    state = ServerState(
        latency_ms=args.latency_ms,
        jitter_pct=args.jitter * 100.0,  # convert 0.1 -> 10%
    )

    with tempfile.TemporaryDirectory() as tmpdir:
        cert_path, key_path = _generate_tls_cert(tmpdir)

        # Start gRPC in background thread
        grpc_server = run_grpc_server(state, port=args.grpc_port)

        # Start REST
        rest_runner = await run_rest_server(
            state, cert_path, key_path,
            port=args.rest_port,
        )

        log.info(json.dumps({
            "event": "ready",
            "rest_port": args.rest_port,
            "grpc_port": args.grpc_port,
            "latency_ms": args.latency_ms,
            "jitter_pct": state.jitter_pct,
        }))

        try:
            # Run until interrupted
            while True:
                await asyncio.sleep(3600)
        except (KeyboardInterrupt, asyncio.CancelledError):
            pass
        finally:
            log.info('"Shutting down mock DPS server"')
            await rest_runner.cleanup()
            if grpc_server is not None:
                grpc_server.stop(grace=2)


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Mock DPS server for PRRO gateway benchmarking")
    p.add_argument("--latency-ms", type=float,
                   default=float(os.environ.get("MOCK_DPS_LATENCY_MS", "100")))
    p.add_argument("--jitter", type=float,
                   default=float(os.environ.get("MOCK_DPS_JITTER_PCT", "10")) / 100.0,
                   help="Jitter as fraction 0.0-1.0 (0.1 = 10%)")
    p.add_argument("--rest-port", type=int, default=8443)
    p.add_argument("--grpc-port", type=int, default=9443)
    return p.parse_args()


if __name__ == "__main__":
    args = parse_args()
    asyncio.run(main(args))
