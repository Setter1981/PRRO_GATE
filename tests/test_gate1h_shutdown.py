"""
Gate 1h — graceful shutdown / drain smoke test.

Two scenarios:

  Scenario A — direct counter drain (unit-level):
    Simulate an in-flight operation via _begin_operation().
    Schedule _end_operation() via Timer (50ms).
    Call shutdown() — must drain (return promptly) once the counter drops.
    Existing test_shutdown_drains_active_operations covers the TIMEOUT case.
    This test covers the SUCCESS drain case: counter drops → shutdown returns.

  Scenario B — full write-path drain (REST integration):
    SELL enters the transport send stage (mock blocks on an Event).
    While active_operations == 1, TestClient lifespan exits → shutdown() starts.
    Timer fires (100ms) → mock unblocks → send completes → finalize_ack writes ACK
    → _end_operation() → active_operations drops to 0 → shutdown() returns.
    After shutdown:
      - No inbox record stuck in PROCESSING
      - SELL document is in ACK state (operation completed, not abandoned)
      - active_operations == 0

Both scenarios prove Invariant #9: graceful shutdown waits for in-flight write-path
operations before returning.

Invariants preserved:
  #1 (no network in tx): write-path transaction structure unchanged.
  #9 (graceful shutdown): directly verified in both scenarios.
"""
from __future__ import annotations

import threading
import time
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


# ---------------------------------------------------------------------------
# Scenario A — direct counter drain
# ---------------------------------------------------------------------------

def test_gate1h_shutdown_drains_when_operation_completes(tmp_path: Path) -> None:
    """
    shutdown() must return promptly once in-flight operations complete.

    Pattern:
      1. _begin_operation() → active_operations == 1
      2. Timer(50ms) → _end_operation() → active_operations == 0
      3. shutdown(timeout=5s) → drains in ~50ms, NOT at 5s timeout
    """
    cfg = AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1h_a.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': False,
        },
        'runtime': {'graceful_shutdown_timeout_seconds': 5},
    })
    container = RuntimeContainer(cfg)

    # Simulate an in-flight operation
    container.ingress_service._begin_operation()
    assert container.ingress_service.active_operations == 1

    # Schedule release after 50ms
    release_timer = threading.Timer(0.05, container.ingress_service._end_operation)
    release_timer.start()

    start = time.monotonic()
    container.shutdown()
    elapsed = time.monotonic() - start

    # Drain completed well before the 5s timeout
    assert elapsed < 2.0, (
        f'Scenario A: shutdown should drain promptly once counter drops, '
        f'took {elapsed:.3f}s (expected < 2s)'
    )
    assert container.ingress_service.active_operations == 0, (
        'Scenario A: active_operations must be 0 after drain'
    )
    assert container.health.live is False, 'Scenario A: health.live must be False after shutdown'
    assert container.health.phase == 'STOPPING', (
        f'Scenario A: phase must be STOPPING, got {container.health.phase!r}'
    )

    release_timer.cancel()  # no-op if already fired


# ---------------------------------------------------------------------------
# Scenario B — full write-path drain via REST
# ---------------------------------------------------------------------------

def _make_blocking_mock():
    """
    Returns (mock_handler, send_started_event, send_can_proceed_event).
    mock_handler blocks at /receipts/sell until send_can_proceed is set.
    """
    send_started = threading.Event()
    send_can_proceed = threading.Event()

    def handler(request: httpx.Request) -> httpx.Response:
        path = request.url.path
        if path.endswith('/cashier/signinPinCode'):
            return httpx.Response(200, json={'access_token': 'mock-token-gate1h'})
        if path.endswith('/shifts'):
            return httpx.Response(200, json={
                'id': 'gate1h-shift-001',
                'status': 'OPENED',
                'fiscal_code': 'SHIFT-GATE1H-001',
                'updated_at': '2026-03-29T17:00:00+00:00',
            })
        if path.endswith('/receipts/sell'):
            send_started.set()        # signal: transport send is in-flight
            send_can_proceed.wait(timeout=10)  # block until released
            return httpx.Response(200, json={
                'id': 'gate1h-receipt-001',
                'status': 'DONE',
                'fiscal_code': 'RCPT-GATE1H-001',
                'updated_at': '2026-03-29T17:01:00+00:00',
            })
        raise AssertionError(f'gate1h: unexpected {request.method} {request.url}')

    return handler, send_started, send_can_proceed


def _config_b(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1h_b.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1h',
        },
        'runtime': {
            'process_immediately': True,
            'graceful_shutdown_timeout_seconds': 10,
        },
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1H-LIC',
            'cashier_pin': '0000',
        },
    })


