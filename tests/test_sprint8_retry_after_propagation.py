"""Sprint 8 step 4: retry_after_seconds propagation tests.

Tests cover:
  RA1: write-path persists submission_status='DPS_RATE_LIMITED' on document
  RA2: reconciliation skips polling for rate-limited docs within cooldown
  RA3: reconciliation does NOT skip after cooldown expires
  RA4: reconciliation skip does NOT increment recovery_attempts
  RA5: POST ingress response includes retry_after_seconds
  RA6: GET document response includes retry_after_seconds for rate-limited docs
  RA7: non-default retry_after_seconds=600 persisted and propagated end-to-end
"""
from __future__ import annotations

import pytest
from datetime import datetime, timedelta, UTC


def _create_rate_limited_doc(conn, *, doc_suffix: str, lnd_offset: int = 99) -> str:
    """Helper: create a rate-limited doc via write_path (respects all FKs)."""
    from prro_gateway.enums import OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.ports import TransportRateLimitedError
    from prro_gateway.repositories import InboxRepository, ShiftRepository
    from prro_gateway.services.write_path import WritePathWorker

    shift_id = f'shift-{doc_suffix}'
    # Only create shift if not exists
    existing = conn.execute("SELECT shift_id FROM shifts WHERE shift_id = ?", (shift_id,)).fetchone()
    if not existing:
        ShiftRepository.create_shift(
            conn, shift_id=shift_id, fiscal_number='FN-DEV-0001',
            state=ShiftState.OPENED, open_mode='ONLINE',
            backend_profile_id='backend_checkbox_default',
            transport_profile_id='transport_checkbox_rest_default',
            protocol=Protocol.CHECKBOX_REST, integration_owner='test',
            channel_lock_acquired_at='2026-04-13T12:00:00Z',
        )
        conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id=f'req-{doc_suffix}', idempotency_key=f'idem-{doc_suffix}',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='test', external_request_id=f'ext-{doc_suffix}',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL', 'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
                             'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 100}}},
        payload_sha256=f'sha-{doc_suffix}',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id=f'c-{doc_suffix}'),
        correlation_id=f'c-{doc_suffix}',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T12:01:00Z',
    )
    conn.commit()

    class _Crypto:
        def sign(self, **kw): return 'signed'
        def sign_raw(self, *, data, document_id=None): return b'\x30\x82SIGNED'

    class _RateLimitTransport:
        def send(self, **kw):
            raise TransportRateLimitedError('Exceeded', retry_after_seconds=300)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_RateLimitTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')
    return result.document_id


# ---------------------------------------------------------------------------
# RA1 — write-path persists submission_status='DPS_RATE_LIMITED'
# ---------------------------------------------------------------------------

def test_ra1_rate_limited_persists_submission_status(conn) -> None:
    from prro_gateway.enums import OperationType, Protocol, ShiftState
    from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
    from prro_gateway.ports import TransportRateLimitedError
    from prro_gateway.repositories import InboxRepository, ShiftRepository, FiscalDocumentRepository
    from prro_gateway.services.write_path import WritePathWorker

    ShiftRepository.create_shift(
        conn, shift_id='shift-ra1', fiscal_number='FN-DEV-0001',
        state=ShiftState.OPENED, open_mode='ONLINE',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        protocol=Protocol.CHECKBOX_REST, integration_owner='test',
        channel_lock_acquired_at='2026-04-13T12:00:00Z',
    )
    conn.commit()

    cmd = CanonicalFiscalCommand(
        request_id='req-ra1', idempotency_key='idem-ra1',
        protocol=Protocol.CHECKBOX_REST, operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001', route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_dps_grpc_default',
        channel_owner='test', external_request_id='ext-ra1',
        business_ts=datetime(2026, 4, 13, 12, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL', 'goods': [{'name': 'X', 'price': 100, 'quantity': 1000, 'sum': 100}],
                             'payments': [{'amount': 100, 'type': 'CASH'}], 'totals': {'total_sum': 100}}},
        payload_sha256='sha-ra1',
        trace_context=TraceContext(source_ip='10.0.0.1', source_port=1, session_id='s', correlation_id='c-ra1'),
        correlation_id='c-ra1',
    )
    conn.execute('BEGIN IMMEDIATE')
    InboxRepository.accept_command(
        conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol, operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number, route_key=cmd.route_key,
        backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id,
        protocol_session_id=None, payload_json=cmd.model_dump_json(),
        payload_sha256=cmd.payload_sha256, response_deadline_at='2026-04-13T12:01:00Z',
    )
    conn.commit()

    class _Crypto:
        def sign_raw(self, *, data, document_id=None): return b'\x30\x82SIGNED'

    class _RateLimitTransport:
        def send(self, **kw):
            raise TransportRateLimitedError('Exceeded', retry_after_seconds=300)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_RateLimitTransport(), tax_number='TN')
    result = worker.process_next(conn, fiscal_number='FN-DEV-0001')

    doc = FiscalDocumentRepository.get_by_id(conn, result.document_id)
    assert doc.submission_status == 'DPS_RATE_LIMITED'


