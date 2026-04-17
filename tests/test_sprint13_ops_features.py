"""Sprint 13 — ops-loop features: shift warning, DPS ping, offline codes auto-fetch.

SW1:  No warning when shift is less than 20 h old
SW2:  Warning emitted (audit + logger) when shift >= 20 h old
SW3:  Warning emitted only once per shift_id (in-memory dedup)
SW4:  No warning when no active shift

PG1:  No GO_ONLINE injected when DPS ping returns False
PG2:  GO_ONLINE inbox entry injected and processed when ping returns True
PG3:  GO_ONLINE not injected twice for the same offline session (dedup)
PG4:  No ping when current_transport_profile_id is None

RC1:  No offline-codes fetch when min_offline_codes == 0
RC2:  No offline-codes fetch when available >= min_offline_codes
RC3:  request_offline_codes called when available < min_offline_codes
RC4:  Range stored in DB when codes returned
RC5:  Overlapping range not stored

CA1:  count_available returns 0 when no ranges
CA2:  count_available returns correct total for single range
CA3:  count_available sums across multiple ranges

PFD1: _parse_fns_data empty string → []
PFD2: _parse_fns_data XML <ID> elements → sorted list
PFD3: _parse_fns_data comma-separated string → sorted list
PFD4: _parse_fns_data whitespace-separated string → sorted list
"""
from __future__ import annotations

import sqlite3
import uuid
from datetime import UTC, datetime, timedelta
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from prro_gateway.config import AppConfig
from prro_gateway.enums import NodeMode, OperationType, Protocol, ShiftState
from prro_gateway.repositories.audit import AuditRepository
from prro_gateway.repositories.fn_config import FiscalNumberConfigRepository
from prro_gateway.repositories.inbox import InboxRepository
from prro_gateway.repositories.node_state import NodeStateRepository
from prro_gateway.repositories.offline import OfflineRepository
from prro_gateway.repositories.shifts import ShiftRepository
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.transports.dps_fiscal_server import _parse_fns_data

ROOT = Path(__file__).resolve().parents[1]
FN = 'FN-SPRINT13-001'


def _config(tmp_path: Path) -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'test.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {'fiscal_number': FN, 'tax_number': '1234567890'},
        'runtime': {
            'reconcile_on_startup': False,
            'ops_loop_enabled': False,
        },
    })


def _seed_node_online(conn: sqlite3.Connection, fn: str = FN) -> None:
    """Ensure node_state row exists (default is ONLINE after migration)."""
    state = NodeStateRepository.get_state(conn, fn)
    if state is None:
        conn.execute(
            "INSERT INTO node_state "
            "(node_id, fiscal_number, mode, shift_state, next_lnd, "
            " readiness_state, recovery_stage, current_month_bucket, current_month_offline_seconds) "
            "VALUES (?, ?, 'ONLINE', 'CLOSED', 1, 'READY', 'DONE', '', 0)",
            (str(uuid.uuid4()), fn),
        )


def _set_mode(conn: sqlite3.Connection, mode: NodeMode, fn: str = FN) -> None:
    _seed_node_online(conn, fn)
    NodeStateRepository.update_mode(conn, fiscal_number=fn, mode=mode)


_BACKEND_ID = 'backend_checkbox_default'
_TRANSPORT_ID = 'transport_checkbox_rest_default'


def _create_shift(conn: sqlite3.Connection, opened_at: str, fn: str = FN) -> str:
    shift_id = str(uuid.uuid4())
    conn.execute(
        """
        INSERT INTO shifts (shift_id, fiscal_number, state, open_mode,
            opened_via_backend_profile_id, opened_via_transport_profile_id,
            opened_via_protocol, opened_via_integration_owner,
            channel_lock_acquired_at, opened_at)
        VALUES (?, ?, 'OPENED', 'ONLINE', ?, ?, 'CHECKBOX_REST', 'test', ?, ?)
        """,
        (shift_id, fn, _BACKEND_ID, _TRANSPORT_ID, opened_at, opened_at),
    )
    return shift_id


