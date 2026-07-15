//! `prro-fleet-contract` — the **fleet lifecycle boundary** of the PRRO gateway.
//!
//! Future home of the **fleet lifecycle port** — the seam through which a
//! control-plane manages a *fleet* of PRRO fiscal numbers (their registration,
//! shift lifecycle, health, and advisory state) without reaching into any one
//! node's write-path. The gateway core is already per-`fiscal_number`
//! (fleet-shaped); this contract is where that per-FN engine will expose a
//! lifecycle interface a fleet manager drives, so the enterprise control-plane
//! never couples to a node's internal state machine and a node never learns the
//! fleet's topology.
//!
//! CS-1d (`docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §8 item 5) creates this crate as an **empty dependency skeleton** — a crate
//! boundary only. **No ports, no traits, no types, no dependencies** (not even
//! `prro-domain`); those land in a later sprint (specs #3–5 / CS-6) when the
//! ports are defined. The RP-CS1-4 DAG gate
//! (`prro-domain/tests/rp_cs1_4_contract_dag.rs`) pins that this crate reaches
//! no adapter/IO crate and stays orthogonal to the ingress/dps contracts.

#![forbid(unsafe_code)]
