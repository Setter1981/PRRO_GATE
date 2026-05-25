//! W9.1 + M3a hardening pass 3 — `App::boot` fail-closed semantics
//! on `PRAGMA quick_check` failure under the **two-phase open**.
//!
//! Per W9 freeze §5.3 + §10.2 + hardening pass 3:
//! 1. quick_check fail (on existing DB) → typed
//!    `BootError::IntegrityCheckFailed { reason }`.
//! 2. NO writes to any domain table after the failed probe (writing
//!    into a corrupt DB compounds corruption).  In the two-phase
//!    open shape this is a STRUCTURAL guarantee: `App::boot` Phase A
//!    returns Err BEFORE Phase B (migrations + Inner construction)
//!    can run, so no migration / no domain-table write is reachable.
//! 3. quick_check ok → boot proceeds normally + `reconcile_pending`
//!    succeeds.
//!
//! ## Active fixtures (all green under hardening pass 3)
//!
//! - **`quick_check_ok_proceeds_to_reconcile`** — happy path on an
//!   existing clean DB: `App::boot` Ok → `reconcile_pending` Ok.
//! - **`quick_check_fail_returns_typed_error`** — corruption-on-
//!   existing-DB fails with the typed `IntegrityCheckFailed`.
//! - **`quick_check_fail_main_file_bytes_unchanged_no_domain_writes`**
//!   — replaces the W9.1-era three table-targeted no_writes_to_X
//!   fixtures with a single mechanism-independent byte-equality
//!   proof: sha256(main_db_file) is identical before and after the
//!   failed boot.  Subsumes the original "no writes to node_state /
//!   audit_log / shifts" assertions without the fragility of doing
//!   post-state SELECT against a corrupted file.
//! - **`fresh_db_boots_through_migrations_with_post_quick_check`** —
//!   positive fresh-DB path: missing file → Phase A skipped → Phase B
//!   creates + migrates → post-migrate quick_check passes.
//!
//! ## History (W9.1 deferral closure)
//!
//! W9.1 had four `#[ignore]`d fixtures (one for typed-error +
//! three for table-targeted no-writes assertions) because reliably
//! corrupting a SQLite DB-file in a way that survives
//! `sqlx::migrate!` re-application proved non-trivial: file-level
//! byte writes were tolerated / WAL replay self-healed corruption /
//! migrations re-ran and overwrote damaged regions.
//!
//! Hardening pass 3 (two-phase open) closes that deferral:
//! `db::open_pool_no_migrate` runs `quick_check` BEFORE
//! `sqlx::migrate!`, so the corruption no longer needs to survive
//! migration re-runs.  The corruption shape that finally works is
//! a targeted overwrite of the sqlite_master btree page header
//! (offset 100..200, 100 bytes of 0xFF) — small enough to leave
//! the file's header intact (pool opens) but breaks the btree-page
//! structure enough for `quick_check` to detect.
//!
//! Fixture #5 from W9 freeze §10.2 (CRITICAL log line capture)
//! remains deferred — no `tracing-test` dev-dep in workspace.  Log
//! emission is operational visibility, not a safety invariant.

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use prro::{config::AppConfig, App, BootError};

fn cfg_toml(db_path: &str) -> String {
    format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{db_path}"
secure_db_path = "{db_path}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#
    )
}

async fn fresh_app(db_path: &Path) -> App {
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    App::boot(cfg).await.expect("fresh boot must succeed")
}

