"""
Sprint 2 / bounded step 1 — offline sync selector unit tests.

Verifies FiscalDocumentRepository.get_pending_for_offline_sync() and
count_pending_for_offline_sync() in isolation against an in-memory SQLite DB.

Coverage matrix:
  S1 — empty DB: returns [] and count=0
  S2 — state filter: ACK docs excluded, OFFLINE_LOCAL_ACK included
  S3 — submission_status filter: OFFLINE_LOCAL required; NULL excluded
  S4 — ordering: offline_fiscal_no ASC, lnd ASC, created_at ASC
  S5 — fiscal_number filter: restricts to a single FN scope
  S6 — exclusion from reconciliation: OFFLINE_LOCAL_ACK NOT in
       get_pending_for_reconciliation() — verifies the two selectors are disjoint
  S7 — count matches selector length

Invariants:
  #2 (single-writer): fiscal_number scope is the correct isolation boundary.
  #8 (recovery must not violate state transitions): OFFLINE_LOCAL_ACK must not
     appear in the reconciliation queue under any condition.
"""
from __future__ import annotations

import itertools
import uuid

from prro_gateway.enums import DocumentState
from prro_gateway.repositories.fiscal_documents import FiscalDocumentRepository

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

_FN1 = 'FN-SYNC-0001'
_FN2 = 'FN-SYNC-0002'
_BP = 'backend_checkbox_default'
_TP = 'transport_checkbox_rest_default'

# Module-level counter for globally unique lnd values within a test session.
# Each test gets a fresh in-memory DB so cross-test uniqueness is not required,
# but an always-incrementing counter avoids UNIQUE violations if pytest ever
# re-uses a connection or runs with -n (xdist).  itertools.count is thread-safe.
_lnd_counter = itertools.count(1)


def _next_lnd() -> int:
    return next(_lnd_counter)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _seed_inbox(conn, req_id: str, fiscal_number: str, doc_type: str) -> None:
    """Insert the minimal ingress_inbox row required by FK on fiscal_documents."""
    conn.execute(
        """
        INSERT INTO ingress_inbox
            (request_id, idempotency_key, protocol, operation_type,
             fiscal_number, payload_json, payload_sha256)
        VALUES (?, ?, 'CHECKBOX_REST', ?, ?, '{}', ?)
        """,
        (
            req_id,
            f'{doc_type.lower()}:{fiscal_number.lower()}:{req_id}',
            doc_type,
            fiscal_number,
            'sha-' + req_id[:8],
        ),
    )


def _insert_doc(
    conn,
    *,
    fiscal_number: str = _FN1,
    lnd: int | None = None,
    doc_type: str = 'SELL',
    fs_mode: str = 'OFFLINE',
    offline_fiscal_no: int | None = None,
    target_state: DocumentState = DocumentState.OFFLINE_LOCAL_ACK,
    submission_status: str | None = 'OFFLINE_LOCAL',
) -> str:
    """Create an ingress_inbox row + fiscal_document record and move it to target_state.

    Returns document_id.

    fiscal_documents.request_id is a FK → ingress_inbox.request_id, so inbox must
    be seeded first.  fiscal_documents.lnd has a UNIQUE constraint; pass an explicit
    value only when the test controls ordering by lnd — otherwise omit to get an
    auto-incremented value.
    """
    doc_id = str(uuid.uuid4())
    req_id = str(uuid.uuid4())
    effective_lnd = lnd if lnd is not None else _next_lnd()

    _seed_inbox(conn, req_id, fiscal_number, doc_type)

    FiscalDocumentRepository.create_prepared(
        conn,
        document_id=doc_id,
        request_id=req_id,
        fiscal_number=fiscal_number,
        lnd=effective_lnd,
        doc_type=doc_type,
        backend_profile_id=_BP,
        transport_profile_id=_TP,
        fs_mode=fs_mode,
        business_ts='2026-03-29T12:00:00Z',
        payload_json='{}',
        payload_sha256='sha-' + doc_id[:8],
        offline_fiscal_no=offline_fiscal_no,
    )
    if target_state != DocumentState.PREPARED:
        FiscalDocumentRepository.update_state(
            conn,
            document_id=doc_id,
            state=target_state,
            submission_status=submission_status,
        )
    conn.commit()
    return doc_id


# ---------------------------------------------------------------------------
# S1 — empty DB
# ---------------------------------------------------------------------------

def test_s1_empty_returns_empty(conn) -> None:
    result = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    assert result == [], f'Expected empty list on empty DB, got {result}'

    count = FiscalDocumentRepository.count_pending_for_offline_sync(conn)
    assert count == 0, f'Expected count=0 on empty DB, got {count}'


