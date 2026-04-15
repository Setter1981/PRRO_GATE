"""
Gate 1v — bootstrap / first-run initialization smoke tests.

Verifies that a fresh database is correctly initialized by the migration seed
and is immediately usable for the first shift open.

Bootstrap mechanism:
  002_seed_reference_data.sql inserts a node_state row for 'FN-DEV-0001' with:
    mode='ONLINE', shift_state='CLOSED', next_lnd=1,
    readiness_state='READY', recovery_stage='DONE',
    current_month_offline_seconds=0

  No code-side initialization is needed — the write-path relies on node_state
  being present (returns BACKEND_TEMPORARY_UNAVAILABLE if None).

Two tests:
  A — node_state has expected initial values after migration.
  B — first SHIFT_OPEN succeeds immediately after a fresh DB init.

Regression value:
  If 002_seed_reference_data.sql is changed or the seed is dropped, Test A
  catches it before a production deployment opens with an unusable node_state.
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import NodeMode, ShiftState
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1v'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1v-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1V-001',
            'updated_at': '2026-03-29T18:00:00+00:00',
        })
    raise AssertionError(f'gate1v: unexpected {request.method} {request.url}')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1v.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1v',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1V-LIC',
            'cashier_pin': '0000',
        },
    })


# ---------------------------------------------------------------------------
# Test A — node_state has correct initial values after fresh migration
# ---------------------------------------------------------------------------

def test_gate1v_fresh_db_node_state_initialized(tmp_path: Path) -> None:
    """
    After auto_migrate on a fresh DB:
      - node_state row exists for FISCAL_NUMBER.
      - mode = ONLINE.
      - shift_state = CLOSED.
      - next_lnd = 1 (first document will get LND=1).
      - readiness_state = READY.
      - recovery_stage = DONE.
      - current_month_offline_seconds = 0.
    """
    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)):
        # Just trigger initialization; no requests needed.
        pass

    with container.connect() as conn:
        node = NodeStateRepository.get_state(conn, FISCAL_NUMBER)

    assert node is not None, (
        f'node_state must exist for {FISCAL_NUMBER!r} after migration. '
        f'If 002_seed_reference_data.sql was changed, this test will catch it.'
    )
    assert node.mode == NodeMode.ONLINE, (
        f'Initial node.mode must be ONLINE, got {node.mode!r}'
    )
    assert node.shift_state == ShiftState.CLOSED, (
        f'Initial node.shift_state must be CLOSED, got {node.shift_state!r}'
    )
    assert node.next_lnd == 1, (
        f'Initial node.next_lnd must be 1, got {node.next_lnd!r}'
    )
    assert node.readiness_state == 'READY', (
        f'Initial node.readiness_state must be READY, got {node.readiness_state!r}'
    )
    assert node.recovery_stage == 'DONE', (
        f'Initial node.recovery_stage must be DONE, got {node.recovery_stage!r}'
    )
    assert node.current_month_offline_seconds == 0, (
        f'Initial current_month_offline_seconds must be 0, got {node.current_month_offline_seconds!r}'
    )


# ---------------------------------------------------------------------------
# Test B — first SHIFT_OPEN works on a fresh DB
# ---------------------------------------------------------------------------

def test_gate1v_first_shift_open_works_on_fresh_db(tmp_path: Path) -> None:
    """
    On a fresh DB (after auto_migrate), SHIFT_OPEN must succeed immediately
    with document_state=ACK. Proves the bootstrap state is operational.

    If node_state were absent or malformed, the write-path would return
    BACKEND_TEMPORARY_UNAVAILABLE instead of ACK.
    """
    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        r = client.post('/v1/ingress/checkbox', json={
            'context': {
                'request_id': 'gate1v-first-open',
                'fiscal_number': FISCAL_NUMBER,
                'backend_profile_id': BACKEND_PROFILE,
                'transport_profile_id': TRANSPORT_PROFILE,
                'channel_owner': 'gate1v',
                'business_ts': '2026-03-29T18:00:00Z',
            },
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'ext-first-open-gate1v',
                'cashier_id': 'cashier-gate1v',
            },
        })

    assert r.status_code == 200, f'First SHIFT_OPEN non-200: {r.text}'
    body = r.json()
    assert body.get('document_state') == 'ACK', (
        f'First SHIFT_OPEN must ACK on a fresh DB. '
        f'If BACKEND_TEMPORARY_UNAVAILABLE: node_state was not initialized. '
        f'Got: {body}'
    )
    assert 'document_id' in body, f'document_id missing: {body}'

    # Verify node_state advanced: next_lnd must have incremented
    with container.connect() as conn:
        node = NodeStateRepository.get_state(conn, FISCAL_NUMBER)

    assert node is not None
    assert node.next_lnd > 1, (
        f'next_lnd must advance after first operation, got {node.next_lnd!r}'
    )
