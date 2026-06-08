//! RS-3 C1 — shift-lifecycle services.
//!
//! Currently the single shift-state [`transition`] service (the sole home
//! for `shifts` primary writes paired with `node_state` projection
//! mirrors). The WL-1 online shift lifecycle (`stage_acquire` intent-edge
//! wiring + the ACK-edge finalize hook) builds on top of it.

pub mod transition;
