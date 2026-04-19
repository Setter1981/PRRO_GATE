"""
Benchmark driver for PRRO gateway transport layer.

Runs the 12 Phase B configurations against the mock DPS server and outputs:
- JSONL file:  bench_results/{timestamp}/raw.jsonl    (every request)
- Summary CSV: bench_results/{timestamp}/summary.csv  (one row per matrix entry)
- Pretty-printed summary to stdout

Usage:
    python scripts/bench_transport.py
    python scripts/bench_transport.py --only B1
    python scripts/bench_transport.py --only B1 B3 --out bench_results/smoke/
    python scripts/bench_transport.py --rest-url https://localhost:8443 --grpc-host localhost --grpc-port 9443
"""

from __future__ import annotations

import argparse
import asyncio
import csv
import importlib
import json
import math
import os
import statistics
import sys
import tempfile
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Test matrix
# ---------------------------------------------------------------------------
TEST_MATRIX = [
    # (id, protocol, client_mode, concurrency, latency_ms, n_requests)
    ("B1",  "rest", "no_keepalive",    1,   100, 100),
    ("B2",  "rest", "keepalive",       1,   100, 100),
    ("B3",  "rest", "async_pool",      4,   100, 100),
    ("B4",  "rest", "async_pool",      16,  100, 100),
    ("B5",  "grpc", "new_channel",     1,   100, 100),
    ("B6",  "grpc", "shared_channel",  1,   100, 100),
    ("B7",  "grpc", "concurrent",      4,   100, 100),
    ("B8",  "grpc", "concurrent",      16,  100, 100),
    ("B9",  "rest", "async_pool",      4,   0,   100),
    ("B10", "grpc", "concurrent",      4,   0,   100),
    ("B11", "rest", "async_pool",      4,   500, 100),
    ("B12", "grpc", "concurrent",      4,   500, 100),
]

WARMUP_REQUESTS = 10


# ---------------------------------------------------------------------------
# Sample receipt payload (canned signed payload — bench measures HTTP layer)
# ---------------------------------------------------------------------------
_CANNED_PAYLOAD = json.dumps({
    "fiscal_number": "9999999999",
    "idempotency_key": "bench-fixture",
    "receipt_type": "SELL",
    "payload": "canned_signed_payload_for_benchmarking_only",
}).encode()


def _make_payload() -> dict[str, Any]:
    return {
        "fiscal_number": "9999999999",
        "idempotency_key": str(uuid.uuid4()),
        "receipt_type": "SELL",
        "payload": "canned_signed_payload_for_benchmarking_only",
    }


# ---------------------------------------------------------------------------
# gRPC stub compilation — done once, cached in module-level globals
# ---------------------------------------------------------------------------
_GRPC_PB2: Any = None
_GRPC_PB2_GRPC: Any = None
_GRPC_COMPILE_ERROR: str | None = None


def _ensure_grpc_compiled() -> tuple[Any, Any]:
    """Compile mock_dps.proto once and return (pb2, pb2_grpc). Raises on failure."""
    global _GRPC_PB2, _GRPC_PB2_GRPC, _GRPC_COMPILE_ERROR

    if _GRPC_PB2 is not None:
        return _GRPC_PB2, _GRPC_PB2_GRPC

    if _GRPC_COMPILE_ERROR is not None:
        raise RuntimeError(_GRPC_COMPILE_ERROR)

    proto_path = Path(__file__).parent / "mock_dps.proto"
    out_dir = tempfile.mkdtemp(prefix="mock_dps_grpc_bench_")

    try:
        from grpc_tools import protoc  # type: ignore[import-not-found]
    except ImportError as e:
        _GRPC_COMPILE_ERROR = f"grpcio-tools not installed: {e}"
        raise RuntimeError(_GRPC_COMPILE_ERROR)

    ret = protoc.main([
        "grpc_tools.protoc",
        f"--proto_path={proto_path.parent}",
        f"--python_out={out_dir}",
        f"--grpc_python_out={out_dir}",
        str(proto_path),
    ])
    if ret != 0:
        _GRPC_COMPILE_ERROR = f"protoc compilation failed with code {ret}"
        raise RuntimeError(_GRPC_COMPILE_ERROR)

    if out_dir not in sys.path:
        sys.path.insert(0, out_dir)

    _GRPC_PB2 = importlib.import_module("mock_dps_pb2")
    _GRPC_PB2_GRPC = importlib.import_module("mock_dps_pb2_grpc")
    return _GRPC_PB2, _GRPC_PB2_GRPC


