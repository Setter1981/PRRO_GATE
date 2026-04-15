from __future__ import annotations

import sqlite3

from ..models.storage import ProtocolTraceRecord, TransportTraceRecord
from ._base import columns_clause, fetchone_model


class ProtocolTraceRepository:
    def log_event(
        conn: sqlite3.Connection,
        *,
        trace_id: str,
        protocol: str,
        fiscal_number: str,
        direction: str,
        trace_level: str,
        mapped_operation: str | None = None,
        canonical_request_id: str | None = None,
        parsed_payload_json: str | None = None,
        result_code: str | None = None,
        error_class: str | None = None,
        error_message: str | None = None,
        source_ip: str | None = None,
        source_port: int | None = None,
        session_id: str | None = None,
        connection_id: str | None = None,
        route_key: str | None = None,
        method_name: str | None = None,
        command_code: str | None = None,
    ) -> ProtocolTraceRecord:
        conn.execute(
            """
            INSERT INTO protocol_trace_log (
                trace_id, protocol, fiscal_number, route_key, connection_id, session_id, direction,
                method_name, command_code, trace_level, parsed_payload_json, mapped_operation,
                canonical_request_id, result_code, error_class, error_message, source_ip, source_port
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (trace_id, protocol, fiscal_number, route_key, connection_id, session_id, direction,
             method_name, command_code, trace_level, parsed_payload_json, mapped_operation,
             canonical_request_id, result_code, error_class, error_message, source_ip, source_port),
        )
        return fetchone_model(conn, f'SELECT {columns_clause(ProtocolTraceRecord)} FROM protocol_trace_log WHERE trace_id = ?', (trace_id,), ProtocolTraceRecord)


class TransportTraceRepository:
    def log_event(
        conn: sqlite3.Connection,
        *,
        trace_id: str,
        fiscal_number: str,
        backend_profile_id: str,
        transport_profile_id: str,
        direction: str,
        endpoint: str | None = None,
        request_envelope: str | None = None,
        response_envelope: str | None = None,
        technical_status: str | None = None,
        business_status: str | None = None,
        correlation_id: str | None = None,
        retry_no: int = 0,
    ) -> TransportTraceRecord:
        conn.execute(
            """
            INSERT INTO transport_trace_log (
                trace_id, fiscal_number, backend_profile_id, transport_profile_id,
                endpoint, direction, request_envelope, response_envelope,
                technical_status, business_status, correlation_id, retry_no
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (trace_id, fiscal_number, backend_profile_id, transport_profile_id, endpoint,
             direction, request_envelope, response_envelope, technical_status,
             business_status, correlation_id, retry_no),
        )
        return fetchone_model(conn, f'SELECT {columns_clause(TransportTraceRecord)} FROM transport_trace_log WHERE trace_id = ?', (trace_id,), TransportTraceRecord)


__all__ = ['ProtocolTraceRepository', 'TransportTraceRepository']