def _create_offline_range(
    conn: sqlite3.Connection,
    first: int,
    last: int,
    fn: str = FN,
) -> None:
    conn.execute(
        """INSERT INTO offline_ranges
           (range_id, fiscal_number, first_fiscal_no, last_fiscal_no,
            next_fiscal_no, issued_at, status)
           VALUES (?, ?, ?, ?, ?, ?, 'ACTIVE')""",
        (str(uuid.uuid4()), fn, first, last, first, datetime.now(UTC).isoformat()),
    )


# ===========================================================================
# CA — count_available
# ===========================================================================

def test_ca1_count_available_no_ranges(conn: sqlite3.Connection) -> None:
    assert OfflineRepository.count_available(conn, FN) == 0


def test_ca2_count_available_single_range(conn: sqlite3.Connection) -> None:
    conn.execute('BEGIN IMMEDIATE')
    _create_offline_range(conn, first=1001, last=1050)
    conn.commit()
    # all 50 codes unused (next_fiscal_no = first)
    assert OfflineRepository.count_available(conn, FN) == 50


def test_ca3_count_available_sums_multiple_ranges(conn: sqlite3.Connection) -> None:
    conn.execute('BEGIN IMMEDIATE')
    _create_offline_range(conn, first=1, last=10)     # 10 codes
    _create_offline_range(conn, first=100, last=115)  # 16 codes
    conn.commit()
    assert OfflineRepository.count_available(conn, FN) == 26


# ===========================================================================
# PFD — _parse_fns_data
# ===========================================================================

def test_pfd1_empty_string() -> None:
    assert _parse_fns_data('') == []


def test_pfd2_xml_id_elements() -> None:
    xml = '<C><ID>1003</ID><ID>1001</ID><ID>1002</ID></C>'
    assert _parse_fns_data(xml) == [1001, 1002, 1003]


def test_pfd3_comma_separated() -> None:
    assert _parse_fns_data('5001,5002,5003') == [5001, 5002, 5003]


def test_pfd4_whitespace_separated() -> None:
    assert _parse_fns_data('200\n201\n202') == [200, 201, 202]


# ===========================================================================
# SW — shift duration warning
# ===========================================================================

