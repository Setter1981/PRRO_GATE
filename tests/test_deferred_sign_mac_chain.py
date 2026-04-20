"""Deferred-sign MAC chain tests (Approach B).

DSC1:  _inject_mac — happy path, n=0 warning, n>1 warning
DSC2:  _resolve_mac_from_last_acked — from ACKed PAYLOAD_XML, empty warning
DSC3:  single deferred-sign doc — MAC injected, signed, sent
DSC4:  two docs — MAC[2] == SHA256(corrected_bytes[1])
DSC5:  sign failure aborts batch — second doc stays OFFLINE_LOCAL_ACK
DSC6:  backward compat — pre-existing SIGNED_XML not re-signed
DSC7:  crypto_provider=None — warning + empty signed_payload
DSC8:  mixed batch [pre-signed, deferred] — MAC uses pre-signed PAYLOAD_XML
DSC9:  no PAYLOAD_XML on last ACKed — warning + node_state fallback
DSC10: persist failure — rollback, retryable, batch aborted
DSC11: lnd ordering governs MAC chain, not insertion order
"""
from __future__ import annotations

import hashlib
import logging
import sqlite3
from unittest.mock import patch

import pytest

from types import SimpleNamespace

from prro_gateway.enums import DocumentState, FileKind, Protocol, ShiftState
from prro_gateway.ports import SendResult
from prro_gateway.repositories import DocumentFilesRepository, FiscalDocumentRepository
from prro_gateway.repositories.shifts import ShiftRepository
from prro_gateway.services.offline_sync import OfflineSyncService


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _seed_deferred_doc(
    conn,
    *,
    doc_id: str,
    lnd: int,
    fiscal_number: str = 'FN-DSC-0001',
    payload_xml: bytes | None = b'<RQ><MAC>PLACEHOLDER</MAC><SN>1</SN></RQ>',
    signed_xml: bytes | None = None,
) -> None:
    req_id = f'req-{doc_id}'
    conn.execute('BEGIN IMMEDIATE')
    conn.execute("""
        INSERT OR IGNORE INTO ingress_inbox (
            request_id, idempotency_key, protocol, operation_type, fiscal_number,
            backend_profile_id, transport_profile_id, channel_owner,
            payload_json, payload_sha256, status, response_deadline_at
        ) VALUES (?, ?, 'CHECKBOX_REST', 'SELL', ?,
                  'backend_checkbox_default', 'transport_dps_grpc_default',
                  'test', '{}', 'sha256stub', 'DONE', '2099-01-01T00:00:00Z')
    """, (req_id, f'idem-{doc_id}', fiscal_number))
    conn.execute("""
        INSERT INTO fiscal_documents (
            document_id, request_id, fiscal_number, doc_type, state, fs_mode,
            lnd, backend_profile_id, transport_profile_id,
            submission_status, payload_json, payload_sha256,
            business_ts, offline_fiscal_no, offline_fiscal_date
        ) VALUES (?, ?, ?, 'SELL', 'OFFLINE_LOCAL_ACK', 'OFFLINE',
            ?, 'backend_checkbox_default', 'transport_dps_grpc_default',
            'OFFLINE_LOCAL', '{}', 'sha256stub',
            '2026-01-01T10:00:00+00:00', ?, '2026-01-01')
    """, (doc_id, req_id, fiscal_number, lnd, lnd))
    if payload_xml is not None:
        DocumentFilesRepository.add_file(
            conn,
            file_id=f'{doc_id}-payload-xml',
            document_id=doc_id,
            file_kind=FileKind.PAYLOAD_XML,
            path=f'/archive/{fiscal_number}/{doc_id}/payload.xml',
            content=payload_xml,
        )
    if signed_xml is not None:
        DocumentFilesRepository.add_file(
            conn,
            file_id=f'{doc_id}-signed',
            document_id=doc_id,
            file_kind=FileKind.SIGNED_XML,
            path=f'/archive/{fiscal_number}/{doc_id}/signed.xml',
            content=signed_xml,
        )
    conn.commit()


