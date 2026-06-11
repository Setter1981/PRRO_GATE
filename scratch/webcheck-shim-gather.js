export const meta = {
  name: 'webcheck-shim-gather',
  description: 'Gather exact WebCheck ClassFiscal verb XML + gateway canonical contract + maria304 mapping template, to author the WebCheck COM-shim ingress spec',
  phases: [{ title: 'Gather', detail: '3 searchers: WebCheck non-receipt verbs / WebCheck FiscalReceipt XML / gateway canonical target' }],
}

const WC = '/mnt/d/PRRO_GATE/.claude/worktrees/agent-a624911a/docs/webcheck_reverse/WebCheckMain/WebCheck'
const DRV = '/mnt/d/PRRO_GATE/rust/maria304_driver'
const SRC = '/mnt/d/PRRO_GATE/rust/prro/src'

const GOAL = `
GOAL: author a spec for a WebCheck COM-shim — a thin Windows .NET COM component (ProgId WebCheck.ClassFiscal / AddIn.vk_WebCheck) that the pilot site's 1С already binds to, which parses the WebCheck verb XML and forwards a CANONICAL command to the gateway's HTTP ingress (POST /v1/ingress), then maps the gateway response back to WebCheck StatusBarXML. We need (a) the exact WebCheck verb/XML contract the shim must ANSWER, and (b) the exact canonical contract it must PRODUCE (same target maria304_driver already produces).
`

const RULES = GOAL + `
HARD RULES: OPEN the actual files, quote exact signatures/XML/JSON with file:line. Be precise about attribute names, element nesting, field types, and enum/command_type vocab — this is a wire contract spec. If a field is ambiguous, flag it.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'contract', 'mapping_notes', 'gaps'],
  properties: {
    area: { type: 'string' },
    contract: { type: 'array', items: { type: 'string' }, description: 'Each: file:line — exact verb/XML/JSON shape (attrs, elements, fields, types) — quote.' },
    mapping_notes: { type: 'array', items: { type: 'string' }, description: 'How a WebCheck verb/field maps to the canonical command (or vice-versa).' },
    gaps: { type: 'array', items: { type: 'string' } },
  },
}

phase('Gather')

const SEARCHERS = [
  {
    key: 'wc_verbs',
    q: 'Read the WebCheck client component at ' + WC + '/ClassFiscal.cs and ' + WC + '/StringXML.cs. Extract the EXACT input + output (StatusBarXML) contract for these verbs: SetSignSettings, Initialization, GetCurrentStatus, OpenShift, ReportZ, Finalization, OnlineToOffline. For each: method signature, the input XML attributes/elements parsed, and the response XML fields returned. Quote file:line + the XML literal. This is the non-receipt half of the contract a COM-shim must answer.',
  },
  {
    key: 'wc_receipt',
    q: 'Read ' + WC + '/ClassFiscal.cs + ' + WC + '/StringXML.cs for the FiscalReceipt verb in FULL detail. Extract: FiscalReceipt input XML — root attributes (FN, Number, OperationType, cashier/operator, department, return-check ref, idempotency/UID), the <Goods> element (Code, Name, Quantity, Price, Sum, TaxRate + ALL other attrs), <Payments> (Pay1..PayN / SMB / by-ID), <L> text lines (UP1-3/DN1-3), and any discount/excise/barcode fields. AND the StatusBarXML response (Err, CheckID, FN, ErrHelp, fiscal date/time, + others). Quote the exact XML + parsing logic with file:line. This is the receipt-mapping source (the meatiest verb).',
  },
  {
    key: 'gateway_canonical',
    q: 'Map the gateway canonical ingress contract a new front-end must PRODUCE. OPEN: ' + SRC + '/runtime/ingress/dto.rs (CanonicalCommand fields + to_canonical_fiscal_command + the payload shape), ' + SRC + '/services/write_path/types.rs (CanonicalFiscalCommand), the stage_sign payload structs CheckJson/ZReportJson/ShiftOpenJson (search ' + SRC + '/services/write_path/stage_sign.rs for parse_payload + these structs), ' + DRV + '/src/bridge/dto.rs (the wire CanonicalCommand the driver POSTs), and ' + DRV + '/src/session/dispatcher.rs (build_canonical — how maria304_driver assembles the canonical command = the TEMPLATE). State: the exact JSON the gateway HTTP ingress accepts (CanonicalCommand fields + types), the command_type / doc_type vocabulary (SELL/RETURN/SHIFT_OPEN/Z_REPORT/...), and the payload JSON shape per doc type (the stage_sign-ready CheckJson/ZReportJson/ShiftOpenJson fields). Cite file:line. This is the TARGET the WebCheck shim maps to.',
  },
]

const results = await parallel(
  SEARCHERS.map((s) => () =>
    agent(s.q + '\n' + RULES, { label: 'shim:' + s.key, phase: 'Gather', schema: SCHEMA }).then((x) => ({ key: s.key, ...x }))
  )
)

return results.filter(Boolean)
