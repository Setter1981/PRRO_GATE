"""
Gate 1l — LND (local document number) integrity smoke test.

Verifies that increment_lnd is atomic and produces no duplicates under
concurrent write-path pressure on a single fiscal_number.

Architecture:
  increment_lnd() runs inside _stage_acquire_and_validate under BEGIN IMMEDIATE.
  SQLite BEGIN IMMEDIATE serialises all concurrent writers to the same DB,
  so each SELL wins its own exclusive transaction for the LND allocation step.

What is tested:
  1. N parallel SELLs via REST (process_immediately=True).
  2. All N succeed (ACK).
  3. The N fiscal_documents have pairwise-distinct LND values.
  4. LNDs form a contiguous range [start, start+N-1] — no gaps within
     successful operations (model guarantees dense allocation when no errors
     occur).
  5. node_state.next_lnd == start + N after all operations.
  6. document_id derived from LND is unique (doc-{fiscal}-{lnd}).

N = 8 — large enough to expose races, small enough to be fast.

Invariants preserved:
  #2 (single-writer): BEGIN IMMEDIATE is the mechanism; tested here under load.
  #4 (idempotency): each SELL uses a distinct external_request_id.
"""
from __future__ import annotations

import concurrent.futures
from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'
N = 8


def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1l'})
    if path.endswith('/shifts'):
        return httpx.Response(200, json={
            'id': 'gate1l-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1L-001',
            'updated_at': '2026-03-29T20:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate1l-receipt',
            'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1L',
            'updated_at': '2026-03-29T20:01:00+00:00',
        })
    raise AssertionError(f'gate1l: unexpected {request.method} {request.url}')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1l.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1l',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1L-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str) -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1l',
        'business_ts': '2026-03-29T20:01:00Z',
    }


def _sell_payload(i: int) -> dict:
    return {
        'context': _ctx(f'gate1l-sell-{i:03d}'),
        'operation': 'SELL',
        'request': {
            'external_request_id': f'ext-sell-gate1l-{i:03d}',
            'cashier_id': 'cashier-gate1l',
            'goods': [{'name': f'Item-{i}', 'price': 1000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1000}],
        },
    }


def test_gate1l_parallel_sells_produce_unique_contiguous_lnds(tmp_path: Path) -> None:
    """
    N concurrent SELLs must each receive a distinct, contiguous LND value.

    No duplicate LNDs means no double-LND-allocation under concurrent pressure.
    Contiguous range means no LND was lost inside a successful transaction.
    """
    cfg = _config(tmp_path)
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        # Open shift
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1l-open'), 'business_ts': '2026-03-29T20:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'ext-open-gate1l',
                'cashier_id': 'cashier-gate1l',
            },
        })
        assert r_shift.status_code == 200, f'SHIFT_OPEN failed: {r_shift.text}'
        assert r_shift.json().get('document_state') == 'ACK'

        # Read next_lnd BEFORE parallel SELLs (shift_open already consumed one LND)
        with container.connect() as conn:
            node = NodeStateRepository.get_state(conn, FISCAL_NUMBER)
        assert node is not None
        lnd_start = node.next_lnd  # first LND that will be allocated by SELLs

        # Run N SELLs in parallel
        responses: list = []
        errors: list = []
        with concurrent.futures.ThreadPoolExecutor(max_workers=N) as executor:
            futures = {
                executor.submit(client.post, '/v1/ingress/checkbox', json=_sell_payload(i)): i
                for i in range(N)
            }
            for future in concurrent.futures.as_completed(futures):
                try:
                    responses.append(future.result())
                except Exception as exc:
                    errors.append(exc)

    assert errors == [], f'Thread exceptions: {errors}'

    non_200 = [(r.status_code, r.text[:200]) for r in responses if r.status_code != 200]
    assert non_200 == [], f'Non-200 responses: {non_200}'

    non_ack = [r.json() for r in responses if r.json().get('document_state') != 'ACK']
    assert non_ack == [], f'Non-ACK responses: {non_ack}'

    # --- LND integrity checks ---
    with container.connect() as conn:
        rows = conn.execute(
            "SELECT lnd, document_id FROM fiscal_documents WHERE doc_type = 'SELL' ORDER BY lnd",
        ).fetchall()
        node_after = NodeStateRepository.get_state(conn, FISCAL_NUMBER)

    assert len(rows) == N, f'Expected {N} SELL documents, found {len(rows)}'

    lnds = [row[0] for row in rows]
    doc_ids = [row[1] for row in rows]

    # No duplicate LNDs
    assert len(set(lnds)) == N, (
        f'LND duplicates detected! values={lnds}'
    )

    # Contiguous range [lnd_start, lnd_start + N - 1]
    expected_range = set(range(lnd_start, lnd_start + N))
    actual_set = set(lnds)
    assert actual_set == expected_range, (
        f'LND range not contiguous: expected {sorted(expected_range)}, got {sorted(actual_set)}'
    )

    # No duplicate document_ids (derived from LND)
    assert len(set(doc_ids)) == N, (
        f'document_id duplicates detected! ids={doc_ids}'
    )

    # next_lnd advanced by exactly N
    assert node_after is not None
    assert node_after.next_lnd == lnd_start + N, (
        f'next_lnd should be {lnd_start + N} after {N} SELLs, '
        f'got {node_after.next_lnd}'
    )


def test_gate1l_lnd_unique_across_shift_open_and_sells(tmp_path: Path) -> None:
    """
    SHIFT_OPEN also allocates an LND. Verify that the LNDs of SHIFT_OPEN and
    subsequent SELLs are all distinct — no LND is shared between operation types.
    """
    cfg = _config(tmp_path)
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1l-open-2'), 'business_ts': '2026-03-29T20:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1l-2', 'cashier_id': 'cashier-gate1l'},
        })
        for i in range(3):
            client.post('/v1/ingress/checkbox', json=_sell_payload(100 + i))

    with container.connect() as conn:
        all_lnds = conn.execute(
            "SELECT lnd, doc_type FROM fiscal_documents ORDER BY lnd"
        ).fetchall()

    lnd_values = [row[0] for row in all_lnds]
    assert len(set(lnd_values)) == len(lnd_values), (
        f'LND collision across operation types: {all_lnds}'
    )
