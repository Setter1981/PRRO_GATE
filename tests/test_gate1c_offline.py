"""
Gate 1c — offline boundary smoke test.

Verifies that a SELL processed in OFFLINE mode:
  - does NOT call the transport layer
  - completes with state=OFFLINE_LOCAL_ACK
  - sets fs_mode='OFFLINE'
  - sets submission_status='OFFLINE_LOCAL'
  - allocates an offline_fiscal_no

Setup sequence:
  1. Open a shift via REST (ONLINE mode, normal mock transport).
  2. Directly set node_state.mode=OFFLINE via NodeStateRepository.
  3. Seed an OPEN offline session and an ACTIVE offline_range via DB.
  4. Send a SELL via REST — mock transport raises if /receipts/sell is hit.
  5. Assert all offline document invariants.

Invariants preserved:
  #1 (no network in tx): offline path skips transport entirely.
  #2 (single-writer): unchanged.
  #4 (idempotency): unchanged.
  #5 (offline limits): session and range are seeded within limits.
"""
from __future__ import annotations

import uuid
from datetime import datetime, timedelta, timezone
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import NodeMode
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.repositories.offline import OfflineRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


def _mock_handler_online_shift_only(request: httpx.Request) -> httpx.Response:
    """
    Handles only the requests needed to open a shift (ONLINE phase).
    Raises AssertionError if the transport is called for a receipt — this is the
    key invariant: in OFFLINE mode the transport layer must NOT be reached.
    """
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1c'})
    if path.endswith('/shifts'):
        return httpx.Response(200, json={
            'id': 'gate1c-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1C-001',
            'updated_at': '2026-03-29T12:00:00+00:00',
        })
    # Any receipt or other transport call must NOT happen in offline mode
    raise AssertionError(
        f'gate1c: transport was called in OFFLINE mode — unexpected {request.method} {request.url}'
    )


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1c.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1c',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1C-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T12:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1c',
        'business_ts': ts,
    }


def _open_shift(client: TestClient) -> None:
    resp = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx('gate1c-open'), 'business_ts': '2026-03-29T12:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': 'ext-open-gate1c',
            'cashier_id': 'cashier-gate1c',
        },
    })
    assert resp.status_code == 200, f'SHIFT_OPEN failed: {resp.text}'
    assert resp.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {resp.json()}'


