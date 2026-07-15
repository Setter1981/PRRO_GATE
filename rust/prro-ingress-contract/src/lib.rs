//! `prro-ingress-contract` — the **ingress boundary** of the PRRO gateway.
//!
//! Future home of the **ingress port** (the trait each protocol adapter — REST
//! / XML-RPC / Maria — implements) and the `CanonicalIngressEnvelope` value the
//! adapters normalise raw protocol payloads into before the write-path sees
//! them. It draws the seam between *"how a request arrived on the wire"* and
//! *"the one canonical shape the core reasons about"*, so the engine never
//! learns a protocol's shape and a new protocol never reaches into the engine.
//!
//! CS-1d (`docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §8 item 5) creates this crate as an **empty dependency skeleton** — a crate
//! boundary only. **No ports, no traits, no types, no dependencies** (not even
//! `prro-domain`); those land in a later sprint (specs #3–5 / CS-6) when the
//! ports are defined. The RP-CS1-4 DAG gate
//! (`prro-domain/tests/rp_cs1_4_contract_dag.rs`) pins that this crate reaches
//! no adapter/IO crate and stays orthogonal to the dps/fleet contracts.

#![forbid(unsafe_code)]