# ---------------------------------------------------------------------------
# Config helper — POST /config to update mock server latency at runtime
# ---------------------------------------------------------------------------
def _config_base_url(rest_receipts_url: str) -> str:
    """Derive /config URL from the receipts URL."""
    # https://localhost:8443/api/v2/receipts  ->  https://localhost:8443/config
    from urllib.parse import urlparse, urlunparse
    p = urlparse(rest_receipts_url)
    return urlunparse((p.scheme, p.netloc, "/config", "", "", ""))


async def _update_mock_config(
    rest_url: str,
    latency_ms: float,
    jitter_pct: float = 5.0,
) -> None:
    """POST /config to mock server to change latency without restart."""
    import httpx
    config_url = _config_base_url(rest_url)
    try:
        async with httpx.AsyncClient(verify=False, timeout=5.0) as client:
            resp = await client.post(config_url, json={"latency_ms": latency_ms, "jitter_pct": jitter_pct})
            if resp.status_code != 200:
                print(f"  [warn] /config returned {resp.status_code}: {resp.text[:200]}", file=sys.stderr)
    except Exception as exc:
        print(f"  [warn] could not update mock config: {exc}", file=sys.stderr)


# ---------------------------------------------------------------------------
# REST benchmark functions
# ---------------------------------------------------------------------------
async def _rest_no_keepalive(url: str, n: int) -> list[dict[str, Any]]:
    """New httpx.Client per request (sync in executor)."""
    import httpx
    results = []
    loop = asyncio.get_running_loop()

    def _one_request() -> dict[str, Any]:
        t0 = time.perf_counter()
        try:
            with httpx.Client(verify=False, timeout=30.0) as client:
                t_connect = time.perf_counter()
                r = client.post(url, json=_make_payload())
                t_done = time.perf_counter()
                return {
                    "t_connect": round((t_connect - t0) * 1000, 3),
                    "t_tls": None,
                    "t_request": round((t_done - t_connect) * 1000, 3),
                    "t_response": None,
                    "t_total": round((t_done - t0) * 1000, 3),
                    "status": r.status_code,
                }
        except Exception as exc:
            t_done = time.perf_counter()
            return {
                "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                "t_total": round((t_done - t0) * 1000, 3),
                "status": f"error:{exc.__class__.__name__}",
            }

    for _ in range(n):
        result = await loop.run_in_executor(None, _one_request)
        results.append(result)
    return results


async def _rest_keepalive(url: str, n: int) -> list[dict[str, Any]]:
    """One httpx.AsyncClient with keep-alive, sequential requests."""
    import httpx
    results = []
    async with httpx.AsyncClient(
        verify=False,
        timeout=30.0,
        limits=httpx.Limits(max_keepalive_connections=10, max_connections=20),
    ) as client:
        for _ in range(n):
            t0 = time.perf_counter()
            try:
                r = await client.post(url, json=_make_payload())
                t_done = time.perf_counter()
                results.append({
                    "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                    "t_total": round((t_done - t0) * 1000, 3),
                    "status": r.status_code,
                })
            except Exception as exc:
                t_done = time.perf_counter()
                results.append({
                    "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                    "t_total": round((t_done - t0) * 1000, 3),
                    "status": f"error:{exc.__class__.__name__}",
                })
    return results


async def _rest_async_pool(url: str, n: int, concurrency: int) -> list[dict[str, Any]]:
    """httpx.AsyncClient + asyncio.gather with semaphore."""
    import httpx
    sem = asyncio.Semaphore(concurrency)
    results: list[dict[str, Any]] = [{}] * n

    async with httpx.AsyncClient(
        verify=False,
        timeout=30.0,
        limits=httpx.Limits(max_keepalive_connections=concurrency * 2, max_connections=concurrency * 4),
    ) as client:
        async def _one(i: int) -> None:
            async with sem:
                t0 = time.perf_counter()
                try:
                    r = await client.post(url, json=_make_payload())
                    t_done = time.perf_counter()
                    results[i] = {
                        "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                        "t_total": round((t_done - t0) * 1000, 3),
                        "status": r.status_code,
                    }
                except Exception as exc:
                    t_done = time.perf_counter()
                    results[i] = {
                        "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                        "t_total": round((t_done - t0) * 1000, 3),
                        "status": f"error:{exc.__class__.__name__}",
                    }

        await asyncio.gather(*(_one(i) for i in range(n)))
    return results


