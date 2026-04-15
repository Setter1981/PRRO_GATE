"""
Gate 1p — offline idempotency replay smoke tests.

Verifies that a replayed SELL in OFFLINE mode:
  - does NOT create a second document,
  - does NOT allocate a second offline_fiscal_no from the range,
  - returns the existing inbox record (existing=True signal),
  - leaves inbox and document state consistent.

Idempotency mechanics:
  idempotency_key = f"{operation}:{fiscal_number}:{external_request_id}"
  InboxRepository.accept_command() tries INSERT; on UNIQUE constraint violation
  (idempotency_key is UNIQUE in schema) it falls back to get_by_idempotency_key()
  and returns (existing_record, is_replay=True).

  Because is_replay=True and the original record is already DONE, _maybe_process()
  finds no NEW records and returns process_result=None.  The response therefore
  carries "existing": true but no document_id/document_state in the body
  (doc_id comes from process_result, which is None on replay).

Offline fiscal_no safety:
  allocate_offline_code() is called inside _stage_acquire_and_validate(), which
  only runs when process_next() picks up a NEW inbox record.  A replayed request
  never creates a new inbox record, so no new code is ever allocated.

Two vectors:
  A — different request_id, same external_request_id (idempotency_key collision):
      canonical replay scenario — client retries with a fresh context.request_id
      but the same business ext id.
  B — identical request_id AND identical external_request_id (exact duplicate):
      bit-for-bit identical retry (e.g. network timeout, client didn't know the
      first attempt landed).

Invariants:
  #1 (no network in tx): offline path never reaches transport — asserted by mock.
  #4 (idempotency): replay must not allocate a second offline code.
  #5 (offline limits): session seeded within limits.
"""
from __future__ import annotations

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


# ---------------------------------------------------------------------------
# Transport mock: only shift-open allowed; receipt call → AssertionError.
# ---------------------------------------------------------------------------

def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1p'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1p-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1P-001',
            'updated_at': '2026-03-29T13:00:00+00:00',
        })
    raise AssertionError(
        f'gate1p: transport must NOT be called in OFFLINE mode — '
        f'unexpected {request.method} {request.url}'
    )


def _config(tmp_path: Path, db_name: str = 'gate1p.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate1p',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1P-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-29T13:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1p',
        'business_ts': ts,
    }


def _open_shift(client: TestClient) -> None:
    resp = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx('gate1p-open'), 'business_ts': '2026-03-29T13:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': 'ext-open-gate1p',
            'cashier_id': 'cashier-gate1p',
        },
    })
    assert resp.status_code == 200, f'SHIFT_OPEN failed: {resp.text}'
    assert resp.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {resp.json()}'


def _seed_offline(container: RuntimeContainer, session_id: str,
                  range_id: str, first_no: int = 1001, last_no: int = 1010) -> None:
    _session_start = (datetime.now(timezone.utc) - timedelta(minutes=5)).strftime('%Y-%m-%dT%H:%M:%S')
    with container.connect() as conn:
        conn.execute('BEGIN IMMEDIATE')
        NodeStateRepository.update_mode(conn, fiscal_number=FISCAL_NUMBER, mode=NodeMode.OFFLINE)
        OfflineRepository.create_open_session(
            conn,
            offline_session_id=session_id,
            fiscal_number=FISCAL_NUMBER,
            started_at=_session_start,
            status='OPEN',
            accumulated_month_seconds=0,
        )
        conn.execute(
            """
            INSERT INTO offline_ranges
                (range_id, fiscal_number, first_fiscal_no, last_fiscal_no,
                 next_fiscal_no, issued_at, status)
            VALUES (?, ?, ?, ?, ?, ?, 'ACTIVE')
            """,
            (range_id, FISCAL_NUMBER, first_no, last_no, first_no, _session_start),
        )
        conn.commit()


def _sell_body(req_id: str, ext_id: str) -> dict:
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1p',
            'goods': [{'name': 'Coffee', 'price': 5000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 5000}],
        },
    }


