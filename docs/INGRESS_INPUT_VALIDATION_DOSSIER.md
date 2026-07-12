# Ingress Input Validation Dossier — fail-closed, validate-for-BOTH-outgresses

**Status:** design locked pending operator ack on §3 merge-semantics.
**Author:** architect session (Opus 4.8), 2026-07-12.
**Track:** L5+ (ingress fail-closed input validation). Hot-zone-adjacent: `runtime/ingress/*`, `services/write_path/stage_sign` (breach surface).
**Proven by:** the shift-life matrix harness (`rust/prro/tests/shift_life_matrix.rs`).

---

## 0. Provenance (why now, in the operator's words)

The full-stack shift-life matrix harness (built 2026-07-12) drove GENERATED wire
packets through the REAL ingress and immediately surfaced that **malformed input
is caught too deep** — as an opaque `500 SIGN_INTERNAL` "internal write-path
breach", not a clean fail-closed `4xx` at the input boundary:

- **C1** — `SHIFT_OPEN` without `cashier_id` → `SHIFT_OPEN_MISSING_CASHIER`, **HTTP 500**.
- **C3** — `SELL` with an unconfigured `tax_group` (e.g. `0`) → **HTTP 500 `SIGN_INTERNAL`** "write-path breach" (slips past convert, panics-equivalent in `stage_sign`).

Operator directives (chronological, verbatim intent):
1. *"ловить такие ошибки нужно на входе валидированием … при каноникал джейсон … там же xml символы, группы оплаты, формат акцизной марки и уктзд"* — catch at input; validate XML-char admissibility, payment groups, excise-stamp format, UKTZED.
2. *"xsd для кабинета у нас нет … это xsd к другому кабинету"* — the `evpz_dps_protokol/check01.xsd` in the repo is **NOT** authoritative for our live cabinet (`cabinet.tax.gov.ua`); it is a **different** cabinet's spec.
3. *"у нас же еще второй outgress в планах"* — there are **two** egress backends.
4. *"валидироваться должно для обоих"* — the input must validate for **BOTH** outgresses.

**Two hard constraints fall out:**
- **No own-cabinet XSD.** We must NOT treat any repo XSD as the authority for outgress-1's acceptance rules.
- **Two outgresses** (`fn_outgress_profile`: default `FSCO_ZZD`, operator-choosable `EVPZ_DPS`). A receipt must be acceptable to **both**.

---

## 1. Problem statement

Today the ingress pipeline is:

```
raw bytes → serde CanonicalCommand → classify_command → to_canonical_fiscal_command (map+validate)
          → convert_to_signer_payload (wire→signer, per-FN pools) → inbox insert → fiscalize → stage_sign (build DPS XML)
```

Gaps (verified via the harness):
- **No format validation** of DPS-bound strings (UKTZED, excise, names) → a malformed value either breaches at `stage_sign` (500) or would build XML the cabinet later rejects.
- **No tax-group existence check** in `convert` → bad `tax_group` breaches at sign (C3).
- **Wrong status class**: input errors map to **5xx** (C1 missing-cashier → 500; C3 → 500). A client-input error rendered as 5xx is itself a defect — monitoring/clients read 5xx as "gateway fault, retry" instead of "your input, fix it".

**Persistence-model pin (must be honored):** *pre-acquire / invalid-ingress refusals are rejected BEFORE any row is minted → `audit_log` only, never `fiscal_documents`.* This validation layer is exactly that refusal class; building it **reinforces** the pin (a malformed receipt must never mint an inbox row nor reach sign).

---

## 2. Architecture — the layering (fail-closed, typed 4xx, pre-inbox)

Validation splits by **context**, not by outgress. Both layers fail-closed to a
typed `4xx` (422) BEFORE the inbox insert / BEFORE sign.