# ---------------------------------------------------------------------------
# gRPC benchmark functions
# ---------------------------------------------------------------------------
def _grpc_new_channel(host: str, port: int, n: int) -> list[dict[str, Any]]:
    """New insecure channel per request (sync)."""
    import grpc  # type: ignore[import-not-found]

    try:
        pb2, pb2_grpc = _ensure_grpc_compiled()
    except Exception as exc:
        return [{"t_total": 0.0, "status": f"grpc_unavailable:{exc}"}] * n

    results = []
    for _ in range(n):
        t0 = time.perf_counter()
        try:
            channel = grpc.insecure_channel(f"{host}:{port}")
            stub = pb2_grpc.FiscalServiceStub(channel)
            req = pb2.Receipt(
                fiscal_number="9999999999",
                idempotency_key=str(uuid.uuid4()),
                receipt_type="SELL",
                payload=_CANNED_PAYLOAD,
            )
            resp = stub.Submit(req, timeout=30)
            t_done = time.perf_counter()
            channel.close()
            results.append({
                "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                "t_total": round((t_done - t0) * 1000, 3),
                "status": resp.status,
            })
        except Exception as exc:
            t_done = time.perf_counter()
            results.append({
                "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                "t_total": round((t_done - t0) * 1000, 3),
                "status": f"error:{exc.__class__.__name__}",
            })
    return results


def _grpc_shared_channel(host: str, port: int, n: int) -> list[dict[str, Any]]:
    """One channel reused for all sequential requests."""
    import grpc  # type: ignore[import-not-found]

    try:
        pb2, pb2_grpc = _ensure_grpc_compiled()
    except Exception as exc:
        return [{"t_total": 0.0, "status": f"grpc_unavailable:{exc}"}] * n

    results = []
    channel = grpc.insecure_channel(f"{host}:{port}")
    stub = pb2_grpc.FiscalServiceStub(channel)
    try:
        for _ in range(n):
            t0 = time.perf_counter()
            try:
                req = pb2.Receipt(
                    fiscal_number="9999999999",
                    idempotency_key=str(uuid.uuid4()),
                    receipt_type="SELL",
                    payload=_CANNED_PAYLOAD,
                )
                resp = stub.Submit(req, timeout=30)
                t_done = time.perf_counter()
                results.append({
                    "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                    "t_total": round((t_done - t0) * 1000, 3),
                    "status": resp.status,
                })
            except Exception as exc:
                t_done = time.perf_counter()
                results.append({
                    "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                    "t_total": round((t_done - t0) * 1000, 3),
                    "status": f"error:{exc.__class__.__name__}",
                })
    finally:
        channel.close()
    return results


async def _grpc_concurrent(host: str, port: int, n: int, concurrency: int) -> list[dict[str, Any]]:
    """asyncio.gather with N concurrent unary calls on shared channel."""
    import grpc  # type: ignore[import-not-found]

    try:
        pb2, pb2_grpc = _ensure_grpc_compiled()
    except Exception as exc:
        return [{"t_total": 0.0, "status": f"grpc_unavailable:{exc}"}] * n

    results: list[dict[str, Any]] = [{}] * n
    sem = asyncio.Semaphore(concurrency)
    channel = grpc.insecure_channel(f"{host}:{port}")
    stub = pb2_grpc.FiscalServiceStub(channel)
    loop = asyncio.get_running_loop()

    def _call_sync(i: int) -> None:
        t0 = time.perf_counter()
        try:
            req = pb2.Receipt(
                fiscal_number="9999999999",
                idempotency_key=str(uuid.uuid4()),
                receipt_type="SELL",
                payload=_CANNED_PAYLOAD,
            )
            resp = stub.Submit(req, timeout=30)
            t_done = time.perf_counter()
            results[i] = {
                "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                "t_total": round((t_done - t0) * 1000, 3),
                "status": resp.status,
            }
        except Exception as exc:
            t_done = time.perf_counter()
            results[i] = {
                "t_connect": None, "t_tls": None, "t_request": None, "t_response": None,
                "t_total": round((t_done - t0) * 1000, 3),
                "status": f"error:{exc.__class__.__name__}",
            }

    async def _one(i: int) -> None:
        async with sem:
            await loop.run_in_executor(None, _call_sync, i)

    try:
        await asyncio.gather(*(_one(i) for i in range(n)))
    finally:
        channel.close()
    return results