def _seed_acked_doc(conn, *, doc_id: str, lnd: int, fiscal_number: str, payload_xml: bytes) -> None:
    req_id = f'req-{doc_id}'
    conn.execute('BEGIN IMMEDIATE')
    conn.execute("""
        INSERT OR IGNORE INTO ingress_inbox (
            request_id, idempotency_key, protocol, operation_type, fiscal_number,
            backend_profile_id, transport_profile_id, channel_owner,
            payload_json, payload_sha256, status, response_deadline_at
        ) VALUES (?, ?, 'CHECKBOX_REST', 'SELL', ?,
                  'backend_checkbox_default', 'transport_dps_grpc_default',
                  'test', '{}', 'sha256stub', 'DONE', '2099-01-01T00:00:00Z')
    """, (req_id, f'idem-{doc_id}', fiscal_number))
    conn.execute("""
        INSERT INTO fiscal_documents (
            document_id, request_id, fiscal_number, doc_type, state, fs_mode,
            lnd, backend_profile_id, transport_profile_id,
            submission_status, payload_json, payload_sha256, business_ts
        ) VALUES (?, ?, ?, 'SELL', 'ACK', 'ONLINE',
            ?, 'backend_checkbox_default', 'transport_dps_grpc_default',
            'ACK', '{}', 'sha256stub', '2026-01-01T09:00:00+00:00')
    """, (doc_id, req_id, fiscal_number, lnd))
    DocumentFilesRepository.add_file(
        conn,
        file_id=f'{doc_id}-payload-xml',
        document_id=doc_id,
        file_kind=FileKind.PAYLOAD_XML,
        path=f'/archive/{fiscal_number}/{doc_id}/payload.xml',
        content=payload_xml,
    )
    conn.commit()


class _OkTransport:
    def send(self, **kw):
        return SendResult(
            state='ACK', transport_request_id='tr-ok',
            submission_status='DPS_ACK', server_fiscal_no='SFN',
            response_json='{}', sent_at=None, ack_at=None,
        )


class _CaptureTransport:
    def __init__(self):
        self.calls: list[dict] = []

    def send(self, **kw):
        self.calls.append(kw)
        return SendResult(
            state='ACK', transport_request_id=f'tr-{len(self.calls)}',
            submission_status='DPS_ACK', server_fiscal_no='SFN',
            response_json='{}', sent_at=None, ack_at=None,
        )


class _OkCrypto:
    def __init__(self, sign_result: bytes = b'FAKE_CMS'):
        self._result = sign_result
        self.calls: list[dict] = []

    def sign_raw(self, *, data: bytes, document_id: str) -> bytes:
        self.calls.append({'data': data, 'document_id': document_id})
        return self._result


class _FailCrypto:
    def sign_raw(self, *, data: bytes, document_id: str) -> bytes:
        raise RuntimeError('HSM unavailable')


# ---------------------------------------------------------------------------
# DSC1 — _inject_mac
# ---------------------------------------------------------------------------

def test_dsc1_inject_mac_replaces_placeholder():
    xml = '<RQ><MAC>OLD_VALUE</MAC><SN>1</SN></RQ>'
    result = OfflineSyncService._inject_mac(xml, 'deadbeef')
    assert '<MAC>deadbeef</MAC>' in result
    assert 'OLD_VALUE' not in result


def test_dsc1_inject_mac_no_match_raises_value_error():
    xml = '<RQ><SN>1</SN></RQ>'
    with pytest.raises(ValueError, match='<MAC> element not found'):
        OfflineSyncService._inject_mac(xml, 'deadbeef')


def test_dsc1_inject_mac_duplicate_logs_warning(caplog):
    xml = '<RQ><MAC>A</MAC><DATA><MAC>B</MAC></DATA></RQ>'
    with caplog.at_level(logging.WARNING, logger='prro_gateway.offline_sync'):
        OfflineSyncService._inject_mac(xml, 'deadbeef')
    assert 'offline_sync_mac_element_duplicate' in caplog.text


