"""
Gate 1b — idempotency replay smoke: concurrent identical requests must not
double-fiscalise.

Two replay vectors covered:

  Vector A — same request_id (exact duplicate):
    N concurrent requests with context.request_id='...' identical.
    InboxRepository.accept_command: second INSERT fails IntegrityError on
    request_id UNIQUE → fallback returns existing record.
    Expected: 1 SELL document, all responses HTTP 200.
    Note: "existing" key is False even for replays in this vector because
    inbox.request_id == command.request_id (same field); replay is transparent
    to the enrichment layer but the DB is idempotent.

  Vector B — different request_id, same idempotency_key via external_request_id:
    N concurrent requests each with a unique context.request_id but identical
    request.external_request_id.
    idempotency_key = f"sell:{fiscal}:{external_request_id}" (same for all N).
    Second+ INSERTs fail IntegrityError on idempotency_key UNIQUE →
    fallback returns the first record (inbox.request_id from original).
    Expected: 1 SELL document, all responses HTTP 200, N-1 responses have
    "existing": true (inbox.request_id != command.request_id).

Invariants preserved:
  #2 (single-writer): acquire_lease LIMIT 1 — only one document created.
  #4 (idempotency): verified directly.
  #1 (no network in tx): not touched.
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
REPLAY_COUNT = 3


def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1b'})
    if path.endswith('/shifts'):
        return httpx.Response(200, json={
            'id': 'gate1b-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1B-001',
            'updated_at': '2026-03-29T11:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate1b-receipt',
            'status': 'DONE',
            'fiscal_code': 'RCPT-GATE1B',
            'updated_at': '2026-03-29T11:01:00+00:00',
        })
    raise AssertionError(f'gate1b_mock: unexpected {request.method} {request.url}')


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
            'channel_owner': 'gate1b',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1B-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T11:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1b',
        'business_ts': ts,
    }


def _open_shift(client: TestClient, req_id: str) -> None:
    resp = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx(req_id), 'business_ts': '2026-03-29T11:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': f'ext-open-{req_id}',
            'cashier_id': 'cashier-gate1b',
        },
    })
    assert resp.status_code == 200, f'SHIFT_OPEN failed: {resp.text}'
    assert resp.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {resp.json()}'


def _sell_payload_same_request_id(req_id: str, ext_id: str) -> dict:
    """Vector A: all replicas use the exact same request_id."""
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1b',
            'goods': [{'name': 'Item', 'price': 1000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1000}],
        },
    }


def _sell_payload_same_ext_id(req_id: str, ext_id: str) -> dict:
    """Vector B: unique request_id per replica, but shared external_request_id."""
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1b',
            'goods': [{'name': 'Item', 'price': 1000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 1000}],
        },
    }


def _run_concurrent(client: TestClient, payloads: list[dict]) -> tuple[list, list[Exception]]:
    responses: list = []
    errors: list[Exception] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=len(payloads)) as executor:
        futures = {
            executor.submit(client.post, '/v1/ingress/checkbox', json=payload): i
            for i, payload in enumerate(payloads)
        }
        for future in concurrent.futures.as_completed(futures):
            try:
                responses.append(future.result())
            except Exception as exc:
                errors.append(exc)
    return responses, errors


# ---------------------------------------------------------------------------
# Vector A — same request_id concurrent replay
# ---------------------------------------------------------------------------

def test_gate1b_same_request_id_concurrent_replay(tmp_path: Path) -> None:
    """
    N concurrent requests with identical request_id must create exactly 1 document.

    Idempotency path: second+ INSERT raises IntegrityError on request_id UNIQUE →
    accept_command returns (existing_record, is_replay=True) → REST response sets
    "existing": true for replay threads.

    Invariant: exactly N-1 (or more) responses have "existing": true.
    The one thread whose INSERT succeeded returns "existing": false (fresh accept).
    """
    cfg = _config(tmp_path, 'gate1b_vec_a.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    SHARED_REQUEST_ID = 'gate1b-sell-vec-a'
    SHARED_EXT_ID = 'ext-gate1b-vec-a'

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1b-open-vec-a')

        # All REPLAY_COUNT requests use the same request_id
        payloads = [
            _sell_payload_same_request_id(SHARED_REQUEST_ID, SHARED_EXT_ID)
            for _ in range(REPLAY_COUNT)
        ]
        responses, errors = _run_concurrent(client, payloads)

    assert errors == [], f'Thread exceptions (Vector A): {errors}'

    non_200 = [(r.status_code, r.text[:200]) for r in responses if r.status_code != 200]
    assert non_200 == [], f'Non-200 responses (Vector A): {non_200}'

    # All responses return the same request_id (the original)
    returned_ids = {r.json().get('request_id') for r in responses}
    assert returned_ids == {SHARED_REQUEST_ID}, (
        f'Vector A: expected all responses to return {SHARED_REQUEST_ID!r}, '
        f'got {returned_ids}'
    )

    # Replay signal: at least N-1 responses must have "existing": true
    bodies = [r.json() for r in responses]
    existing_count = sum(1 for b in bodies if b.get('existing') is True)
    assert existing_count >= REPLAY_COUNT - 1, (
        f'Vector A: expected at least {REPLAY_COUNT - 1} responses with "existing": true '
        f'(replay signal), got {existing_count}. Bodies: {bodies}'
    )

    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id, state FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()

        # Exactly 1 SELL document — no double-fiscalisation
        assert len(sell_docs) == 1, (
            f'Vector A: expected 1 SELL document, got {len(sell_docs)}. '
            f'Documents: {sell_docs}'
        )
        assert sell_docs[0][1] == 'ACK', (
            f'Vector A: SELL document not in ACK state: {sell_docs[0]}'
        )

        # Inbox: exactly 1 record for this request_id
        inbox_rows = conn.execute(
            'SELECT request_id, status FROM ingress_inbox WHERE request_id = ?',
            (SHARED_REQUEST_ID,),
        ).fetchall()
        assert len(inbox_rows) == 1, (
            f'Vector A: expected 1 inbox record for {SHARED_REQUEST_ID!r}, '
            f'got {len(inbox_rows)}'
        )

        # Shift still OPENED
        shift = conn.execute(
            'SELECT state FROM shifts WHERE fiscal_number = ? '
            'ORDER BY created_at DESC LIMIT 1',
            (FISCAL_NUMBER,),
        ).fetchone()
        assert shift is not None and shift[0] == 'OPENED', (
            f'Vector A: shift state after replay: {shift}'
        )


# ---------------------------------------------------------------------------
# Vector B — different request_id, same idempotency_key (external_request_id)
# ---------------------------------------------------------------------------

def test_gate1b_same_external_request_id_concurrent_replay(tmp_path: Path) -> None:
    """
    N concurrent requests with distinct request_ids but shared external_request_id
    must create exactly 1 document.

    Idempotency path: idempotency_key = f"sell:{fiscal}:{external_request_id}"
    is identical for all N requests. Second+ INSERTs raise IntegrityError on
    idempotency_key UNIQUE → accept_command fallback returns the original inbox
    record (with the first request_id).

    Observed: N-1 responses have "existing": true (inbox.request_id from the
    first request ≠ command.request_id from the replay).
    """
    cfg = _config(tmp_path, 'gate1b_vec_b.sqlite3')
    transport_client = httpx.Client(transport=httpx.MockTransport(_mock_handler))
    container = RuntimeContainer(cfg, transport_http_client=transport_client)

    SHARED_EXT_ID = 'ext-gate1b-vec-b'

    with TestClient(create_app(container)) as client:
        _open_shift(client, 'gate1b-open-vec-b')

        # REPLAY_COUNT requests: unique request_ids, shared external_request_id
        payloads = [
            _sell_payload_same_ext_id(f'gate1b-sell-vec-b-{i:03d}', SHARED_EXT_ID)
            for i in range(REPLAY_COUNT)
        ]
        responses, errors = _run_concurrent(client, payloads)

    assert errors == [], f'Thread exceptions (Vector B): {errors}'

    non_200 = [(r.status_code, r.text[:200]) for r in responses if r.status_code != 200]
    assert non_200 == [], f'Non-200 responses (Vector B): {non_200}'

    bodies = [r.json() for r in responses]

    # All responses return the SAME request_id (the first one that won the INSERT race)
    returned_ids = {b.get('request_id') for b in bodies}
    assert len(returned_ids) == 1, (
        f'Vector B: expected all responses to return the same (original) request_id, '
        f'got {returned_ids}'
    )

    # At least N-1 responses signal "existing": true
    existing_count = sum(1 for b in bodies if b.get('existing') is True)
    assert existing_count >= REPLAY_COUNT - 1, (
        f'Vector B: expected at least {REPLAY_COUNT - 1} responses with "existing": true, '
        f'got {existing_count}. Bodies: {bodies}'
    )

    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id, state FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()

        # Exactly 1 SELL document — no double-fiscalisation despite N concurrent requests
        assert len(sell_docs) == 1, (
            f'Vector B: expected 1 SELL document, got {len(sell_docs)}. '
            f'Documents: {sell_docs}'
        )
        assert sell_docs[0][1] == 'ACK', (
            f'Vector B: SELL document not in ACK state: {sell_docs[0]}'
        )

        # Inbox: exactly 1 record (only the first request succeeded in inserting)
        inbox_sell = conn.execute(
            "SELECT request_id, status FROM ingress_inbox "
            "WHERE operation_type = 'SELL' AND fiscal_number = ?",
            (FISCAL_NUMBER,),
        ).fetchall()
        assert len(inbox_sell) == 1, (
            f'Vector B: expected 1 SELL inbox record, got {len(inbox_sell)}. '
            f'Records: {inbox_sell}'
        )

        # Shift still OPENED
        shift = conn.execute(
            'SELECT state FROM shifts WHERE fiscal_number = ? '
            'ORDER BY created_at DESC LIMIT 1',
            (FISCAL_NUMBER,),
        ).fetchone()
        assert shift is not None and shift[0] == 'OPENED', (
            f'Vector B: shift state after replay: {shift}'
        )