# ---------------------------------------------------------------------------
# S2 — state filter: only OFFLINE_LOCAL_ACK included
# ---------------------------------------------------------------------------

def test_s2_state_filter_excludes_online_ack(conn) -> None:
    """Online ACK document must not appear in the offline sync selector."""
    _insert_doc(
        conn,
        fiscal_number=_FN1,
        fs_mode='ONLINE',
        target_state=DocumentState.ACK,
        submission_status=None,
        offline_fiscal_no=None,
    )
    offline_id = _insert_doc(
        conn,
        fiscal_number=_FN1,
        fs_mode='OFFLINE',
        offline_fiscal_no=1001,
        target_state=DocumentState.OFFLINE_LOCAL_ACK,
        submission_status='OFFLINE_LOCAL',
    )

    result = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    ids = [r.document_id for r in result]

    assert offline_id in ids, f'OFFLINE_LOCAL_ACK doc must be in selector, got {ids}'
    assert len(result) == 1, f'Only 1 doc expected (offline), got {len(result)}: {ids}'
    assert result[0].state == DocumentState.OFFLINE_LOCAL_ACK


def test_s2_sent_doc_not_in_offline_selector(conn) -> None:
    """SENT (online in-flight) must not appear in offline selector."""
    _insert_doc(
        conn,
        fiscal_number=_FN1,
        fs_mode='ONLINE',
        target_state=DocumentState.SENT,
        submission_status=None,
        offline_fiscal_no=None,
    )

    result = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    assert result == [], f'SENT doc must not appear in offline selector, got {result}'


# ---------------------------------------------------------------------------
# S3 — submission_status filter: 'OFFLINE_LOCAL' required
# ---------------------------------------------------------------------------

def test_s3_submission_status_filter(conn) -> None:
    """OFFLINE_LOCAL_ACK doc without submission_status='OFFLINE_LOCAL' must be excluded."""
    # Doc in OFFLINE_LOCAL_ACK state but NULL submission_status (pathological edge case).
    # Must seed inbox separately because we need raw control over submission_status.
    doc_id_null = str(uuid.uuid4())
    req_id_null = str(uuid.uuid4())
    _seed_inbox(conn, req_id_null, _FN1, 'SELL')
    FiscalDocumentRepository.create_prepared(
        conn,
        document_id=doc_id_null,
        request_id=req_id_null,
        fiscal_number=_FN1,
        lnd=_next_lnd(),
        doc_type='SELL',
        backend_profile_id=_BP,
        transport_profile_id=_TP,
        fs_mode='OFFLINE',
        business_ts='2026-03-29T12:00:00Z',
        payload_json='{}',
        payload_sha256='sha-null-sub',
        offline_fiscal_no=9999,
    )
    FiscalDocumentRepository.update_state(
        conn,
        document_id=doc_id_null,
        state=DocumentState.OFFLINE_LOCAL_ACK,
        # submission_status deliberately omitted → remains NULL via COALESCE
    )
    conn.commit()

    # Normal offline doc with correct submission_status
    good_id = _insert_doc(
        conn,
        fiscal_number=_FN1,
        offline_fiscal_no=1001,
        target_state=DocumentState.OFFLINE_LOCAL_ACK,
        submission_status='OFFLINE_LOCAL',
    )

    result = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    ids = [r.document_id for r in result]

    assert good_id in ids, f'Doc with OFFLINE_LOCAL must be returned, got {ids}'
    assert doc_id_null not in ids, (
        f'Doc with NULL submission_status must NOT be returned, got {ids}'
    )
    assert len(result) == 1


# ---------------------------------------------------------------------------
# S4 — ordering: offline_fiscal_no ASC, lnd ASC
# ---------------------------------------------------------------------------

def test_s4_ordering_fiscal_no_asc(conn) -> None:
    """Documents must be returned in offline_fiscal_no ASC order."""
    # Each call omits lnd → auto-incremented (avoids UNIQUE violation).
    id_3000 = _insert_doc(conn, offline_fiscal_no=3000)
    id_1000 = _insert_doc(conn, offline_fiscal_no=1000)
    id_2000 = _insert_doc(conn, offline_fiscal_no=2000)

    result = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    ids = [r.document_id for r in result]

    assert ids == [id_1000, id_2000, id_3000], (
        f'Expected [1000, 2000, 3000] order, got fiscal_nos='
        f'{[r.offline_fiscal_no for r in result]}'
    )
    assert [r.offline_fiscal_no for r in result] == [1000, 2000, 3000]