def test_gate1h_shutdown_drains_in_flight_sell_no_stuck_processing(tmp_path: Path) -> None:
    """
    SELL is in mid-transport-send when TestClient lifespan exits (→ shutdown).
    Shutdown must drain: wait for the SELL to complete, then return.

    After shutdown:
      - active_operations == 0
      - No inbox record in PROCESSING state (lease was released)
      - SELL document in ACK state (not abandoned)

    Sequence:
      1. SELL thread: blocks at /receipts/sell, sets send_started.
      2. Main thread: waits for send_started, schedules release Timer(100ms).
      3. Main thread: exits `with TestClient` → shutdown() starts spinning.
      4. Timer fires → send_can_proceed.set() → SELL completes → counter drops.
      5. shutdown() sees active_operations == 0 → returns → with block exits.
    """
    mock_handler, send_started, send_can_proceed = _make_blocking_mock()
    cfg = _config_b(tmp_path)
    transport_client = httpx.Client(transport=httpx.MockTransport(mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    sell_resp_holder: list = []
    sell_exc_holder: list = []

    def do_sell(client: TestClient) -> None:
        try:
            resp = client.post('/v1/ingress/checkbox', json={
                'context': {
                    'request_id': 'gate1h-sell-001',
                    'fiscal_number': FISCAL_NUMBER,
                    'backend_profile_id': BACKEND_PROFILE,
                    'transport_profile_id': TRANSPORT_PROFILE,
                    'channel_owner': 'gate1h',
                    'business_ts': '2026-03-29T17:01:00Z',
                },
                'operation': 'SELL',
                'request': {
                    'external_request_id': 'ext-sell-gate1h',
                    'cashier_id': 'cashier-gate1h',
                    'goods': [{'name': 'Widget', 'price': 1000, 'quantity': 1000}],
                    'payments': [{'type': 'CASH', 'amount': 1000}],
                },
            })
            sell_resp_holder.append(resp)
        except Exception as exc:
            sell_exc_holder.append(exc)

    release_timer: threading.Timer | None = None

    with TestClient(create_app(container)) as client:
        # Open shift synchronously (fast — no blocking mock for /shifts)
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {
                'request_id': 'gate1h-open',
                'fiscal_number': FISCAL_NUMBER,
                'backend_profile_id': BACKEND_PROFILE,
                'transport_profile_id': TRANSPORT_PROFILE,
                'channel_owner': 'gate1h',
                'business_ts': '2026-03-29T17:00:00Z',
            },
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'ext-open-gate1h',
                'cashier_id': 'cashier-gate1h',
            },
        })
        assert r_shift.status_code == 200, f'SHIFT_OPEN failed: {r_shift.text}'
        assert r_shift.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {r_shift.json()}'

        # Start SELL in background — will block at transport send
        sell_thread = threading.Thread(target=do_sell, args=(client,), daemon=True)
        sell_thread.start()

        # Wait until transport send is in-flight
        reached = send_started.wait(timeout=10)
        assert reached, 'gate1h: SELL transport send mock not reached within timeout'
        assert container.ingress_service.active_operations == 1, (
            f'Expected active_operations=1 while SELL is in-flight, '
            f'got {container.ingress_service.active_operations}'
        )

        # Schedule release of the blocking mock (fires while shutdown is spinning)
        release_timer = threading.Timer(0.1, send_can_proceed.set)
        release_timer.start()

        # Exit the `with` block → TestClient lifespan cleanup → container.shutdown()
        # shutdown() will spin until the Timer fires and SELL completes

    # At this point shutdown() has returned — drain completed (or timed out)
    sell_thread.join(timeout=10)

    if release_timer is not None:
        release_timer.cancel()  # no-op if already fired

    # --- Assertions ---
    assert not sell_exc_holder, f'SELL thread raised: {sell_exc_holder}'
    assert sell_resp_holder, 'SELL thread did not produce a response'
    assert sell_resp_holder[0].status_code == 200, (
        f'SELL response non-200: {sell_resp_holder[0].text}'
    )

    assert container.ingress_service.active_operations == 0, (
        'active_operations must be 0 after graceful shutdown drain'
    )
    assert container.health.live is False, 'health.live must be False after shutdown'
    assert container.health.phase == 'STOPPING', (
        f'phase must be STOPPING after shutdown, got {container.health.phase!r}'
    )

    with container.connect() as conn:
        processing_count = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox WHERE status = 'PROCESSING'"
        ).fetchone()[0]
        sell_doc = conn.execute(
            "SELECT state FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchone()

    assert processing_count == 0, (
        f'No inbox record must remain in PROCESSING after graceful shutdown drain, '
        f'found {processing_count}'
    )
    assert sell_doc is not None, 'SELL document must exist after completed operation'
    assert sell_doc[0] == 'ACK', (
        f'SELL document must be ACK after drain-completed shutdown, got {sell_doc[0]!r}'
    )
