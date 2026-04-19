"""Gate 5A — DocumentState KVT1/KVT2 path through reconciliation.

DPS двухфазное подтверждение:
  SENT → (poll returns KVT1) → still_pending  (reconciliation keeps polling)
  KVT1 → (poll returns ACK)  → ACK + outbox entry
  KVT2 → (poll returns ACK)  → ACK (KVT2 documents are valid reconciliation candidates)

The current reconciliation implementation does not transition documents to KVT1/KVT2
state from poll responses — those are states set externally (e.g. by write_path or
offline_sync when DPS reports KVT receipt).  Reconciliation recognises KVT1/KVT2
documents as candidates and processes them to ACK/REJECTED/ERROR just like SENT docs.

Verified:
  - DocumentState.KVT1 and KVT2 rows are picked up by get_pending_for_reconciliation
  - A KVT1 document is promoted to ACK by reconcile_pending when poll returns ACK
  - A KVT2 document is promoted to ACK by reconcile_pending when poll returns ACK
  - An outbox entry is created when a KVT1 document reaches ACK
  - A poll response with state='KVT1' (DPS still processing) keeps doc as still_pending
    and does NOT mark the document ERROR_RETRYABLE

Invariants preserved:
  #1  No network/crypto inside transaction — reconcile_pending opens its own
      BEGIN IMMEDIATE per document; poll occurs outside.
  #8  Recovery must not violate state transitions — KVT1/KVT2 → ACK are valid
      transitions; no spurious state change is introduced.
"""
from __future__ import annotations

import sqlite3
import uuid
from datetime import UTC, datetime
from typing import Iterator

import pytest

from prro_gateway.enums import DocumentState, OperationType, Protocol, ShiftState
from prro_gateway.ports import PollResult
from prro_gateway.repositories import (
    FiscalDocumentRepository,
    InboxRepository,
    ShiftRepository,
)
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.services.reconciliation import ReconciliationService

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

FN = 'FN-DEV-0001'
BACKEND = 'backend_checkbox_default'
TRANSPORT = 'transport_checkbox_rest_default'

_seq: list[int] = [0]


def _uid(prefix: str) -> str:
    _seq[0] += 1
    return f'{prefix}-kvt5a-{_seq[0]}'


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _seed_shift(conn: sqlite3.Connection, shift_id: str) -> None:
    """Insert an OPENED shift so fiscal_documents FK is satisfied."""
    ShiftRepository.create_shift(
        conn,
        shift_id=shift_id,
        fiscal_number=FN,
        state=ShiftState.OPENED,
        open_mode='ONLINE',
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        protocol=Protocol.CHECKBOX_REST,
        integration_owner='test-gate5a',
        channel_lock_acquired_at=datetime.now(UTC).isoformat(),
    )


def _seed_inbox(conn: sqlite3.Connection, request_id: str, idem_key: str) -> None:
    """Insert a minimal inbox record so the fiscal_document FK is satisfied."""
    InboxRepository.accept_command(
        conn,
        request_id=request_id,
        idempotency_key=idem_key,
        protocol=Protocol.CHECKBOX_REST,
        operation_type=OperationType.SELL,
        fiscal_number=FN,
        backend_profile_id=BACKEND,
        transport_profile_id=TRANSPORT,
        channel_owner='test-gate5a',
        external_request_id=_uid('ext'),
        payload_json='{}',
        payload_sha256=_uid('sha'),
    )


