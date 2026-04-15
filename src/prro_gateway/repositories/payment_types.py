from __future__ import annotations

import sqlite3

from ..models.storage import PaymentTypeDefRecord


class PaymentTypeRepository:
    @staticmethod
    def get_for_fiscal_number(conn: sqlite3.Connection, fiscal_number: str) -> dict[str, PaymentTypeDefRecord]:
        """All active payment types for a fiscal number, keyed by type_code."""
        rows = conn.execute(
            "SELECT fiscal_number, type_code, type_group, name, is_active "
            "FROM payment_type_definitions WHERE fiscal_number = ? AND is_active = 1",
            (fiscal_number,),
        ).fetchall()
        return {
            row[1]: PaymentTypeDefRecord(
                fiscal_number=row[0], type_code=row[1], type_group=row[2],
                name=row[3], is_active=bool(row[4]),
            )
            for row in rows
        }

    @staticmethod
    def get_type(conn: sqlite3.Connection, fiscal_number: str, type_code: str) -> PaymentTypeDefRecord | None:
        """Single type by (fiscal_number, type_code). None if not found or is_active=0."""
        row = conn.execute(
            "SELECT fiscal_number, type_code, type_group, name, is_active "
            "FROM payment_type_definitions WHERE fiscal_number = ? AND type_code = ? AND is_active = 1",
            (fiscal_number, type_code),
        ).fetchone()
        if row is None:
            return None
        return PaymentTypeDefRecord(
            fiscal_number=row[0], type_code=row[1], type_group=row[2],
            name=row[3], is_active=bool(row[4]),
        )


__all__ = ['PaymentTypeRepository']
