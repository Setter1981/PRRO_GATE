from __future__ import annotations

import sqlite3
from dataclasses import dataclass


@dataclass(frozen=True)
class TaxGroupDef:
    fiscal_number: str
    tax_id: str
    name: str
    tax_rate: float
    additional_rate: float
    tax_type: int
    tax_algorithm: int
    requires_uktzed: bool
    requires_excise_mark: bool
    is_active: bool


class TaxGroupRepository:
    @staticmethod
    def get_for_fiscal_number(conn: sqlite3.Connection, fiscal_number: str) -> dict[str, TaxGroupDef]:
        """Return all active tax groups for a fiscal number, keyed by tax_id."""
        rows = conn.execute(
            "SELECT fiscal_number, tax_id, name, tax_rate, additional_rate, "
            "tax_type, tax_algorithm, requires_uktzed, requires_excise_mark, is_active "
            "FROM tax_group_definitions WHERE fiscal_number = ? AND is_active = 1",
            (fiscal_number,),
        ).fetchall()
        return {
            row[1]: TaxGroupDef(
                fiscal_number=row[0], tax_id=row[1], name=row[2],
                tax_rate=row[3], additional_rate=row[4],
                tax_type=row[5], tax_algorithm=row[6],
                requires_uktzed=bool(row[7]), requires_excise_mark=bool(row[8]),
                is_active=bool(row[9]),
            )
            for row in rows
        }

    @staticmethod
    def get_group(conn: sqlite3.Connection, fiscal_number: str, tax_id: str) -> TaxGroupDef | None:
        row = conn.execute(
            "SELECT fiscal_number, tax_id, name, tax_rate, additional_rate, "
            "tax_type, tax_algorithm, requires_uktzed, requires_excise_mark, is_active "
            "FROM tax_group_definitions WHERE fiscal_number = ? AND tax_id = ? AND is_active = 1",
            (fiscal_number, tax_id),
        ).fetchone()
        if row is None:
            return None
        return TaxGroupDef(
            fiscal_number=row[0], tax_id=row[1], name=row[2],
            tax_rate=row[3], additional_rate=row[4],
            tax_type=row[5], tax_algorithm=row[6],
            requires_uktzed=bool(row[7]), requires_excise_mark=bool(row[8]),
            is_active=bool(row[9]),
        )


__all__ = ['TaxGroupDef', 'TaxGroupRepository']
