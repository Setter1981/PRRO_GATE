"""
Gate 1d — offline time limits smoke test.

Two limit vectors:

  Vector A — continuous limit (36h):
    Seed started_at = 37h ago.  _check_offline_limits detects continuous_seconds >= 129600.
    Expected: HTTP 200, error_code='OFFLINE_LIMIT_REACHED', no document_state=ACK,
              no document row created, transport NOT called.

  Vector B — monthly limit (168h):
    Seed started_at = now (continuous OK), accumulated_month_seconds = 605000 (> 604800).
    Expected: same error shape, no ACK, transport NOT called.

Both vectors prove that offline time limit enforcement is real and blocks SELL before
any document is written or transport is reached.

Invariants preserved:
  #1 (no network in tx): limit check is inside the write transaction, transport skipped.
  #5 (offline respects time limits): directly verified by both vectors.
"""
from __future__ import annotations

from datetime import datetime, timezone, timedelta
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.enums import NodeMode
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


def _mock_handler_shift_only(request: httpx.Request) -> httpx.Response:
    """Allows shift-open calls; raises if transport is reached for a receipt."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1d'})
    if path.endswith('/shifts'):
        return httpx.Response(200, json={
            'id': 'gate1d-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1D-001',
            'updated_at': '2026-03-29T13:00:00+00:00',
        })
    raise AssertionError(
        f'gate1d: transport reached in OFFLINE mode — {request.method} {request.url}'
    )


def _config(tmp_path: Path, db_name: str) -> AppConfig:
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
            'channel_owner': 'gate1d',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1D-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str) -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1d',
        'business_ts': '2026-03-29T13:01:00Z',
    }


def _open_shift(client: TestClient, req_id: str) -> None:
    resp = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx(req_id), 'business_ts': '2026-03-29T13:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': f'ext-open-{req_id}',
            'cashier_id': 'cashier-gate1d',
        },
    })
    assert resp.status_code == 200, f'SHIFT_OPEN failed: {resp.text}'
    assert resp.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {resp.json()}'


def _sell(client: TestClient, req_id: str, ext_id: str):
    return client.post('/v1/ingress/checkbox', json={
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1d',
            'goods': [{'name': 'Widget', 'price': 1000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1000}],
        },
    })


def _seed_offline_base(container: RuntimeContainer, session_id: str, started_at: str,
                       accumulated_month_seconds: int = 0) -> None:
    """Sets node OFFLINE, creates open session and a fresh ACTIVE range."""
    with container.connect() as conn:
        conn.execute('BEGIN IMMEDIATE')
        NodeStateRepository.update_mode(conn, fiscal_number=FISCAL_NUMBER, mode=NodeMode.OFFLINE)
        conn.execute(
            """
            INSERT INTO offline_sessions
                (offline_session_id, fiscal_number, started_at, status,
                 accumulated_month_seconds)
            VALUES (?, ?, ?, 'OPEN', ?)
            """,
            (session_id, FISCAL_NUMBER, started_at, accumulated_month_seconds),
        )
        conn.execute(
            """
            INSERT INTO offline_ranges
                (range_id, fiscal_number, first_fiscal_no, last_fiscal_no,
                 next_fiscal_no, issued_at, status)
            VALUES (?, ?, 3001, 3010, 3001, '2026-03-29T13:00:00', 'ACTIVE')
            """,
            (f'{session_id}-range', FISCAL_NUMBER),
        )
        conn.commit()


# ---------------------------------------------------------------------------
# Vector A — continuous limit exceeded (started_at = 37h ago)
# ---------------------------------------------------------------------------

def test_gate1d_continuous_limit_blocks_sell(tmp_path: Path) -> None:
    """
    SELL must be rejected when the offline session has been open for > 36h.

    Seed: started_at = 37h before now → continuous_seconds ≈ 133200 > 129600.
    Expected:
      - HTTP 200 (controlled error, not a crash)
      - response contains error_code='OFFLINE_LIMIT_REACHED'
      - document_state is NOT 'ACK'
      - no fiscal_documents row is created
      - transport NOT reached
    """
    cfg = _config(tmp_path, 'gate1d_vec_a.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler_shift_only))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    started_at_str = (
        datetime.now(timezone.utc) - timedelta(hours=37)
    ).strftime('%Y-%m-%dT%H:%M:%S')

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1d-open-vec-a')
        _seed_offline_base(container, 'gate1d-session-vec-a', started_at_str)

        resp = _sell(client, 'gate1d-sell-vec-a', 'ext-vec-a')

    assert resp.status_code == 200, f'Expected 200 (controlled error), got {resp.status_code}: {resp.text}'
    body = resp.json()

    assert body.get('error_code') == 'OFFLINE_LIMIT_REACHED', (
        f'Vector A: expected error_code=OFFLINE_LIMIT_REACHED, got: {body}'
    )
    assert body.get('document_state') != 'ACK', (
        f'Vector A: SELL must not ACK when continuous limit is exceeded: {body}'
    )

    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()

    assert len(sell_docs) == 0, (
        f'Vector A: no SELL document must be created on limit exceeded, found {sell_docs}'
    )


# ---------------------------------------------------------------------------
# Vector B — monthly limit exceeded (accumulated_month_seconds > 168h)
# ---------------------------------------------------------------------------

def test_gate1d_monthly_limit_blocks_sell(tmp_path: Path) -> None:
    """
    SELL must be rejected when the monthly offline seconds budget is exhausted.

    Seed: started_at = now (continuous OK), accumulated_month_seconds = 605000 > 604800.
    Expected: same error shape as Vector A.
    """
    cfg = _config(tmp_path, 'gate1d_vec_b.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler_shift_only))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    # started_at is recent — continuous limit will NOT fire
    started_at_str = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%S')
    MONTHLY_LIMIT = 168 * 60 * 60  # 604800
    OVER_LIMIT = MONTHLY_LIMIT + 200  # 605000

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1d-open-vec-b')
        _seed_offline_base(
            container, 'gate1d-session-vec-b', started_at_str,
            accumulated_month_seconds=OVER_LIMIT,
        )

        resp = _sell(client, 'gate1d-sell-vec-b', 'ext-vec-b')

    assert resp.status_code == 200, f'Expected 200 (controlled error), got {resp.status_code}: {resp.text}'
    body = resp.json()

    assert body.get('error_code') == 'OFFLINE_LIMIT_REACHED', (
        f'Vector B: expected error_code=OFFLINE_LIMIT_REACHED, got: {body}'
    )
    assert body.get('document_state') != 'ACK', (
        f'Vector B: SELL must not ACK when monthly limit is exceeded: {body}'
    )

    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()

    assert len(sell_docs) == 0, (
        f'Vector B: no SELL document must be created on monthly limit exceeded, found {sell_docs}'
    )
