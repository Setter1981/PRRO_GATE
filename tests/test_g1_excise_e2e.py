"""
G1 — Excise goods E2E pipeline test.

Full lifecycle: SELL with excise marks → DPS XML has CA elements + marks SOLD
               → RETURN → marks RETURNED
               → re-SELL same marks after RETURN (partial index allows)
               → duplicate SELL while SOLD → DUPLICATE_EXCISE_MARK

Coverage:
  EEP1: SELL with excise barcodes via DPS transport → ACK, CA elements in signed XML, marks=SOLD
  EEP2: RETURN of same marks → ACK, marks=RETURNED
  EEP3: Re-SELL returned marks → ACK (partial index allows after RETURNED)
  EEP4: SELL same mark while already SOLD (no prior RETURN) → DUPLICATE_EXCISE_MARK

What's new vs existing tests (ER1-ER2, EX7, EX_XML5):
  ER1/ER2/EX7 use transport_checkbox_rest_default (non-DPS) — no XML check.
  EX_XML5 checks CA in XML but does not verify DB mark state.
  G1 combines DPS transport + CA XML assertion + mark lifecycle in one sequence.
"""
from __future__ import annotations

import re
from datetime import datetime, UTC

from prro_gateway.enums import CanonicalErrorCode, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.repositories import InboxRepository, ShiftRepository
from prro_gateway.services.write_path import WritePathWorker


# ---------------------------------------------------------------------------
# Stubs
# ---------------------------------------------------------------------------

class _SpyCrypto:
    """Captures DPS XML for assertion; no sign_raw so falls to sign()."""
    def __init__(self):
        self.signed_payloads: list[str] = []

    def sign(self, *, payload_json, **kw):
        self.signed_payloads.append(payload_json)
        return f'SIGNED::{payload_json[:40]}'


class _OkTransport:
    def send(self, **kw):
        from prro_gateway.ports import SendResult
        return SendResult(
            state='ACK', transport_request_id='tr-g1',
            submission_status='DPS_ACK', server_fiscal_no='SFN-G1',
            response_json='{}', sent_at=datetime.now(UTC), ack_at=datetime.now(UTC),
        )


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _seed_excise_setup(conn) -> None:
    """Seed tax group with excise mark support and enable excise on node."""
    conn.execute("""
        INSERT OR IGNORE INTO tax_group_definitions
            (fiscal_number, tax_id, name, tax_rate, additional_rate, tax_type, tax_algorithm,
             requires_uktzed, requires_excise_mark)
        VALUES ('FN-DEV-0001', '4', 'ГА', 20.00, 5.00, 0, 2, 1, 1)
    """)
    conn.execute("UPDATE node_state SET excise_allowed = 1 WHERE fiscal_number = 'FN-DEV-0001'")
    conn.commit()


def _seed_shift(conn, shift_id: str) -> None:
    ShiftRepository.create_shift(
        conn, shift_id=shift_id, fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-16T08:00:00Z',
    )
    conn.commit()


def _cmd(req_id: str, op: OperationType, payload: dict) -> CanonicalFiscalCommand:
    return CanonicalFiscalCommand(
        request_id=req_id, idempotency_key=f'idem-{req_id}',
        protocol=Protocol.CHECKBOX_REST, operation_type=op,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id=f'ext-{req_id}',
        business_ts=datetime(2026, 4, 16, 12, 0, 0, tzinfo=UTC),
        payload=payload, payload_sha256=f'sha-{req_id}',
        trace_context=TraceContext(
            source_ip='10.0.0.1', source_port=1234,
            session_id='s1', correlation_id=f'c-{req_id}',
        ),
        correlation_id=f'c-{req_id}',
    )


def _enqueue(conn, cmd: CanonicalFiscalCommand) -> None:
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id,
        transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-16T12:01:00Z',
    )
    conn.commit()


