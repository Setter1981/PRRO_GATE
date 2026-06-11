export const meta = {
  name: 'blueprint-gather',
  description: 'Gather code-grounded inventory of every gateway subsystem + the missing runtime spine, to author a single connection blueprint',
  phases: [{ title: 'Gather', detail: '6 mappers inventory ingress / write-path / loops+supervisor / shift+node+offline / runtime cross-cuts / existing plans' }],
}

const REPO = '/mnt/d/PRRO_GATE'
const SRC = REPO + '/rust/prro/src'

const ESTABLISHED = `
ESTABLISHED FACTS (verified 2026-05-30, do NOT contradict — build on them):
- prro serve (main.rs:359-369) = boot_from_path_or_exit then idle ("M1 — idle"); ZERO spawned tasks; comment main.rs:365 "M3+ adds the supervisor + ingress shells. M1 just idles."
- stage_acquire::run (the PREPARED-row creator) has ZERO production callers repo-wide (28 test sites only).
- stage_sign/send + dispatch_post_sign prod callers are ALL boot_phase reconcile (enters at stage_sign:2486 over EXISTING PREPARED rows) OR backlog_drain:1192 (offline drain).
- App::reconcile_pending_with (app.rs:556), App::drain_offline_backlog_with (app.rs:620), spawn_return_online_probe have ZERO prod callers (tests only); Serve never calls them.
- HTTP ingress IngressServer::serve() {} (runtime/ingress/mod.rs:22) = empty stub; W5/W7 TBD; dto.rs:37-40 first real receipt "will fail with SignError::PayloadSchema".
- maria304_driver has NO prro dependency; talks an HTTP/DTO bridge whose prro-side receiver is the empty stub.
- prod bootstrap seeds ShiftState::CLOSED (boot_phase.rs:1304); no Offline/GoingOffline node-mode setter; OfflineSessionService::open_session 0 prod callers → offline unreachable end-to-end.
`