# ---------------------------------------------------------------------------
# DSC2 — _resolve_mac_from_last_acked
# ---------------------------------------------------------------------------

def test_dsc2_resolve_mac_from_last_acked(conn):
    payload = b'<RQ><MAC>SEED</MAC></RQ>'
    _seed_acked_doc(conn, doc_id='acked-dsc2', lnd=1, fiscal_number='FN-DSC-MAC', payload_xml=payload)

    class _Doc:
        fiscal_number = 'FN-DSC-MAC'
        transport_profile_id = 'transport_dps_grpc_default'

    mac = OfflineSyncService._resolve_mac_from_last_acked(conn, _Doc())
    assert mac == hashlib.sha256(payload).hexdigest()


def test_dsc2_resolve_mac_empty_when_no_history(conn, caplog):
    class _Doc:
        fiscal_number = 'FN-BRAND-NEW'
        transport_profile_id = 'transport_dps_grpc_default'

    with caplog.at_level(logging.WARNING, logger='prro_gateway.offline_sync'):
        mac = OfflineSyncService._resolve_mac_from_last_acked(conn, _Doc())

    assert mac == ''
    assert 'offline_sync_mac_seed_empty' in caplog.text


# ---------------------------------------------------------------------------
# DSC3 — single deferred-sign doc
# ---------------------------------------------------------------------------

def test_dsc3_single_doc_signed_before_transport(conn):
    payload_xml = b'<RQ><MAC>PLACEHOLDER</MAC><SN>1</SN></RQ>'
    _seed_deferred_doc(conn, doc_id='doc-dsc3', lnd=10, payload_xml=payload_xml)

    crypto = _OkCrypto(sign_result=b'REAL_CMS_BYTES')
    transport = _CaptureTransport()
    svc = OfflineSyncService(transport_client=transport, crypto_provider=crypto)
    result = svc.sync_pending(conn, fiscal_number='FN-DSC-0001')

    assert result.synced == 1
    assert len(crypto.calls) == 1, 'sign_raw must be called exactly once'
    assert transport.calls[0]['signed_payload'] == 'REAL_CMS_BYTES'


# ---------------------------------------------------------------------------
# DSC4 — sequential MAC chain across two docs
# ---------------------------------------------------------------------------

def test_dsc4_mac_chain_sequential(conn):
    payload1 = b'<RQ><MAC>PLACEHOLDER</MAC><SN>1</SN></RQ>'
    payload2 = b'<RQ><MAC>PLACEHOLDER</MAC><SN>2</SN></RQ>'
    _seed_deferred_doc(conn, doc_id='doc-chain-1', lnd=20, payload_xml=payload1)
    _seed_deferred_doc(conn, doc_id='doc-chain-2', lnd=21, payload_xml=payload2)

    crypto = _OkCrypto(sign_result=b'SIGNED')
    transport = _CaptureTransport()
    svc = OfflineSyncService(transport_client=transport, crypto_provider=crypto)
    svc.sync_pending(conn, fiscal_number='FN-DSC-0001')

    assert len(crypto.calls) == 2, 'both docs must be signed'

    # corrected_bytes for doc-1 (what was passed to sign_raw)
    corrected_bytes_1: bytes = crypto.calls[0]['data']
    # MAC for doc-2 must equal SHA256(corrected_bytes_1)
    expected_mac_2 = hashlib.sha256(corrected_bytes_1).hexdigest()
    corrected_xml_2 = crypto.calls[1]['data'].decode('utf-8')
    assert f'<MAC>{expected_mac_2}</MAC>' in corrected_xml_2, (
        f'MAC chain broken: expected <MAC>{expected_mac_2[:16]}...</MAC> in doc-2, '
        f'got: {corrected_xml_2}'
    )


