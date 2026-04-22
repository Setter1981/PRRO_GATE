from __future__ import annotations

import argparse
import hashlib
import sqlite3
from pathlib import Path


def ensure_migrations_table(conn: sqlite3.Connection) -> None:
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS schema_migrations (
            migration_name TEXT PRIMARY KEY,
            checksum TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        """
    )
    conn.commit()


def file_checksum(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def applied(conn: sqlite3.Connection) -> dict[str, str]:
    """Return {migration_name: checksum} for all previously applied migrations."""
    rows = conn.execute("SELECT migration_name, checksum FROM schema_migrations").fetchall()
    return {row[0]: row[1] for row in rows}


def _split_sql_statements(sql_text: str) -> list[str]:
    """Split SQL into individual statements using the SQLite tokenizer.

    sqlite3.complete_statement() correctly handles semicolons inside string
    literals, block comments, and multi-statement DDL (e.g. triggers).
    Leading comment lines are stripped from each statement so that the
    caller's PRAGMA/BEGIN/COMMIT keyword checks work on the actual SQL.
    """
    statements: list[str] = []
    buf = ''
    for char in sql_text:
        buf += char
        if sqlite3.complete_statement(buf):
            # Strip leading comment-only lines to expose the first SQL keyword.
            sql_lines = [l for l in buf.splitlines() if not l.strip().startswith('--')]
            stmt = '\n'.join(sql_lines).strip()
            if stmt:
                statements.append(stmt)
            buf = ''
    return statements


def apply_migrations_to_connection(conn: sqlite3.Connection, sql_dir: Path, dry_run: bool = False) -> list[str]:
    ensure_migrations_table(conn)
    done = applied(conn)
    executed = []
    for sql_file in sorted(sql_dir.glob('*.sql')):
        if sql_file.name in done:
            stored_checksum = done[sql_file.name]
            current_checksum = file_checksum(sql_file)
            if stored_checksum != current_checksum:
                raise RuntimeError(
                    f'Migration checksum mismatch for {sql_file.name!r}: '
                    f'stored={stored_checksum[:12]}… current={current_checksum[:12]}… '
                    f'— migration file was modified after it was applied.'
                )
            continue
        checksum = file_checksum(sql_file)
        if not dry_run:
            stmts = _split_sql_statements(sql_file.read_text(encoding='utf-8'))
            pragma_stmts = [s for s in stmts if s.upper().startswith('PRAGMA')]
            body_stmts = [
                s for s in stmts
                if not s.upper().startswith('PRAGMA')
                and s.upper().rstrip('; ') not in ('BEGIN', 'COMMIT', 'ROLLBACK')
                and not s.upper().startswith(('BEGIN ', 'COMMIT ', 'ROLLBACK '))
            ]
            for stmt in pragma_stmts:
                conn.execute(stmt)
            conn.execute('BEGIN IMMEDIATE')
            try:
                for stmt in body_stmts:
                    conn.execute(stmt)
                conn.execute(
                    'INSERT INTO schema_migrations (migration_name, checksum) VALUES (?, ?)',
                    (sql_file.name, checksum),
                )
                conn.commit()
            except Exception:
                conn.rollback()
                raise
            # Restore FK enforcement disabled by migrations that use PRAGMA foreign_keys=OFF.
            conn.execute('PRAGMA foreign_keys=ON')
        executed.append(sql_file.name)
    return executed


def apply_migrations(db_path: Path, sql_dir: Path, dry_run: bool = False) -> list[str]:
    conn = sqlite3.connect(db_path)
    try:
        return apply_migrations_to_connection(conn, sql_dir, dry_run=dry_run)
    finally:
        conn.close()


def main() -> int:
    parser = argparse.ArgumentParser(description='Apply PRRO Gateway SQL migrations')
    parser.add_argument('--db', required=True, help='Path to SQLite DB')
    parser.add_argument('--sql-dir', default='sql', help='Directory with .sql files')
    parser.add_argument('--dry-run', action='store_true')
    args = parser.parse_args()

    executed = apply_migrations(Path(args.db), Path(args.sql_dir), dry_run=args.dry_run)
    print('Applied:' if not args.dry_run else 'Would apply:')
    for name in executed:
        print(f' - {name}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
