//! `prro-testkit` — shared **test-only** scaffolding for the PRRO workspace.
//!
//! Intended to be pulled in as a `dev-dependency` by test targets only. The
//! crate is marked `publish = false` and, per RP-CS1-4, must stay **absent
//! from every production package's normal/build dependency graph** — it may be
//! a workspace member that `cargo build --workspace` compiles, but no prod
//! crate may `[dependencies] prro-testkit`.
//!
//! CS-1a is the **scaffolding** slice: this is an empty skeleton. Shared test
//! helpers land in later CS-1 slices as the domain move creates the need.

#![forbid(unsafe_code)]
