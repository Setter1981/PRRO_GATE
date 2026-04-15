# Plan: prro_crypto — Rust Crypto Module

**Дата:** 2026-04-14
**Статус:** Planned (post-pilot)

---

## Мета

Замінити Node.js crypto sidecar (jkurwa) на нативний Rust модуль з PyO3 bindings. Один процес замість двох, cross-platform.

---

## Поточна архітектура

```
Python gateway → HTTP localhost:8091 → Node.js sidecar (jkurwa + gost89) → CMS/PKCS#7 DER bytes
```

Проблеми:
- Два процеси — ops overhead, sidecar = SPOF
- HTTP latency на кожен підпис
- Node.js залежність
- Не працює на Android без Termux

---

## Цільова архітектура

```
Python gateway → import prro_crypto → CMS/PKCS#7 DER bytes (in-process)
Android app → JNI → libprro_crypto.so → CMS/PKCS#7 DER bytes
```

---

## Scope crate: prro_crypto

| Модуль | Опис | ~Рядків | Reference |
|--------|------|---------|-----------|
| dstu4145 | EC point math, DSTU 4145-2002 curve 6 (параметри з ДСТУ) | ~500 | jkurwa/lib/curve.js |
| gost89 | GOST 28147-89 block cipher (CFB mode для JKS) | ~300 | jkurwa/lib/gost89.js |
| jks | JKS keystore reader (feedfeed format, SHA1 XOR keystream) | ~200 | jkurwa/lib/jks.js |
| cms | CMS/PKCS#7 SignedData DER builder (attached signature) | ~400 | jkurwa/lib/cms.js |
| pyo3_bindings | Python module: sign_raw(payload_bytes, key_bytes) -> signed_bytes | ~100 | — |
| **Разом** | | **~1500** | |

---

## Target platforms

| Target | Артефакт | Use case |
|--------|----------|----------|
| x86_64-unknown-linux-gnu | .so (PyO3 wheel) | Production Linux servers, mini-PC |
| aarch64-unknown-linux-gnu | .so (PyO3 wheel) | Raspberry Pi, ARM edge devices |
| x86_64-pc-windows-msvc | .pyd (PyO3 wheel) | Windows POS terminals |
| aarch64-linux-android | .so (JNI) | Android POS tablets |
| aarch64-apple-darwin | .so (PyO3 wheel) | macOS dev machines |

Build tool: **maturin** (Rust → Python wheel, single command per platform).

---

## Формати ключів

| Формат | Пріоритет | Поширеність | Reference |
|--------|-----------|-------------|-----------|
| **JKS** (feedfeed, SHA1 XOR) | P0 — пілот | ~15% | jkurwa/lib/jks.js |
| **Key-6.dat** (IIT proprietary) | P0 — масштабування | ~40% | jkurwa/lib/key6.js |
| **ZS2** (АЦСК Україна / M.E.Doc) | Поза scope — закритий формат | ~30% (конвертують в Key-6.dat) | Немає публічної специфікації |
| **PKCS#12** (.p12/.pfx) | P0 — ДПС видає печатки в .pfx | активно росте | Стандартний, є crates |

Auto-detect по magic bytes / extension: JKS = `feedfeed`, Key-6 = IIT header, ZS2 = `.zs2` extension + АЦСК structure, PKCS#12 = ASN.1 DER.

ZS2 файли мають парний `.pck` (сертифікат). Naming convention: `ЄДРПОУ_ІПН_SuфіксДата.ZS2` (D=директор, S=печатка, B=бухгалтер, U=універсальний).

PKCS#11 (hardware tokens) — поза scope, на касах не зустрічається.

## Python API

```python
import prro_crypto

# Auto-detect формат по magic bytes
signer = prro_crypto.Signer.from_file(
    key_bytes=open('key.dat', 'rb').read(),  # JKS, Key-6.dat, або .p12
    password='Jrcfyf123',
)

# Підпис
signed_bytes = signer.sign_raw(payload_bytes)
# Returns: CMS/PKCS#7 SignedData DER (attached), byte-identical to sidecar output
```

---

## Інтеграція з gateway

Новий provider в `src/prro_gateway/runtime/providers.py`:

```python
class RustCryptoProvider:
    def __init__(self, jks_path: str, password: str):
        import prro_crypto
        self.signer = prro_crypto.Signer.from_jks(
            jks_bytes=open(jks_path, 'rb').read(),
            password=password,
        )
    
    async def sign_raw(self, payload: bytes) -> bytes:
        return self.signer.sign_raw(payload)
```

Config:
```yaml
crypto:
  provider: rust          # passthrough | sidecar | rust
  jks_path: ./key.jks
  jks_password_env: JKS_PASSWORD
```

---

## Критерій приймання

**Byte-identical test:**
1. Взяти payload (DPS XML)
2. Підписати через sidecar → bytes_a
3. Підписати через prro_crypto → bytes_b
4. `bytes_a == bytes_b` — must match

**Live DPS test:**
1. Підписати SELL чек через prro_crypto
2. Відправити на cabinet.tax.gov.ua:9443
3. Отримати ACK (status=1)

---

## Фази

### Фаза 1: Пілот (Sprint 10-11)
Node.js sidecar як є. Працює, протестовано.

### Фаза 2: Rust crate (post-pilot)
1. Портувати DSTU 4145 curve 6 з jkurwa reference
2. Портувати GOST 89
3. JKS reader
4. CMS/PKCS#7 builder
5. PyO3 bindings + maturin build
6. Byte-identical test з sidecar output
7. Live DPS test → ACK
8. `RustCryptoProvider` в gateway
9. Publish wheels: Linux x86_64, Linux ARM64, Windows x86_64

### Фаза 3: Android
1. JNI bindings (jni crate)
2. Kotlin wrapper
3. Android .aar library
4. Integration з Android POS app

---

## Ризики

| Ризик | Мітигація |
|-------|-----------|
| DSTU curve parameters wrong | Verify against jkurwa test vectors + live DPS signing |
| CMS DER encoding mismatch | Byte-identical comparison with sidecar output |
| JKS parsing edge cases | Test with all existing .jks keystores |
| Cross-compilation issues | CI matrix: Linux/Windows/macOS/Android |
| Crypto audit requirement | Use jkurwa as reference, document deviations |

---

## Rust dependencies (expected)

```toml
[dependencies]
pyo3 = { version = "0.22", features = ["extension-module"] }
# EC math: custom implementation (DSTU curve not in standard crates)
# ASN.1 DER: der = "0.7" or manual encoding (CMS is small subset)
# SHA-256: sha2 = "0.10"
# No openssl dependency — pure Rust
```

Key principle: **minimal dependencies, pure Rust, no system OpenSSL**.