def _insert_document_in_state(
    conn: sqlite3.Connection,
    *,
    state: DocumentState,
) -> str:
    """Insert a fiscal_document directly in the requested state, bypassing write_path.

    Returns the document_id.
    """
    shift_id = _uid('shift')
    request_id = _uid('req')
    doc_id = _uid('doc')

    conn.execute('BEGIN IMMEDIATE')
    _seed_shift(conn, shift_id)
    _seed_inbox(conn, request_id, _uid('idem'))
    lnd = NodeStateRepository.increment_lnd(conn, fiscal_number=FN)
    conn.execute(
        """
        INSERT INTO fiscal_documents
            (document_id, request_id, fiscal_number, shift_id, lnd, doc_type,
             backend_profile_id, transport_profile_id, fs_mode, state,
             business_ts, payload_json, payload_sha256,
             transport_request_id, submission_status)
        VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
        """,
        (
            doc_id, request_id, FN, shift_id, lnd, 'SELL',
            BACKEND, TRANSPORT, 'ONLINE', state.value,
            '2026-04-17T10:00:00+00:00', '{}', _uid('sha'),
            _uid('tx'), state.value,
        ),
    )
    conn.commit()
    return doc_id


class _StaticStatusClient:
    """Transport status client that always returns the same PollResult."""

    def __init__(self, result: PollResult) -> None:
        self._result = result
        self.call_count = 0

    def poll_status(self, **kwargs) -> PollResult:
        self.call_count += 1
        return self._result


class _SequenceStatusClient:
    """Returns results from a queue; last item is repeated when queue exhausted."""

    def __init__(self, results: list[PollResult]) -> None:
        self._results = list(results)
        self.call_count = 0

    def poll_status(self, **kwargs) -> PollResult:
        self.call_count += 1
        if len(self._results) > 1:
            return self._results.pop(0)
        return self._results[0]


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def test_kvt1_document_reconciled_to_ack(conn: sqlite3.Connection) -> None:
    """A document already in KVT1 state is promoted to ACK when poll returns ACK.

    Verifies acceptance criterion: DocumentState.KVT1 is picked up by
    get_pending_for_reconciliation and successfully transitions to ACK.
    """
    doc_id = _insert_document_in_state(conn, state=DocumentState.KVT1)

    ack_result = PollResult(
        state=DocumentState.ACK.value,
        submission_status='ACK',
        ack_at=datetime.now(UTC),
        response_json='{"status": "ACK"}',
    )
    svc = ReconciliationService(transport_status_client=_StaticStatusClient(ack_result))
    run = svc.reconcile_pending(conn, fiscal_number=FN)

    assert run.acked == 1, f'expected 1 acked, got {run}'
    assert run.retryable == 0
    assert run.still_pending == 0

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc is not None
    assert doc.state == DocumentState.ACK, f'expected ACK, got {doc.state}'
    assert doc.ack_at is not None, "ack_at must be set after ACK"


def test_kvt2_document_reconciled_to_ack(conn: sqlite3.Connection) -> None:
    """A document already in KVT2 state is promoted to ACK when poll returns ACK.

    Verifies acceptance criterion: DocumentState.KVT2 is picked up by
    get_pending_for_reconciliation and successfully transitions to ACK.
    Also verifies KVT2 documents are NOT treated as ERROR_RETRYABLE on the ACK path.
    """
    doc_id = _insert_document_in_state(conn, state=DocumentState.KVT2)

    ack_result = PollResult(
        state=DocumentState.ACK.value,
        submission_status='ACK',
        ack_at=datetime.now(UTC),
        response_json='{"status": "ACK"}',
    )
    svc = ReconciliationService(transport_status_client=_StaticStatusClient(ack_result))
    run = svc.reconcile_pending(conn, fiscal_number=FN)

    assert run.acked == 1, f'expected 1 acked, got {run}'
    assert run.retryable == 0
    assert run.still_pending == 0

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc is not None
    assert doc.state == DocumentState.ACK, f'expected ACK, got {doc.state}'
    assert doc.ack_at is not None, "ack_at must be set after ACK"


