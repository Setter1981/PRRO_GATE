"""
Gate 1a — concurrency smoke: N parallel SELL requests to one fiscal_number.

Observed write-path model under concurrent REST ingress:
- _store_command:       BEGIN IMMEDIATE serializes all N inbox inserts.
- acquire_lease:        BEGIN IMMEDIATE + LIMIT 1 — picks ONE oldest-NEW item
                        per fiscal_number atomically; only one writer wins.
- FIFO processing:      items are processed in created_at order; a thread may
                        process a different thread's item (not its own).
- NOOP on empty queue:  if a thread's item was already consumed by another
                        thread, process_next() returns NOOP → HTTP 200 with no
                        document enrichment, but all N items ARE processed
                        across the N concurrent threads.
- LND monotonicity:     NodeStateRepository.increment_lnd runs inside the
                        BEGIN IMMEDIATE transaction — no collision possible.
- No double-fiscal:     document_id = doc-{fiscal_number}-{lnd}; lnd is
                        strictly monotone under the exclusive lock.

Success criteria (verified):
  1. All HTTP responses 200 (no crash, no BUSY timeout under fast mock).
  2. Exactly N SELL documents in DB after all threads complete.
  3. All SELL documents in ACK state.
  4. No duplicate document_ids (LND atomicity preserved).
  5. Shift remains OPENED (no accidental side-effect from concurrent traffic).
"""
from __future__ import annotations

import concurrent.futures
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
CONCURRENCY = 10


def _mock_handler(request: httpx.Request) -> httpx.Response:
    """Pure function — thread-safe, no shared mutable state."""
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1a'})
    if path.endswith('/shifts'):
        return httpx.Response(200, json={
            'id': 'gate1a-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1A-001',
            'updated_at': '2026-03-29T10:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate1a-receipt',
            'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1A',
            'updated_at': '2026-03-29T10:01:00+00:00',
        })
    raise AssertionError(f'gate1a_mock: unexpected {request.method} {request.url}')


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'gate1a.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1a',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1A-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T10:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1a',
        'business_ts': ts,
    }


def _sell_payload(req_id: str) -> dict:
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': f'ext-{req_id}',
            'cashier_id': 'cashier-gate1a',
            'goods': [{'name': 'Item', 'price': 1000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1000}],
        },
    }