def test_sw1_no_warning_when_shift_young(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        opened_at = (datetime.now(UTC) - timedelta(hours=5)).isoformat()
        with container.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            _seed_node_online(conn)
            _create_shift(conn, opened_at)
            conn.commit()
        container._check_shift_duration_warning(FN)
        with container.connect() as conn:
            rows = conn.execute(
                "SELECT 1 FROM audit_log WHERE event_type = 'SHIFT_DURATION_WARNING'"
            ).fetchall()
        assert len(rows) == 0
    finally:
        container.shutdown()


def test_sw2_warning_emitted_when_shift_old(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        opened_at = (datetime.now(UTC) - timedelta(hours=21)).isoformat()
        with container.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            _seed_node_online(conn)
            _create_shift(conn, opened_at)
            conn.commit()
        container._check_shift_duration_warning(FN)
        with container.connect() as conn:
            rows = conn.execute(
                "SELECT event_type, severity FROM audit_log WHERE event_type = 'SHIFT_DURATION_WARNING'"
            ).fetchall()
        assert len(rows) == 1
        assert rows[0][1] == 'WARNING'
    finally:
        container.shutdown()


def test_sw3_warning_only_once_per_shift(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        opened_at = (datetime.now(UTC) - timedelta(hours=25)).isoformat()
        with container.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            _seed_node_online(conn)
            _create_shift(conn, opened_at)
            conn.commit()
        # Call twice
        container._check_shift_duration_warning(FN)
        container._check_shift_duration_warning(FN)
        with container.connect() as conn:
            count = conn.execute(
                "SELECT COUNT(*) FROM audit_log WHERE event_type = 'SHIFT_DURATION_WARNING'"
            ).fetchone()[0]
        assert count == 1
    finally:
        container.shutdown()


def test_sw4_no_warning_when_no_active_shift(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        # No shift inserted
        container._check_shift_duration_warning(FN)
        with container.connect() as conn:
            count = conn.execute(
                "SELECT COUNT(*) FROM audit_log WHERE event_type = 'SHIFT_DURATION_WARNING'"
            ).fetchone()[0]
        assert count == 0
    finally:
        container.shutdown()


# ===========================================================================
# PG — DPS ping + auto-GO_ONLINE
# ===========================================================================

def _setup_offline_node(container: RuntimeContainer, fn: str = FN) -> str:
    """Set node to OFFLINE with current_transport_profile_id set. Returns offline_session_id."""
    session_id = f'sess-{uuid.uuid4()}'
    with container.connect() as conn:
        conn.execute('BEGIN IMMEDIATE')
        _seed_node_online(conn, fn)
        NodeStateRepository.update_mode(conn, fiscal_number=fn, mode=NodeMode.OFFLINE)
        # Simulate that there is a transport profile in the node state
        conn.execute(
            """UPDATE node_state SET current_transport_profile_id = 'tp-dps',
               current_backend_profile_id = 'bp-1',
               current_offline_session_id = ? WHERE fiscal_number = ?""",
            (session_id, fn),
        )
        conn.commit()
    return session_id


def _make_mock_router(ping_result: bool):
    """Router mock with a DPS handler that responds to ping()."""
    mock_handler = MagicMock()
    mock_handler.ping = MagicMock(return_value=ping_result)
    mock_handler.request_offline_codes = MagicMock(return_value=[])
    mock_router = MagicMock()
    mock_router._resolve = MagicMock(return_value=(mock_handler, MagicMock()))
    return mock_router, mock_handler


def test_pg1_no_go_online_when_ping_false(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        _setup_offline_node(container)
        mock_router, mock_handler = _make_mock_router(ping_result=False)
        container.transport_router = mock_router
        container.command_processor = None

        with container.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, FN)
        container._maybe_ping_and_go_online(FN, node_state)

        with container.connect() as conn:
            count = conn.execute(
                "SELECT COUNT(*) FROM ingress_inbox WHERE operation_type = 'GO_ONLINE'"
            ).fetchone()[0]
        assert count == 0
    finally:
        container.shutdown()


def test_pg2_go_online_injected_when_ping_true(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        _setup_offline_node(container)
        mock_router, mock_handler = _make_mock_router(ping_result=True)
        container.transport_router = mock_router

        # command_processor: records process_next calls without doing anything
        mock_proc = MagicMock()
        mock_proc.process_next = MagicMock(return_value=None)
        container.command_processor = mock_proc

        with container.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, FN)
        container._maybe_ping_and_go_online(FN, node_state)

        with container.connect() as conn:
            rows = conn.execute(
                "SELECT operation_type, fiscal_number FROM ingress_inbox "
                "WHERE operation_type = 'GO_ONLINE'"
            ).fetchall()
        assert len(rows) == 1
        assert rows[0][1] == FN
        mock_proc.process_next.assert_called_once()
    finally:
        container.shutdown()


def test_pg3_go_online_not_injected_twice(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        _setup_offline_node(container)
        mock_router, _ = _make_mock_router(ping_result=True)
        container.transport_router = mock_router
        mock_proc = MagicMock()
        mock_proc.process_next = MagicMock(return_value=None)
        container.command_processor = mock_proc

        with container.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, FN)
        container._maybe_ping_and_go_online(FN, node_state)
        container._maybe_ping_and_go_online(FN, node_state)  # second call

        with container.connect() as conn:
            count = conn.execute(
                "SELECT COUNT(*) FROM ingress_inbox WHERE operation_type = 'GO_ONLINE'"
            ).fetchone()[0]
        assert count == 1
        assert mock_proc.process_next.call_count == 1
    finally:
        container.shutdown()


def test_pg4_no_ping_when_no_transport_profile(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        with container.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            _seed_node_online(conn)
            NodeStateRepository.update_mode(conn, fiscal_number=FN, mode=NodeMode.OFFLINE)
            # current_transport_profile_id remains NULL
            conn.commit()
        mock_router = MagicMock()
        container.transport_router = mock_router

        with container.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, FN)
        container._maybe_ping_and_go_online(FN, node_state)

        mock_router._resolve.assert_not_called()
    finally:
        container.shutdown()


# ===========================================================================
# RC — auto-fetch offline codes (T=112)
# ===========================================================================

def test_rc1_no_fetch_when_min_zero(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        _setup_offline_node(container)
        mock_router, mock_handler = _make_mock_router(ping_result=False)
        container.transport_router = mock_router
        # fn_cfg defaults: min_offline_codes=0, max_offline_codes=0

        with container.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, FN)
        container._maybe_request_offline_codes(FN, node_state)

        mock_handler.request_offline_codes.assert_not_called()
    finally:
        container.shutdown()


def test_rc2_no_fetch_when_available_sufficient(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        _setup_offline_node(container)
        with container.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            FiscalNumberConfigRepository.upsert(
                conn, fiscal_number=FN, min_offline_codes=10, max_offline_codes=50
            )
            _create_offline_range(conn, first=1, last=20)  # 20 codes >= min=10
            conn.commit()

        mock_router, mock_handler = _make_mock_router(ping_result=False)
        container.transport_router = mock_router

        with container.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, FN)
        container._maybe_request_offline_codes(FN, node_state)

        mock_handler.request_offline_codes.assert_not_called()
    finally:
        container.shutdown()


def test_rc3_request_called_when_codes_low(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        _setup_offline_node(container)
        with container.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            FiscalNumberConfigRepository.upsert(
                conn, fiscal_number=FN, min_offline_codes=20, max_offline_codes=50
            )
            _create_offline_range(conn, first=1, last=5)  # only 5 available < min=20
            conn.commit()

        mock_router, mock_handler = _make_mock_router(ping_result=False)
        mock_handler.request_offline_codes = MagicMock(return_value=[])
        container.transport_router = mock_router

        mock_crypto = MagicMock()
        mock_crypto.sign = MagicMock(return_value='signed-xml')
        container.crypto_provider = mock_crypto

        with container.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, FN)
        container._maybe_request_offline_codes(FN, node_state)

        mock_handler.request_offline_codes.assert_called_once()
        call_kwargs = mock_handler.request_offline_codes.call_args[1]
        assert call_kwargs['fiscal_number'] == FN
        assert call_kwargs['qty'] == 45  # max(50) - available(5)
    finally:
        container.shutdown()


def test_rc4_range_stored_when_codes_returned(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        _setup_offline_node(container)
        with container.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            FiscalNumberConfigRepository.upsert(
                conn, fiscal_number=FN, min_offline_codes=5, max_offline_codes=10
            )
            conn.commit()

        mock_router, mock_handler = _make_mock_router(ping_result=False)
        mock_handler.request_offline_codes = MagicMock(return_value=[2001, 2002, 2003, 2004, 2005])
        container.transport_router = mock_router

        mock_crypto = MagicMock()
        mock_crypto.sign = MagicMock(return_value='signed-xml')
        container.crypto_provider = mock_crypto

        with container.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, FN)
        container._maybe_request_offline_codes(FN, node_state)

        with container.connect() as conn:
            rows = conn.execute(
                "SELECT first_fiscal_no, last_fiscal_no, status FROM offline_ranges "
                "WHERE fiscal_number = ?",
                (FN,),
            ).fetchall()
        assert len(rows) == 1
        assert rows[0][0] == 2001
        assert rows[0][1] == 2005
        assert rows[0][2] == 'ACTIVE'
    finally:
        container.shutdown()


def test_rc5_overlapping_range_not_stored(tmp_path: Path) -> None:
    container = RuntimeContainer(_config(tmp_path))
    container.initialize()
    try:
        _setup_offline_node(container)
        with container.connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            FiscalNumberConfigRepository.upsert(
                conn, fiscal_number=FN, min_offline_codes=5, max_offline_codes=10
            )
            # Existing overlapping range
            _create_offline_range(conn, first=2000, last=2010)
            conn.commit()

        mock_router, mock_handler = _make_mock_router(ping_result=False)
        # DPS returns same range — would overlap
        mock_handler.request_offline_codes = MagicMock(return_value=[2001, 2002, 2003, 2004, 2005])
        container.transport_router = mock_router

        mock_crypto = MagicMock()
        mock_crypto.sign = MagicMock(return_value='signed-xml')
        container.crypto_provider = mock_crypto

        with container.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, FN)
        container._maybe_request_offline_codes(FN, node_state)

        with container.connect() as conn:
            count = conn.execute(
                "SELECT COUNT(*) FROM offline_ranges WHERE fiscal_number = ?", (FN,)
            ).fetchone()[0]
        assert count == 1  # only the pre-existing one
    finally:
        container.shutdown()