/// Deliberately corrupt the DB by:
///   1. Forcing a WAL checkpoint so all data lives in the main file
///      (W3-installed `journal_mode = WAL`; without checkpoint the
///      main file stays at header-only size).
///   2. Deleting the WAL + SHM sidecar files so SQLite reads ONLY
///      from the (about-to-be-corrupted) main file.
///   3. Overwriting the sqlite_master btree page header at offset
///      100..200 with 0xFF — small enough that the SQLite file
///      header (offset 0..100) stays intact so pool open succeeds,
///      but the sqlite_master page-header (cell count, cell
///      pointers) is structurally damaged enough that
///      `PRAGMA quick_check` reports the failure.
///
/// **Why this corruption shape:** see the module-level
/// `History (W9.1 deferral closure)` section.  Earlier attempts
/// (offset-200 byte writes / tail truncation / PRAGMA
/// writable_schema rootpage swaps) were either tolerated by SQLite
/// or broke too much (post-state SELECTs failed).  The targeted
/// 100-byte btree-header overwrite is the smallest precise
/// corruption that `quick_check` cannot ignore.  Combined with
/// hardening pass 3's pre-migration probe (no `sqlx::migrate!`
/// re-run before `quick_check`), this finally produces a stable
/// fail-closed test surface.
///
/// **M3a hardening pass 3:** helper is now actively used by the
/// fail-closed quick_check fixtures (no longer `#[ignore]`d under
/// the two-phase open path).  W9.1's `#[allow(dead_code)]` is no
/// longer needed.
async fn corrupt_via_schema_mutation(db_path: &Path) {
    // (1) Open raw pool, checkpoint WAL into main file, close pool.
    let url = format!("sqlite://{}", db_path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("open raw pool for checkpoint");
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint WAL into main file");
    pool.close().await;

    // (2) Delete WAL + SHM sidecar files so SQLite reads ONLY from the
    //     (about-to-be-corrupted) main file.  Without this, SQLite may
    //     "self-heal" via the WAL on the next open and quick_check
    //     passes.
    let wal_path = db_path.with_extension(
        db_path
            .extension()
            .map(|e| format!("{}-wal", e.to_string_lossy()))
            .unwrap_or_else(|| String::from("-wal")),
    );
    let shm_path = db_path.with_extension(
        db_path
            .extension()
            .map(|e| format!("{}-shm", e.to_string_lossy()))
            .unwrap_or_else(|| String::from("-shm")),
    );
    let _ = std::fs::remove_file(&wal_path);
    let _ = std::fs::remove_file(&shm_path);

    // (3) Overwrite the sqlite_master btree page header with
    //     0xFF garbage.  The DB file is laid out as:
    //       - bytes 0..100   : DB header (magic, page_size, etc.)
    //       - bytes 100..(page_size) : page 1 = sqlite_master root
    //                                  btree page; the btree-page
    //                                  header lives at offset 100
    //                                  (page type / cell offset /
    //                                  cell count / ...).
    //     Writing 0xFF at bytes 100..200 corrupts the
    //     sqlite_master page header structurally without touching
    //     the file's own header (so SQLite still recognises the
    //     file as a database, opens the pool, and runs
    //     `quick_check` — which then fails because cell counts
    //     and pointers are nonsense).  Earlier btree pages of
    //     other tables live BEYOND the page-1 sqlite_master root,
    //     so they remain readable for the "no_writes_to_X"
    //     post-state verification SELECTs.
    //
    //     **Why this beats byte-writes at offset 200 / tail
    //     truncation / full truncation:**
    //       - Offset 200 writes (the original helper, 2KB of 0xFF)
    //         hit the body of the sqlite_master page where free
    //         space exists; SQLite tolerated it.
    //       - 50-byte tail truncation broke only the last page;
    //         SQLite's quick_check still returned `ok`.
    //       - Truncation to 4096 broke ALL pages and made post-
    //         state verification SELECTs fail too.
    //       - 100-byte targeted overwrite of the sqlite_master
    //         btree header is the smallest precise corruption that
    //         `quick_check` cannot ignore yet other tables survive.
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(db_path)
        .expect("open db file for corruption");
    let size = f.metadata().expect("read db file metadata").len();
    assert!(
        size > 4096,
        "db file (size={size}) too small for corruption; checkpoint may have failed"
    );
    f.seek(SeekFrom::Start(100))
        .expect("seek to sqlite_master btree page header");
    let garbage = vec![0xFFu8; 100];
    f.write_all(&garbage)
        .expect("write garbage over btree page header");
    f.sync_all().expect("sync corrupted file");
    drop(f);
}

#[tokio::test]
async fn quick_check_fail_returns_typed_error() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.db");
    // 1) Create clean DB through normal boot path.
    let app = fresh_app(&db_path).await;
    drop(app); // release singleton + close pool.

    // 2) Corrupt sqlite_master via writable_schema.
    corrupt_via_schema_mutation(&db_path).await;

    // 3) Re-boot — quick_check MUST detect corruption.
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let result = App::boot(cfg).await;

    match result {
        Err(BootError::IntegrityCheckFailed { reason }) => {
            assert!(
                !reason.is_empty(),
                "BootError::IntegrityCheckFailed.reason must be non-empty"
            );
            assert_ne!(reason, "ok", "reason must not be the success string");
        }
        Ok(_) => panic!("expected IntegrityCheckFailed, got Ok"),
        Err(other) => panic!("expected IntegrityCheckFailed, got {other:?}"),
    }
}