def test_s4_ordering_lnd_secondary(conn) -> None:
    """When offline_fiscal_no is NULL, lnd ASC is the decisive sort key.

    Uses offline_fiscal_no=None to avoid the UNIQUE constraint while still
    exercising the secondary sort.  NULLs sort equal under ASC, so lnd order
    determines the final row sequence.  Docs are inserted in reverse lnd order
    to confirm the result is not insertion-order.
    """
    # lnd values: 300 inserted first, then 100, then 200 → expected order 100,200,300
    id_lnd3 = _insert_doc(conn, offline_fiscal_no=None, lnd=300)
    id_lnd1 = _insert_doc(conn, offline_fiscal_no=None, lnd=100)
    id_lnd2 = _insert_doc(conn, offline_fiscal_no=None, lnd=200)

    result = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    ids = [r.document_id for r in result]

    assert ids == [id_lnd1, id_lnd2, id_lnd3], (
        f'Expected lnd ASC order [100,200,300], got doc sequence: {ids}'
    )


# ---------------------------------------------------------------------------
# S5 — fiscal_number filter
# ---------------------------------------------------------------------------

def test_s5_fiscal_number_filter_isolates_scope(conn) -> None:
    """fiscal_number filter restricts selector to the requested FN only."""
    id_fn1 = _insert_doc(conn, fiscal_number=_FN1, offline_fiscal_no=1001)
    id_fn2 = _insert_doc(conn, fiscal_number=_FN2, offline_fiscal_no=2001)

    # Without filter: both returned
    all_docs = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    all_ids = {r.document_id for r in all_docs}
    assert {id_fn1, id_fn2} == all_ids, (
        f'Without filter, both FNs expected, got {all_ids}'
    )

    # With FN1 filter
    fn1_docs = FiscalDocumentRepository.get_pending_for_offline_sync(conn, fiscal_number=_FN1)
    fn1_ids = [r.document_id for r in fn1_docs]
    assert fn1_ids == [id_fn1], f'FN1 filter must return only FN1 doc, got {fn1_ids}'
    assert all(r.fiscal_number == _FN1 for r in fn1_docs)

    # With FN2 filter
    fn2_docs = FiscalDocumentRepository.get_pending_for_offline_sync(conn, fiscal_number=_FN2)
    fn2_ids = [r.document_id for r in fn2_docs]
    assert fn2_ids == [id_fn2], f'FN2 filter must return only FN2 doc, got {fn2_ids}'

    # Unknown FN: empty
    unknown = FiscalDocumentRepository.get_pending_for_offline_sync(conn, fiscal_number='FN-UNKNOWN')
    assert unknown == [], f'Unknown FN must return empty, got {unknown}'


# ---------------------------------------------------------------------------
# S6 — mutual exclusion from reconciliation queue
# ---------------------------------------------------------------------------

def test_s6_offline_local_ack_excluded_from_reconciliation(conn) -> None:
    """OFFLINE_LOCAL_ACK must never appear in get_pending_for_reconciliation().

    This is the state-machine invariant (INV-8): reconciliation operates on
    SENT|KVT1|KVT2|ERROR_RETRYABLE only. Offline-committed docs must not enter
    the retry loop.
    """
    # Seed offline doc
    _insert_doc(conn, fiscal_number=_FN1, offline_fiscal_no=1001)

    # Seed online SENT doc (as if crash-after-send)
    sent_id = _insert_doc(
        conn,
        fiscal_number=_FN1,
        fs_mode='ONLINE',
        offline_fiscal_no=None,
        target_state=DocumentState.SENT,
        submission_status=None,
    )

    offline_sync_docs = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    reconciliation_docs = FiscalDocumentRepository.get_pending_for_reconciliation(conn)

    offline_ids = {r.document_id for r in offline_sync_docs}
    recon_ids = {r.document_id for r in reconciliation_docs}

    # The two sets must be disjoint
    overlap = offline_ids & recon_ids
    assert overlap == set(), (
        f'Selectors must be disjoint — overlap found: {overlap}'
    )

    # Offline doc is in offline selector
    assert len(offline_sync_docs) >= 1, 'Offline doc must appear in sync selector'

    # Online SENT doc is in reconciliation selector
    assert sent_id in recon_ids, f'SENT doc must appear in reconciliation selector'


# ---------------------------------------------------------------------------
# S7 — count matches selector length
# ---------------------------------------------------------------------------

def test_s7_count_matches_selector_length(conn) -> None:
    """count_pending_for_offline_sync() must equal len(get_pending_for_offline_sync())."""
    assert FiscalDocumentRepository.count_pending_for_offline_sync(conn) == 0

    for i in range(1, 4):
        _insert_doc(conn, offline_fiscal_no=1000 + i)

    result = FiscalDocumentRepository.get_pending_for_offline_sync(conn)
    count = FiscalDocumentRepository.count_pending_for_offline_sync(conn)

    assert count == len(result), (
        f'count={count} must equal len(selector)={len(result)}'
    )
    assert count == 3