### Layer A — context-free (on `CanonicalCommand`, right after deser, no DB)
The operator's *"при каноникал джейсон"* — correct for everything that needs no per-FN config:
- **XML-char safety** — reject `'`, `"`, `<`, `>`, `&`, and control chars in DPS-bound string fields. Cabinet-INDEPENDENT (unescaped specials corrupt DPS XML attributes for ANY backend). WebCheck itself rejects at input (`FormAddPay.cs:352`: *"для апострофа/лапки використовуйте HTML-заміну"*). **Decision: reject fail-closed, do NOT silently escape** — matches WebCheck, avoids emit-time surprises, keeps the DPS XML contract clean.
- **UKTZED format** — must match the effective (intersection) pattern (§4).
- **Excise-stamp format** — must match the effective (intersection) pattern (§4).
- **Amounts / quantities** — non-negative where required; decimal-scale bounds (kopecks integer; quantity_milli integer).
- **payform code numeric sanity** — non-negative integer (both outgresses; XSD `PAYFORMCD` is `NonNegativeInteger`).

### Layer B — context-dependent (in `map`/`convert`, has per-FN pools)
Needs the per-FN config, so it lives where the pools already are:
- **tax_group EXISTS** in this FN's `tax_groups` (1..11 seeded by `bootstrap_fn_defaults`, WebCheck `Directorys.cs`) → **fixes C3** (currently breaches at sign).
- **payform EXISTS + iscash agrees** in `payment_methods` → already present (`MISSING_PAYMENT_METHOD` / iscash-disagreement); this is **normal, confirmed** (defaults `Готівка/Картка/Кредит/Сертифікат` = WebCheck `CreateDB.cs:459-474`; individual forms id 5+ per `SavePayForms`).
- **cashier present** where the command requires it (SHIFT_OPEN) → already detected (`SHIFT_OPEN_MISSING_CASHIER`); **fix its status class**.

### Status-code contract (fixes C1/C3)
**Every input-validation failure → 422** (typed `error_code` + `error_message`), routed through the existing `error_code → HTTP status` map (`handler.rs`, "operator-locked map"). **Never 5xx.** `SIGN_INTERNAL` (500) must become unreachable-by-bad-input — it may remain only for genuine crypto/backend faults, never for a malformed receipt.

---

## 3. "Validate for BOTH" = intersection (strictest of the two)

**Semantics (locked pending ack):** a receipt is **valid ⟺ it would be accepted by BOTH outgresses.** The input becomes **portable across backends** — safe under any routing / profile choice.

This **rehabilitates** the `evpz_dps_protokol` XSD: it is not junk — it is the **authority for outgress-2 (`EVPZ_DPS`)**, i.e. one half of the intersection.

| Outgress | Rule source |
|---|---|
| `FSCO_ZZD` (og-1, live-tested cabinet) | WebCheck reverse (`docs/webcheck_reverse_v2/`) + **live-probe `cabinet.tax.gov.ua`** |
| `EVPZ_DPS` (og-2) | **`schemas/xsd/... evpz_dps_protokol/check01.xsd`** (+ live when access exists) |
| (both) national standards | УКТЗЕД classifier, акцизна-марка format |

**Per-field merge rule:**
- **agree** → single rule.
- **differ (one stricter)** → **stricter wins** (fail-closed: reject anything EITHER would reject).
- **incompatible** (og-1 needs X, og-2 needs ¬X, no common value) → **escalate as a design-blocker** (§10). Do NOT silently pick one. This means a single canonical input can't serve both, which is an architecture decision (e.g. per-outgress output normalization), not a validation default.

> The dossier deliberately keeps validation in the **shared** canonical+convert
> layer (rule-set = intersection), NOT a per-profile split. Simpler, and it makes
> a stored/queued receipt safe regardless of which backend drains it.

---

## 4. Rule table (per-field, source-tagged, confidence-tagged)

`eff` = effective rule (the min/strictest). `conf` = verified | webcheck | xsd(og-2) | national | **live-needed**.