def test_gate1a_concurrent_sell_single_fiscal_number(tmp_path: Path) -> None:
    """
    N concurrent SELL requests to one fiscal_number must not corrupt DB state.

    Documents the actual serialization behaviour of the single-writer write-path
    under concurrent REST ingress with process_immediately=True and a fast mock.
    """
    cfg = _config(tmp_path)
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:

        # 1. Sequential SHIFT_OPEN before concurrent sells
        open_resp = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate1a-open'), 'business_ts': '2026-03-29T10:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': 'gate1a-ext-open',
                'cashier_id': 'cashier-gate1a',
            },
        })
        assert open_resp.status_code == 200, f'SHIFT_OPEN failed: {open_resp.text}'
        assert open_resp.json().get('document_state') == 'ACK', (
            f'SHIFT_OPEN not ACK: {open_resp.json()}'
        )

        # 2. Fire N concurrent SELLs from a thread pool
        payloads = [_sell_payload(f'gate1a-sell-{i:03d}') for i in range(CONCURRENCY)]

        responses: list = []
        thread_errors: list[Exception] = []

        with concurrent.futures.ThreadPoolExecutor(max_workers=CONCURRENCY) as executor:
            futures = {
                executor.submit(client.post, '/v1/ingress/checkbox', json=payload): i
                for i, payload in enumerate(payloads)
            }
            for future in concurrent.futures.as_completed(futures):
                try:
                    responses.append(future.result())
                except Exception as exc:
                    thread_errors.append(exc)

    # 3. No thread-level exceptions (no crash, no uncaught OperationalError)
    assert thread_errors == [], (
        f'{len(thread_errors)} thread exception(s) under {CONCURRENCY}-concurrent SELL: '
        f'{thread_errors}'
    )

    # 4. All HTTP responses are 200 (no BUSY timeout, no 500)
    non_200 = [(r.status_code, r.text[:300]) for r in responses if r.status_code != 200]
    assert non_200 == [], (
        f'Non-200 responses under {CONCURRENCY}-concurrent SELL:\n'
        + '\n'.join(f'  HTTP {s}: {t}' for s, t in non_200)
    )

    # 5-8. DB consistency — checked after TestClient lifespan exits (container still readable)
    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id, state FROM fiscal_documents "
            "WHERE doc_type = 'SELL' ORDER BY created_at"
        ).fetchall()

        # Exactly N SELL documents — no doubled or missing
        assert len(sell_docs) == CONCURRENCY, (
            f'Expected {CONCURRENCY} SELL docs, got {len(sell_docs)}. '
            f'Documents: {[(r[0], r[1]) for r in sell_docs]}'
        )

        # All ACK — no partial/stuck states after synchronous processing
        non_ack = [(row[0], row[1]) for row in sell_docs if row[1] != 'ACK']
        assert non_ack == [], (
            f'Non-ACK SELL documents after concurrent ingress: {non_ack}'
        )

        # No duplicate document_ids — LND atomicity preserved under concurrent writes
        doc_ids = [row[0] for row in sell_docs]
        assert len(doc_ids) == len(set(doc_ids)), (
            f'Duplicate document_ids detected (LND collision?): {doc_ids}'
        )

        # Shift still OPENED — concurrent SELLs must not accidentally close the shift
        shift = conn.execute(
            'SELECT state FROM shifts WHERE fiscal_number = ? '
            'ORDER BY created_at DESC LIMIT 1',
            (FISCAL_NUMBER,),
        ).fetchone()
        assert shift is not None, 'No shift found after concurrent SELLs'
        assert shift[0] == 'OPENED', (
            f'Shift state after {CONCURRENCY} concurrent SELLs: {shift[0]} '
            f'(expected OPENED)'
        )


def test_three_concurrent_workers_unique_lnds(tmp_path: Path) -> None:
    """Three concurrent workers must produce unique LNDs (Invariant 2)."""
    N = 3
    cfg = _config(tmp_path)
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    with TestClient(create_app(container)) as client:
        # Open shift first
        open_resp = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('3w-open'), 'business_ts': '2026-03-29T10:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {
                'external_request_id': '3w-ext-open',
                'cashier_id': 'cashier-3w',
            },
        })
        assert open_resp.status_code == 200, f'SHIFT_OPEN failed: {open_resp.text}'
        assert open_resp.json().get('document_state') == 'ACK', (
            f'SHIFT_OPEN not ACK: {open_resp.json()}'
        )

        # Enqueue N SELLs via 3 concurrent threads
        payloads = [_sell_payload(f'3w-sell-{i:03d}') for i in range(N)]
        responses: list = []
        thread_errors: list[Exception] = []

        with concurrent.futures.ThreadPoolExecutor(max_workers=N) as executor:
            futures = {
                executor.submit(client.post, '/v1/ingress/checkbox', json=payload): i
                for i, payload in enumerate(payloads)
            }
            for future in concurrent.futures.as_completed(futures):
                try:
                    responses.append(future.result())
                except Exception as exc:
                    thread_errors.append(exc)

    assert thread_errors == [], (
        f'Thread exception(s) in 3-worker concurrent SELL: {thread_errors}'
    )

    non_200 = [(r.status_code, r.text[:200]) for r in responses if r.status_code != 200]
    assert non_200 == [], f'Non-200 responses: {non_200}'

    # Verify unique LNDs via document_ids (doc_id encodes LND)
    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()

        assert len(sell_docs) == N, (
            f'Expected {N} SELL docs, got {len(sell_docs)}: {sell_docs}'
        )
        doc_ids = [row[0] for row in sell_docs]
        assert len(doc_ids) == len(set(doc_ids)), (
            f'Duplicate document_ids — LND collision detected: {doc_ids}'
        )
