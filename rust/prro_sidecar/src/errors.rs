//! `SidecarError` — unified error enum for the fiscal driver pipeline.

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
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("internal: {0}")]
    Internal(String),
}

impl axum::response::IntoResponse for SidecarError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode;
        use axum::Json;
        // Log full diagnostic before sanitizing for the HTTP response.
        // Credentials / Db / Internal details must NOT reach the wire — they may
        // contain key-encoding hints, SQL fragments, or file paths.
        tracing::error!(error = %self, "fiscal handler error");
        let (status, msg) = match &self {
            Self::BadRequest(m)  => (StatusCode::BAD_REQUEST,           m.clone()),
            Self::NotFound(m)    => (StatusCode::NOT_FOUND,             m.clone()),
            Self::License(m)     => (StatusCode::FORBIDDEN,             m.clone()),
            Self::Credentials(_) => (StatusCode::INTERNAL_SERVER_ERROR, "credential error".into()),
            Self::CmsSign(_)     => (StatusCode::BAD_GATEWAY,           "cms sign failed".into()),
            Self::Grpc(_)        => (StatusCode::BAD_GATEWAY,           "dps unavailable".into()),
            Self::Db(_)          => (StatusCode::INTERNAL_SERVER_ERROR, "database error".into()),
            Self::Internal(_)    => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };
        (status, Json(serde_json::json!({"error": msg}))).into_response()
    }
}