# ---------------------------------------------------------------------------
# DSC5 — sign failure aborts batch; second doc untouched
# ---------------------------------------------------------------------------

def test_dsc5_sign_failure_aborts_batch(conn):
    payload1 = b'<RQ><MAC>PLACEHOLDER</MAC><SN>1</SN></RQ>'
    payload2 = b'<RQ><MAC>PLACEHOLDER</MAC><SN>2</SN></RQ>'
    _seed_deferred_doc(conn, doc_id='doc-abort-1', lnd=30, payload_xml=payload1)
    _seed_deferred_doc(conn, doc_id='doc-abort-2', lnd=31, payload_xml=payload2)

    svc = OfflineSyncService(transport_client=_OkTransport(), crypto_provider=_FailCrypto())
    result = svc.sync_pending(conn, fiscal_number='FN-DSC-0001')

    assert result.synced == 0, 'nothing must be ACKed after sign failure'
    assert result.retryable == 1, 'only first doc must be marked retryable'

    doc1 = FiscalDocumentRepository.get_by_id(conn, 'doc-abort-1')
    assert (doc1.recovery_attempts or 0) == 1, 'first doc must have recovery_attempts=1'

    # Second doc must NOT be processed — MAC chain would be wrong
    doc2 = FiscalDocumentRepository.get_by_id(conn, 'doc-abort-2')
    assert doc2 is not None
    assert doc2.state == DocumentState.OFFLINE_LOCAL_ACK, (
        f'doc-abort-2 must remain OFFLINE_LOCAL_ACK after batch abort, got {doc2.state}'
    )
    assert (doc2.recovery_attempts or 0) == 0, 'second doc must not have had a recovery attempt'


# ---------------------------------------------------------------------------
# DSC6 — backward compat: pre-existing SIGNED_XML not re-signed
# ---------------------------------------------------------------------------

def test_dsc6_presigned_doc_not_re_signed(conn):
    _seed_deferred_doc(
        conn,
        doc_id='doc-presigned',
        lnd=40,
        payload_xml=b'<RQ><MAC>ORIG</MAC></RQ>',
        signed_xml=b'ALREADY_SIGNED_CMS',
    )

    crypto = _OkCrypto()
    transport = _CaptureTransport()
    svc = OfflineSyncService(transport_client=transport, crypto_provider=crypto)
    svc.sync_pending(conn, fiscal_number='FN-DSC-0001')

    assert len(crypto.calls) == 0, 'crypto must NOT be called for pre-signed doc'
    assert transport.calls[0]['signed_payload'] == 'ALREADY_SIGNED_CMS'


# ---------------------------------------------------------------------------
# DSC7 — crypto_provider=None: deferred doc falls through to empty signed_payload
# ---------------------------------------------------------------------------

def test_dsc7_no_crypto_provider_sends_empty_payload(conn, caplog):
    _seed_deferred_doc(conn, doc_id='doc-no-crypto', lnd=50, payload_xml=b'<RQ><MAC>X</MAC></RQ>')

    transport = _CaptureTransport()
    svc = OfflineSyncService(transport_client=transport, crypto_provider=None)
    with caplog.at_level(logging.WARNING, logger='prro_gateway.offline_sync'):
        svc.sync_pending(conn, fiscal_number='FN-DSC-0001')

    assert 'offline_sync_missing_signed_payload' in caplog.text
    assert transport.calls[0]['signed_payload'] == ''


# ---------------------------------------------------------------------------
# DSC8 — mixed batch: [pre-signed, deferred]; MAC chain uses pre-signed PAYLOAD_XML
# ---------------------------------------------------------------------------

