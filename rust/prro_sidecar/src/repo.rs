//! SQLite repository — rusqlite queries for fn_config, sidecar_operators,
//! licenses, operator_certs. Short transactions only — invariant (1).
//! Phase 1.2 implements full Repo; this is a Phase 0 stub.

#![allow(dead_code)]

pub struct Repo;

impl Repo {
    pub fn open(_path: &str) -> Result<Self, rusqlite::Error> {
        unimplemented!("Repo::open: Phase 1 stub")
    }
}
