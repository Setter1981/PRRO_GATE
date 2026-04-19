"""Unit tests for runtime/alerts.py — AlertSink and AlertEvent."""
from __future__ import annotations

import json
import sqlite3

import pytest

from prro_gateway.runtime.alerts import AlertEvent, AlertSink

FN = 'FN-DEV-0001'


def test_emit_writes_to_audit_log(conn: sqlite3.Connection) -> None:
    """emit() with a connection writes one row to audit_log."""
    sink = AlertSink(enabled=True, persist_to_audit=True)
    event = AlertEvent(
        entity_type='NODE',
        entity_id=FN,
        event_type='TEST_ALERT',
        severity='WARNING',
        payload={'reason': 'test'},
    )
    sink.emit(conn, event=event)
    row = conn.execute(
        "SELECT event_type, severity, event_payload_json FROM audit_log "
        "WHERE event_type = 'TEST_ALERT' ORDER BY audit_id DESC LIMIT 1"
    ).fetchone()
    assert row is not None, "Expected audit_log entry"
    assert row[0] == 'TEST_ALERT'
    assert row[1] == 'WARNING'
    payload = json.loads(row[2])
    assert payload.get('reason') == 'test'


def test_emit_disabled_does_not_write(conn: sqlite3.Connection) -> None:
    """emit() with enabled=False must not write anything to audit_log."""
    sink = AlertSink(enabled=False, persist_to_audit=True)
    sink.emit(conn, event=AlertEvent(
        entity_type='NODE',
        entity_id=FN,
        event_type='SHOULD_NOT_APPEAR',
        severity='ERROR',
    ))
    row = conn.execute(
        "SELECT 1 FROM audit_log WHERE event_type = 'SHOULD_NOT_APPEAR'"
    ).fetchone()
    assert row is None


def test_emit_without_conn_does_not_crash() -> None:
    """emit() with conn=None must not raise even with persist_to_audit=True."""
    sink = AlertSink(enabled=True, persist_to_audit=True)
    sink.emit(None, event=AlertEvent(
        entity_type='NODE',
        entity_id=FN,
        event_type='NO_CONN_EVENT',
        severity='INFO',
    ))  # must not raise


def test_emit_persist_to_audit_false_does_not_write(conn: sqlite3.Connection) -> None:
    """emit() with persist_to_audit=False must not write to audit_log."""
    sink = AlertSink(enabled=True, persist_to_audit=False)
    sink.emit(conn, event=AlertEvent(
        entity_type='NODE',
        entity_id=FN,
        event_type='NO_PERSIST_EVENT',
        severity='WARNING',
    ))
    row = conn.execute(
        "SELECT 1 FROM audit_log WHERE event_type = 'NO_PERSIST_EVENT'"
    ).fetchone()
    assert row is None


def test_emit_default_severity_is_warning() -> None:
    """AlertEvent.severity defaults to 'WARNING' when not specified."""
    event = AlertEvent(entity_type='NODE', entity_id=FN, event_type='DEFAULT_SEV_TEST')
    assert event.severity == 'WARNING'


def test_emit_payload_serialised_as_json(conn: sqlite3.Connection) -> None:
    """emit() serialises payload dict as compact JSON in event_payload_json."""
    sink = AlertSink(enabled=True, persist_to_audit=True)
    payload = {'code': 42, 'flag': True}
    sink.emit(conn, event=AlertEvent(
        entity_type='SHIFT',
        entity_id=FN,
        event_type='PAYLOAD_JSON_TEST',
        severity='INFO',
        payload=payload,
    ))
    row = conn.execute(
        "SELECT event_payload_json FROM audit_log WHERE event_type = 'PAYLOAD_JSON_TEST'"
    ).fetchone()
    assert row is not None
    parsed = json.loads(row[0])
    assert parsed['code'] == 42
    assert parsed['flag'] is True
