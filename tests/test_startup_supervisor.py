from __future__ import annotations

import sqlite3
from datetime import UTC, datetime
from pathlib import Path

from prro_gateway.config import AppConfig
from prro_gateway.migrations.runner import apply_migrations_to_connection
from prro_gateway.ports import PollResult
from prro_gateway.repositories import FiscalDocumentRepository, InboxRepository, ShiftRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.services.reconciliation import ReconciliationService
from prro_gateway.services.write_path import WritePathWorker
from prro_gateway.enums import DocumentState, OperationType, Protocol, ShiftState
from prro_gateway.models.canonical import CanonicalFiscalCommand, TraceContext
from prro_gateway.utils.json_codec import dumps_json

ROOT = Path(__file__).resolve().parents[1]


class AckStatusClient:
    def poll_status(self, **kwargs):
        now = datetime.now(UTC)
        return PollResult(state='ACK', submission_status='ACK', server_fiscal_no='SFN', server_fiscal_date=now.isoformat(), response_json='{"ok":true}', ack_at=now)


def _accept_sell(conn: sqlite3.Connection, request_id: str = 'req-startup') -> None:
    cmd = CanonicalFiscalCommand(
        request_id=request_id,
        idempotency_key=f'idem-{request_id}',
        protocol=Protocol.CHECKBOX_REST,
        operation_type=OperationType.SELL,
        fiscal_number='FN-DEV-0001',
        route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='front-a',
        external_request_id=f'ext-{request_id}',
        business_ts=datetime(2026, 1, 1, 10, 0, 0, tzinfo=UTC),
        payload={'receipt': {'type': 'SELL', 'goods': [{'code':'A','name':'A','price':1000,'quantity':1000,'sum':1000}], 'payments': [{'payment_type':'CASH','amount':1000}], 'totals': {'total_sum':1000}}},
        payload_sha256='sha',
        trace_context=TraceContext(source_ip='10.0.0.10', source_port=12000, session_id='sess', correlation_id='corr'),
        correlation_id='corr',
    )
    InboxRepository.accept_command(conn, request_id=cmd.request_id, idempotency_key=cmd.idempotency_key, protocol=cmd.protocol, operation_type=cmd.operation_type, fiscal_number=cmd.fiscal_number, route_key=cmd.route_key, backend_profile_id=cmd.backend_profile_id, transport_profile_id=cmd.transport_profile_id, channel_owner=cmd.channel_owner, external_request_id=cmd.external_request_id, protocol_session_id='sess', payload_json=dumps_json(cmd.model_dump(mode='json')), payload_sha256=cmd.payload_sha256)


def test_startup_supervisor_runs_reconciliation_phase(tmp_path: Path) -> None:
    db_path = tmp_path / 'startup.sqlite3'
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    apply_migrations_to_connection(conn, ROOT / 'sql')
    conn.execute('BEGIN IMMEDIATE')
    ShiftRepository.create_shift(conn, shift_id='shift-1', fiscal_number='FN-DEV-0001', state=ShiftState.OPENED, open_mode='ONLINE', backend_profile_id='backend_checkbox_default', transport_profile_id='transport_checkbox_rest_default', protocol=Protocol.CHECKBOX_REST, integration_owner='front-a', channel_lock_acquired_at='2026-01-01T09:59:00+00:00')
    _accept_sell(conn)
    FiscalDocumentRepository.create_prepared(conn, document_id='doc-pending', request_id='req-startup', fiscal_number='FN-DEV-0001', lnd=1, doc_type='SELL', backend_profile_id='backend_checkbox_default', transport_profile_id='transport_checkbox_rest_default', fs_mode='ONLINE', business_ts='2026-01-01T10:00:00+00:00', payload_json='{}', payload_sha256='sha', shift_id='shift-1')
    FiscalDocumentRepository.update_state(conn, document_id='doc-pending', state=DocumentState.SENT, transport_request_id='tx-1', submission_status='SENT')
    conn.commit()
    conn.close()

    cfg = AppConfig.from_mapping({'database': {'db_path': str(db_path), 'sql_dir': str(ROOT / 'sql'), 'auto_migrate': True}, 'runtime': {'reconcile_on_startup': True, 'startup_ready': True}})
    container = RuntimeContainer(cfg, reconciliation_service=ReconciliationService(transport_status_client=AckStatusClient()))
    container.initialize()
    assert container.health.ready is True
    assert container.health.startup_complete is True
    assert container.health.phase == 'RUNNING'
    assert container.last_startup_report is not None
    assert container.last_startup_report.phase2_attempted is True
    assert container.last_startup_report.reconciliation_checked >= 1
