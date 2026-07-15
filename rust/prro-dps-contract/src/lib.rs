//! `prro-dps-contract` — the **DPS (tax-service) boundary** of the PRRO gateway.
//!
//! Future home of the **DPS port** (the trait the transport/backend
//! implementations satisfy to submit a signed document to the Ukrainian tax
//! service) and the **typed delivery-outcome** the core reasons over —
//! discriminating *definitely-not-sent* from *submitted-unknown* from
//! *remote-response* from *fiscal-accepted*, the distinction the online↔offline
//! transition and the double-issue safety depend on. It draws the seam between
//! *"we spoke to DPS over some wire"* and *"what the core is allowed to
//! conclude about that exchange"*, so the engine never sees a raw HTTP/XML
//! error and a transport never decides fiscal state.
//!
//! CS-1d (`docs/superpowers/specs/2026-07-14-cs1-contract-behaviour-neutral-skeleton.md`
//! §8 item 5) creates this crate as an **empty dependency skeleton** — a crate
//! boundary only. **No ports, no traits, no types, no dependencies** (not even
//! `prro-domain`); those land in a later sprint (specs #3–5 / CS-6) when the
//! ports are defined (the typed-delivery *semantic* change stays CS-3). The
//! RP-CS1-4 DAG gate (`prro-domain/tests/rp_cs1_4_contract_dag.rs`) pins that
//! this crate reaches no adapter/IO crate and stays orthogonal to the
//! ingress/fleet contracts.

#![forbid(unsafe_code)]