/// Compute SHA-256 of the bytes of `path`.  Used by the
/// post-failed-boot byte-equality assertion below.
fn file_sha256(path: &Path) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).expect("read db file for hash");
    let mut h = Sha256::new();
    h.update(&bytes);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

/// M3a hardening — quick_check fail-closed no-domain-writes proof.
///
/// **Why this replaces the original three table-specific
/// no_writes_to_X fixtures:**
///
/// The original tests (`quick_check_fail_emits_no_writes_to_{node_state,
/// audit_log,shifts}`) pre-seeded individual tables and verified
/// post-failed-boot via `SELECT COUNT(*)` against each table.  Any
/// corruption strong enough to trigger `PRAGMA quick_check` failure
/// (target damaged sqlite_master / page header / btree structure)
/// also propagates to those `SELECT` queries — they fail with
/// `SQLITE_CORRUPT` against the same broken file, so post-state
/// verification cannot run.
///
/// **Byte-equality proof:** the cleanest, mechanism-independent
/// proof of "no writes happened on the failed boot path" is:
///   sha256(main_db_file_bytes) BEFORE failed boot
///       == sha256(main_db_file_bytes) AFTER failed boot
///
/// If the main DB file's bytes are unchanged, `App::boot` did not
/// write to it — regardless of which corruption shape was used.
/// This subsumes the original three table-targeted assertions
/// without any of their fragility.
///
/// ## Scope of the assertion — explicit carve-out (HP3 post-merge)
///
/// **What this fixture proves:**
///   - `App::boot` does NOT run `sqlx::migrate!` on the corrupted
///     existing DB (Phase B is unreachable because Phase A returns
///     `Err(IntegrityCheckFailed)`).
///   - No persisted domain DML — `node_state` / `audit_log` /
///     `shifts` / `fiscal_documents` rows are not touched — because
///     the main DB file is byte-identical across the failed boot.
///
/// **What this fixture does NOT prove — and intentionally so:**
///   - The WAL / SHM sidecar files (`*-wal` / `*-shm`) are NOT
///     hashed.  Phase A's probe pool issues `PRAGMA journal_mode =
///     WAL` on `connect_with`, which legitimately touches sidecar
///     metadata (WAL header initialisation, SHM mapping) without
///     constituting domain DML.  This is platform-dependent SQLite
///     behaviour: byte sizes of the sidecars after a probe-open are
///     not constant across SQLite versions / OS / filesystem, so
///     asserting "sidecar unchanged" would flake on legitimate
///     metadata writes.
///   - The Finding-2 closure contract is "do not migrate; do not
///     write domain rows before fail-closed return", NOT "do not
///     touch WAL metadata ever".  Sidecar touches are operational
///     noise; not a safety contract.
///
/// Asserting the main-file SHA-equality is the correct, minimal,
/// mechanism-independent proof of the closure contract.
#[tokio::test]
async fn quick_check_fail_main_file_bytes_unchanged_no_domain_writes() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.db");

    // 1) Clean DB via App::boot.
    let app = fresh_app(&db_path).await;

    // 2) Pre-seed both domain tables AND the audit log so we can
    //    prove no NEW rows were appended on the failed boot path.
    //    fiscal_number CHECK: exactly 10 digits.
    sqlx::query(
        "INSERT INTO fiscal_number_config (fiscal_number, tax_number, fiscal_mode) \
         VALUES ('1234567890', '1234567890', 'prod')",
    )
    .execute(app.db())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, next_lnd) \
         VALUES ('1234567890', 'ONLINE', 'CLOSED', 1)",
    )
    .execute(app.db())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO audit_log (entity_type, entity_id, event_type, severity, event_payload_json) \
         VALUES ('test', 'preseed-0001', 'TEST_PRESEED', 'INFO', '{}')",
    )
    .execute(app.db())
    .await
    .unwrap();
    drop(app); // release singleton + close pool

    // 3) Corrupt the main DB file.
    corrupt_via_schema_mutation(&db_path).await;

    // 4) Snapshot file bytes IMMEDIATELY after corruption — this is
    //    the "ground truth" of what the file looks like before we
    //    try to boot.  If the failed boot path is truly fail-closed
    //    with no domain writes, the file should be byte-identical
    //    to this snapshot after the boot returns Err.
    let hash_post_corruption = file_sha256(&db_path);

    // 5) Re-boot — quick_check must detect corruption and return
    //    IntegrityCheckFailed BEFORE migrations or any domain
    //    writes can land.
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let result = App::boot(cfg).await;
    match result {
        Err(BootError::IntegrityCheckFailed { .. }) => {}
        Err(other) => panic!("boot must fail with IntegrityCheckFailed, got {other:?}"),
        Ok(_) => panic!("boot must fail with IntegrityCheckFailed, got Ok"),
    }

    // 6) **LOAD-BEARING:** the main DB file is byte-for-byte
    //    identical to the post-corruption snapshot.  Subsumes the
    //    original "no_writes_to_{node_state, audit_log, shifts}"
    //    assertions — any write that landed via the failed boot
    //    path would have changed the file's SHA-256.
    let hash_post_failed_boot = file_sha256(&db_path);
    assert_eq!(
        hash_post_failed_boot, hash_post_corruption,
        "main DB file bytes MUST be unchanged across failed boot (HP3 fail-closed; \
         no migrations / no node_state writes / no audit_log writes / no shifts \
         writes / no fiscal_documents writes)"
    );
}

