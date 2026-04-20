from __future__ import annotations

import sqlite3
from datetime import UTC, datetime, timedelta
from threading import Lock
from typing import Any, Protocol as TypingProtocol

from ..adapters.base import AdapterContext, AdapterMappingError
from ..adapters.checkbox_rest import CheckboxRestAdapter
from ..adapters.maria304_native import Maria304NativeAdapter
from ..adapters.maria_tcp import MariaTcpAdapter
from ..adapters.webcheck_xmlrpc import WebCheckXmlRpcAdapter
from ..constants import (
    DEFAULT_RESPONSE_TIMEOUT_MARIA304_SECONDS,
    DEFAULT_RESPONSE_TIMEOUT_MARIA_SECONDS,
    DEFAULT_RESPONSE_TIMEOUT_REST_SECONDS,
)
from ..models.canonical import CanonicalFiscalCommand, TraceContext
from ..models.storage import InboxRecord
from ..repositories import InboxRepository


class CommandProcessor(TypingProtocol):
    def process_next(self, conn: sqlite3.Connection, *, fiscal_number: str, lease_owner: str = ...) -> Any: ...


class IngressAcceptService:
    def __init__(self, *, command_processor: CommandProcessor | None = None, worker_lease_owner: str = "runtime-worker") -> None:
        self.checkbox_adapter = CheckboxRestAdapter()
        self.webcheck_adapter = WebCheckXmlRpcAdapter()
        self.maria_adapter = MariaTcpAdapter()
        self.maria304_adapter = Maria304NativeAdapter()
        self.command_processor = command_processor
        self.worker_lease_owner = worker_lease_owner
        self._active_operations = 0
        self._ops_lock = Lock()

    def accept_checkbox(self, conn: sqlite3.Connection, *, raw_request: dict[str, Any], response_timeout_seconds: int = DEFAULT_RESPONSE_TIMEOUT_REST_SECONDS) -> tuple[InboxRecord, CanonicalFiscalCommand, Any, bool]:
        command = self.checkbox_adapter.map_command(raw_request)
        inbox, is_replay = self._store_command(conn, command=command, response_timeout_seconds=response_timeout_seconds)
        process_result = self._maybe_process(conn, fiscal_number=command.fiscal_number)
        return inbox, command, process_result, is_replay

    def accept_webcheck(self, conn: sqlite3.Connection, *, method: str, params: dict[str, Any], context: dict[str, Any], response_timeout_seconds: int = DEFAULT_RESPONSE_TIMEOUT_REST_SECONDS) -> tuple[InboxRecord, CanonicalFiscalCommand]:
        command = self.webcheck_adapter.map_command({"method": method, "params": params, "context": context})
        inbox, _ = self._store_command(conn, command=command, response_timeout_seconds=response_timeout_seconds)
        self._maybe_process(conn, fiscal_number=command.fiscal_number)
        return inbox, command

    def accept_maria(self, conn: sqlite3.Connection, *, command_name: str, fields: dict[str, Any], context: dict[str, Any], response_timeout_seconds: int = DEFAULT_RESPONSE_TIMEOUT_MARIA_SECONDS) -> tuple[InboxRecord, CanonicalFiscalCommand]:
        command = self.maria_adapter.map_command({"command": command_name, "fields": fields, "context": context})
        inbox, _ = self._store_command(conn, command=command, response_timeout_seconds=response_timeout_seconds)
        self._maybe_process(conn, fiscal_number=command.fiscal_number)
        return inbox, command

    def accept_maria304(
        self,
        conn: sqlite3.Connection,
        *,
        raw_request: dict[str, Any],
        response_timeout_seconds: int = DEFAULT_RESPONSE_TIMEOUT_MARIA304_SECONDS,
    ) -> tuple[InboxRecord, CanonicalFiscalCommand, Any, bool]:
        command = self.maria304_adapter.map_command(raw_request)
        inbox, is_replay = self._store_command(
            conn, command=command, response_timeout_seconds=response_timeout_seconds,
        )
        process_result = self._maybe_process(conn, fiscal_number=command.fiscal_number)
        return inbox, command, process_result, is_replay


    @property
    def active_operations(self) -> int:
        with self._ops_lock:
            return self._active_operations

    def _begin_operation(self) -> None:
        with self._ops_lock:
            self._active_operations += 1

    def _end_operation(self) -> None:
        with self._ops_lock:
            if self._active_operations > 0:
                self._active_operations -= 1

    @staticmethod
    def build_context(
        *,
        fiscal_number: str,
        backend_profile_id: str,
        transport_profile_id: str,
        channel_owner: str,
        route_key: str | None = None,
        source_ip: str | None = None,
        source_port: int | None = None,
        session_id: str | None = None,
        correlation_id: str | None = None,
    ) -> dict[str, Any]:
        return AdapterContext(
            request_id=f"req-{fiscal_number}-{int(datetime.now(UTC).timestamp() * 1000000)}",
            fiscal_number=fiscal_number,
            route_key=route_key,
            backend_profile_id=backend_profile_id,
            transport_profile_id=transport_profile_id,
            channel_owner=channel_owner,
            business_ts=datetime.now(UTC),
            trace_context=TraceContext(source_ip=source_ip, source_port=source_port, session_id=session_id, correlation_id=correlation_id),
        ).model_dump(mode="json", exclude_none=True)

    def _store_command(self, conn: sqlite3.Connection, *, command: CanonicalFiscalCommand, response_timeout_seconds: int) -> tuple[InboxRecord, bool]:
        deadline = (datetime.now(UTC) + timedelta(seconds=response_timeout_seconds)).isoformat()
        conn.execute("BEGIN IMMEDIATE")
        inbox, is_replay = InboxRepository.accept_command(
            conn,
            request_id=command.request_id,
            idempotency_key=command.idempotency_key,
            protocol=command.protocol,
            operation_type=command.operation_type,
            fiscal_number=command.fiscal_number,
            route_key=command.route_key,
            backend_profile_id=command.backend_profile_id,
            transport_profile_id=command.transport_profile_id,
            channel_owner=command.channel_owner,
            external_request_id=command.external_request_id,
            protocol_session_id=command.trace_context.session_id if command.trace_context else None,
            payload_json=command.model_dump_json(),
            payload_sha256=command.payload_sha256,
            response_deadline_at=deadline,
        )
        conn.commit()
        return inbox, is_replay

    def _maybe_process(self, conn: sqlite3.Connection, *, fiscal_number: str) -> Any:
        if self.command_processor is None:
            return None
        self._begin_operation()
        try:
            return self.command_processor.process_next(conn, fiscal_number=fiscal_number, lease_owner=self.worker_lease_owner)
        finally:
            self._end_operation()


__all__ = ["IngressAcceptService", "AdapterMappingError"]
