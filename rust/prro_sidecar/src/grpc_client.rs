//! Per-mode persistent tonic channel pool — prod / test endpoints.
//! Phase 4 implements full pool + 7 RPC wrappers; this is a Phase 0 stub.

#![allow(dead_code)]

pub struct DpsGrpcPool;

impl DpsGrpcPool {
    pub async fn new() -> Self {
        unimplemented!("DpsGrpcPool::new: Phase 4 stub")
    }
}