| # | Field | Layer | og-1 rule (FSCO_ZZD) | og-2 rule (EVPZ_DPS / XSD) | Effective (intersection) | conf |
|---|---|---|---|---|---|---|
| V1 | XML-unsafe chars in DPS strings | A | reject (WebCheck) | reject (XML well-formedness) | **reject `'"<>&`+ctrl** | verified |
| V2 | UKTZED format | A | live-needed | `([0-9]){10\|8\|6\|4}\|00[0-9]{3}` (DGUKTZED) | **= og-2 pattern, live-confirm og-1** | xsd+live-needed |
| V3 | Excise-stamp format | A | live-needed | 2 cyrillic + 6 digits / digit-series (XSD lines 7/19/39/49) | **conservative pattern, live-confirm** | xsd+live-needed |
| V4 | payform code numeric | A | non-neg int | `PAYFORMCD` NonNegativeInteger | **non-neg int** | verified |
| V5 | payform EXISTS (+iscash) | B | `payment_methods` per-FN | same | **exists in `payment_methods`** | verified (already) |
| V6 | tax_group EXISTS | B | `tax_groups` per-FN | same | **exists in `tax_groups`** → fixes C3 | verified |
| V7 | cashier present (required cmds) | B | required (SHIFT_OPEN) | required | **required** → fix status | verified (already) |
| V8 | amounts/qty non-neg + scale | A | integer kopecks / milli | Decimal2/Decimal3 scale | **non-neg + scale bound** | verified |
| — | status class of ALL above | A+B | — | — | **422, never 5xx** → fixes C1/C3 | verified |

**Confidence discipline:** V1/V4/V5/V6/V7/V8 have a solid, cabinet-independent or
own-config source → **build now, high confidence.** V2/V3 exact boundaries for
og-1 are **not** known (no own-cabinet XSD) → start from og-2's XSD pattern as a
conservative bound, mark **`live-probe pending` (known-red)**, tighten only after
a live-probe fact. **Never reject an input we are not sure is invalid** (a false
reject of valid business data is worse than a deferred tightening).

---

## 5. Scope / non-goals

**In:** V1–V8 + the 422 status fix. A small `ingress::validate` module (Layer A) +
targeted `convert` additions (Layer B: V6 tax-group existence) + the error_code
map entries.

**Out:**
- Business-rule guards already shipped in **L5** (50k cash-cap, zero-line, zero-payment, underpayment) — unchanged, complementary.
- DPS *semantic* acceptance (what the cabinet does with a well-formed receipt) — that is a live-campaign concern, not input validation.
- Per-outgress OUTPUT normalization — only relevant if §3 surfaces an incompatible field (design-blocker path), out of this increment.
- No schema/DDL churn — rules are code constants + existing config lookups (`tax_groups`, `payment_methods`). **No migration.**

---

## 6. Implementer contract (strict RED-first TDD)

**Base:** branch from `origin/main`. Method: RED-first, minimal diff. Local until architect verifies + 2-lens review; batch push on operator command.

### V0 — RED pins (write, watch fail FIRST)
For EACH of V1–V8, a pin that a malformed wire `CanonicalCommand`:
1. returns the **specific typed `error_code`** (e.g. `INVALID_UKTZED`, `INVALID_EXCISE`, `XML_UNSAFE_CHAR`, `UNKNOWN_TAX_GROUP`, `MISSING_PAYMENT_METHOD` (exists), `SHIFT_OPEN_MISSING_CASHIER` (exists)),
2. maps to **HTTP 422** (NOT 5xx),
3. is **row-less** — asserts **0 new `ingress_inbox` rows, 0 new `fiscal_documents` rows**, audit-only (the persistence pin).
Plus a **C1/C3 regression pin**: the exact harness inputs that produced 500 now produce 422 + row-less.