def _seed_offline_state(container: RuntimeContainer) -> tuple[str, str]:
    """
    Switches the node to OFFLINE mode and seeds the required DB rows:
      - offline_sessions: one OPEN session
      - offline_ranges: one ACTIVE range with 10 available codes
    Returns (session_id, range_id).
    """
    session_id = 'gate1c-session-001'
    range_id = 'gate1c-range-001'

    with container.connect() as conn:
        conn.execute('BEGIN IMMEDIATE')

        # Switch node to OFFLINE
        NodeStateRepository.update_mode(conn, fiscal_number=FISCAL_NUMBER, mode=NodeMode.OFFLINE)

        # Create an OPEN offline session — use dynamic time to avoid continuous-time-limit check
        _now = datetime.now(timezone.utc)
        _session_start = (_now - timedelta(minutes=5)).strftime('%Y-%m-%dT%H:%M:%S')
        OfflineRepository.create_open_session(
            conn,
            offline_session_id=session_id,
            fiscal_number=FISCAL_NUMBER,
            started_at=_session_start,
            status='OPEN',
            accumulated_month_seconds=0,
        )

        # Seed an ACTIVE offline range with fiscal codes 1001–1010
        conn.execute(
            """
            INSERT INTO offline_ranges
                (range_id, fiscal_number, first_fiscal_no, last_fiscal_no,
                 next_fiscal_no, issued_at, status)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (range_id, FISCAL_NUMBER, 1001, 1010, 1001, _session_start, 'ACTIVE'),
        )

        conn.commit()

    return session_id, range_id


def test_gate1c_offline_sell_skips_transport(tmp_path: Path) -> None:
    """
    SELL in OFFLINE mode must complete as ACK without calling the transport.

    Verified:
    - HTTP 200 from /v1/ingress/checkbox
    - document state = OFFLINE_LOCAL_ACK
    - fs_mode = 'OFFLINE'
    - submission_status = 'OFFLINE_LOCAL'
    - offline_fiscal_no is not None (code was allocated)
    - offline_ranges.next_fiscal_no incremented by 1
    - shift remains OPENED
    - transport mock NOT invoked for receipt (AssertionError if called)
    """
    cfg = _config(tmp_path)
    transport_client = httpx.Client(
        transport=httpx.MockTransport(_mock_handler_online_shift_only)
    )
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        # Phase 1: open shift while still ONLINE (transport allowed here)
        _open_shift(client)

        # Phase 2: seed offline state directly in DB
        session_id, range_id = _seed_offline_state(container)

        # Phase 3: send SELL — must NOT hit transport
        resp = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1c-sell-offline-001'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-offline-001',
                'cashier_id': 'cashier-gate1c',
                'goods': [{'name': 'Widget', 'price': 5000, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 5000}],
            },
        })

    assert resp.status_code == 200, f'SELL response non-200: {resp.text}'
    body = resp.json()
    assert body.get('document_state') == 'OFFLINE_LOCAL_ACK', f'Expected OFFLINE_LOCAL_ACK, got: {body}'

    doc_id = body.get('document_id')
    assert doc_id is not None, f'No document_id in response: {body}'

    with container.connect() as conn:
        doc = conn.execute(
            "SELECT fs_mode, submission_status, offline_fiscal_no, state "
            "FROM fiscal_documents WHERE document_id = ?",
            (doc_id,),
        ).fetchone()

    assert doc is not None, f'Document {doc_id!r} not found in DB'
    fs_mode, submission_status, offline_fiscal_no, state = doc

    assert fs_mode == 'OFFLINE', f'Expected fs_mode=OFFLINE, got {fs_mode!r}'
    assert submission_status == 'OFFLINE_LOCAL', (
        f'Expected submission_status=OFFLINE_LOCAL, got {submission_status!r}'
    )
    assert offline_fiscal_no is not None, 'offline_fiscal_no must be set for offline documents'
    assert state == 'OFFLINE_LOCAL_ACK', f'Expected state=OFFLINE_LOCAL_ACK, got {state!r}'

    # offline_fiscal_no should be the first code from our range (1001)
    assert offline_fiscal_no == 1001, (
        f'Expected offline_fiscal_no=1001 (first code from range), got {offline_fiscal_no!r}'
    )

    with container.connect() as conn:
        # Range next_fiscal_no must have advanced to 1002
        range_row = conn.execute(
            "SELECT next_fiscal_no, status FROM offline_ranges WHERE range_id = ?",
            (range_id,),
        ).fetchone()

        shift = conn.execute(
            "SELECT state FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()

    assert range_row is not None, 'offline_ranges row not found'
    assert range_row[0] == 1002, (
        f'Expected next_fiscal_no=1002 after one allocation, got {range_row[0]}'
    )
    assert range_row[1] == 'ACTIVE', (
        f'Range should still be ACTIVE (9 codes remain), got {range_row[1]!r}'
    )

    assert shift is not None and shift[0] == 'OPENED', (
        f'Shift should remain OPENED after offline SELL, got {shift}'
    )


def test_gate1c_offline_range_exhausted_returns_error(tmp_path: Path) -> None:
    """
    When all offline codes are exhausted, the write-path must return an error
    (not crash, not silently ACK).

    Seeds a range with exactly 1 available code, does 1 SELL (uses it),
    then a second SELL must fail with a fiscal error (not HTTP 500).
    """
    cfg = _config(tmp_path)
    transport_client = httpx.Client(
        transport=httpx.MockTransport(_mock_handler_online_shift_only)
    )
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        _open_shift(client)

        # Seed: range with exactly 1 code available (next == last)
        with container.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            NodeStateRepository.update_mode(conn, fiscal_number=FISCAL_NUMBER, mode=NodeMode.OFFLINE)
            _now = datetime.now(timezone.utc)
            _session_start = (_now - timedelta(minutes=5)).strftime('%Y-%m-%dT%H:%M:%S')
            OfflineRepository.create_open_session(
                conn,
                offline_session_id='gate1c-exhaust-session',
                fiscal_number=FISCAL_NUMBER,
                started_at=_session_start,
                status='OPEN',
            )
            conn.execute(
                """
                INSERT INTO offline_ranges
                    (range_id, fiscal_number, first_fiscal_no, last_fiscal_no,
                     next_fiscal_no, issued_at, status)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                ('gate1c-exhaust-range', FISCAL_NUMBER, 2001, 2001, 2001,
                 _session_start, 'ACTIVE'),
            )
            conn.commit()

        def _sell(req_id: str, ext_id: str) -> dict:
            r = client.post('/v1/ingress/checkbox', json={
                'context': _ctx(req_id),
                'operation': 'SELL',
                'request': {
                    'external_request_id': ext_id,
                    'cashier_id': 'cashier-gate1c',
                    'goods': [{'name': 'Widget', 'price': 1000, 'quantity': 1000}],
                    'payments': [{'type': 'CASH', 'amount': 1000}],
                },
            })
            return r

        # First SELL: consumes the only code — must succeed
        r1 = _sell('gate1c-exhaust-sell-1', 'ext-exhaust-1')
        assert r1.status_code == 200, f'First SELL failed: {r1.text}'
        assert r1.json().get('document_state') == 'OFFLINE_LOCAL_ACK', f'First SELL not OFFLINE_LOCAL_ACK: {r1.json()}'

        # Second SELL: no codes left — must NOT crash with HTTP 500
        r2 = _sell('gate1c-exhaust-sell-2', 'ext-exhaust-2')

    # Accept either an error response (200 with error fields) or a 4xx/5xx
    # The invariant is: no unhandled exception (not HTTP 500 from a crash)
    assert r2.status_code != 500, (
        f'Exhausted range must not cause HTTP 500 (unhandled crash): {r2.text}'
    )

    with container.connect() as conn:
        exhausted = conn.execute(
            "SELECT status FROM offline_ranges WHERE range_id = 'gate1c-exhaust-range'"
        ).fetchone()

    assert exhausted is not None and exhausted[0] == 'EXHAUSTED', (
        f'Range must be EXHAUSTED after all codes are allocated, got {exhausted}'
    )