# ---------------------------------------------------------------------------
# RA2 — reconciliation skips polling while cooldown active
# ---------------------------------------------------------------------------

def test_ra2_reconciliation_skips_during_cooldown(conn) -> None:
    """Rate-limited doc within cooldown window should NOT be polled."""
    from prro_gateway.services.reconciliation import ReconciliationService

    # Create a rate-limited doc via write_path (respects all FKs)
    doc_id = _create_rate_limited_doc(conn, doc_suffix='ra2', lnd_offset=99)

    poll_called = False

    class _SpyTransport:
        def poll_status(self, **kw):
            nonlocal poll_called
            poll_called = True
            from prro_gateway.ports import PollResult
            return PollResult(state='ERROR_RETRYABLE', retryable=True)

    svc = ReconciliationService(transport_status_client=_SpyTransport())
    result = svc.reconcile_pending(conn)

    assert not poll_called, 'poll_status should NOT be called for rate-limited doc within cooldown'
    assert result.still_pending == 1

    # Verify audit event was written
    audit = conn.execute(
        "SELECT event_type FROM audit_log WHERE entity_id = ? ORDER BY rowid DESC LIMIT 1",
        (doc_id,),
    ).fetchone()
    assert audit is not None and audit[0] == 'RECONCILE_COOLDOWN_SKIP'


# ---------------------------------------------------------------------------
# RA3 — reconciliation does NOT skip after cooldown expires
# ---------------------------------------------------------------------------

def test_ra3_reconciliation_polls_after_cooldown(conn) -> None:
    """Rate-limited doc whose cooldown has expired SHOULD be polled normally."""
    from prro_gateway.services.reconciliation import ReconciliationService

    doc_id = _create_rate_limited_doc(conn, doc_suffix='ra3', lnd_offset=98)

    # Backdate updated_at to simulate expired cooldown (10 min ago)
    old_time = (datetime.now(UTC) - timedelta(minutes=10)).strftime('%Y-%m-%d %H:%M:%S')
    conn.execute("UPDATE fiscal_documents SET updated_at = ? WHERE document_id = ?", (old_time, doc_id))
    conn.commit()

    poll_called = False

    class _SpyTransport:
        def poll_status(self, **kw):
            nonlocal poll_called
            poll_called = True
            from prro_gateway.ports import PollResult
            return PollResult(state='ERROR_RETRYABLE', retryable=True)

    svc = ReconciliationService(transport_status_client=_SpyTransport())
    svc.reconcile_pending(conn)

    assert poll_called, 'poll_status SHOULD be called after cooldown expires'


# ---------------------------------------------------------------------------
# RA4 — cooldown skip does NOT increment recovery_attempts
# ---------------------------------------------------------------------------

