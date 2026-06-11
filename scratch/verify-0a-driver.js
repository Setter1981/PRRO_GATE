export const meta = {
  name: 'verify-0a-driver',
  description: 'Verify WL-1 prerequisite 0a: does a LIVE ingress -> write-path driver exist anywhere, or is stage_acquire genuinely undriven on HEAD?',
  phases: [{ title: 'Investigate', detail: '4 searchers trace the live write-path driver across the whole repo' }],
}

const REPO = '/mnt/d/PRRO_GATE'
const RUST = REPO + '/rust'
const SRC = RUST + '/prro/src'

const RULES = `
HARD RULES: trust nothing, OPEN the actual files, cite EXACT file:line for every claim. Search the WHOLE repo (` + RUST + ` incl. prro/src, prro/src/bin, prro/src/runtime, examples, tests; and ` + REPO + ` for any non-Rust driver). The question is binary + decisive: does ANYTHING drive the online write-path (stage_acquire -> stage_sign -> stage_send) on a LIVE ingress request in production, or do those stages run ONLY from boot recovery + offline drain? A round-3 reviewer found stage_acquire::run has ZERO callers in prro/src but could NOT rule out an out-of-tree driver. Resolve it definitively.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['summary', 'verdict', 'findings', 'anchors', 'gaps'],
  properties: {
    summary: { type: 'string', description: 'Dense answer to the assigned question. Facts only.' },
    verdict: { type: 'string', description: 'For your angle: does a LIVE-ingress write-path driver exist? YES (cite it) / NO / PARTIAL — with the decisive evidence.' },
    findings: { type: 'array', items: { type: 'string' }, description: 'Each a fact with file:line.' },
    anchors: { type: 'array', items: { type: 'string' } },
    gaps: { type: 'array', items: { type: 'string' }, description: 'what you could not determine + why.' },
  },
}

phase('Investigate')

const SEARCHERS = [
  {
    key: 'writepath_callers',
    q: 'Find EVERY caller of the online write-path entry points across the WHOLE ' + RUST + ' tree (prro/src, prro/src/bin, prro/src/runtime, tests, examples) AND ' + REPO + ' broadly: grep for `stage_acquire::run`, `stage_sign::run`, `stage_send::run`, `dispatch_post_sign`, and any `process_request` / write-path-worker entry. For EACH caller, OPEN it and classify: (A) boot/crash-recovery (boot_phase), (B) offline drain (backlog_drain), (C) test/#[cfg(test)], or (D) LIVE-ingress foreground driver. The decisive question: is there ANY caller of class D (a live ingress request driving stage_acquire/stage_sign/stage_send), or are ALL callers A/B/C? Give the full caller list with file:line + class.',
  },
  {
    key: 'runtime_ingress',
    q: 'Read ' + SRC + '/runtime/ end-to-end (the ingress shells: REST/Axum, XML-RPC, Maria; the RuntimeContainer / DI root; any IngressService; runtime/ingress/*). Trace what happens from a LIVE inbound request (e.g. an HTTP POST sell) all the way to the write-path: does the shell (1) store to ingress_inbox and SYNCHRONOUSLY drive stage_acquire->...->stage_send, (2) store to inbox and trigger/spawn a worker that drives the stages, or (3) only store to inbox with NO processing on HEAD? Quote the handler bodies + the exact call chain (or its absence) with file:line. Is the REST app even mounted/served (is there a route that processes a fiscal op end-to-end), or are the handlers stubs?',
  },
  {
    key: 'main_run_loops',
    q: 'Read ' + SRC + '/main.rs and any ' + SRC + '/bin/* and the App boot/run code (' + SRC + '/app.rs). Enumerate EVERY background task / loop the binary spawns on startup (e.g. App::boot recovery, reconcile_pending loop, return-online probe, offline drain loop, an HTTP server, a write-path/inbox-processing worker). For EACH: does it drive the online write-path (stage_acquire/stage_sign/stage_send) on NEW live ingress rows, or only recover/drain existing docs? Specifically: is there a periodic/triggered loop that picks up freshly-ingested inbox rows and drives them through the write-path, or does live processing depend entirely on the boot/reconcile path? Cite file:line for each spawned task + what it drives.',
  },
  {
    key: 'inbox_flow',
    q: 'Trace the ingress_inbox + fiscal_documents lifecycle to find the DE-FACTO live driver. OPEN ' + SRC + '/db/repositories/ingress_inbox.rs + the InboxStatus enum (NEW/PROCESSING/DONE) + ' + SRC + '/services/ingress* if present. Answer: (1) Who INSERTs an ingress_inbox row (status NEW) on a live request — and does that same path create the PREPARED fiscal_documents row (i.e. is stage_acquire run inline) or just the inbox row? (2) Who moves NEW->PROCESSING and drives the write-path? (3) Does stage_acquire::run actually create the PREPARED fiscal_documents row, and is it called from a live path or only boot/drain? (4) My W4-Z3 live smoke seeded PREPARED fiscal_documents rows directly + drove them via App::reconcile_pending_with (the boot/reconcile path) — confirm whether that reconcile path is the ONLY thing that drives PREPARED->SIGNED->SENT in production, i.e. is the live model "ingress writes PREPARED, a reconcile loop drains it" or "ingress synchronously drives the write-path" or "nothing drives it". Cite file:line.',
  },
]

const results = await parallel(
  SEARCHERS.map((s) => () =>
    agent(s.q + '\n' + RULES, { label: '0a:' + s.key, phase: 'Investigate', schema: SCHEMA }).then((x) => ({ key: s.key, ...x }))
  )
)

return results.filter(Boolean)
