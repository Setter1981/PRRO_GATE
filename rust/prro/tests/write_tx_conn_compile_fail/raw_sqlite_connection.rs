//! Case 1 (W0-2 §9.2): caller passes a raw `&mut SqliteConnection`
//! to `transition_state`.  Must NOT compile — `transition_state` now
//! takes `&mut WriteTxConn<'_>` (ADR-M3-A4).  Expected: E0308
//! mismatched types.

use prro::db::models::enums::DocState;
use prro::db::models::ids::DocumentId;
use prro::db::repositories::fiscal_documents::transition_state;

async fn boom(conn: &mut sqlx::SqliteConnection, id: DocumentId) {
    let _ = transition_state(&mut *conn, id, DocState::Prepared, DocState::Signed).await;
}

fn main() {}