/// Positive fresh-DB path: ensure `App::boot` against a missing
/// file still creates + migrates + passes post-migrate quick_check.
/// This pins the "fresh DB" branch of the two-phase open — Phase A
/// is skipped (db_exists = false), Phase B runs create+migrate,
/// and the defence-in-depth post-migrate quick_check confirms the
/// migrated schema is structurally sound.
#[tokio::test]
async fn fresh_db_boots_through_migrations_with_post_quick_check() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("fresh.db");
    assert!(!db_path.exists(), "pre-state: fresh DB path must not exist");

    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.expect("fresh DB boot must succeed");

    // Post-state: file exists, has non-zero size (migrations ran),
    // and basic schema is present.
    assert!(db_path.exists(), "fresh boot must create the DB file");
    let size = std::fs::metadata(&db_path).expect("metadata").len();
    assert!(size > 0, "fresh boot must populate the DB file (size > 0)");

    // Sanity: a known table from migration 001 is queryable.
    let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_state")
        .fetch_one(app.db())
        .await
        .expect("fresh DB must have migrated node_state schema");
}

#[tokio::test]
async fn quick_check_ok_proceeds_to_reconcile() {
    // Verify the happy path: clean DB → App::boot OK → reconcile_pending OK.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("a.db");
    let toml_text = cfg_toml(&db_path.display().to_string().replace('\\', "/"));
    let cfg = AppConfig::from_toml(&toml_text).unwrap();
    let app = App::boot(cfg).await.expect("clean DB must boot OK");
    // W9.1 stub returns Ok(()); W9.3 wires real dispatch.
    app.reconcile_pending()
        .await
        .expect("reconcile_pending stub must return Ok");
}
