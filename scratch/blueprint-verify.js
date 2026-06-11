export const meta = {
  name: 'blueprint-verify',
  description: 'Adversarially verify the runtime-spine connection blueprint against real code: anchors, vocabulary alignment, over-claims',
  phases: [{ title: 'Verify', detail: '3 adversarial checkers: anchors / vocab+order / over-claim+completeness' }],
}

const DOC = '/mnt/d/PRRO_GATE/docs/architecture/2026-05-30-runtime-spine-connection-blueprint.md'
const SRC = '/mnt/d/PRRO_GATE/rust/prro/src'
const MAP = '/mnt/d/PRRO_GATE/docs/architecture/2026-05-29-pilot-integration-map.md'
const WL1 = '/mnt/d/PRRO_GATE/docs/superpowers/plans/2026-05-29-online-shift-lifecycle-wiring.md'
const AMAP = '/mnt/d/PRRO_GATE/docs/architecture/ALGORITHMIC_MAP.md'

const RULES = `
HARD RULES: READ the blueprint at ` + DOC + ` first. Then OPEN the actual code/docs to verify — do NOT trust the blueprint's own claims. For every defect cite the blueprint location (section + the quoted phrase) AND the correct fact with a real file:line. Be adversarial: your job is to find what is WRONG, IMPRECISE, or OVER-CLAIMED, not to praise. If a claim checks out, do not report it. A blueprint that ships a wrong file:line anchor or a wrong WIRED/0-callers tag is worse than useless — it relaunches the same "invented vocab" failure the prior reviews caught.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['verdict', 'defects', 'notes'],
  properties: {
    verdict: { type: 'string', enum: ['PASS', 'MINOR_FIXES', 'MAJOR_FIXES'] },
    defects: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['severity', 'doc_location', 'problem', 'correct_fact'],
        properties: {
          severity: { type: 'string', enum: ['HIGH', 'MEDIUM', 'LOW'] },
          doc_location: { type: 'string', description: 'section + quoted phrase from the blueprint' },
          problem: { type: 'string' },
          correct_fact: { type: 'string', description: 'the right statement with a real file:line / doc-anchor' },
        },
      },
    },
    notes: { type: 'string', description: 'overall read; anything load-bearing you confirmed correct.' },
  },
}

phase('Verify')

const CHECKERS = [
  {
    key: 'anchors',
    q: 'ANCHOR VERIFICATION. Read the blueprint. Extract EVERY `file:line` and symbol citation in §1, §2 (the inventory table), and §3 (RS-1..RS-4). OPEN each cited file in ' + SRC + ' (or the maria304_driver / repo path) and confirm: (a) the symbol actually exists at/near that line; (b) the WIRED vs "boot/drain only" vs "STUB / 0-callers" tag is correct (a symbol tagged 0-callers must truly have no production caller; a symbol tagged WIRED must have one). Focus hardest on the load-bearing ones: IngressServer::serve mod.rs:22, dto.rs:241/360 + the :37-43 payload gap, ingress_inbox.rs:65 insert 0-callers, stage_acquire.rs:48 + INSERT @650, the 4 boot/drain stage callers (boot_phase.rs:2486/2507/2511, backlog_drain.rs:1192), app.rs:477/606/654/768 all 0-callers, bindings.rs:179, runtime.rs:64/94, node_state.rs:177/208 + the NO-Offline-setter claim, offline_session.rs:74, shifts.rs:67/119/237 + the edge wiring (only 5/6/13/14 reached via drain), main.rs:359-369 idle + :318 await_shutdown_signal, config/mod.rs:10 + listeners-unused, Cargo.toml:106-109 axum unused, the "no /health in Rust" claim. Report every mismatch.',
  },
  {
    key: 'vocab_order',
    q: 'VOCABULARY + ORDER VERIFICATION. Read the blueprint, then OPEN ' + MAP + ', ' + WL1 + ', and ' + AMAP + '. Verify: (a) the WL-0..WL-6 names + the 2 named pilot-blockers (WL-1, WL-3) + the Q1/Q3/Q5/Q6/Q-load decisions match the integration map EXACTLY (no invented worklets, no renumbering); (b) the claim that RS = WL-1 §0.4 Piece 0a "hoisted" is faithful to the WL-1 plan §0.1/§0.4 (piece tokens 0a/0b/1/2/3/4/5/6) — confirm the 0a description + the 0b..5 list in §4 match the plan; (c) the Hard-Blocker(1)/(2) references + the NO-GO verdict match ALGORITHMIC_MAP §1.11; (d) the §4 dependency order is SOUND — is anything sequenced before its true prerequisite, or could a listed item actually proceed in parallel? (e) does calling WL-0 "DONE" contradict the map (where WL-0 is a confirm-gate)? Report mismatches + any unsound ordering.',
  },
  {
    key: 'overclaim',
    q: 'OVER-CLAIM + COMPLETENESS + CONSISTENCY. Read the blueprint adversarially. Find: (a) any statement more confident than the evidence supports — especially the §6 "verify-before-build" items (RS-Q3 fn_sign in BindingsRegistry, RS-Q5 request_id/Protocol variant) must NOT be asserted as fact elsewhere in the doc; (b) any subsystem that EXISTS on HEAD but is MISSING from the §2 inventory (e.g. XML-RPC/WebCheck ingress, the secure pool, tax-snapshot, outgress/FSCO routing, admin CLI) — note if its omission misleads; (c) internal contradictions (a thing called WIRED in one place and 0-callers in another); (d) any invariant-guard claim in §5 that is wrong (e.g. does reconcile_mutex really serialize per-FN or globally? is the "copied verbatim from boot_phase.rs:2505-2549" branch claim accurate?); (e) anything that would mislead an implementer about scope (is RS really "just wiring", or does the payload-conversion layer / DpsChannel constructor / Offline setter make it substantially MORE than wiring?). Be specific with doc_location + correct_fact.',
  },
]

const results = await parallel(
  CHECKERS.map((c) => () =>
    agent(c.q + '\n' + RULES, { label: 'verify:' + c.key, phase: 'Verify', schema: SCHEMA }).then((x) => ({ key: c.key, ...x }))
  )
)

return results.filter(Boolean)