def test_ra4_cooldown_skip_preserves_recovery_attempts(conn) -> None:
    from prro_gateway.repositories import FiscalDocumentRepository
    from prro_gateway.services.reconciliation import ReconciliationService

    doc_id = _create_rate_limited_doc(conn, doc_suffix='ra4', lnd_offset=97)

    # Set recovery_attempts to 2 to verify it doesn't increment
    conn.execute("UPDATE fiscal_documents SET recovery_attempts = 2 WHERE document_id = ?", (doc_id,))
    conn.commit()

    class _NoopTransport:
        def poll_status(self, **kw):
            raise AssertionError('should not be called')

    svc = ReconciliationService(transport_status_client=_NoopTransport())
    svc.reconcile_pending(conn)

    row = conn.execute("SELECT recovery_attempts FROM fiscal_documents WHERE document_id = ?", (doc_id,)).fetchone()
    assert row[0] == 2, f'recovery_attempts should stay at 2, got {row[0]}'


# ---------------------------------------------------------------------------
# RA5 — POST ingress returns retry_after_seconds when present
# ---------------------------------------------------------------------------

def test_ra5_post_ingress_retry_after(tmp_path) -> None:
    from prro_gateway.config import AppConfig
    from prro_gateway.runtime.container import RuntimeContainer
    from prro_gateway.runtime.rest_app import create_app
    from starlette.testclient import TestClient

    cfg = AppConfig(database={"db_path": str(tmp_path / "ra5.db"), "sql_dir": "sql"})

    from prro_gateway.ports import TransportRateLimitedError
    from prro_gateway.services.write_path import WritePathWorker

    class _Crypto:
        def sign(self, **kw): return 'signed'
        def sign_raw(self, *, data, document_id=None): return b'\x30\x82SIGNED'

    class _RateLimitTransport:
        def send(self, **kw):
            raise TransportRateLimitedError('Exceeded', retry_after_seconds=300)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_RateLimitTransport(), tax_number='TN')
    container = RuntimeContainer(cfg, command_processor=worker)

    import uuid
    with TestClient(create_app(container)) as client:
        resp = client.post("/v1/ingress/checkbox", json={
            "context": {
                "request_id": str(uuid.uuid4()),
                "fiscal_number": "FN-DEV-0001",
                "backend_profile_id": "backend_checkbox_default",
                "transport_profile_id": "transport_checkbox_rest_default",
                "channel_owner": "test",
                "business_ts": "2026-04-13T12:00:00Z",
            },
            "operation": "SHIFT_OPEN",
            "request": {},
        })
    assert resp.status_code == 200
    data = resp.json()
    assert data.get("retry_after_seconds") == 300, f'Expected retry_after_seconds=300, got {data}'


# ---------------------------------------------------------------------------
# RA6 — GET document includes retry_after_seconds for rate-limited docs
# ---------------------------------------------------------------------------

def test_ra6_get_document_retry_after(tmp_path) -> None:
    from prro_gateway.config import AppConfig
    from prro_gateway.ports import TransportRateLimitedError
    from prro_gateway.runtime.container import RuntimeContainer
    from prro_gateway.runtime.rest_app import create_app
    from prro_gateway.services.write_path import WritePathWorker
    from starlette.testclient import TestClient
    import uuid

    cfg = AppConfig(database={"db_path": str(tmp_path / "ra6.db"), "sql_dir": "sql"})

    class _Crypto:
        def sign(self, **kw): return 'signed'
        def sign_raw(self, *, data, document_id=None): return b'\x30\x82SIGNED'

    class _RateLimitTransport:
        def send(self, **kw):
            raise TransportRateLimitedError('Exceeded', retry_after_seconds=300)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_RateLimitTransport(), tax_number='TN')
    container = RuntimeContainer(cfg, command_processor=worker)

    # Create rate-limited doc via REST ingress
    req_id = str(uuid.uuid4())
    with TestClient(create_app(container)) as client:
        client.post("/v1/ingress/checkbox", json={
            "context": {
                "request_id": req_id,
                "fiscal_number": "FN-DEV-0001",
                "backend_profile_id": "backend_checkbox_default",
                "transport_profile_id": "transport_checkbox_rest_default",
                "channel_owner": "test",
                "business_ts": "2026-04-13T12:00:00Z",
            },
            "operation": "SHIFT_OPEN",
            "request": {},
        })

        resp = client.get(f"/v1/documents/{req_id}")
    assert resp.status_code == 200
    data = resp.json()
    assert data["submission_status"] == "DPS_RATE_LIMITED"
    assert data["retry_after_seconds"] == 300


