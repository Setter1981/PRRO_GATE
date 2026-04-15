from __future__ import annotations

import json
import sqlite3
from typing import Any

from ..models.storage import BackendProfileRecord, TransportProfileRecord
from ._base import columns_clause, fetchone_model


class BackendProfileRepository:
    def get_by_id(conn: sqlite3.Connection, backend_profile_id: str) -> BackendProfileRecord | None:
        return fetchone_model(conn, f'SELECT {columns_clause(BackendProfileRecord)} FROM backend_profiles WHERE backend_profile_id = ?', (backend_profile_id,), BackendProfileRecord)

    def get_capabilities(conn: sqlite3.Connection, backend_profile_id: str) -> dict[str, Any]:
        record = BackendProfileRepository.get_by_id(conn, backend_profile_id)
        if record is None:
            raise LookupError(f'backend_profile not found: {backend_profile_id}')
        return json.loads(record.capability_flags_json)


class TransportProfileRepository:
    def get_by_id(conn: sqlite3.Connection, transport_profile_id: str) -> TransportProfileRecord | None:
        return fetchone_model(conn, f'SELECT {columns_clause(TransportProfileRecord)} FROM transport_profiles WHERE transport_profile_id = ?', (transport_profile_id,), TransportProfileRecord)


__all__ = ['BackendProfileRepository', 'TransportProfileRepository']