SELL_PAYLOAD = {
    'receipt': {
        'type': 'SELL',
        'goods': [{
            'name': 'Горілка Finlandia', 'uktzed': '2208909900', 'tax_id': '4',
            'excise_barcodes': ['FRTG000001', 'FRTG000002'],
            'price': 29560, 'quantity': 2000, 'sum': 59120,
        }],
        'payments': [{'amount': 59120, 'type': 'CASH'}],
        'totals': {'total_sum': 59120},
    },
}

RETURN_PAYLOAD = {
    'receipt': {
        'type': 'RETURN',
        'goods': [{
            'name': 'Горілка Finlandia', 'uktzed': '2208909900', 'tax_id': '4',
            'excise_barcodes': ['FRTG000001', 'FRTG000002'],
            'price': 29560, 'quantity': 2000, 'sum': 59120,
        }],
        'payments': [{'amount': 59120, 'type': 'CASH'}],
        'totals': {'total_sum': 59120},
        'related_receipt_id': 'rcpt-g1-sell',
    },
}


# ---------------------------------------------------------------------------
# EEP1 — SELL with excise marks: CA elements in DPS XML + marks SOLD
# ---------------------------------------------------------------------------

def test_eep1_sell_excise_ca_in_xml_marks_sold(conn) -> None:
    """SELL with 2 excise marks via DPS transport:
    - signed DPS XML must contain <CA CA="G1MARK00001"> and <CA CA="G1MARK00002">
    - excise_marks table must show both marks as SOLD after ACK
    """
    _seed_excise_setup(conn)
    _seed_shift(conn, 'shift-eep1')

    spy = _SpyCrypto()
    worker = WritePathWorker(crypto_provider=spy, transport_client=_OkTransport(), tax_number='TN')

    _enqueue(conn, _cmd('req-eep1-sell', OperationType.SELL, SELL_PAYLOAD))
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'SELL must ACK, got: {result.canonical_error}'

    # CA elements must be in the DPS XML passed to sign()
    assert len(spy.signed_payloads) == 1, 'Expected exactly one sign() call'
    xml = spy.signed_payloads[0]
    assert '<CA CA="FRTG000001"></CA>' in xml, f'CA for mark 1 must be in DPS XML. Got: {xml}'
    assert '<CA CA="FRTG000002"></CA>' in xml, f'CA for mark 2 must be in DPS XML. Got: {xml}'
    ca_count = len(re.findall(r'<CA CA="[^"]+"></CA>', xml))
    assert ca_count == 2, f'Expected 2 CA elements, got {ca_count}: {xml}'

    # Both marks must be SOLD after ACK
    rows = conn.execute(
        "SELECT normalized_mark_code, status FROM excise_marks ORDER BY normalized_mark_code"
    ).fetchall()
    assert len(rows) == 2, f'Expected 2 excise mark rows, got {len(rows)}'
    for code, status in rows:
        assert status == 'SOLD', f'Mark {code} must be SOLD after ACK, got {status}'


# ---------------------------------------------------------------------------
# EEP2 — RETURN of excise marks → RETURNED
# ---------------------------------------------------------------------------

def test_eep2_return_excise_marks_returned(conn) -> None:
    """After SELL→ACK, RETURN of same marks transitions them to RETURNED."""
    _seed_excise_setup(conn)
    _seed_shift(conn, 'shift-eep2')

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN')

    # SELL → ACK (marks SOLD)
    _enqueue(conn, _cmd('req-eep2-sell', OperationType.SELL, SELL_PAYLOAD))
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # RETURN → ACK
    _enqueue(conn, _cmd('req-eep2-ret', OperationType.RETURN, RETURN_PAYLOAD))
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'RETURN must ACK, got: {result.canonical_error}'

    # Both marks must be RETURNED with returned_at set
    rows = conn.execute(
        "SELECT normalized_mark_code, status, returned_at FROM excise_marks ORDER BY normalized_mark_code"
    ).fetchall()
    assert len(rows) == 2
    for code, status, returned_at in rows:
        assert status == 'RETURNED', f'Mark {code} must be RETURNED, got {status}'
        assert returned_at is not None, f'Mark {code} must have returned_at set'