def test_kvt1_ack_produces_outbox_entry(conn: sqlite3.Connection) -> None:
    """KVT1→ACK reconciliation creates an outbox entry for downstream delivery.

    Full path test: doc starts in KVT1, reconcile_pending returns ACK,
    assert doc.state == ACK AND outbox entry exists.
    """
    doc_id = _insert_document_in_state(conn, state=DocumentState.KVT1)

    ack_result = PollResult(
        state=DocumentState.ACK.value,
        submission_status='ACK',
        ack_at=datetime.now(UTC),
        response_json='{"status": "ACK"}',
    )
    svc = ReconciliationService(transport_status_client=_StaticStatusClient(ack_result))
    run = svc.reconcile_pending(conn, fiscal_number=FN)

    assert run.acked == 1, f'expected 1 acked, got {run}'

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc is not None
    assert doc.state == DocumentState.ACK

    # Outbox entry must be enqueued by reconcile_pending
    outbox_entries = conn.execute(
        "SELECT outbox_id FROM sync_outbox WHERE aggregate_id = ?",
        (doc_id,),
    ).fetchall()
    assert len(outbox_entries) >= 1, (
        f'expected outbox entry for document {doc_id}, got none'
    )


def test_poll_returning_kvt1_state_keeps_document_pending(conn: sqlite3.Connection) -> None:
    """When poll returns state='KVT1', the document stays pending (still_pending += 1).

    This tests the scenario where DPS reports KVT1-phase receipt but not yet
    final confirmation. The document must NOT be moved to ERROR_RETRYABLE.
    """
    doc_id = _insert_document_in_state(conn, state=DocumentState.SENT)

    # DPS poll returns KVT1 — still waiting for final confirmation
    kvt1_poll = PollResult(
        state=DocumentState.KVT1.value,
        submission_status='KVT1_RECEIVED',
        response_json='{"status": "KVT1"}',
        retryable=False,
    )
    svc = ReconciliationService(transport_status_client=_StaticStatusClient(kvt1_poll))
    run = svc.reconcile_pending(conn, fiscal_number=FN)

    assert run.still_pending == 1, f'expected 1 still_pending, got {run}'
    assert run.acked == 0
    assert run.retryable == 0

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc is not None
    # Document must remain SENT — KVT1 poll response does not trigger a state change
    assert doc.state == DocumentState.SENT, (
        f'expected SENT (unchanged), got {doc.state}'
    )


def test_sent_to_kvt1_to_ack_multi_hop(conn: sqlite3.Connection) -> None:
    """SENT document reaches ACK via two reconciliation calls.

    First call: poll returns KVT1 — document stays still_pending (SENT unchanged).
    Second call: poll returns ACK  — document is promoted to ACK.

    Verifies that _SequenceStatusClient correctly sequences responses and that
    a SENT document is a valid reconciliation candidate on both calls.
    """
    doc_id = _insert_document_in_state(conn, state=DocumentState.SENT)

    kvt1_poll = PollResult(
        state=DocumentState.KVT1.value,
        submission_status='KVT1_RECEIVED',
        response_json='{"status": "KVT1"}',
        retryable=False,
    )
    ack_poll = PollResult(
        state=DocumentState.ACK.value,
        submission_status='ACK',
        ack_at=datetime.now(UTC),
        response_json='{"status": "ACK"}',
    )

    client = _SequenceStatusClient([kvt1_poll, ack_poll])
    svc = ReconciliationService(transport_status_client=client)

    # First reconciliation pass — DPS still processing (KVT1 response)
    run1 = svc.reconcile_pending(conn, fiscal_number=FN)
    assert run1.still_pending == 1, f'expected 1 still_pending on pass 1, got {run1}'
    assert run1.acked == 0
    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc is not None
    assert doc.state == DocumentState.SENT, (
        f'expected SENT after pass 1, got {doc.state}'
    )

    # Second reconciliation pass — DPS confirms ACK
    run2 = svc.reconcile_pending(conn, fiscal_number=FN)
    assert run2.acked == 1, f'expected 1 acked on pass 2, got {run2}'
    assert run2.still_pending == 0
    assert client.call_count == 2, f'expected 2 poll calls, got {client.call_count}'

    doc = FiscalDocumentRepository.get_by_id(conn, doc_id)
    assert doc is not None
    assert doc.state == DocumentState.ACK, f'expected ACK after pass 2, got {doc.state}'
    assert doc.ack_at is not None, "ack_at must be set after ACK"
