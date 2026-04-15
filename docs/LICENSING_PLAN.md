# Licensing Model — PRRO Gateway

**Дата:** 2026-04-14
**Статус:** Planned

---

## Модель: license per fiscal_number (FN)

Ліцензія прив'язується до **fiscal_number** (номер каси від ДПС), не до машини, користувача чи серверу.

**Принцип:** FN унікальний і зареєстрований в ДПС. Хто володіє FN — той платить за нього ліцензію. Скопіювати gateway можна, але без ліцензії на конкретний FN — не працює.

---

## Природний anti-piracy

Навіть якщо хтось обійде license check:
- Чеки підписуються ключем власника FN
- Без ключа — не підпишеш
- Без підпису — ДПС reject
- Підробити FN не можна (ДПС реєстрація)

Тобто **сам ДПС виступає anti-piracy сервером** за нами. Нам не треба параноїчний захист коду — фіскальна верифікація робить це за нас.

---

## Архітектура

### Варіант 1: Signed license file (MVP)

**Файл:** `license.dat` на gateway

```yaml
# Signed by vendor master key (RSA-2048 or Ed25519)
version: 1
issued_at: "2026-04-14T00:00:00Z"
valid_until: "2027-04-14T00:00:00Z"
customer:
  name: "ТОВ Ромашка"
  edrpou: "12345678"
fiscal_numbers:
  - fn: "4000162280"
    features: ["online", "offline", "x-report", "rounding"]
  - fn: "4001017042"
    features: ["online", "offline"]
revocation_check_url: "https://license.vendor.ua/revocation"
signature: "BASE64_OF_RSA_SIGNATURE"
```

**Валідація при старті gateway:**
1. Load `license.dat`
2. Verify signature vendor's public key
3. Check `valid_until > now()`
4. Build `allowed_fns` set

**Валідація на кожен запит:**
```python
def _guard_license(conn, command):
    if command.fiscal_number not in license.allowed_fns:
        return build_canonical_error(
            CanonicalErrorCode.LICENSE_NOT_VALID,
            message=f"FN {command.fiscal_number} not licensed"
        )
    if license.expired():
        return build_canonical_error(
            CanonicalErrorCode.LICENSE_EXPIRED,
            message=f"License expired on {license.valid_until}"
        )
```

### Варіант 2: Online license server (Phase 2)

Gateway періодично (раз на день) пінгує license server:
- `POST /v1/license/check` з FN list
- Response: active/revoked/expired per FN
- Cache на 7 днів для offline tolerance

Переваги:
- Можна revoke ліцензію без заміни файлу
- Realtime контроль активних FN
- Usage analytics

Мінуси:
- Потрібна інфраструктура vendor
- Залежність від інтернету (пом'якшено cache)

### Варіант 3: Hybrid (Production)

- Signed file = primary source of truth
- Online check = optional revocation + usage telemetry
- Gateway працює offline якщо file валідний
- Online check gracefully degrades

---

## Vendor infrastructure (what we need)

| Компонент | Що робить |
|-----------|-----------|
| **Master key** | RSA-2048 або Ed25519, підписує ліцензії. Зберігається hardware-ізольовано (HSM/offline vault) |
| **License generator** | CLI tool: `license-gen --customer=X --fn=Y --expires=Z` → signed file |
| **Customer portal** (опц.) | Web UI для клієнтів: дивитись свої ліцензії, продовжувати |
| **Revocation server** (опц.) | API для online check + telemetry |

---

## Features gating

Ліцензія може включати список фіч:
```yaml
features:
  - online         # default
  - offline        # premium
  - x-report       # default
  - rounding       # default
  - multi_fn       # якщо більше 1 FN на gateway
  - webcheck_xmlrpc  # якщо треба WebCheck ingress
  - maria_tcp      # якщо треба Maria POS ingress
```

Gateway перевіряє на старті що включені features реально активні в конфіги.

---

## Ціноутворення (приклад, для orientation)

| Tier | Fn count | Ціна | Features |
|------|----------|------|----------|
| **Starter** | 1 FN | 500 грн/міс | online, x-report |
| **Business** | до 3 FN | 1200 грн/міс | + offline, rounding |
| **Premium** | до 10 FN | 3000 грн/міс | + multi_fn, all ingress |
| **Enterprise** | unlimited | Договір | + SLA, priority support |

---

## Чому не PyArmor / обфускація

Додатково до того що code-level obfuscation можна зреверсити:

1. **FN-based licensing** робить захист природним
2. **Rust crate** вже закриває sensitive crypto
3. **ДПС verification** — зовнішній anti-piracy
4. **Цільовий ринок** — retail, не secret-sauce software
5. **Opex cost** PyArmor не виправдана для цього кейсу

**Простий підхід сильніший за складний.**

---

## Implementation plan

### Phase 1 (pre-launch)
- CLI license generator (Python tool + master key)
- License file format + signature verification
- `_guard_license` в write_path
- Unit tests для expired/wrong-FN/invalid-signature

### Phase 2 (post-launch)
- Customer portal (проста Flask app)
- Revocation API
- Gateway: online check з fallback на cached file

### Phase 3 (scale)
- Usage analytics
- Per-feature metering
- Auto-renewal workflow

---

## Важливо

Ліцензія регулює **business relationship**, не фіскальну коректність. Без ліцензії gateway відмовляється обслуговувати — але ДПС продовжує вимагати фіскалізацію. Це означає:

**Gracefully degrade:**
- Ліцензія прострочена → grace period 7 днів з warning
- Після grace → reject з чітким message "Contact support"
- Ніколи не "тихо ламатися" — касир має знати що відбувається

**Pre-expiry notifications:**
- 30 днів до закінчення: email + gateway log warning
- 7 днів: додатковий warning
- 1 день: остання нотифікація