# ---------------------------------------------------------------------------
# RA7 — non-default retry_after_seconds=600 propagated end-to-end
# ---------------------------------------------------------------------------

def test_ra7_non_default_retry_after_propagated(tmp_path) -> None:
    """When transport raises with retry_after_seconds=600:
    - response_json persists 600
    - reconciliation uses 600 for cooldown
    - POST and GET return 600
    """
    import json
    from prro_gateway.config import AppConfig
    from prro_gateway.ports import TransportRateLimitedError
    from prro_gateway.runtime.container import RuntimeContainer
    from prro_gateway.runtime.rest_app import create_app
    from prro_gateway.services.reconciliation import ReconciliationService
    from prro_gateway.services.write_path import WritePathWorker
    from starlette.testclient import TestClient
    import uuid

    cfg = AppConfig(database={"db_path": str(tmp_path / "ra7.db"), "sql_dir": "sql"})

    class _Crypto:
        def sign(self, **kw): return 'signed'
        def sign_raw(self, *, data, document_id=None): return b'\x30\x82SIGNED'

    class _RateLimit600:
        def send(self, **kw):
            raise TransportRateLimitedError('Exceeded', retry_after_seconds=600)

    worker = WritePathWorker(crypto_provider=_Crypto(), transport_client=_RateLimit600(), tax_number='TN')
    container = RuntimeContainer(cfg, command_processor=worker)

    req_id = str(uuid.uuid4())
    with TestClient(create_app(container)) as client:
        post_resp = client.post("/v1/ingress/checkbox", json={
            "context": {
                "request_id": req_id,
                "fiscal_number": "FN-DEV-0001",
                "backend_profile_id": "backend_checkbox_default",
                "transport_profile_id": "transport_checkbox_rest_default",
                "channel_owner": "test",
                "business_ts": "2026-04-13T12:00:00Z",
            },
            "operation": "SHIFT_OPEN",
            "request": {},
        })
        get_resp = client.get(f"/v1/documents/{req_id}")
    assert post_resp.status_code == 200
    post_data = post_resp.json()

    # 1. POST returns 600
    assert post_data.get("retry_after_seconds") == 600, f'POST should return 600, got {post_data}'

    # 2. GET returns 600
    get_data = get_resp.json()
    assert get_data["retry_after_seconds"] == 600, f'GET should return 600, got {get_data}'

    # 3. Persisted in response_json
    with container.connect() as conn:
        doc_id = post_data["document_id"]
        row = conn.execute("SELECT response_json FROM fiscal_documents WHERE document_id = ?", (doc_id,)).fetchone()
        assert row is not None
        meta = json.loads(row[0])
        assert meta["retry_after_seconds"] == 600

    # 4. Reconciliation respects 600s cooldown (doc just created → within 600s → skip)
    poll_called = False

    class _SpyPoll:
        def poll_status(self, **kw):
            nonlocal poll_called
            poll_called = True
            from prro_gateway.ports import PollResult
            return PollResult(state='ERROR_RETRYABLE', retryable=True)

    with container.connect() as conn:
        svc = ReconciliationService(transport_status_client=_SpyPoll())
        svc.reconcile_pending(conn)
    assert not poll_called, 'Reconciliation should skip during 600s cooldown'