# ---------------------------------------------------------------------------
# Dispatch by client_mode
# ---------------------------------------------------------------------------
async def run_mode(
    row: tuple,
    rest_url: str,
    grpc_host: str,
    grpc_port: int,
    n_requests: int,
    warmup: bool = False,
) -> list[dict[str, Any]]:
    _id, protocol, client_mode, concurrency, latency_ms, _n = row
    n = WARMUP_REQUESTS if warmup else n_requests

    if protocol == "rest":
        if client_mode == "no_keepalive":
            return await _rest_no_keepalive(rest_url, n)
        elif client_mode == "keepalive":
            return await _rest_keepalive(rest_url, n)
        elif client_mode == "async_pool":
            return await _rest_async_pool(rest_url, n, concurrency)
        else:
            raise ValueError(f"Unknown REST client_mode: {client_mode}")
    elif protocol == "grpc":
        loop = asyncio.get_running_loop()
        if client_mode == "new_channel":
            return await loop.run_in_executor(None, _grpc_new_channel, grpc_host, grpc_port, n)
        elif client_mode == "shared_channel":
            return await loop.run_in_executor(None, _grpc_shared_channel, grpc_host, grpc_port, n)
        elif client_mode == "concurrent":
            return await _grpc_concurrent(grpc_host, grpc_port, n, concurrency)
        else:
            raise ValueError(f"Unknown gRPC client_mode: {client_mode}")
    else:
        raise ValueError(f"Unknown protocol: {protocol}")


# ---------------------------------------------------------------------------
# Statistics
# ---------------------------------------------------------------------------
def compute_stats(results: list[dict[str, Any]]) -> dict[str, Any]:
    totals = [r["t_total"] for r in results if isinstance(r.get("t_total"), (int, float))]
    errors = sum(
        1 for r in results
        if not isinstance(r.get("t_total"), (int, float))
        or str(r.get("status", "")).startswith("error:")
        or str(r.get("status", "")).startswith("grpc_unavailable:")
    )

    if not totals:
        return {"n": 0, "errors": len(results), "p50": 0, "p95": 0, "p99": 0,
                "mean": 0, "stddev": 0, "min": 0, "max": 0}

    sorted_t = sorted(totals)
    n = len(sorted_t)

    def pct(p: float) -> float:
        idx = max(0, min(n - 1, int(math.ceil(p / 100.0 * n)) - 1))
        return round(sorted_t[idx], 3)

    return {
        "n": n,
        "errors": errors,
        "p50": pct(50),
        "p95": pct(95),
        "p99": pct(99),
        "mean": round(statistics.mean(sorted_t), 3),
        "stddev": round(statistics.stdev(sorted_t), 3) if n > 1 else 0.0,
        "min": round(sorted_t[0], 3),
        "max": round(sorted_t[-1], 3),
    }


# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------
def _write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    with open(path, "a", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row) + "\n")


CSV_FIELDS = [
    "id", "protocol", "client_mode", "concurrency", "latency_ms", "n_requests",
    "n_measured", "errors", "p50_ms", "p95_ms", "p99_ms",
    "mean_ms", "stddev_ms", "min_ms", "max_ms",
]


def _write_csv_header(path: Path) -> None:
    with open(path, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=CSV_FIELDS)
        writer.writeheader()


def _append_csv_row(path: Path, row: dict[str, Any]) -> None:
    with open(path, "a", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=CSV_FIELDS)
        writer.writerow(row)


def _pretty_print_row(row: dict[str, Any]) -> None:
    print(
        f"  {row['id']:4s}  {row['protocol']:4s}/{row['client_mode']:16s}"
        f"  c={row['concurrency']:2d}  lat={row['latency_ms']:4.0f}ms"
        f"  p50={row['p50_ms']:7.1f}ms  p95={row['p95_ms']:7.1f}ms"
        f"  p99={row['p99_ms']:7.1f}ms  mean={row['mean_ms']:7.1f}ms"
        f"  err={row['errors']:3d}/{row['n_requests']}"
    )


