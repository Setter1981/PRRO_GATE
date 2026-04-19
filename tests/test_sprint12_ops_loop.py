"""Sprint 12 — ops-loop tests.

OL1: ops_loop_enabled=False → no thread started
OL2: ops_loop_enabled=True → daemon thread started after initialize()
OL3: shutdown() stops loop thread within 10 seconds
OL4: _ops_tick_for_fn calls reconcile_pending when node is ONLINE
OL5: _ops_tick_for_fn calls sync_pending when backlog > 0
OL6: _ops_tick_for_fn skips sync when no backlog
OL7: _ops_tick_for_fn skips all actions when node is OFFLINE
"""
from __future__ import annotations

import sqlite3
import threading
import time
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from prro_gateway.config import AppConfig
from prro_gateway.enums import NodeMode
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.repositories.offline import OfflineRepository
from prro_gateway.runtime.container import RuntimeContainer

ROOT = Path(__file__).resolve().parents[1]
FN = 'FN-DEV-0001'


def _config(tmp_path: Path, ops_loop_enabled: bool = True, interval: int = 3600) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'test.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {'fiscal_number': FN},
        'runtime': {
            'reconcile_on_startup': False,
            'ops_loop_enabled': ops_loop_enabled,
            'ops_loop_interval_seconds': interval,
        },
    })


# ---------------------------------------------------------------------------
# OL1 — disabled → no thread
# ---------------------------------------------------------------------------

def test_ol1_ops_loop_not_started_when_disabled(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path, ops_loop_enabled=False))
    container.initialize()
    try:
        assert container._ops_loop_thread is None
    finally:
        container.shutdown()


# ---------------------------------------------------------------------------
# OL2 — enabled → daemon thread running
# ---------------------------------------------------------------------------

def test_ol2_ops_loop_thread_starts_when_enabled(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path, ops_loop_enabled=True, interval=3600))
    container.initialize()
    try:
        assert container._ops_loop_thread is not None
        assert container._ops_loop_thread.is_alive()
        assert container._ops_loop_thread.daemon
    finally:
        container.shutdown()


# ---------------------------------------------------------------------------
# OL3 — shutdown() joins thread
# ---------------------------------------------------------------------------

def test_ol3_shutdown_stops_loop(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path, ops_loop_enabled=True, interval=3600))
    container.initialize()
    thread = container._ops_loop_thread
    assert thread is not None and thread.is_alive()
    container.shutdown()
    assert not thread.is_alive()


# ---------------------------------------------------------------------------
# OL4 — reconcile_pending called for ONLINE node
# ---------------------------------------------------------------------------

def test_ol4_reconcile_called_for_online_node(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path, ops_loop_enabled=False))
    container.initialize()
    mock_recon = MagicMock()
    mock_recon.reconcile_pending = MagicMock(return_value=MagicMock(checked=0))
    container.reconciliation_service = mock_recon
    container.offline_sync_service = None
    try:
        container._ops_tick_for_fn(FN)
    finally:
        container.shutdown()
    mock_recon.reconcile_pending.assert_called_once()


# ---------------------------------------------------------------------------
# OL5 — sync_pending called when backlog > 0
# ---------------------------------------------------------------------------

def test_ol5_sync_called_when_backlog(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path, ops_loop_enabled=False))
    container.initialize()

    # Seed an OFFLINE_LOCAL_ACK document so backlog > 0
    with container.connect() as conn:
        conn.execute('BEGIN IMMEDIATE')
        conn.execute(
            """INSERT INTO ingress_inbox
               (request_id, idempotency_key, protocol, operation_type, fiscal_number,
                backend_profile_id, transport_profile_id, channel_owner,
                external_request_id, payload_json, payload_sha256, status)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?)""",
            ('req-ol5', 'idem-ol5', 'CHECKBOX_REST', 'SELL', FN,
             'backend_checkbox_default', 'transport_checkbox_rest_default',
             'front', 'ext-ol5', '{}', 'sha-ol5', 'DONE'),
        )
        conn.execute(
            """INSERT INTO fiscal_documents
               (document_id, request_id, fiscal_number, lnd, doc_type,
                backend_profile_id, transport_profile_id, fs_mode, state,
                submission_status, business_ts, payload_json, payload_sha256)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)""",
            ('doc-ol5', 'req-ol5', FN, 9999, 'SELL',
             'backend_checkbox_default', 'transport_checkbox_rest_default',
             'OFFLINE', 'OFFLINE_LOCAL_ACK', 'OFFLINE_LOCAL',
             '2026-04-15T10:00:00+00:00', '{}', 'sha-ol5'),
        )
        conn.commit()

    mock_sync = MagicMock()
    mock_sync.sync_pending = MagicMock(return_value=MagicMock(synced=0))
    container.offline_sync_service = mock_sync
    container.reconciliation_service = None

    try:
        container._ops_tick_for_fn(FN)
    finally:
        container.shutdown()

    mock_sync.sync_pending.assert_called_once()


# ---------------------------------------------------------------------------
# OL6 — sync NOT called when no backlog
# ---------------------------------------------------------------------------

def test_ol6_sync_not_called_when_no_backlog(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path, ops_loop_enabled=False))
    container.initialize()
    mock_sync = MagicMock()
    container.offline_sync_service = mock_sync
    container.reconciliation_service = None
    try:
        container._ops_tick_for_fn(FN)
    finally:
        container.shutdown()
    mock_sync.sync_pending.assert_not_called()


# ---------------------------------------------------------------------------
# OL7 — skips when node is OFFLINE
# ---------------------------------------------------------------------------

def test_ol7_skips_when_offline(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path, ops_loop_enabled=False))
    container.initialize()
    with container.connect() as conn:
        conn.execute('BEGIN IMMEDIATE')
        NodeStateRepository.update_mode(conn, fiscal_number=FN, mode=NodeMode.OFFLINE)
        conn.commit()
    mock_recon = MagicMock()
    mock_sync = MagicMock()
    container.reconciliation_service = mock_recon
    container.offline_sync_service = mock_sync
    try:
        container._ops_tick_for_fn(FN)
    finally:
        container.shutdown()
    mock_recon.reconcile_pending.assert_not_called()
    mock_sync.sync_pending.assert_not_called()