### V1 — implement (minimal diff)
- New `runtime/ingress/validate.rs` (Layer A): pure `fn validate_canonical(cmd) -> Result<(), ValidationError>`; called in `handle_command` **Step 0.5** (after classify, before map/convert/inbox). Char-safety + UKTZED + excise + amount/qty + payform-numeric.
- `convert` (Layer B): add **V6 tax-group existence** lookup (mirror the existing `payment_methods` lookup) → typed `UNKNOWN_TAX_GROUP`, not a sign breach.
- Error map: add new codes → **422**; re-point `SHIFT_OPEN_MISSING_CASHIER` (and any input-class code currently 5xx) → **422**.

### V2 — fuzzer (MANDATORY — see §8)
Adversarial-input ops in the invariant fuzzer + the shift-life matrix harness: bad UKTZED / bad excise / XML-unsafe / unknown tax_group → **clean refusal, row-less, no breach, `assert_clean` holds**.

### Teeth (prove empirically)
Revert each guard → its pin goes RED (bad input reaches sign / mints a row / returns 5xx). Include one canary per guard family.

### Verification gate (run yourself, final commit, actual numbers)
`cargo nextest run -p prro --features test-support` (0 failed) + `cargo fmt --check` + `cargo clippy --all-targets --features test-support -D warnings` (0) + the matrix harness C1/C3 scenarios flip 500→422.

---

## 7. Invariants preserved (state each)
- **Persistence pin** — invalid-ingress → `audit_log` only, never `fiscal_documents`. **Reinforced** (rejection moves EARLIER, before inbox).
- **Idempotency / advance-at-SEND / D2 / z-quiescence** — untouched (validation is pre-write, mutates nothing).
- **No network/crypto in txn** — Layer A is pure; Layer B lookups are short reads, no network.
- **Fail-closed** — every ambiguous input rejected, never silently coerced (except the explicit "don't reject the unsure" rule for V2/V3 boundaries, which is itself fail-safe: don't over-reject).

---

## 8. Fuzzer-impact (mandatory per operator rule 2026-07-10)

This is a **new refusal class** → the fuzzer's alphabet must gain adversarial-input
ops. Add `Op::MalformedInput{kind}` (bad UKTZED / bad excise / XML-unsafe / unknown
tax_group / unknown payform), with the model predicting a **clean refusal**
(no lnd, no seed advance, no `fiscal_documents` row, no `ingress_inbox` row) and
the oracle asserting **row-less audit-only + no state mutation + `assert_clean`**.
Teeth: revert a guard → a seeded harness with the adversarial op goes RED. Track
the residual (og-1 UKTZED/excise exact boundary) as a **known-red** until the
live-probe fact lands. Map coverage note: `docs/FUZZER_TIER2_RAGE_DOSSIER.md`.

---

## 9. Confidence & known-reds (live-probe pending)
- **V2 UKTZED** / **V3 excise** exact boundaries for **og-1 (FSCO_ZZD / cabinet.tax.gov.ua)** are UNKNOWN (no own-cabinet XSD). Ship a conservative pattern (og-2 XSD as the bound); mark `live-probe pending`; tighten from live facts only.
- The **DPS-error corpus + live-probe** harness slices (next in the matrix queue) are the fact source that closes these known-reds — turning V2/V3 boundaries from assumptions into cabinet facts.

---

## 10. Open questions / design-blockers (operator decision)
1. **§3 merge-semantics ack** — confirm "validate for both" = **intersection / strictest-of-both** (this dossier's assumption), vs per-profile validation. If per-profile, Layer C splits per outgress and input is NOT portable.
2. **Incompatible-field escalation** — if a field's og-1 and og-2 rules turn out mutually exclusive (no common value), the resolution (per-outgress output normalization vs reject-both) is an architecture call, not a validation default. None known yet; surfaced here so it is not silently defaulted.
3. **XML-char policy** — locked to **reject** (WebCheck model). Confirm we do NOT want silent HTML/entity escaping of business names instead.
```
