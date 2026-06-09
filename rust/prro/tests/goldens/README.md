# M2/W4 byte-equivalence goldens

Frozen test fixtures consumed by `tests/goldens_byte_equiv.rs` (lands
in W4-C3) to prove the Rust canonical-XML builder in
`rust/prro/src/xml/` produces byte-identical output to the Python
serializer at `src/prro_gateway/serializers/dps_xml.py`.

## Oracle contract

Per ADR-M2-3 + W4-C0 contract:

- **Python serializer is the byte-oracle.**  Goldens in this directory
  are captured by running `regenerate.py` against a Python checkout;
  Rust does NOT freeze its own output as the golden.
- **Rust is the candidate.**  The C3 harness reads each `.bin` file
  here and asserts the Rust builder output (with the same hard-coded
  payload as `regenerate.py` ships) is byte-for-byte identical.
- **CI does NOT run `regenerate.py`.**  Re-capture is a deliberate
  spec-change action, not a regression-fix action.  Every re-capture
  goes through manual review of the new bytes + manifest diff.

## Wire-document mapping (W4-C0)

`WebCheck CloseShift == DPS Z_REPORT (typCheck=2, doctype=80)`.

The `xml/z_report.bin` fixture IS the close-shift wire artifact.  Do
NOT add a separate `xml/shift_close.bin` — that would obscure the
wire reality and create a divergence point between the typed Rust
payloads and the Python oracle.  Evidence:

- `docs/webcheck_reverse/WEBCHECK_ANALYSIS.md:77`
- `docs/webcheck_reverse/WebCheckMain/WebCheck/CreateDB.cs:624`
- `docs/webcheck_reverse/WebCheckMain/WebCheck/StringXML.cs:2509`
- DPS proto: `Check.Type::ZREPORT = 2` in
  `rust/prro/proto/fiscal_server.proto:24`

## Files

- `xml/shift_open.bin` — SHIFT_OPEN canonical XML (`<C T="108">` +
  `<O>` + `<E>`).
- `xml/sell.bin` — SELL canonical XML (`<C T="0">` + `<P>` +
  `<M>` + closing `<E FN N NO SM TS>`).
- `xml/return.bin` — RETURN (same body shape as SELL, `<C T="1">`).
- `xml/z_report.bin` — Z_REPORT a.k.a. close-shift (`<Z NO=...>` +
  per-payment-type `<M>` + `<NC NI NO>`).
- `cms/deterministic_prefix.bin` — frozen "XML-to-be-signed" bytes
  for the CMS-signed XML golden (per ADR-M2-3 split: pinned
  byte-equivalent, distinct from the signature-shape verify path).
- `prevhash/seed.bin` — first-after-bootstrap previous-hash seed
  (32-byte zero placeholder).
- `manifest.json` — sha256 + length per file; reviewer artefact for
  spotting unintended drift before opening individual `.bin` files.

## How to re-capture

Only when a deliberate spec change demands new bytes:

```bash
cd /path/to/PRRO_GATE
python3 rust/prro/tests/goldens/regenerate.py
git diff rust/prro/tests/goldens/      # review every byte change
git add rust/prro/tests/goldens/ && git commit -m "..."
```

If the diff is unintended, revert and investigate the source of drift
(`git checkout -- rust/prro/tests/goldens/` restores the previous
state; do NOT commit accidental TS/timestamp drift).

## W4 first-round subset

The fixtures cover a deliberately narrow subset of the Python
serializer's feature surface:

- per-item: only the six required attrs `C / N / NM / PRC / Q / SM`
  (no barcodes / excise marks / tax codes / discounts);
- per-payment: only the four required attrs `N / NM / SM / T`
  (no EPZ payment metadata / change / rounding);
- closing `<E>`: no tax-group `<TX>` children;
- header / footer text lines (`<L>`): omitted;
- check-level / per-item discounts (`<D>` / `<S>`): omitted;
- Z-report: only `<Z NO=...>` + per-payment `<M>` + `<NC>` (no
  `<TXS>` / `<IO>` / `<EPZ>`).

Tag/attr names already match the Python `_build_*` helpers, so
expanding the subset later (when M3 ingress wires real receipt
shapes) is purely additive in both Python and Rust.

## Determinism

`regenerate.py` passes an explicit `datetime` object as
`business_ts` (see the `BUSINESS_TS` constant at the top of the
script).  Passing a `str` would fall through Python's
`datetime.fromisoformat` and silently default to
`datetime.now(UTC)` — making every run produce different bytes.
The constant is set so that the Kyiv-local conversion produces TS
`"20260506120000"` on the wire.
