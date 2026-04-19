# prro_sidecar ops

## Ship checklist

1. Run `prro_license_keygen` once to generate the master DSTU keypair.
2. Copy the produced `license_pubkey_current.der` into `src/license_pubkey_current.der`.
3. Copy same file into `src/license_pubkey_next.der` (identical at first deploy — swapped during key rotation).
4. Rebuild: `cargo build --release`.
5. Download the DPS TLS chain: see `prro-tax-gov-ua-chain.pem.placeholder`.
6. Copy `sidecar.example.toml` → `sidecar.toml`, fill in DB path and license path.

## Key rotation

When rotating the signing key:
1. Generate new keypair → produces `license_pubkey_new.der`.
2. Copy old `license_pubkey_current.der` → `license_pubkey_next.der`.
3. Copy `license_pubkey_new.der` → `license_pubkey_current.der`.
4. Rebuild and deploy.
5. Sign new licenses with the new key. Old licenses verified by `license_pubkey_next.der` continue to work.
6. After all customers have new licenses, remove the old key from `license_pubkey_next.der`.
