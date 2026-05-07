# `prro` test fixtures

Static fixture files used by `cargo test -p prro` and by
`cargo clippy -p prro --all-targets` (which compiles the same `tests/`
and `#[cfg(test)]` modules under `src/`).

These files are vendored into the repository so that CI can compile and
run `prro` tests without any out-of-tree dependency (no `npm install`,
no `node_modules`, no SSH-authenticated git fetches).

## Files

### `SELF_SIGNED_ENC_6929.cer`

DER-encoded self-signed DSTU-4145 EC certificate.  525 bytes.

- **Source**: `jkurwa` upstream test corpus
  ([npm package `jkurwa`](https://www.npmjs.com/package/jkurwa),
  path `test/data/SELF_SIGNED_ENC_6929.cer`).
- **License**: BSD (per `jkurwa/package.json` from the upstream
  package).
- **Status**: public test certificate — **not a secret**.  It is a
  self-signed test cert generated to exercise DSTU-4145 envelope and
  parse paths; it is not bound to any real CA, RA, or operator
  identity.

### Subjects/issuers / validity

`openssl x509 -inform DER -text -noout` on this fixture prints
(values reproduced here for quick reference; if they ever drift,
re-run the openssl command):

- Subject == Issuer: `O=Very Much CA, serialNumber=UA-99999991, L=Wakanda`
- Validity: `Jul 14 02:40:00 2017 GMT` → `Nov 14 22:13:20 2023 GMT`

The fixture is intentionally **expired**.  Tests that depend on
expiry semantics build their `ParsedCertMetadata` with adjusted
`not_before` / `not_after` rather than relying on the fixture's
calendar dates.

## Used by

- `tests/cert_refresher_smoke.rs` — `FIXTURE_CERT_DER`.
- `tests/cert_refresher_branches.rs` — `FIXTURE_CERT_DER`.
- `src/services/cert_refresher.rs` (under `#[cfg(test)] mod`) —
  `FIXTURE_CERT_DER` referenced via `concat!(env!("CARGO_MANIFEST_DIR"),
  "/tests/fixtures/SELF_SIGNED_ENC_6929.cer")`.

If you add a new compile-time `include_bytes!` that points at this
file, do it relative to this directory (`tests/fixtures/`) — never via
`node_modules/`.

## Adding new fixtures

Adding a new file here is fine when the data is:

1. **Public** (BSD-licensed jkurwa corpus, RFC examples, etc.) — never
   commit anything that could leak operator keys, real CA private
   material, or production data.
2. **Static** — fixtures that change infrequently and benefit from
   compile-time `include_bytes!` review through `git diff`.
3. **Small** — single-digit KB, bounded.  Do not vendor large blobs.

Update this README with the new file's source, license, and intended
test consumers.