# ---------------------------------------------------------------------------
# Vector A — different request_id, same external_request_id
# ---------------------------------------------------------------------------

def test_gate1p_offline_replay_different_request_id(tmp_path: Path) -> None:
    """
    Client retries with a fresh request_id but the same external_request_id.

    Expected:
    - First SELL: ACK, offline_fiscal_no=1001, range.next_fiscal_no advances to 1002.
    - Replay: HTTP 200, "existing"=True, no second document, range unchanged.
    - DB: exactly 1 SELL document, offline_fiscal_no still 1001.
    - No PROCESSING lease leak.
    """
    SESSION_ID = 'gate1p-session-vec-a'
    RANGE_ID = 'gate1p-range-vec-a'
    FIRST_NO = 1001
    EXT_ID = 'ext-sell-gate1p-vec-a'

    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler)),
    )

    with TestClient(create_app(container)) as client:
        _open_shift(client)
        _seed_offline(container, SESSION_ID, RANGE_ID, first_no=FIRST_NO, last_no=1010)

        # --- Original SELL ---
        r_first = client.post('/v1/ingress/checkbox',
                              json=_sell_body('gate1p-sell-vec-a-001', EXT_ID))
        assert r_first.status_code == 200, f'First SELL failed: {r_first.text}'
        first_body = r_first.json()
        assert first_body.get('document_state') == 'OFFLINE_LOCAL_ACK', (
            f'First SELL must be OFFLINE_LOCAL_ACK: {first_body}'
        )
        original_doc_id = first_body['document_id']

        # Verify range consumed one code
        with container.connect() as conn:
            range_after_first = conn.execute(
                "SELECT next_fiscal_no FROM offline_ranges WHERE range_id = ?", (RANGE_ID,)
            ).fetchone()
        assert range_after_first[0] == FIRST_NO + 1, (
            f'After first SELL: next_fiscal_no should be {FIRST_NO + 1}, '
            f'got {range_after_first[0]}'
        )

        # --- Replay: same external_request_id, different request_id ---
        r_replay = client.post('/v1/ingress/checkbox',
                               json=_sell_body('gate1p-sell-vec-a-002', EXT_ID))

    assert r_replay.status_code == 200, f'Replay response non-200: {r_replay.text}'
    replay_body = r_replay.json()

    # Replay signal in response
    assert replay_body.get('existing') is True, (
        f'Replay must signal "existing": true, got: {replay_body}'
    )
    # Response carries original request_id (the existing inbox record)
    assert replay_body.get('request_id') == 'gate1p-sell-vec-a-001', (
        f'Replay response.request_id must be the original, got: {replay_body.get("request_id")!r}'
    )

    with container.connect() as conn:
        # Exactly 1 SELL document — no double-fiscalisation
        sell_docs = conn.execute(
            "SELECT document_id, state, offline_fiscal_no "
            "FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()

        # Range not consumed a second time
        range_after_replay = conn.execute(
            "SELECT next_fiscal_no FROM offline_ranges WHERE range_id = ?", (RANGE_ID,)
        ).fetchone()

        # No PROCESSING leak
        processing_count = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox WHERE status = 'PROCESSING'"
        ).fetchone()[0]

        # Inbox: only 1 record for EXT_ID (no duplicate inbox entry)
        inbox_count = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox "
            "WHERE idempotency_key = ?",
            (f'sell:{FISCAL_NUMBER.lower()}:{EXT_ID}',),
        ).fetchone()[0]

    assert len(sell_docs) == 1, (
        f'Exactly 1 SELL document must exist after replay, got {len(sell_docs)}. '
        f'Docs: {sell_docs}'
    )
    assert sell_docs[0][0] == original_doc_id, (
        f'Document id must be unchanged: {sell_docs[0][0]!r} != {original_doc_id!r}'
    )
    assert sell_docs[0][1] == 'OFFLINE_LOCAL_ACK', (
        f'Document must still be OFFLINE_LOCAL_ACK: {sell_docs[0][1]!r}'
    )
    assert sell_docs[0][2] == FIRST_NO, (
        f'offline_fiscal_no must remain {FIRST_NO}, got {sell_docs[0][2]!r}'
    )
    assert range_after_replay[0] == FIRST_NO + 1, (
        f'Range must not be consumed again: next_fiscal_no should still be {FIRST_NO + 1}, '
        f'got {range_after_replay[0]}'
    )
    assert processing_count == 0, (
        f'No PROCESSING lease must remain: {processing_count} record(s) stuck'
    )
    assert inbox_count == 1, (
        f'Only 1 inbox record must exist for this idempotency_key, got {inbox_count}'
    )


