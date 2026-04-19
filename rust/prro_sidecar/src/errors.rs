//! `SidecarError` — unified error enum for the fiscal driver pipeline.
//! Phase 0 stub; filled in Phase 1.

#![allow(dead_code)]

#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("license: {0}")]
    License(String),
    #[error("credentials: {0}")]
    Credentials(String),
    #[error("cms sign failed: {0}")]
    CmsSign(String),
    #[error("grpc: {0}")]
    Grpc(String),
    #[error("tsp failed: {0}")]
    Tsp(String),
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("internal: {0}")]
    Internal(String),
}
