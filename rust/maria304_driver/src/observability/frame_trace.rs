//! Per-session ring buffer of recent wire frames.
//!
//! Stores the last `capacity` frames (incoming + outgoing) so the
//! admin API can produce a snapshot for support / debugging.  Bounded
//! size — old entries are dropped when the buffer fills.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Direction of a traced frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameTraceKind {
    /// Frame received from the client.
    Inbound,
    /// Frame emitted to the client.
    Outbound,
}

/// A single traced wire frame.
#[derive(Debug, Clone)]
pub struct FrameTraceEntry {
    pub kind: FrameTraceKind,
    /// When the frame was observed, relative to session start.
    pub offset: Duration,
    /// Text content (CP866-decoded) of the frame.
    pub text: String,
    /// Whether the frame carried a trailing CRC on the wire.
    pub had_crc: bool,
}

/// Fixed-capacity ring buffer of [`FrameTraceEntry`].
#[derive(Debug)]
pub struct FrameTrace {
    capacity: usize,
    session_started: Instant,
    entries: VecDeque<FrameTraceEntry>,
}

impl FrameTrace {
    /// Create a trace buffer with the given capacity.  Capacity 0 is
    /// clamped to 1 — a zero-sized buffer is never useful.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            session_started: Instant::now(),
            entries: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    /// Push one frame into the trace.  Drops the oldest entry when
    /// the buffer is at capacity.
    pub fn push(&mut self, kind: FrameTraceKind, text: impl Into<String>, had_crc: bool) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(FrameTraceEntry {
            kind,
            offset: self.session_started.elapsed(),
            text: text.into(),
            had_crc,
        });
    }

    /// Snapshot — returns all stored entries in insertion order.
    #[must_use]
    pub fn snapshot(&self) -> Vec<FrameTraceEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Current number of stored entries.
    ///
    /// (Note: deliberately not using `usize` `is_empty` convention
    /// because both `len` and `is_empty` are part of the observer
    /// API and kept in parallel for ergonomics.)
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the buffer has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for FrameTrace {
    /// 100-frame buffer — covers a typical multi-line receipt with
    /// `PSDt` + `ACLD` + `COMP` plus handshake overhead.
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_capacity_is_clamped_to_one() {
        let mut t = FrameTrace::new(0);
        t.push(FrameTraceKind::Inbound, "X", false);
        t.push(FrameTraceKind::Inbound, "Y", false);
        assert_eq!(t.len(), 1);
        assert_eq!(t.snapshot()[0].text, "Y");
    }

    #[test]
    fn ring_buffer_drops_oldest_when_full() {
        let mut t = FrameTrace::new(3);
        for ch in ["A", "B", "C", "D"] {
            t.push(FrameTraceKind::Inbound, ch, false);
        }
        let texts: Vec<String> = t.snapshot().into_iter().map(|e| e.text).collect();
        assert_eq!(texts, vec!["B", "C", "D"]);
    }

    #[test]
    fn insertion_order_is_preserved() {
        let mut t = FrameTrace::new(5);
        t.push(FrameTraceKind::Inbound, "first", false);
        t.push(FrameTraceKind::Outbound, "second", true);
        let snap = t.snapshot();
        assert_eq!(snap[0].text, "first");
        assert_eq!(snap[0].kind, FrameTraceKind::Inbound);
        assert_eq!(snap[1].text, "second");
        assert_eq!(snap[1].kind, FrameTraceKind::Outbound);
        assert!(snap[1].had_crc);
    }

    #[test]
    fn offsets_are_monotonically_non_decreasing() {
        let mut t = FrameTrace::new(10);
        for i in 0..5 {
            t.push(FrameTraceKind::Inbound, format!("f{i}"), false);
        }
        let snap = t.snapshot();
        for w in snap.windows(2) {
            assert!(w[1].offset >= w[0].offset, "offsets went backwards");
        }
    }

    #[test]
    fn empty_buffer_is_len_zero_and_is_empty_is_true() {
        let t = FrameTrace::new(5);
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
    }

    #[test]
    fn default_capacity_is_100() {
        let mut t = FrameTrace::default();
        for i in 0..150 {
            t.push(FrameTraceKind::Inbound, format!("f{i}"), false);
        }
        assert_eq!(t.len(), 100);
        assert_eq!(t.snapshot()[0].text, "f50"); // oldest preserved is 50
    }
}