def test_dsc8_mixed_batch_mac_chain(conn):
    presigned_payload = b'<RQ><MAC>PREV_MAC</MAC><SN>50</SN></RQ>'
    deferred_payload = b'<RQ><MAC>PLACEHOLDER</MAC><SN>51</SN></RQ>'
    # pre-signed doc (lnd=60): has both PAYLOAD_XML and SIGNED_XML
    _seed_deferred_doc(
        conn, doc_id='doc-mix-presigned', lnd=60,
        payload_xml=presigned_payload,
        signed_xml=b'PRESIGNED_CMS',
    )
    # deferred doc (lnd=61): PAYLOAD_XML only
    _seed_deferred_doc(conn, doc_id='doc-mix-deferred', lnd=61, payload_xml=deferred_payload)

    crypto = _OkCrypto(sign_result=b'NEW_CMS')
    svc = OfflineSyncService(transport_client=_CaptureTransport(), crypto_provider=crypto)
    svc.sync_pending(conn, fiscal_number='FN-DSC-0001')

    assert len(crypto.calls) == 1, 'crypto called only for the deferred doc'
    expected_mac = hashlib.sha256(presigned_payload).hexdigest()
    corrected_xml = crypto.calls[0]['data'].decode('utf-8')
    assert f'<MAC>{expected_mac}</MAC>' in corrected_xml, (
        f'deferred doc MAC must be SHA256(pre-signed PAYLOAD_XML). '
        f'Expected {expected_mac[:16]}..., got: {corrected_xml}'
    )


# ---------------------------------------------------------------------------
# DSC9 — _resolve_mac_from_last_acked: no PAYLOAD_XML → warning + last_known_mac fallback
# ---------------------------------------------------------------------------

def test_dsc9_resolve_mac_no_payload_xml_uses_node_state(conn, caplog):
    # Seed ACKed doc WITHOUT any document_files
    req_id = 'req-acked-no-payload'
    conn.execute('BEGIN IMMEDIATE')
    conn.execute("""
        INSERT OR IGNORE INTO ingress_inbox (
            request_id, idempotency_key, protocol, operation_type, fiscal_number,
            backend_profile_id, transport_profile_id, channel_owner,
            payload_json, payload_sha256, status, response_deadline_at
        ) VALUES (?, ?, 'CHECKBOX_REST', 'SELL', 'FN-DSC-NOPAYLOAD',
                  'backend_checkbox_default', 'transport_dps_grpc_default',
                  'test', '{}', 'sha', 'DONE', '2099-01-01T00:00:00Z')
    """, (req_id, 'idem-acked-no-payload'))
    conn.execute("""
        INSERT INTO fiscal_documents (
            document_id, request_id, fiscal_number, doc_type, state, fs_mode,
            lnd, backend_profile_id, transport_profile_id,
            submission_status, payload_json, payload_sha256, business_ts
        ) VALUES ('acked-no-payload', ?, 'FN-DSC-NOPAYLOAD', 'SELL', 'ACK', 'ONLINE',
            70, 'backend_checkbox_default', 'transport_dps_grpc_default',
            'ACK', '{}', 'sha', '2026-01-01T09:00:00+00:00')
    """, (req_id,))
    # Seed node_state with last_known_mac
    conn.execute("""
        INSERT INTO node_state (
            node_id, fiscal_number, mode, shift_state, next_lnd,
            readiness_state, recovery_stage, current_month_bucket,
            current_month_offline_seconds, last_known_mac
        ) VALUES ('node-dsc9', 'FN-DSC-NOPAYLOAD', 'ONLINE', 'CLOSED', 1,
                  'READY', 'DONE', '', 0, 'SEED_MAC_FROM_NODE_STATE')
    """)
    conn.commit()

    class _Doc:
        fiscal_number = 'FN-DSC-NOPAYLOAD'
        transport_profile_id = 'transport_dps_grpc_default'

    with caplog.at_level(logging.WARNING, logger='prro_gateway.offline_sync'):
        mac = OfflineSyncService._resolve_mac_from_last_acked(conn, _Doc())

    assert 'offline_sync_mac_last_acked_no_payload_xml' in caplog.text
    assert mac == 'SEED_MAC_FROM_NODE_STATE'


# ---------------------------------------------------------------------------
# DSC10 — persist failure: rollback, doc stays retryable, batch aborted
# ---------------------------------------------------------------------------