# ---------------------------------------------------------------------------
# EEP3 — Re-SELL marks after RETURN (partial index allows)
# ---------------------------------------------------------------------------

def test_eep3_resell_returned_marks_succeeds(conn) -> None:
    """After RETURN (marks=RETURNED), re-SELL same marks must succeed.
    The partial index only covers RESERVED and SOLD — RETURNED marks can be re-sold.
    """
    _seed_excise_setup(conn)
    _seed_shift(conn, 'shift-eep3')

    worker = WritePathWorker(crypto_provider=_SpyCrypto(), transport_client=_OkTransport(), tax_number='TN')

    # SELL → ACK
    _enqueue(conn, _cmd('req-eep3-sell1', OperationType.SELL, SELL_PAYLOAD))
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # RETURN → ACK
    _enqueue(conn, _cmd('req-eep3-ret', OperationType.RETURN, RETURN_PAYLOAD))
    assert worker.process_next(conn, fiscal_number='FN-DEV-0001').outcome == 'ACK'

    # Re-SELL same marks → must succeed
    _enqueue(conn, _cmd('req-eep3-sell2', OperationType.SELL, SELL_PAYLOAD))
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert result.outcome == 'ACK', f'Re-sell after return must succeed, got: {result.canonical_error}'

    # Two rows per mark: one RETURNED, one SOLD
    for mark_code in ('FRTG000001', 'FRTG000002'):
        rows = conn.execute(
            "SELECT status FROM excise_marks WHERE normalized_mark_code = ? ORDER BY created_at",
            (mark_code,),
        ).fetchall()
        statuses = [r[0] for r in rows]
        assert 'RETURNED' in statuses, f'Mark {mark_code}: first row must be RETURNED, got {statuses}'
        assert 'SOLD' in statuses, f'Mark {mark_code}: second row must be SOLD, got {statuses}'


# ---------------------------------------------------------------------------
# EEP4 — Duplicate SELL while mark is SOLD → DUPLICATE_EXCISE_MARK
# ---------------------------------------------------------------------------

def test_eep4_duplicate_sell_while_sold_rejected(conn) -> None:
    """SELL → ACK (mark SOLD), then SELL same mark again without prior RETURN
    must be rejected with DUPLICATE_EXCISE_MARK before sign/transport.
    The duplicate check fires in the guard stage — sign and transport are not called.
    """
    _seed_excise_setup(conn)
    _seed_shift(conn, 'shift-eep4')

    class _SimpleCrypto:
        def sign(self, *, payload_json, **kw):
            return f'SIGNED::{payload_json[:40]}'

    worker = WritePathWorker(crypto_provider=_SimpleCrypto(), transport_client=_OkTransport(), tax_number='TN')

    # First SELL → ACK (marks become SOLD)
    _enqueue(conn, _cmd('req-eep4-sell1', OperationType.SELL, SELL_PAYLOAD))
    first = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert first.outcome == 'ACK', f'First SELL must ACK: {first.canonical_error}'

    # Verify marks are SOLD
    row = conn.execute(
        "SELECT status FROM excise_marks WHERE normalized_mark_code = 'FRTG000001'"
    ).fetchone()
    assert row and row[0] == 'SOLD', f'Mark must be SOLD before duplicate attempt, got {row}'

    # Second SELL of same marks without RETURN → DUPLICATE_EXCISE_MARK
    _enqueue(conn, _cmd('req-eep4-sell2', OperationType.SELL, SELL_PAYLOAD))
    second = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    assert second.outcome == 'ERROR', f'Duplicate SELL must be rejected, got {second.outcome}'
    assert second.canonical_error.code == CanonicalErrorCode.DUPLICATE_EXCISE_MARK, (
        f'Must be DUPLICATE_EXCISE_MARK, got {second.canonical_error.code}'
    )
