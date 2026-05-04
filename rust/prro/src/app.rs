//! Composition root.  M1 wires DB pool + config; M2+ adds crypto,
//! transports, services.
//!
//! IMPORTANT (per bd-issue PRRO_GATE-ah8): `App::boot` MUST NOT blindly
//! call `node_state::upsert_initial(fn, Online, Closed, 1)` for every
//! configured FN.  Doing so would overwrite a `shift_state = Opened`
//! left by a crashed in-flight shift and mask the recovery requirement.
//! M1's boot only opens the pool + applies migrations; bootstrap of
//! `node_state` rows is deferred to a later task with explicit
//! reconciliation against `shifts` / `fiscal_documents`.

use crate::config::AppConfig;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct App {
    inner: Arc<Inner>,
}

struct Inner {
    config: AppConfig,
    db: SqlitePool,
}

impl App {
    pub async fn boot(config: AppConfig) -> anyhow::Result<Self> {
        if let Some(parent) = config.database.db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let db = crate::db::open_pool(&config.database.db_path).await?;
        Ok(Self {
            inner: Arc::new(Inner { config, db }),
        })
    }

    pub fn config(&self) -> &AppConfig {
        &self.inner.config
    }

    pub fn db(&self) -> &SqlitePool {
        &self.inner.db
    }
}
