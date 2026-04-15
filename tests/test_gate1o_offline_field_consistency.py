"""
Gate 1o — offline field consistency smoke tests.

Verifies that after an offline SELL flow the offline-specific persisted fields
are consistent between ingress_inbox and fiscal_documents and are suitable for
audit/recovery.

Offline-specific fields in fiscal_documents:
  fs_mode              = 'OFFLINE'
  submission_status    = 'OFFLINE_LOCAL'
  offline_fiscal_no    = integer allocated from offline_ranges
  offline_fiscal_date  = business_ts of the request (isoformat)
  offline_session_id   = FK to the open offline_session
  response_json        = {'mode': 'OFFLINE_LOCAL', 'offline_fiscal_no': N}

ingress_inbox has NO offline-specific columns.
Consistency anchors between inbox and document:
  inbox.payload_sha256       == document.payload_sha256
  inbox.payload_json         == document.payload_json
  inbox.status               == 'DONE'  (lease released, not stuck)
  inbox.result_document_id   == document.document_id

Two test vectors:
  A — happy path: offline SELL completes ACK; all fields consistent.
  B — exhausted range: inbox finishes ERROR, no ACK document, no PROCESSING leak.

Invariants:
  #1 (no network in tx): offline path never reaches transport — asserted by mock.
  #5 (offline limits): session seeded within limits.
  #8 (no silent state violations): exhausted path must not yield ACK.
"""
from __future__ import annotations

import json
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

BUSINESS_TS = '2026-03-29T12:01:00Z'


# ---------------------------------------------------------------------------
# Transport mock — only allows shift-open calls; asserts on receipt calls.
# ---------------------------------------------------------------------------

def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1o'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1o-shift-001',
            'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1O-001',
            'updated_at': '2026-03-29T12:00:00+00:00',
        })
    raise AssertionError(
        f'gate1o: transport must NOT be called in OFFLINE mode — '
        f'unexpected {request.method} {request.url}'
    )


def _config(tmp_path: Path, db_name: str = 'gate1o.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate1o',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1O-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = BUSINESS_TS) -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate1o',
        'business_ts': ts,
    }


def _open_shift(client: TestClient) -> None:
    resp = client.post('/v1/ingress/checkbox', json={
        'context': {**_ctx('gate1o-open'), 'business_ts': '2026-03-29T12:00:00Z'},
        'operation': 'SHIFT_OPEN',
        'request': {
            'external_request_id': 'ext-open-gate1o',
            'cashier_id': 'cashier-gate1o',
        },
    })
    assert resp.status_code == 200, f'SHIFT_OPEN failed: {resp.text}'
    assert resp.json().get('document_state') == 'ACK', f'SHIFT_OPEN not ACK: {resp.json()}'


def _seed_offline(container: RuntimeContainer, session_id: str, range_id: str,
                  first_no: int = 1001, last_no: int = 1010) -> None:
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


def _sell_payload(req_id: str, ext_id: str) -> dict:
    return {
        'context': _ctx(req_id),
        'operation': 'SELL',
        'request': {
            'external_request_id': ext_id,
            'cashier_id': 'cashier-gate1o',
            'goods': [{'name': 'Coffee', 'price': 5000, 'quantity': 1000}],
            'payments': [{'type': 'CASH', 'amount': 5000}],
        },
    }


# ---------------------------------------------------------------------------
# Vector A — happy path: all offline fields consistent
# ---------------------------------------------------------------------------