def test_dsc10_persist_failure_rollback_and_abort(conn):
    _seed_deferred_doc(conn, doc_id='doc-pf-1', lnd=80, payload_xml=b'<RQ><MAC>P</MAC></RQ>')
    _seed_deferred_doc(conn, doc_id='doc-pf-2', lnd=81, payload_xml=b'<RQ><MAC>P</MAC></RQ>')

    crypto = _OkCrypto()
    svc = OfflineSyncService(transport_client=_OkTransport(), crypto_provider=crypto)

    with patch.object(
        DocumentFilesRepository,
        'add_file',
        side_effect=sqlite3.OperationalError('disk full'),
    ):
        result = svc.sync_pending(conn, fiscal_number='FN-DSC-0001')

    assert result.synced == 0
    assert result.retryable == 1, 'first doc must be retryable'

    doc1 = FiscalDocumentRepository.get_by_id(conn, 'doc-pf-1')
    assert doc1.state == DocumentState.OFFLINE_LOCAL_ACK
    assert (doc1.recovery_attempts or 0) == 1

    doc2 = FiscalDocumentRepository.get_by_id(conn, 'doc-pf-2')
    assert doc2.state == DocumentState.OFFLINE_LOCAL_ACK, 'batch must abort after persist failure'
    assert (doc2.recovery_attempts or 0) == 0

    # PAYLOAD_XML for doc-1 must still be intact after rollback
    payload_after = DocumentFilesRepository.get_content(conn, document_id='doc-pf-1', file_kind=FileKind.PAYLOAD_XML)
    assert payload_after == b'<RQ><MAC>P</MAC></RQ>', 'rollback must restore original PAYLOAD_XML'


# ---------------------------------------------------------------------------
# DSC11 — lnd ordering governs MAC chain, not insertion order
# ---------------------------------------------------------------------------

def test_dsc11_lnd_ordering_governs_mac_chain(conn):
    # Insert in REVERSE lnd order to confirm ordering is by lnd, not insertion
    payload_90 = b'<RQ><MAC>PLACEHOLDER</MAC><SN>90</SN></RQ>'
    payload_91 = b'<RQ><MAC>PLACEHOLDER</MAC><SN>91</SN></RQ>'
    _seed_deferred_doc(conn, doc_id='doc-ord-91', lnd=91, payload_xml=payload_91)  # inserted first
    _seed_deferred_doc(conn, doc_id='doc-ord-90', lnd=90, payload_xml=payload_90)  # inserted second

    crypto = _OkCrypto(sign_result=b'SIGNED')
    svc = OfflineSyncService(transport_client=_CaptureTransport(), crypto_provider=crypto)
    svc.sync_pending(conn, fiscal_number='FN-DSC-0001')

    assert len(crypto.calls) == 2

    # First signed must be lnd=90 (lower lnd = earlier in fiscal sequence)
    first_signed_xml = crypto.calls[0]['data'].decode('utf-8')
    assert '<SN>90</SN>' in first_signed_xml, 'lnd=90 must be processed first regardless of insertion order'

    # MAC for lnd=91 must equal SHA256(corrected_bytes of lnd=90)
    corrected_90 = crypto.calls[0]['data']
    expected_mac_91 = hashlib.sha256(corrected_90).hexdigest()
    second_signed_xml = crypto.calls[1]['data'].decode('utf-8')
    assert f'<MAC>{expected_mac_91}</MAC>' in second_signed_xml, (
        f'MAC chain must follow lnd order. Expected SHA256(lnd=90), got: {second_signed_xml[:200]}'
    )


# ---------------------------------------------------------------------------
# A3 (H-2) — shift state guard in _apply_shift_side_effects_locked
# ---------------------------------------------------------------------------

def _fake_shift_doc(doc_type: str = 'SHIFT_OPEN') -> object:
    return SimpleNamespace(
        doc_type=doc_type,
        fiscal_number='FN-DSC-0001',
        document_id='doc-shift-test',
        fs_mode='OFFLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        request_id='req-shift-test',
        ack_at=None,
    )


