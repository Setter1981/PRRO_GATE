from __future__ import annotations

import hashlib
import sqlite3

from ..enums import FileKind
from ..models.storage import DocumentFileRecord
from ._base import columns_clause, fetchall_model, fetchone_model


class DocumentFilesRepository:
    def add_file(
        conn: sqlite3.Connection,
        *,
        file_id: str,
        document_id: str,
        file_kind: FileKind,
        path: str,
        content: str | bytes,
    ) -> DocumentFileRecord:
        content_bytes = content if isinstance(content, bytes) else content.encode('utf-8')
        sha256 = hashlib.sha256(content_bytes).hexdigest()
        conn.execute(
            """
            INSERT INTO document_files (file_id, document_id, file_kind, path, sha256, size_bytes, content)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (file_id, document_id, str(file_kind), path, sha256, len(content_bytes), content_bytes),
        )
        return fetchone_model(conn, f'SELECT {columns_clause(DocumentFileRecord)} FROM document_files WHERE file_id = ?', (file_id,), DocumentFileRecord)

    def get_content(conn: sqlite3.Connection, *, document_id: str, file_kind: FileKind) -> bytes | None:
        """Retrieve persisted file content by document_id and kind. Returns None if not found."""
        row = conn.execute(
            "SELECT content FROM document_files WHERE document_id = ? AND file_kind = ?",
            (document_id, str(file_kind)),
        ).fetchone()
        return row[0] if row and row[0] is not None else None

    def list_for_document(conn: sqlite3.Connection, document_id: str) -> list[DocumentFileRecord]:
        return fetchall_model(conn, f'SELECT {columns_clause(DocumentFileRecord)} FROM document_files WHERE document_id = ? ORDER BY created_at, file_kind', (document_id,), DocumentFileRecord)


__all__ = ['DocumentFilesRepository']
