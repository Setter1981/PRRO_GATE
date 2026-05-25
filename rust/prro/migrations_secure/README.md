# `migrations_secure/` — secure database migration set

This directory holds migrations for the **secure** SQLite database
(`var/secure.db` per `DatabaseCfg.secure_db_path`), which is physically
isolated from the main ledger (`var/prro.db`) per HIGH-AUDIT-01
("hard-isolation callout for var/secure.db").

## Why a second directory

`sqlx::migrate!()` is a **compile-time macro** — its path is fixed at
build time, and the bundled SQL files are baked into the binary. Each
`migrate!()` invocation owns one set of files and one checksum table
(`_sqlx_migrations`).

The main migrations directory (`migrations/`) targets `prro.db`
(documents / shifts / transport_trace / audit_log). Mixing the
`operators` table migration into that set would:

  1. Place the cashier-key obfuscated-password BLOB in the same file
     as the document ledger, defeating the HIGH-AUDIT-01 isolation.
  2. Cause `sqlx::migrate!("./migrations").run(&main_pool)` to attempt
     to create `operators` inside `prro.db` (since the macro doesn't
     know about per-file scoping).

Hence the split: `migrations_secure/` is invoked by
`db::open_secure_pool` against the secure pool only, via a separate
`sqlx::migrate!("./migrations_secure")` call.

## Conventions

- Numbering continues the main set (020+ — main set ends at 019 at
  W2 start). This keeps the two histories visually distinct from
  each other.
- Each file MUST be additive only on a fresh `secure.db`; the file
  starts empty in pre-pilot environments, so backfill logic is
  unnecessary.
- Cross-DB foreign keys are not supported by SQLite. Constraints that
  reference the main ledger (`fiscal_number_config`, `audit_log`, etc.)
  must be implemented as runtime compensating checks in the repository
  / boot-phase Rust code, not as SQL `REFERENCES` clauses.
- The migration runner uses the same checksum-verified machinery as
  the main set (sqlx applies migrations in numeric order, records
  hashes in `_sqlx_migrations` table within `secure.db`, and refuses
  to re-apply a migration whose checksum changed).

## Rollback

True rollback is **manual**: stop `prro`, `DELETE FROM _sqlx_migrations
WHERE version = <N>` against `secure.db`, `DROP TABLE <table>` as
needed, restart. This is the same procedure as the main set (sqlx does
not generate down-migrations).