def test_gate1o_offline_sell_fields_consistent(tmp_path: Path) -> None:
    """
    After offline SELL:
      - inbox.status == 'DONE', no PROCESSING leak.
      - inbox.result_document_id points to the created document.
      - inbox.payload_sha256 == document.payload_sha256.
      - inbox.payload_json == document.payload_json.
      - document.fs_mode == 'OFFLINE'.
      - document.submission_status == 'OFFLINE_LOCAL'.
      - document.offline_fiscal_no == first code from range.
      - document.offline_fiscal_date is set (= business_ts).
      - document.offline_session_id == seeded session.
      - document.response_json['offline_fiscal_no'] == document.offline_fiscal_no.
    """
    SESSION_ID = 'gate1o-session-vec-a'
    RANGE_ID = 'gate1o-range-vec-a'
    FIRST_NO = 1001

    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        _open_shift(client)
        _seed_offline(container, SESSION_ID, RANGE_ID, first_no=FIRST_NO, last_no=1010)

        resp = client.post('/v1/ingress/checkbox', json=_sell_payload(
            'gate1o-sell-vec-a', 'ext-sell-gate1o-vec-a'
        ))

    assert resp.status_code == 200, f'Offline SELL non-200: {resp.text}'
    body = resp.json()
    assert body.get('document_state') == 'OFFLINE_LOCAL_ACK', f'Expected OFFLINE_LOCAL_ACK, got: {body}'
    doc_id = body['document_id']

    with container.connect() as conn:
        inbox = conn.execute(
            "SELECT status, result_document_id, payload_json, payload_sha256 "
            "FROM ingress_inbox WHERE request_id = 'gate1o-sell-vec-a'",
        ).fetchone()
        doc = conn.execute(
            "SELECT state, fs_mode, submission_status, offline_fiscal_no, "
            "       offline_fiscal_date, offline_session_id, response_json, "
            "       payload_json, payload_sha256 "
            "FROM fiscal_documents WHERE document_id = ?",
            (doc_id,),
        ).fetchone()
        processing_count = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox WHERE status = 'PROCESSING'"
        ).fetchone()[0]

    assert inbox is not None, 'Inbox row not found'
    assert doc is not None, f'Document {doc_id!r} not found'

    (inbox_status, inbox_result_doc_id,
     inbox_payload_json, inbox_payload_sha256) = inbox

    (doc_state, doc_fs_mode, doc_submission_status, doc_offline_fiscal_no,
     doc_offline_fiscal_date, doc_offline_session_id, doc_response_json,
     doc_payload_json, doc_payload_sha256) = doc

    # ---- No PROCESSING leak ----
    assert processing_count == 0, (
        f'PROCESSING lease leaked: {processing_count} inbox record(s) still PROCESSING'
    )

    # ---- Inbox status ----
    assert inbox_status == 'DONE', (
        f'inbox.status must be DONE after offline ACK, got {inbox_status!r}'
    )

    # ---- Inbox result_document_id link ----
    assert inbox_result_doc_id == doc_id, (
        f'inbox.result_document_id={inbox_result_doc_id!r} != doc_id={doc_id!r}'
    )

    # ---- Payload consistency: inbox ↔ document ----
    assert inbox_payload_sha256 == doc_payload_sha256, (
        f'payload_sha256 diverged between inbox and document:\n'
        f'  inbox    = {inbox_payload_sha256!r}\n'
        f'  document = {doc_payload_sha256!r}'
    )
    assert inbox_payload_json == doc_payload_json, (
        'payload_json diverged between inbox and document — silent content drift'
    )

    # ---- Document offline-specific fields ----
    assert doc_state == 'OFFLINE_LOCAL_ACK', f'document.state must be OFFLINE_LOCAL_ACK, got {doc_state!r}'
    assert doc_fs_mode == 'OFFLINE', f'document.fs_mode must be OFFLINE, got {doc_fs_mode!r}'
    assert doc_submission_status == 'OFFLINE_LOCAL', (
        f'document.submission_status must be OFFLINE_LOCAL, got {doc_submission_status!r}'
    )
    assert doc_offline_fiscal_no == FIRST_NO, (
        f'document.offline_fiscal_no must be {FIRST_NO} (first range code), '
        f'got {doc_offline_fiscal_no!r}'
    )
    assert doc_offline_fiscal_date is not None, (
        'document.offline_fiscal_date must be set for offline documents'
    )
    assert doc_offline_session_id == SESSION_ID, (
        f'document.offline_session_id={doc_offline_session_id!r} != seeded session {SESSION_ID!r}'
    )

    # ---- response_json internal consistency ----
    response_parsed = json.loads(doc_response_json)
    assert response_parsed.get('mode') == 'OFFLINE_LOCAL', (
        f'response_json.mode must be OFFLINE_LOCAL, got {response_parsed!r}'
    )
    assert response_parsed.get('offline_fiscal_no') == doc_offline_fiscal_no, (
        f'response_json.offline_fiscal_no={response_parsed.get("offline_fiscal_no")!r} '
        f'!= document.offline_fiscal_no={doc_offline_fiscal_no!r} — internal inconsistency'
    )


