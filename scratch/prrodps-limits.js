export const meta = {
  name: 'prrodps-limits',
  description: 'How the PRRODPS C# product enforces offline 36h/168h + shift max-duration limits + where it configures the values (2nd reference vs WebCheck)',
  phases: [{ title: 'Study', detail: '3 searchers over PRRODPS C# source: offline timers / shift close / config surface' }],
}

const P = '/mnt/d/prrodps_src'

const GOAL = `
GOAL: 2nd reference product (PRRODPS, C#/.NET WPF, talks DFS=ДФС + Maria ingress) for the SAME question as WebCheck: how does it ENFORCE (1) 36h continuous offline, (2) 168h cumulative monthly offline, (3) shift max-duration (operator practice = close before 23:59:59 of opening day; legal = 24h continuous) — and WHERE are the threshold VALUES configured? We are mirroring this into the Rust gateway with CONFIGURABLE values, so the exact behavior + config surface matters.
`

const RULES = GOAL + `
HARD RULES: OPEN the actual .cs files (NOT the bin/ DLLs), quote exact lines/constants with file:line. Surface every configurable value + default + storage location (.config, ConstStrings, DB, settings). If a limit is hard-coded vs configurable, say which. If something is absent, say so.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'rules', 'config_values', 'vs_protocol', 'gaps'],
  properties: {
    area: { type: 'string' },
    rules: { type: 'array', items: { type: 'string' }, description: 'Each an exact behavioral rule with file:line + quote.' },
    config_values: { type: 'array', items: { type: 'string' }, description: 'Each: name — default — where configured (file:line) — hard-coded|configurable — what it gates.' },
    vs_protocol: { type: 'array', items: { type: 'string' }, description: 'Where PRRODPS agrees/differs from the ФСКО protocol timer rules (36h reset, 168h monthly zero at 1st, check-168h-before-offline, blocked transitions).' },
    gaps: { type: 'array', items: { type: 'string' } },
  },
}

phase('Study')

const SEARCHERS = [
  {
    key: 'offline_timers',
    q: 'In PRRODPS C# source, find how the 36h + 168h offline timers are tracked, reset, and drive the offline/blocked transition. OPEN: ' + P + '/PRRODPS.Core.DataAccess/RroEngine.cs, ' + P + '/PRRODPS.Core.Models/OFFLINE_SESSION_INFO.cs, ' + P + '/PRRODPS.DFS/DFSReturnToOnline.cs, ' + P + '/PRRODPS.DFS/CmdCashRegisterStateReq.cs + CmdCashRegisterStateResp.cs, ' + P + '/PRRODPS/CurrentRegister.cs + MainData.cs. Find: when 36h timer starts/zeroes (offline entry, online return); how 168h accumulates + monthly zero at 1st-of-month; the ORDER (check 168h before going offline → Blocked if maxed); the Blocked-mode transition + the -11/error codes; the connectivity-probe interval. Quote constants + logic with file:line.',
  },
  {
    key: 'shift_close',
    q: 'In PRRODPS C# source, find the SHIFT max-duration / end-of-day close handling. OPEN: ' + P + '/PRRODPS.Core.DataAccess/ShiftEngine.cs, ' + P + '/PRRODPS.Core.DataAccess/ReceiptEngine.cs, ' + P + '/PRRODPS.Core.Models/Report.cs. The operator says: practice = close before 23:59:59 of the opening day; legal-theoretical = 24h continuous. Does PRRODPS enforce a shift-duration cap (24h timer? midnight/end-of-day auto-close or warning? a 23:59:59 cutoff? force Z?)? How does it handle Z-report close while OFFLINE (so the shift can close without DFS)? Quote exact logic + any configurable cutoff with file:line. If PRRODPS does NOT cap shift duration, say so explicitly.',
  },
  {
    key: 'config_surface',
    q: 'In PRRODPS, find WHERE all the offline/shift limit VALUES + parameters are configured. OPEN: ' + P + '/PRRODPS.Core.Utils/ConstStrings.cs, any *.config files (search ' + P + ' for App.config/appsettings), ' + P + '/PRRODPS.Config/ dir, ' + P + '/PRRODPS.Core.DataAccess.Setup/DBSetup.cs, settings/preferences classes. List EVERY limit-related value (36h, 168h, probe interval, shift cap, offline code watermarks, cert-expiry gate) with: name, default, file:line, and whether it is hard-coded or operator-configurable. This drives the Rust configurable-values design.',
  },
]

const results = await parallel(
  SEARCHERS.map((s) => () =>
    agent(s.q + '\n' + RULES, { label: 'dps:' + s.key, phase: 'Study', schema: SCHEMA }).then((x) => ({ key: s.key, ...x }))
  )
)

return results.filter(Boolean)
