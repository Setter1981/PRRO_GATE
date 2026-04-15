from __future__ import annotations

import sqlite3
from functools import lru_cache
from typing import Any, TypeVar

from pydantic import BaseModel

ModelT = TypeVar("ModelT", bound=BaseModel)


@lru_cache(maxsize=None)
def columns_clause(model_type: type[ModelT]) -> str:
    return ", ".join(model_type.model_fields.keys())


def fetchone_model(conn: sqlite3.Connection, query: str, params: tuple[Any, ...], model_type: type[ModelT]) -> ModelT | None:
    conn.row_factory = sqlite3.Row
    row = conn.execute(query, params).fetchone()
    if row is None:
        return None
    return model_type.model_validate(dict(row))


def fetchall_model(conn: sqlite3.Connection, query: str, params: tuple[Any, ...], model_type: type[ModelT]) -> list[ModelT]:
    conn.row_factory = sqlite3.Row
    rows = conn.execute(query, params).fetchall()
    return [model_type.model_validate(dict(row)) for row in rows]
