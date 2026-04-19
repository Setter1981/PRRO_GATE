"""CA endpoint registry — URLs of Ukrainian qualified-trust providers.

Seeded from `sql/016_ca_endpoints.sql`. The IIT proprietary CMP protocol
is identical across every Ukrainian CSP (reverse-engineered from
`dstucrypt/agent`); only the hostname differs. This table lets
`cert_provisioning.provision_cert` try CAs in priority order until one
returns a matching certificate, without a hardcoded URL list in code.
"""
from __future__ import annotations

import sqlite3
from typing import Callable

from ..models.storage import CaEndpointRecord
from ._base import columns_clause, fetchall_model, fetchone_model


class CaEndpointsRepository:
    def __init__(self, connection_factory: Callable[[], sqlite3.Connection]):
        self._connection_factory = connection_factory

    def list_enabled(self) -> list[CaEndpointRecord]:
        """All enabled endpoints in priority order (lowest first)."""
        cols = columns_clause(CaEndpointRecord)
        with self._connection_factory() as conn:
            return fetchall_model(
                conn,
                f"SELECT {cols} FROM ca_endpoints WHERE enabled = 1 ORDER BY priority ASC",
                (),
                CaEndpointRecord,
            )

    def get_by_name(self, name: str) -> CaEndpointRecord | None:
        cols = columns_clause(CaEndpointRecord)
        with self._connection_factory() as conn:
            return fetchone_model(
                conn,
                f"SELECT {cols} FROM ca_endpoints WHERE name = ?",
                (name,),
                CaEndpointRecord,
            )

    def match_by_issuer_dn(self, issuer_dn: str) -> CaEndpointRecord | None:
        """Find the CA whose `issuer_pattern` occurs in `issuer_dn`.

        Case-insensitive substring match. Returns `None` if nothing
        matches — caller should fall back to iterating `list_enabled`.
        """
        if not issuer_dn:
            return None
        needle = issuer_dn.lower()
        for ep in self.list_enabled():
            if ep.issuer_pattern and ep.issuer_pattern.lower() in needle:
                return ep
        return None