# ---------------------------------------------------------------------------
# Vector B — exhausted range: inbox ERROR, no ACK document, no PROCESSING leak
# ---------------------------------------------------------------------------

def test_gate1o_exhausted_range_inbox_error_no_ack(tmp_path: Path) -> None:
    """
    When offline range is exhausted:
      - First SELL ACKs and exhausts the single code.
      - Second SELL must yield a non-ACK controlled error.
      - inbox.status == 'ERROR' for the second request (not PROCESSING).
      - No PROCESSING lease remains.
      - Only 1 SELL document exists in ACK state.
    """
    SESSION_ID = 'gate1o-session-vec-b'
    RANGE_ID = 'gate1o-range-vec-b'

    cfg = _config(tmp_path, 'gate1o_vec_b.sqlite3')
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        _open_shift(client)
        # Seed range with exactly 1 code
        _seed_offline(container, SESSION_ID, RANGE_ID, first_no=2001, last_no=2001)

        # First SELL: consumes the only code — must ACK
        r1 = client.post('/v1/ingress/checkbox', json=_sell_payload(
            'gate1o-sell-vec-b-1', 'ext-sell-gate1o-vec-b-1'
        ))
        assert r1.status_code == 200, f'First SELL failed: {r1.text}'
        assert r1.json().get('document_state') == 'OFFLINE_LOCAL_ACK', f'First SELL not OFFLINE_LOCAL_ACK: {r1.json()}'

        # Second SELL: no codes — must error
        r2 = client.post('/v1/ingress/checkbox', json=_sell_payload(
            'gate1o-sell-vec-b-2', 'ext-sell-gate1o-vec-b-2'
        ))

    # ---- Second SELL must not crash with 500 ----
    assert r2.status_code != 500, (
        f'Exhausted range must not cause HTTP 500, got {r2.status_code}: {r2.text}'
    )

    # ---- Second SELL must not ACK ----
    body2 = r2.json()
    assert body2.get('document_state') != 'ACK', (
        f'Exhausted-range SELL must not ACK: {body2}'
    )

    with container.connect() as conn:
        # ---- No PROCESSING leak ----
        processing_count = conn.execute(
            "SELECT COUNT(*) FROM ingress_inbox WHERE status = 'PROCESSING'"
        ).fetchone()[0]

        # ---- Second inbox row ended as ERROR ----
        second_inbox = conn.execute(
            "SELECT status FROM ingress_inbox WHERE request_id = 'gate1o-sell-vec-b-2'"
        ).fetchone()

        # ---- Only 1 SELL document in OFFLINE_LOCAL_ACK state ----
        ack_sell_count = conn.execute(
            "SELECT COUNT(*) FROM fiscal_documents "
            "WHERE doc_type = 'SELL' AND state = 'OFFLINE_LOCAL_ACK'"
        ).fetchone()[0]

        # ---- Range exhausted ----
        range_status = conn.execute(
            "SELECT status FROM offline_ranges WHERE range_id = ?", (RANGE_ID,)
        ).fetchone()

    assert processing_count == 0, (
        f'PROCESSING lease leaked after exhausted-range error: {processing_count} record(s)'
    )
    assert second_inbox is not None, 'Second inbox row not found at all'
    assert second_inbox[0] == 'ERROR', (
        f'Second inbox.status must be ERROR after exhausted-range, got {second_inbox[0]!r}'
    )
    assert ack_sell_count == 1, (
        f'Exactly 1 SELL ACK document must exist (only the first), found {ack_sell_count}'
    )
    assert range_status is not None and range_status[0] == 'EXHAUSTED', (
        f'Range must be EXHAUSTED, got {range_status}'
    )