const RULES = ESTABLISHED + `
HARD RULES: OPEN the actual files, cite EXACT file:line for every claim. Distinguish WIRED (has a production caller) vs STUB/0-CALLERS. Do NOT invent type/function names — quote them. Your output feeds a single connection blueprint, so be precise about (1) what EXISTS+TESTED, (2) its real entry point, (3) what must be BUILT to drive it live, (4) the test anchor.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['subsystem', 'status_summary', 'entries', 'must_build', 'test_anchors', 'gaps'],
  properties: {
    subsystem: { type: 'string' },
    status_summary: { type: 'string', description: 'One dense paragraph: what exists, wired vs stub.' },
    entries: { type: 'array', items: { type: 'string' }, description: 'Each: "file:line — symbol — what it is — WIRED|STUB|0-CALLERS".' },
    must_build: { type: 'array', items: { type: 'string' }, description: 'Concrete connective work needed to drive this subsystem live, in dependency order.' },
    test_anchors: { type: 'array', items: { type: 'string' }, description: 'test file:line proving the subsystem works in isolation.' },
    gaps: { type: 'array', items: { type: 'string' }, description: 'what you could not determine + why.' },
  },
}

phase('Gather')

const MAPPERS = [
  {
    key: 'ingress',
    q: 'Map the INGRESS layer on HEAD. OPEN + cite file:line: (a) ' + REPO + '/rust/maria304_driver/ — bin entry, TCP listener, the protocol it speaks, and confirm NO prro dependency (what bridge contract does it target?); (b) ' + SRC + '/runtime/ingress/ — mod.rs IngressServer::serve stub, dto.rs DTO types + to_canonical_fiscal_command conversion + the PayloadSchema gap, any router/handler; (c) what an ingress handler must PRODUCE to hand off to the write-path (the canonical command / inbox-row shape). Blueprint question: what concretely must be built so a live inbound fiscal request reaches ingress_inbox?',
  },
  {
    key: 'writepath',
    q: 'Map the INBOX + WRITE-PATH stage library on HEAD. OPEN + cite file:line: (a) ' + SRC + '/db/repositories/ingress_inbox.rs — insert (NEW), claim/transition (NEW->PROCESSING->DONE), InboxStatus enum; (b) the stages stage_acquire / stage_sign / dispatch_post_sign / stage_send / stage_finalize — each fn signature, what it consumes/produces, the WorkerContext + dependencies it needs; (c) confirm stage_acquire::run creates the PREPARED fiscal_documents row (cite fn+line). Blueprint question: what is the EXACT call-chain + the inputs a live worker loop must assemble to run the online ladder PREPARED->SIGNED->SENT on one fresh inbox row?',
  },
  {
    key: 'loops_supervisor',
    q: 'Map the BACKGROUND-LOOP + SUPERVISOR/DI layer on HEAD. OPEN + cite file:line: (a) App::reconcile_pending / reconcile_pending_with (' + SRC + '/app.rs ~556) — what it does, one-shot vs loop, config knobs; (b) App::drain_offline_backlog_with / drain_offline_backlog_scheduled (app.rs ~606-654); (c) spawn_return_online_probe — interval/config + how it spawns; (d) is there ANY supervisor / task-orchestrator / RuntimeContainer / DI root in Rust (W4-Z0 deferred "dispatch DI" + "W4 supervisor PR" hint at a concept) — what scaffolding exists to spawn+supervise tasks together sharing App/pools, or is it absent? Blueprint question: what supervisor must wrap the ingress server + a live worker + the reconcile/drain/probe loops, and how do they share state?',
  },
  {
    key: 'shift_node_offline',
    q: 'Map SHIFT-LIFECYCLE + NODE-STATE + OFFLINE-REACHABILITY on HEAD. OPEN + cite file:line: (a) the shift 14-edge whitelist (search ' + SRC + ' for the shift transition whitelist, ~shifts.rs:67) — list each edge + WIRED (drain/boot: 1/2/5/6/7/9/13/14) vs 0-PROD-CALLER (online 3/4/8/10/11/12 = WL-1); (b) node_state mode setters — confirm NO Offline/GoingOffline setter (only set_mode_blocked_tx / set_mode_stop_mode_tx) + what sets ONLINE; (c) OfflineSessionService::open_session prod-caller count + stage_offline_ack preconditions (Opened + active offline session); (d) prod bootstrap seeds ShiftState::CLOSED (boot_phase.rs:1304). Blueprint question: what state-transition DRIVERS are missing for (i) shift opens online, (ii) node enters Offline, (iii) an offline session exists?',
  },
  {
    key: 'runtime_crosscuts',
    q: 'Map RUNTIME CROSS-CUTS on HEAD. OPEN + cite file:line: (a) ' + SRC + '/main.rs Cmd::Serve (359-369) + boot_from_path_or_exit + App::boot — what App::boot ACTUALLY does (migrate? integrity? crash-recovery? does it run reconcile?); (b) config layer — how serve reads config (' + REPO + '/ops/config*.yaml + the config struct), knobs for intervals/transports/crypto; (c) health endpoints (/health/live,/ready,/startup) + /metrics — do they exist in RUST or only Python? cite; (d) graceful shutdown — await_shutdown_signal + how future supervisor tasks would cooperate. Blueprint question: what runtime scaffolding (config, health, graceful shutdown) already exists to HOST the spine vs must be added?',
  },
  {
    key: 'existing_plans',
    q: 'Read the EXISTING planning docs so the blueprint ALIGNS instead of inventing a parallel taxonomy. OPEN + summarize with section anchors: (a) ' + REPO + '/docs/architecture/2026-05-29-pilot-integration-map.md — the WL-0..6 critical path (what each WL is), the 2 named pilot-blockers, open decisions Q1/Q3/Q5/Q6/Q-load; (b) ' + REPO + '/docs/superpowers/plans/2026-05-29-online-shift-lifecycle-wiring.md — the WL-1 A-prime decomposition pieces 0a..6 incl. prerequisite 0a; (c) ' + REPO + '/docs/architecture/ALGORITHMIC_MAP.md §1.11 gap table + Hard-Blocker list. Blueprint question: what is the EXISTING WL-0..6 vocabulary + the WL-1 piece breakdown, so the new blueprint re-expresses WL-0 as the runtime spine using the SAME names?',
  },
]

const results = await parallel(
  MAPPERS.map((m) => () =>
    agent(m.q + '\n' + RULES, { label: 'gather:' + m.key, phase: 'Gather', schema: SCHEMA }).then((x) => ({ key: m.key, ...x }))
  )
)

return results.filter(Boolean)
