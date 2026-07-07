//! M3b W8 — offline-sync services (return-online probe + backlog
//! drain coordinator).

pub mod backlog_drain;
pub mod backoff;
pub mod kvt2_confirm;
pub mod offline_surface;
pub mod return_online_probe;
