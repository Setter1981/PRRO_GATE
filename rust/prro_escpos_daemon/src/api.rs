//! HTTP JSON API for the ESC/POS daemon.
//!
//! Request / response shapes are stable JSON contracts — clients in
//! any language can consume them without a Rust-specific SDK.

use crate::registry::{ProfileRegistry, ProfileSummary};
use crate::transport;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use prro_escpos::{Alignment, CodePage, Instruction, ReceiptCompiler};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<ProfileRegistry>,
    pub default_timeout_ms: u64,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/profiles", get(list_profiles))
        .route("/compile", post(compile))
        .route("/print", post(print))
        .with_state(state)
}

// ─── /health ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct HealthResponse {
    pub live: bool,
    pub name: &'static str,
    pub version: &'static str,
    pub profiles_loaded: usize,
}

async fn health(State(st): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        live: true,
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        profiles_loaded: st.registry.list().len(),
    })
}

// ─── /profiles ───────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ProfilesResponse {
    pub profiles: Vec<ProfileSummary>,
}

async fn list_profiles(State(st): State<AppState>) -> Json<ProfilesResponse> {
    Json(ProfilesResponse {
        profiles: st.registry.list(),
    })
}

// ─── /compile ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CompileRequest {
    /// Profile key registered in the daemon (e.g. "tm-t88ii").
    pub profile: String,
    /// Ordered semantic instructions to compile.
    pub instructions: Vec<InstructionSpec>,
}

#[derive(Serialize)]
pub struct CompileResponse {
    pub bytes_hex: String,
    pub bytes_base64: String,
    pub length: usize,
}

async fn compile(
    State(st): State<AppState>,
    Json(req): Json<CompileRequest>,
) -> Result<Json<CompileResponse>, ApiError> {
    let bytes = compile_request(&st, &req)?;
    use base64::Engine;
    Ok(Json(CompileResponse {
        length: bytes.len(),
        bytes_hex: hex_lower(&bytes),
        bytes_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
    }))
}

// ─── /print ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PrintRequest {
    pub profile: String,
    pub instructions: Vec<InstructionSpec>,
    pub destination: Destination,
    /// Optional override for the connect + write timeout.  Falls back
    /// to the daemon's configured default.
    pub timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Destination {
    Tcp { host: String, port: u16 },
    // Serial { device: String, baud: u32 },   // Step 3
    // Usb { vendor_id: u16, product_id: u16 },// Step 3
}

#[derive(Serialize)]
pub struct PrintResponse {
    pub ok: bool,
    pub length: usize,
}

async fn print(
    State(st): State<AppState>,
    Json(req): Json<PrintRequest>,
) -> Result<Json<PrintResponse>, ApiError> {
    let bytes = compile_request(
        &st,
        &CompileRequest {
            profile: req.profile.clone(),
            instructions: req.instructions,
        },
    )?;
    let timeout = req.timeout_ms.unwrap_or(st.default_timeout_ms);
    match req.destination {
        Destination::Tcp { host, port } => {
            transport::send_tcp(&host, port, &bytes, timeout)
                .await
                .map_err(ApiError::Transport)?;
        }
    }
    Ok(Json(PrintResponse { ok: true, length: bytes.len() }))
}

// ─── Instruction JSON mapping ────────────────────────────────────────
//
// The wire JSON uses tagged-union form so clients don't have to guess
// which field to set.  Example:
//
//   {"op": "text", "value": "HELLO"}
//   {"op": "align", "value": "center"}
//   {"op": "feed", "value": 3}
//   {"op": "raw", "hex": "1b40"}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum InstructionSpec {
    Init,
    Codepage { value: CodePageSpec },
    Align { value: AlignmentSpec },
    Bold { value: bool },
    Text { value: String },
    Raw { hex: String },
    Newline,
    Feed { value: u8 },
    Cut,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodePageSpec {
    Cp866,
    Cp1251,
    Ascii,
}

impl From<CodePageSpec> for CodePage {
    fn from(v: CodePageSpec) -> Self {
        match v {
            CodePageSpec::Cp866 => CodePage::Cp866,
            CodePageSpec::Cp1251 => CodePage::Cp1251,
            CodePageSpec::Ascii => CodePage::Ascii,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlignmentSpec {
    Left,
    Center,
    Right,
}

impl From<AlignmentSpec> for Alignment {
    fn from(v: AlignmentSpec) -> Self {
        match v {
            AlignmentSpec::Left => Alignment::Left,
            AlignmentSpec::Center => Alignment::Center,
            AlignmentSpec::Right => Alignment::Right,
        }
    }
}

// ─── Shared compile path ─────────────────────────────────────────────

fn compile_request(st: &AppState, req: &CompileRequest) -> Result<Vec<u8>, ApiError> {
    let profile = st
        .registry
        .get(&req.profile)
        .ok_or_else(|| ApiError::UnknownProfile(req.profile.clone()))?;
    let mut c = ReceiptCompiler::new(profile);
    for spec in &req.instructions {
        let inst = match spec {
            InstructionSpec::Init => Instruction::Init,
            InstructionSpec::Codepage { value } => {
                let cp: CodePage = (*value).clone().into();
                Instruction::Codepage(cp)
            }
            InstructionSpec::Align { value } => Instruction::Align((*value).clone().into()),
            InstructionSpec::Bold { value } => Instruction::Bold(*value),
            InstructionSpec::Text { value } => Instruction::Text(value.clone()),
            InstructionSpec::Raw { hex } => {
                let bytes = hex::decode(hex).map_err(|_| ApiError::BadHex(hex.clone()))?;
                Instruction::Raw(bytes)
            }
            InstructionSpec::Newline => Instruction::Newline,
            InstructionSpec::Feed { value } => Instruction::Feed(*value),
            InstructionSpec::Cut => Instruction::Cut,
        };
        c.push(inst);
    }
    c.compile().map_err(|e| ApiError::Compile(e.to_string()))
}

// Allow Clone so JSON tagged enums can be converted twice (we move
// once from spec into Instruction builder).  Avoids intermediate heap.
impl Clone for CodePageSpec {
    fn clone(&self) -> Self {
        match self {
            CodePageSpec::Cp866 => CodePageSpec::Cp866,
            CodePageSpec::Cp1251 => CodePageSpec::Cp1251,
            CodePageSpec::Ascii => CodePageSpec::Ascii,
        }
    }
}
impl Clone for AlignmentSpec {
    fn clone(&self) -> Self {
        match self {
            AlignmentSpec::Left => AlignmentSpec::Left,
            AlignmentSpec::Center => AlignmentSpec::Center,
            AlignmentSpec::Right => AlignmentSpec::Right,
        }
    }
}

// ─── Errors → HTTP response ──────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("unknown profile: {0}")]
    UnknownProfile(String),

    #[error("invalid hex in Raw instruction: {0}")]
    BadHex(String),

    #[error("compile: {0}")]
    Compile(String),

    #[error("transport: {0}")]
    Transport(#[from] transport::TransportError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            ApiError::UnknownProfile(_) => StatusCode::NOT_FOUND,
            ApiError::BadHex(_) => StatusCode::BAD_REQUEST,
            ApiError::Compile(_) => StatusCode::BAD_REQUEST,
            ApiError::Transport(_) => StatusCode::BAD_GATEWAY,
        };
        let body = serde_json::json!({
            "error": self.to_string(),
        });
        (status, Json(body)).into_response()
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(hex_nibble(b >> 4));
        out.push(hex_nibble(b & 0x0F));
    }
    out
}
fn hex_nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'a' + n - 10) as char,
    }
}