def _create_shift(conn, shift_id: str, state: ShiftState) -> None:
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(
        conn,
        shift_id=shift_id,
        fiscal_number='FN-DSC-0001',
        state=state,
        open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol('CHECKBOX_REST'),
        integration_owner='test',
        channel_lock_acquired_at='2026-01-01T10:00:00+00:00',
    )
    conn.commit()


def test_a3_shift_in_closing_not_moved_to_opened(conn, caplog):
    """A3 (H-2): CLOSING shift must stay CLOSING on deferred SHIFT_OPEN ACK — not transitioned to OPENED."""
    _create_shift(conn, 'shift-a3-closing', ShiftState.CLOSING)

    with caplog.at_level(logging.WARNING, logger='prro_gateway.offline_sync'):
        OfflineSyncService._apply_shift_side_effects_locked(
            conn, doc=_fake_shift_doc('SHIFT_OPEN'), target_state=DocumentState.ACK,
        )

    shift = ShiftRepository.get_by_id(conn, 'shift-a3-closing')
    assert shift.state == ShiftState.CLOSING, 'CLOSING shift must not be moved to OPENED'
    assert 'offline_sync_shift_invalid_state_for_open' in caplog.text


def test_a3_shift_opening_transitions_to_opened(conn):
    """A3 (H-2): OPENING shift IS correctly transitioned to OPENED."""
    _create_shift(conn, 'shift-a3-opening', ShiftState.OPENING)

    OfflineSyncService._apply_shift_side_effects_locked(
        conn, doc=_fake_shift_doc('SHIFT_OPEN'), target_state=DocumentState.ACK,
    )

    shift = ShiftRepository.get_by_id(conn, 'shift-a3-opening')
    assert shift.state == ShiftState.OPENED, 'OPENING shift must be moved to OPENED on ACK'


def test_a3_shift_close_ignored_when_already_closed(conn):
    """A3 (H-2): SHIFT_CLOSE on an already-CLOSED shift (not active) is a no-op."""
    # CLOSED is not returned by get_active_shift → no update attempted
    conn.execute('BEGIN IMMEDIATE')
    conn.execute("""
        INSERT INTO shifts (
            shift_id, fiscal_number, state, open_mode,
            opened_via_backend_profile_id, opened_via_transport_profile_id,
            opened_via_protocol, opened_via_integration_owner, channel_lock_acquired_at
        ) VALUES ('shift-a3-closed', 'FN-DSC-0001', 'CLOSED', 'ONLINE',
                  'backend_checkbox_default', 'transport_dps_grpc_default',
                  'CHECKBOX_REST', 'test', '2026-01-01T10:00:00+00:00')
    """)
    conn.commit()

    # Should complete without error — get_active_shift returns None for CLOSED
    OfflineSyncService._apply_shift_side_effects_locked(
        conn, doc=_fake_shift_doc('SHIFT_CLOSE'), target_state=DocumentState.ACK,
    )
    shift = ShiftRepository.get_by_id(conn, 'shift-a3-closed')
    assert shift.state == ShiftState.CLOSED, 'CLOSED shift must remain CLOSED'


# ---------------------------------------------------------------------------
# A9 (M-4) — unknown op type logs warning in _apply_shift_side_effects_locked
# ---------------------------------------------------------------------------

def test_a9_unknown_op_type_logs_warning(conn, caplog):
    """A9 (M-4): unknown doc_type must log a WARNING instead of silently returning."""
    with caplog.at_level(logging.WARNING, logger='prro_gateway.offline_sync'):
        OfflineSyncService._apply_shift_side_effects_locked(
            conn, doc=_fake_shift_doc('UNKNOWN_OP_XYZ'), target_state=DocumentState.ACK,
        )
    assert 'offline_sync_shift_unknown_op_type' in caplog.text
