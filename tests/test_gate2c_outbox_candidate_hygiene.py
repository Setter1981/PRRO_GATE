"""
Gate 2c — outbox candidate hygiene / selection smoke tests.

Verifies that the sync_outbox table's status model correctly separates
candidate (PENDING) records from terminal (DONE, DEAD) records, and that
a "pending sender" query would return only the right set.

Context:
  OutboxRepository has no get_pending() method yet — only enqueue_document().
  This test documents the intended selection contract at the SQL level:
    pending candidates: status = 'PENDING'
    in-flight:          status = 'SENDING'
    terminal:           status IN ('DONE', 'DEAD')

  The index idx_sync_outbox_status(status, next_attempt_at, fiscal_number,
  artifact_class, sequence_no) is designed for this selection pattern.

Two tests:
  A — After write-path ACK, outbox record is PENDING (candidate for delivery).
      Changing it to DONE makes it invisible to a PENDING-only query.
  B — Mixed status set: PENDING + DONE + DEAD.
      Only PENDING is returned by the candidate selection query.
      Terminal statuses do not contaminate the pending count.
"""
from __future__ import annotations

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


def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate2c'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate2c-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE2C-001',
            'updated_at': '2026-03-30T14:00:00+00:00',
        })
    if path.endswith('/receipts/sell'):
        return httpx.Response(200, json={
            'id': 'gate2c-receipt-001', 'status': 'DONE',
            'fiscal_code': 'RCPT-GATE2C-001',
            'updated_at': '2026-03-30T14:01:00+00:00',
        })
    raise AssertionError(f'gate2c: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str = 'gate2c.sqlite3') -> AppConfig:
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
            'channel_owner': 'gate2c',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE2C-LIC',
            'cashier_pin': '0000',
        },
    })


def _ctx(req_id: str, ts: str = '2026-03-30T14:01:00Z') -> dict:
    return {
        'request_id': req_id,
        'fiscal_number': FISCAL_NUMBER,
        'backend_profile_id': BACKEND_PROFILE,
        'transport_profile_id': TRANSPORT_PROFILE,
        'channel_owner': 'gate2c',
        'business_ts': ts,
    }


def _pending_count(conn, fiscal_number: str) -> int:
    """Simulates the query a future outbox sender would use."""
    return conn.execute(
        "SELECT COUNT(*) FROM sync_outbox WHERE status='PENDING' AND fiscal_number=?",
        (fiscal_number,),
    ).fetchone()[0]


# ---------------------------------------------------------------------------
# Test A — PENDING → DONE transition removes record from candidate set
# ---------------------------------------------------------------------------

