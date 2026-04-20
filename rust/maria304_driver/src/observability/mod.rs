//! Observability — structured logs + in-session frame trace buffer.
//!
//! M8 lands:
//!   * [`init_json_subscriber`] — opt-in JSON logging via
//!     tracing-subscriber.  Applications that already have their own
//!     subscriber can skip this and just rely on our `tracing::info!`
//!     emits which propagate naturally.
//!   * [`FrameTrace`] — per-session ring buffer of the last N wire
//!     frames (in + out).  The admin API (M10) exposes this so
//!     operators can extract a capture of a running session for
//!     support / debugging.
//!   * [`SessionMetrics`] — lightweight per-session counters that the
//!     metrics exporter (M10) aggregates into Prometheus series.

pub mod frame_trace;
pub mod init;
pub mod metrics;

pub use frame_trace::{FrameTrace, FrameTraceEntry, FrameTraceKind};
pub use init::init_json_subscriber;
pub use metrics::SessionMetrics;