# ---------------------------------------------------------------------------
# Vector B — identical request_id (exact duplicate replay)
# ---------------------------------------------------------------------------

def test_gate1p_offline_replay_same_request_id(tmp_path: Path) -> None:
    """
    Client sends the exact same request twice (same request_id AND external_request_id).
    E.g. network timeout where the client retried without knowing the first landed.

    Expected:
    - First SELL: ACK, offline_fiscal_no=2001, range.next_fiscal_no → 2002.
    - Exact duplicate: HTTP 200, "existing"=True, range still at 2002, 1 doc only.
    """
    SESSION_ID = 'gate1p-session-vec-b'
    RANGE_ID = 'gate1p-range-vec-b'
    FIRST_NO = 2001
    SHARED_REQUEST_ID = 'gate1p-sell-vec-b'
    SHARED_EXT_ID = 'ext-sell-gate1p-vec-b'

    cfg = _config(tmp_path, 'gate1p_vec_b.sqlite3')
    container = RuntimeContainer(
        cfg,
        transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler)),
    )

    with TestClient(create_app(container)) as client:
        _open_shift(client)
        _seed_offline(container, SESSION_ID, RANGE_ID, first_no=FIRST_NO, last_no=2010)

        # --- Original SELL ---
        r_first = client.post('/v1/ingress/checkbox',
                              json=_sell_body(SHARED_REQUEST_ID, SHARED_EXT_ID))
        assert r_first.status_code == 200, f'First SELL failed: {r_first.text}'
        assert r_first.json().get('document_state') == 'OFFLINE_LOCAL_ACK', (
            f'First SELL must be OFFLINE_LOCAL_ACK: {r_first.json()}'
        )

        # --- Exact duplicate: same body ---
        r_dup = client.post('/v1/ingress/checkbox',
                            json=_sell_body(SHARED_REQUEST_ID, SHARED_EXT_ID))

    assert r_dup.status_code == 200, f'Duplicate SELL non-200: {r_dup.text}'
    dup_body = r_dup.json()

    # Replay signal
    assert dup_body.get('existing') is True, (
        f'Exact duplicate must signal "existing": true, got: {dup_body}'
    )

    with container.connect() as conn:
        sell_docs = conn.execute(
            "SELECT document_id, offline_fiscal_no "
            "FROM fiscal_documents WHERE doc_type = 'SELL'"
        ).fetchall()

        range_row = conn.execute(
            "SELECT next_fiscal_no FROM offline_ranges WHERE range_id = ?", (RANGE_ID,)
        ).fetchone()

        processing_count = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox WHERE status = 'PROCESSING'"
        ).fetchone()[0]

    assert len(sell_docs) == 1, (
        f'Exactly 1 SELL document must exist after exact duplicate, got {len(sell_docs)}'
    )
    assert sell_docs[0][1] == FIRST_NO, (
        f'offline_fiscal_no must remain {FIRST_NO}, got {sell_docs[0][1]!r}'
    )
    assert range_row[0] == FIRST_NO + 1, (
        f'Range consumed again: next_fiscal_no should be {FIRST_NO + 1}, got {range_row[0]}'
    )
    assert processing_count == 0, (
        f'No PROCESSING leak: {processing_count} record(s) stuck'
    )
