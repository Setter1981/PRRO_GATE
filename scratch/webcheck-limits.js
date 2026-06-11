export const meta = {
  name: 'webcheck-limits',
  description: 'Mirror WebCheck/protocol offline-limit + shift-close enforcement (36h/168h/24h) with configurable values; scope the Rust enforce build',
  phases: [{ title: 'Study', detail: '4 searchers: protocol semantics / WebCheck impl+config / shift-close / current Rust state' }],
}

const WT = '/mnt/d/PRRO_GATE/.claude/worktrees/agent-a624911a/docs'
const PROTO = WT + '/dps_protocol'
const WC = WT + '/webcheck_reverse'
const RUST = '/mnt/d/PRRO_GATE/rust/prro/src'

const GOAL = `
GOAL: the operator wants the Rust gateway to ENFORCE all three offline/shift limits, mirroring WebCheck, with CONFIGURABLE values:
(1) 36h continuous offline, (2) 168h cumulative monthly offline, (3) shift max-duration (operator practice = close before 23:59:59 of the opening day; legal-theoretical = 24h continuous — BOTH should be configurable thresholds). We need the EXACT WebCheck/protocol behavior + the config surface to mirror it.
`

const RULES = GOAL + `
HARD RULES: OPEN the actual files, quote exact lines/constants with file:line. Distinguish the AUTHORITATIVE protocol rule from WebCheck's implementation choice. Surface every configurable value + its default + where it lives. If something is NOT in the source, say so.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'rules', 'config_values', 'rust_gap', 'gaps'],
  properties: {
    area: { type: 'string' },
    rules: { type: 'array', items: { type: 'string' }, description: 'Each an exact behavioral rule with file:line + quote.' },
    config_values: { type: 'array', items: { type: 'string' }, description: 'Each: name — default — where configured (file:line) — what it gates.' },
    rust_gap: { type: 'array', items: { type: 'string' }, description: 'For the Rust enforce build: what exists vs must be built, with file:line.' },
    gaps: { type: 'array', items: { type: 'string' } },
  },
}

phase('Study')

const SEARCHERS = [
  {
    key: 'protocol_semantics',
    q: 'Read the official ФСКО protocol offline/blocked state-machine at ' + PROTO + '/251051_(1).md (esp. the section around lines 42-55) and any sibling files in ' + PROTO + '. Extract the AUTHORITATIVE rules with line cites: the "36h" timer (when it starts/zeroes/pauses), the "168h" timer (accumulate across sessions, monthly zero at midnight of the 1st, pause-not-zero on online return), the ORDER of checks (check 168h BEFORE entering offline → go Blocked if already maxed), online→Blocked path (connection drop + 168h maxed), Blocked→offline only at month rollover, the user-notification + shift-close advisory before Blocked, and the -11 (ERROR_OFFLINE_168) / blocked error codes. State the exact rules a Rust enforcer must mirror.',
  },
  {
    key: 'webcheck_impl_config',
    q: 'Read the WebCheck decompiled .NET in ' + WC + ' (start with WebCheckExe/WebCheck/All.cs, WebCheckMain/WebCheck/vk_WebCheck.cs, and the settings/offline forms: FormSettings, FormOfflineStatus). Find HOW WebCheck implements the 36h + 168h timers and — critically — WHERE the threshold VALUES are configured (registry keys, settings, constants, .config). The operator said "values must be configurable" — surface every configurable limit value + its default + storage location. Also find the 36h cert-expiry gate (~2160 minutes / DateInterval=Minute) at SHIFT_OPEN. Quote constants with file:line.',
  },
  {
    key: 'shift_close',
    q: 'In the WebCheck decompiled source (' + WC + ') and the protocol doc (' + PROTO + '), find how the SHIFT max-duration / end-of-day close is handled. The operator says: practice = close before 23:59:59 of the opening day; legal-theoretical = 24h continuous. Does WebCheck enforce a shift-duration limit (a 24h timer? a midnight/end-of-day auto-close or warning? a 23:59:59 cutoff?)? How does it force/advise closing? How does the offline Z_REPORT close-of-day work when offline (so the shift can close without DPS)? Quote the exact logic + any configurable cutoff with file:line. If WebCheck does NOT enforce a shift-duration cap, say so explicitly.',
  },
  {
    key: 'rust_current_state',
    q: 'In the current Rust gateway (' + RUST + '), inventory what already exists for offline/shift limit enforcement vs what must be built. Check: (a) node_state fields for offline timing (offline_session_started_at, current_month_offline_seconds, or equivalents — search db/repositories/node_state.rs + migrations); (b) any threshold checks / limit constants (grep 36, 168, MAX_OFFLINE, 86400, offline_session, shift duration); (c) the BLOCKED setter (set_mode_blocked_tx node_state.rs:177) + STOP_MODE + how mode transitions happen; (d) the offline session start/timing (services/offline_session.rs); (e) the config struct (config/mod.rs) — where limit values would be added. State precisely: which of the three limits has ANY scaffolding vs is greenfield, with file:line. Note W10 (offline Z local close) status.',
  },
]

const results = await parallel(
  SEARCHERS.map((s) => () =>
    agent(s.q + '\n' + RULES, { label: 'wc:' + s.key, phase: 'Study', schema: SCHEMA }).then((x) => ({ key: s.key, ...x }))
  )
)

return results.filter(Boolean)