# ---------------------------------------------------------------------------
# Main benchmark loop
# ---------------------------------------------------------------------------
async def run_benchmark(
    matrix: list[tuple],
    rest_url: str,
    grpc_host: str,
    grpc_port: int,
    out_dir: Path,
) -> None:
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = out_dir / ts
    run_dir.mkdir(parents=True, exist_ok=True)

    raw_path = run_dir / "raw.jsonl"
    csv_path = run_dir / "summary.csv"
    _write_csv_header(csv_path)

    print(f"\nBenchmark run: {ts}")
    print(f"  REST URL:   {rest_url}")
    print(f"  gRPC:       {grpc_host}:{grpc_port}")
    print(f"  Output dir: {run_dir}\n")
    print(f"  {'ID':4s}  {'proto/mode':21s}  {'conc':4s}  {'lat':7s}  {'p50':10s}  {'p95':10s}  {'p99':10s}  {'mean':10s}  {'errors':8s}")
    print("  " + "-" * 110)

    for row in matrix:
        bench_id, protocol, client_mode, concurrency, latency_ms, n_requests = row

        # Update mock server latency before this run
        print(f"\n  [{bench_id}] updating mock latency to {latency_ms}ms ...")
        await _update_mock_config(rest_url, latency_ms)
        await asyncio.sleep(0.1)  # brief pause for config to take effect

        # Warm-up (discarded)
        print(f"  [{bench_id}] warming up ({WARMUP_REQUESTS} requests) ...")
        try:
            await run_mode(row, rest_url, grpc_host, grpc_port, WARMUP_REQUESTS, warmup=True)
        except Exception as exc:
            print(f"  [{bench_id}] warmup failed: {exc}", file=sys.stderr)

        # Measured run
        print(f"  [{bench_id}] measuring ({n_requests} requests) ...")
        t_wall_start = time.perf_counter()
        try:
            raw_results = await run_mode(row, rest_url, grpc_host, grpc_port, n_requests)
        except Exception as exc:
            print(f"  [{bench_id}] FAILED: {exc}", file=sys.stderr)
            raw_results = [{"t_total": None, "status": f"exception:{exc}"}] * n_requests

        _t_wall_ms = (time.perf_counter() - t_wall_start) * 1000.0

        stats = compute_stats(raw_results)

        # Annotate raw results for JSONL output
        annotated = [
            {
                "bench_id": bench_id,
                "protocol": protocol,
                "client_mode": client_mode,
                "concurrency": concurrency,
                "latency_ms": latency_ms,
                "req_index": i,
                **r,
            }
            for i, r in enumerate(raw_results)
        ]
        _write_jsonl(raw_path, annotated)

        csv_row = {
            "id": bench_id,
            "protocol": protocol,
            "client_mode": client_mode,
            "concurrency": concurrency,
            "latency_ms": latency_ms,
            "n_requests": n_requests,
            "n_measured": stats["n"],
            "errors": stats["errors"],
            "p50_ms": stats["p50"],
            "p95_ms": stats["p95"],
            "p99_ms": stats["p99"],
            "mean_ms": stats["mean"],
            "stddev_ms": stats["stddev"],
            "min_ms": stats["min"],
            "max_ms": stats["max"],
        }
        _append_csv_row(csv_path, csv_row)
        _pretty_print_row(csv_row)

    print(f"\n  Summary CSV: {csv_path}")
    print(f"  Raw JSONL:   {raw_path}\n")


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------
def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="PRRO transport benchmark driver")
    p.add_argument("--only", nargs="+", metavar="ID",
                   help="Run only specified test IDs (e.g. --only B1 B3)")
    p.add_argument("--out", default="bench_results",
                   help="Output directory (default: bench_results/)")
    p.add_argument("--rest-url", default="https://localhost:8443/api/v2/receipts",
                   help="Mock DPS REST receipts URL")
    p.add_argument("--grpc-host", default="localhost")
    p.add_argument("--grpc-port", type=int, default=9443)
    return p.parse_args()


def main() -> None:
    args = parse_args()

    matrix = TEST_MATRIX
    if args.only:
        filter_ids = set(args.only)
        matrix = [row for row in TEST_MATRIX if row[0] in filter_ids]
        if not matrix:
            print(f"ERROR: No matching test IDs in {filter_ids}", file=sys.stderr)
            sys.exit(1)

    out_dir = Path(args.out)

    asyncio.run(run_benchmark(
        matrix,
        rest_url=args.rest_url,
        grpc_host=args.grpc_host,
        grpc_port=args.grpc_port,
        out_dir=out_dir,
    ))


if __name__ == "__main__":
    main()
