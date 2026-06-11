export const meta = {
  name: 'webcheck-ingress',
  description: 'O1: what ingress does WebCheck expose (1С External Component vs terminal protocol), how does 1С actually feed the gateway, and does the built rust/maria304_driver match the pilot operator path?',
  phases: [{ title: 'Ingress', detail: '4 searchers: WebCheck 1С-component / WebCheck terminal / 1С integration docs / maria304-driver reconcile' }],
}

const WC = '/mnt/d/PRRO_GATE/.claude/worktrees/agent-a624911a/docs/webcheck_reverse'
const DOCS = '/mnt/d/PRRO_GATE/docs'
const DRV = '/mnt/d/PRRO_GATE/rust/maria304_driver'

const GOAL = `
GOAL (O1 — pilot ingress protocol): the Rust gateway built ONE ingress = maria304 (rust/maria304_driver, a Maria-304 TCP/wire front-end that HTTP-POSTs canonical commands). The operator's actual fleet is WebCheck + 1С. We must determine what the pilot operator's POS/1С actually speaks, so RS-2 finishes the RIGHT ingress. Decisive question: does 1С feed the gateway via (A) Maria-304 emulation (1С → Maria-304 fiscal-printer protocol → maria304_driver, matching the built ingress), or (B) a 1С External Component / OLE-COM surface (a separate ingress the gateway does NOT have), or (C) both/other?
`

const RULES = GOAL + `
HARD RULES: OPEN the actual files, quote exact lines/signatures/section text with file:line. Distinguish what is DOCUMENTED (the integration docs) from what the decompiled code DOES. Be decisive about whether maria304 is the correct pilot ingress or a mismatch.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'summary', 'ingress_surface', 'o1_answer', 'gaps'],
  properties: {
    area: { type: 'string' },
    summary: { type: 'string', description: 'Dense answer. Facts only.' },
    ingress_surface: { type: 'array', items: { type: 'string' }, description: 'Each: file:line — what ingress mechanism / method / protocol — quote.' },
    o1_answer: { type: 'string', description: 'For your angle: is the pilot ingress Maria-304 (matches maria304_driver) / 1С-External-Component-or-OLE (mismatch) / other? With evidence.' },
    gaps: { type: 'array', items: { type: 'string' } },
  },
}

phase('Ingress')

const SEARCHERS = [
  {
    key: 'wc_1c_component',
    q: 'Read the WebCheck 1С External Component: ' + WC + '/WebCheckMain/WebCheck/vk_WebCheck.cs and the interface files in that dir (ILanguageExtender.cs, IInitDone.cs, IStatusLine.cs, IErrorLog.cs, IAsyncEvent.cs). Confirm whether vk_WebCheck is a 1С:Enterprise External Component (ВнешняяКомпонента / зовнішня компонента) — ILanguageExtender is the classic 1С native-component interface (CallAsProc/CallAsFunc/GetNProps/FindProp/RegisterExtensionAs). LIST the methods/properties 1С can call on it (the ingress verb surface — open shift / register receipt / report / etc.) + the parameter shape 1С passes. This is the native 1С→WebCheck ingress contract. Cite file:line + quote the method dispatch table.',
  },
  {
    key: 'wc_terminal',
    q: 'Read ' + WC + '/WebCheckMain/WebCheck/FormTerminal.cs (the only WebCheck file with network/serial markers). Determine WebCheck terminal/POS ingress: is it a TCP listener, a serial/COM port, named pipe? What WIRE PROTOCOL does it speak (Maria-304 fiscal-printer? a serial ECR/РРО protocol? custom TCP/JSON)? What commands + data shape does it accept from an external POS/terminal? Quote the listener/port setup + the command parser with file:line. State plainly: is this Maria-304 emulation, a serial ECR protocol, or something else?',
  },
  {
    key: 'integration_docs',
    q: 'Read these integration docs IN FULL: ' + DOCS + '/OLE_METHODS_USED_BY_1C.md and ' + DOCS + '/maria304/1C_SETUP.md (and anything else under ' + DOCS + '/maria304/). Answer precisely: HOW does the operator 1С feed the fiscal gateway? (A) by driving an emulated Maria-304 fiscal printer (1С → Maria-304 protocol → the maria304 driver), (B) via OLE/COM automation methods on an external component, or (C) another path? List the exact OLE methods 1С calls (from OLE_METHODS_USED_BY_1C.md) AND whether 1C_SETUP.md configures 1С to emit Maria-304 to a TCP port. The crux: do these docs say 1С talks Maria-304 (matching rust/maria304_driver) or a separate OLE surface? Quote section/line.',
  },
  {
    key: 'driver_reconcile',
    q: 'Read the Maria-304 protocol the built driver accepts: ' + DRV + '/src/protocol/ (commands/opcodes), ' + DRV + '/src/listener/server.rs, ' + DRV + '/src/session/dispatcher.rs, ' + DRV + '/src/Cargo.toml. Then compare against what ' + DOCS + '/maria304/1C_SETUP.md says 1С emits and (if relevant) WebCheck FormTerminal. DECISIVE for O1: is rust/maria304_driver the SAME protocol the operator 1С/POS would speak — i.e. is maria304 the correct pilot ingress for a WebCheck+1С operator — or a different/separate protocol? Is maria304 the 1С path, a POS path, or a mismatch that means the pilot needs a different (OLE/WebCheck-XMLRPC) ingress the gateway lacks? Cite file:line.',
  },
]

const results = await parallel(
  SEARCHERS.map((s) => () =>
    agent(s.q + '\n' + RULES, { label: 'ing:' + s.key, phase: 'Ingress', schema: SCHEMA }).then((x) => ({ key: s.key, ...x }))
  )
)

return results.filter(Boolean)
