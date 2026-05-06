# M2 byte-equivalence goldens — operator capture procedure

This document is the operator-facing procedure for re-capturing the
M2/W4 canonical-XML byte-equivalence goldens at
`rust/prro/tests/goldens/`.  It complements the developer-facing
README at `rust/prro/tests/goldens/README.md`.

## When to re-capture

You should re-capture goldens **only** when one of the following is
deliberately true:

1. The Python serializer's canonical-XML output for one of the
   four W4 doc types (SHIFT_OPEN, SELL, RETURN, Z_REPORT) has
   intentionally changed because of a spec amendment, a bugfix that
   changes the wire shape, or a new ФСКО protocol revision.
2. The W4 first-round subset has expanded — e.g. the Rust builder
   gained per-item discount support and we now want a golden that
   exercises that expansion.
3. The fixture payloads themselves were redesigned (different
   FN / TN / TS / item set) for some operational reason.

You should **NOT** re-capture goldens to "fix a CI failure".  A CI
failure means the Rust builder drifted from the Python oracle —
investigate the Rust code, do not silently update the goldens.

## Pre-requisites

- Working Python checkout with the `prro_gateway` package importable
  (i.e. `src/prro_gateway/serializers/dps_xml.py` exists and works).
- Python 3.11+ (for `zoneinfo` / `datetime` UTC + Kyiv conversions).
- `cp1251` codec available in the Python install (it is part of
  the standard library; no third-party dep needed).

## Procedure

```bash
cd /path/to/PRRO_GATE

# 1. Sanity-check the Python checkout still imports cleanly.
python3 -c "from prro_gateway.serializers.dps_xml import build_dps_xml" \
  || { echo 'Python serializer import failed — fix before continuing'; exit 1; }

# 2. Run the manual capture script.
python3 rust/prro/tests/goldens/regenerate.py

# 3. Review the diff before committing.  Every byte change
#    MUST have a reason recorded in the commit message.
git diff rust/prro/tests/goldens/

# 4a. If the diff is intended:
git add rust/prro/tests/goldens/
git commit -m "test(rust/goldens): refresh W4 fixtures (<reason>)"

# 4b. If the diff is unintended:
git checkout -- rust/prro/tests/goldens/
# Investigate why regenerate.py produced different bytes (Python
# package update? upstream serializer change? clock-related
# determinism leak?).  Do NOT commit accidental drift.
```

## Reviewer checklist

When reviewing a commit that re-captures goldens:

- [ ] The commit message names the reason for the re-capture (spec
      change / subset expansion / payload redesign).
- [ ] `manifest.json` sha256 + length values change consistently
      with the `.bin` byte changes.
- [ ] Every `.bin` whose sha256 changed has its byte change
      explained in the commit message OR is obviously aligned
      with the named reason.
- [ ] No silent fixture drift (e.g. no `xml/sell.bin` byte change
      when the named reason is "Z-report-only spec amendment").
- [ ] The Rust builder in `rust/prro/src/xml/` was updated in a
      previous commit to produce the new shape — re-capturing
      goldens BEFORE the Rust producer changes is invalid (Rust
      will then fail CI against the new goldens).

## Determinism gotchas

- **`business_ts` MUST be a `datetime` object**, not a string.  See
  the `BUSINESS_TS` constant at the top of `regenerate.py`.
- **The Kyiv-local conversion is timezone-aware.**  In the summer
  half of the year `Europe/Kyiv` is UTC+3; in winter it's UTC+2.
  The frozen `BUSINESS_TS = datetime(2026, 5, 6, 9, 0, 0, tzinfo=UTC)`
  lands at 12:00 Kyiv-local in the summer half (EEST).  If the
  fixture date is moved to winter, expected wire TS shifts by an
  hour; the constant should be re-anchored explicitly.
- **`previous_hash` is `"deadbeef"`** in the fixtures.  Real
  production payloads carry a 64-char hex; we use a short literal
  here to make accidental copy-paste from a production trace
  obvious.

## What does this NOT cover

- KVT1 / KVT2 parser input → struct goldens.  Deferred from W4
  first round; W1's `prro::crypto` wrapper does not include a
  KVT parser.  Tracked as a follow-up against the M2 epic.
- Live DPS round-trip tests.  Those live in W3's
  `dps_channel_smoke.rs` via the native tonic mock; they do not
  share fixtures with W4 because W3 covers the gRPC wire shape
  (proto messages) while W4 covers the canonical-XML wire shape.
- Signature-shape goldens.  Per ADR-M2-3, CMS goldens are split
  into "deterministic prefix" (pinned byte-equivalent — the
  `cms/deterministic_prefix.bin` fixture in this directory) and
  "signature shape" (parsed + verified, NOT byte-compared — that
  path lives in W1's `crypto_provider_smoke.rs`).