def test_gate2c_done_status_excluded_from_pending_selection(tmp_path: Path) -> None:
    """
    A write-path ACK creates a PENDING outbox record.
    After delivery (status → DONE), it must not appear in PENDING selection.

    This test documents the terminal transition: DONE means delivered,
    no further processing needed.
    """
    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2c-open-a'), 'business_ts': '2026-03-30T14:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2c-a', 'cashier_id': 'cashier-gate2c'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2c-sell-a'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2c-a',
                'cashier_id': 'cashier-gate2c',
                'goods': [{'name': 'Tea', 'price': 1500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 1500}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    with container.connect() as conn:
        # After ACK: one PENDING outbox record from write-path
        pending_before = _pending_count(conn, FISCAL_NUMBER)
        assert pending_before >= 1, (
            f'At least 1 PENDING outbox record expected after ACK: got {pending_before}'
        )

        wp_outbox_id = f'{sell_doc_id}-outbox'
        row = conn.execute(
            "SELECT status FROM sync_outbox WHERE outbox_id=?",
            (wp_outbox_id,),
        ).fetchone()
        assert row is not None and row[0] == 'PENDING', (
            f'Write-path outbox must be PENDING: {row}'
        )

        # Simulate delivery: mark as DONE
        conn.execute(
            "UPDATE sync_outbox SET status='DONE' WHERE outbox_id=?",
            (wp_outbox_id,),
        )
        conn.commit()

        pending_after = _pending_count(conn, FISCAL_NUMBER)

    assert pending_after == pending_before - 1, (
        f'DONE record must be excluded from PENDING selection: '
        f'before={pending_before}, after={pending_after}'
    )


# ---------------------------------------------------------------------------
# Test B — Mixed PENDING + DONE + DEAD: only PENDING is a candidate
# ---------------------------------------------------------------------------

def test_gate2c_mixed_statuses_only_pending_is_candidate(tmp_path: Path) -> None:
    """
    With PENDING, DONE, and DEAD records in sync_outbox:
    - PENDING-only query returns exactly the PENDING count
    - DONE and DEAD records do not appear in PENDING selection
    - Terminal statuses do not contaminate the pending queue

    DEAD = permanently failed delivery (max retry exceeded).
    Future sender must skip DEAD records to avoid re-delivering failed docs.
    """
    cfg = _config(tmp_path, 'gate2c_b.sqlite3')
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        r_shift = client.post('/v1/ingress/checkbox', json={
            'context': {**_ctx('gate2c-open-b'), 'business_ts': '2026-03-30T14:00:00Z'},
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate2c-b', 'cashier_id': 'cashier-gate2c'},
        })
        assert r_shift.status_code == 200
        assert r_shift.json().get('document_state') == 'ACK'

        r_sell = client.post('/v1/ingress/checkbox', json={
            'context': _ctx('gate2c-sell-b'),
            'operation': 'SELL',
            'request': {
                'external_request_id': 'ext-sell-gate2c-b',
                'cashier_id': 'cashier-gate2c',
                'goods': [{'name': 'Coffee', 'price': 2500, 'quantity': 1000}],
                'payments': [{'type': 'CASH', 'amount': 2500}],
            },
        })
        assert r_sell.status_code == 200
        assert r_sell.json().get('document_state') == 'ACK'
        sell_doc_id = r_sell.json()['document_id']

    with container.connect() as conn:
        # Starting state: 1 PENDING (shift outbox) + 1 PENDING (sell outbox)
        # or possibly just sell; either way record the baseline
        baseline = _pending_count(conn, FISCAL_NUMBER)
        assert baseline >= 1

        # Inject DONE and DEAD records directly (simulating post-delivery states)
        conn.execute(
            """
            INSERT INTO sync_outbox (
                outbox_id, aggregate_type, aggregate_id, fiscal_number,
                artifact_class, sequence_no, destination, payload_path, payload_sha256, status
            ) VALUES (?, 'DOCUMENT', ?, ?, 'DOCUMENT', 0, 'CLOUD_HUB', '/archive/done', 'done', 'DONE')
            """,
            (f'{sell_doc_id}-done-sentinel', sell_doc_id, FISCAL_NUMBER),
        )
        conn.execute(
            """
            INSERT INTO sync_outbox (
                outbox_id, aggregate_type, aggregate_id, fiscal_number,
                artifact_class, sequence_no, destination, payload_path, payload_sha256, status
            ) VALUES (?, 'DOCUMENT', ?, ?, 'DOCUMENT', 0, 'CLOUD_HUB', '/archive/dead', 'dead', 'DEAD')
            """,
            (f'{sell_doc_id}-dead-sentinel', sell_doc_id, FISCAL_NUMBER),
        )
        conn.commit()

        # Total records increased by 2 (DONE + DEAD)
        total = conn.execute(
            "SELECT COUNT(*) FROM sync_outbox WHERE fiscal_number=?",
            (FISCAL_NUMBER,),
        ).fetchone()[0]
        assert total == baseline + 2, (
            f'Expected {baseline + 2} total records after inserts: got {total}'
        )

        # PENDING count must not include DONE and DEAD
        pending_after_inject = _pending_count(conn, FISCAL_NUMBER)

    assert pending_after_inject == baseline, (
        f'PENDING count must remain {baseline} after injecting DONE+DEAD records: '
        f'got {pending_after_inject}. DONE and DEAD must not appear in PENDING selection.'
    )

    with container.connect() as conn:
        # Individually confirm terminal records are excluded
        done_in_pending = conn.execute(
            "SELECT COUNT(*) FROM sync_outbox WHERE status='PENDING' AND outbox_id=?",
            (f'{sell_doc_id}-done-sentinel',),
        ).fetchone()[0]
        dead_in_pending = conn.execute(
            "SELECT COUNT(*) FROM sync_outbox WHERE status='PENDING' AND outbox_id=?",
            (f'{sell_doc_id}-dead-sentinel',),
        ).fetchone()[0]

    assert done_in_pending == 0, 'DONE record must not appear in PENDING query'
    assert dead_in_pending == 0, 'DEAD record must not appear in PENDING query'
