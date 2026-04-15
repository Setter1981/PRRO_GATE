"""
Gate 1u — channel lock persistence smoke tests.

Verifies that the channel lock fields are correctly persisted in the shifts table
and that the lock semantics (active / released) match the shift lifecycle.

Channel lock is stored in the shifts table as four columns:
  opened_via_backend_profile_id   TEXT NOT NULL
  opened_via_transport_profile_id TEXT NOT NULL
  opened_via_protocol             TEXT NOT NULL
  opened_via_integration_owner    TEXT NOT NULL
  channel_lock_acquired_at        TEXT NOT NULL

get_active_shift() selects WHERE state IN ('OPENING','OPENED','CLOSING') — excludes 'CLOSED'.
get_channel_lock() returns a ChannelLock model from active shifts only.

Two tests:
  A — SHIFT_OPEN: lock fields persisted correctly in DB; get_channel_lock() returns them.
  B — SHIFT_CLOSE: get_active_shift() returns None, get_channel_lock() returns None;
      a subsequent SHIFT_OPEN correctly persists a new lock entry.

Invariant #3 (channel switch forbidden): persistence is the foundation — the runtime
guard (Gate 1f) relies on get_channel_lock() returning the correct persisted values.
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.repositories.shifts import ShiftRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'
CHANNEL_OWNER = 'gate1u'


def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1u'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1u-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1U-001',
            'updated_at': '2026-03-29T17:00:00+00:00',
        })
    if path.endswith('/shifts/close'):
        return httpx.Response(200, json={
            'id': 'gate1u-shift-001', 'status': 'CLOSED',
            'fiscal_code': 'SHIFT-GATE1U-001',
            'updated_at': '2026-03-29T17:30:00+00:00',
        })
    raise AssertionError(f'gate1u: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str = 'gate1u.sqlite3') -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / db_name),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': CHANNEL_OWNER,
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1U-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str) -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': CHANNEL_OWNER,
        'business_ts': ts,
    }


# ---------------------------------------------------------------------------
# Test A — SHIFT_OPEN persists channel lock fields
# ---------------------------------------------------------------------------

def test_gate1u_shift_open_persists_channel_lock(tmp_path: Path) -> None:
    """
    After SHIFT_OPEN:
    - shifts row exists with correct opened_via_* fields.
    - get_channel_lock() returns a ChannelLock matching the opened_via fields.
    - get_active_shift() returns the shift in OPENED state.
    """
    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        r = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1u-open', '2026-03-29T17:00:00Z'),
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1u', 'cashier_id': 'cashier-gate1u'},
        })
        assert r.status_code == 200, f'SHIFT_OPEN failed: {r.text}'
        assert r.json().get('document_state') == 'ACK'

    with container.connect() as conn:
        # Raw DB row inspection
        row = conn.execute(
            """
            SELECT opened_via_backend_profile_id, opened_via_transport_profile_id,
                   opened_via_protocol, opened_via_integration_owner,
                   channel_lock_acquired_at, state
            FROM shifts WHERE fiscal_number = ? ORDER BY created_at DESC LIMIT 1
            """,
            (FISCAL_NUMBER,),
        ).fetchone()

        active_shift = ShiftRepository.get_active_shift(conn, FISCAL_NUMBER)
        lock = ShiftRepository.get_channel_lock(conn, FISCAL_NUMBER)

    assert row is not None, 'No shift row found after SHIFT_OPEN'
    (via_backend, via_transport, via_protocol, via_owner,
     lock_acquired_at, shift_state) = row

    assert via_backend == BACKEND_PROFILE, (
        f'opened_via_backend_profile_id={via_backend!r} != {BACKEND_PROFILE!r}'
    )
    assert via_transport == TRANSPORT_PROFILE, (
        f'opened_via_transport_profile_id={via_transport!r} != {TRANSPORT_PROFILE!r}'
    )
    assert via_protocol is not None, 'opened_via_protocol must be set'
    assert via_owner == CHANNEL_OWNER, (
        f'opened_via_integration_owner={via_owner!r} != {CHANNEL_OWNER!r}'
    )
    assert lock_acquired_at is not None, 'channel_lock_acquired_at must be set'
    assert shift_state == 'OPENED', f'shift.state must be OPENED, got {shift_state!r}'

    # get_active_shift() must find the shift
    assert active_shift is not None, 'get_active_shift() returned None after SHIFT_OPEN'
    assert active_shift.state.value == 'OPENED'

    # get_channel_lock() must return the persisted lock
    assert lock is not None, 'get_channel_lock() returned None after SHIFT_OPEN'
    assert lock.backend_profile_id == BACKEND_PROFILE
    assert lock.transport_profile_id == TRANSPORT_PROFILE
    assert lock.integration_owner == CHANNEL_OWNER


# ---------------------------------------------------------------------------
# Test B — SHIFT_CLOSE releases active lock; SHIFT_OPEN sets new lock
# ---------------------------------------------------------------------------

def test_gate1u_shift_close_releases_lock_and_reopen_sets_new(tmp_path: Path) -> None:
    """
    After SHIFT_CLOSE:
    - get_active_shift() returns None (CLOSED excluded from query).
    - get_channel_lock() returns None (no active shift to read from).
    After a subsequent SHIFT_OPEN:
    - A new shift row is created with a fresh lock.
    """
    cfg = _config(tmp_path, 'gate1u_b.sqlite3')
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        # Open
        r_open = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1u-open-b', '2026-03-29T17:00:00Z'),
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1u-b', 'cashier_id': 'cashier-gate1u'},
        })
        assert r_open.status_code == 200
        assert r_open.json().get('document_state') == 'ACK'

        # Close
        r_close = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1u-close-b', '2026-03-29T17:30:00Z'),
            'operation': 'SHIFT_CLOSE',
            'request': {'external_request_id': 'ext-close-gate1u-b', 'cashier_id': 'cashier-gate1u'},
        })
        assert r_close.status_code == 200
        assert r_close.json().get('document_state') == 'ACK'

    with container.connect() as conn:
        active_after_close = ShiftRepository.get_active_shift(conn, FISCAL_NUMBER)
        lock_after_close = ShiftRepository.get_channel_lock(conn, FISCAL_NUMBER)
        closed_row = conn.execute(
            "SELECT state FROM shifts WHERE fiscal_number=? ORDER BY created_at DESC LIMIT 1",
            (FISCAL_NUMBER,),
        ).fetchone()

    assert active_after_close is None, (
        f'get_active_shift() must return None after SHIFT_CLOSE, got {active_after_close}'
    )
    assert lock_after_close is None, (
        'get_channel_lock() must return None when no active shift exists'
    )
    assert closed_row is not None and closed_row[0] == 'CLOSED', (
        f'Shift must be in CLOSED state in DB: {closed_row}'
    )

    # Re-open: new SHIFT_OPEN must succeed and set a new lock
    with TestClient(create_app(container)) as client:
        r_reopen = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate1u-reopen-b', '2026-03-29T18:00:00Z'),
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'ext-reopen-gate1u-b',
                'cashier_id': 'cashier-gate1u',
            },
        })
        assert r_reopen.status_code == 200, f'Re-open SHIFT_OPEN failed: {r_reopen.text}'
        assert r_reopen.json().get('document_state') == 'ACK'

    with container.connect() as conn:
        active_after_reopen = ShiftRepository.get_active_shift(conn, FISCAL_NUMBER)
        lock_after_reopen = ShiftRepository.get_channel_lock(conn, FISCAL_NUMBER)
        shift_count = conn.execute(
            "SELECT COUNT(*) FROM shifts WHERE fiscal_number=?", (FISCAL_NUMBER,)
        ).fetchone()[0]

    assert active_after_reopen is not None, 'get_active_shift() must return shift after re-open'
    assert active_after_reopen.state.value == 'OPENED'
    assert lock_after_reopen is not None, 'get_channel_lock() must return new lock after re-open'
    assert lock_after_reopen.backend_profile_id == BACKEND_PROFILE
    assert shift_count == 2, (
        f'Two shift rows expected (original + reopened), got {shift_count}'
    )
