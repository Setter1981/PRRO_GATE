# Per-FN Configuration — налаштування фіскальних номерів та операторів

**Статус:** draft для review · **Цільова міграція:** `sql/017_sidecar_ops_and_fn_business.sql` · **Дата:** 2026-04-19

Документ фіксує схему зберігання per-FN налаштувань у БД — що вже є, що додаємо в рамках спринту Rust Fiscal Driver (ADR-004), що заплановано на майбутнє.

Модель даних:
- **1 FN → 1 власник** (юрособа/ФОП) — ідентифікація бізнесу, налаштування режиму
- **1 FN → N операторів** (касирів) — кожен зі своїм JKS ключем і ІПН
- **1 FN → 1 поточний сертифікат** (кеш, автооновлюється)

---

## Таблиця `fiscal_number_config` — бізнес-ідентичність + per-FN поведінка

### Існуючі поля (migration 011, вже в production)

| Поле | Тип | Default | Призначення |
|---|---|---|---|
| `fiscal_number` | TEXT PK | — | Фіскальний номер РРО (10 digits) |
| `enforce_blocked_mode` | INTEGER | `0` | Чи працює FN у блок-режимі (після `ERROR_NOT_REGISTERED_RRO` від ДПС) |
| `min_offline_codes` | INTEGER | `0` | Мінімальний поріг залишку офлайн-номерів (low watermark) |
| `max_offline_codes` | INTEGER | `0` | Цільова кількість офлайн-номерів у пулі (target watermark) |
| `created_at` | TEXT | CURRENT_TIMESTAMP | — |
| `updated_at` | TEXT | CURRENT_TIMESTAMP | — |

**Де читається:** `repositories/fn_config.py::get()`, `repositories/fn_config.py::get_or_default()`. Викликають write_path (enforce_blocked_mode) та offline_sync (watermarks).

### Нові поля (migration 017 — цей спринт)

| Поле | Тип | Default | Призначення | Хто читає |
|---|---|---|---|---|
| `tax_number` | TEXT NOT NULL | `''` | ІПН/EDRPOU **власника РРО**. Потрапляє в XML атрибут `<DAT TN="...">`. | Rust sidecar (для TIN license check) · Python dps_xml.py (legacy) |
| `fiscal_mode` | TEXT NOT NULL CHECK IN ('prod','test') | `'test'` | Режим: `prod` → `prro.tax.gov.ua:443`, `test` → `cabinet.tax.gov.ua:9443` | Rust sidecar — роутинг gRPC channel; Python — архівування в окрему папку |
| `national_check_enabled` | INTEGER NOT NULL | `0` | Увімкнути "Національний чек" — додаткові `<L>ERECEIPT</L>`, `<L>BID=...</L>`, `<L>RID=...</L>`, `<L>BTX=...</L>`, `<L>TIN=...</L>` теги при наявності RRN в оплаті | Rust `xml_builder.rs` |
| `offline_enabled` | INTEGER NOT NULL | `1` | Дозвіл офлайн-режиму для цього FN. Якщо `0` — `GO_OFFLINE` відхиляється навіть при паролі ліцензії. | Python write_path; Rust sidecar (у demo тарифі forcibly `0`) |
| `tsp_enabled` | INTEGER NOT NULL | `0` | Додавати RFC 3161 timestamp до CMS підпису (юридично-значимий чек). Потребує доступу до `acskTSP`/`ACSK TSP server`. Default off — швидкість важливіша для більшості FN. | Rust `cms_adapter.rs` (через `CmsSigner::sign_with_tst()`) |

Відповідність WebCheck INI-ключів:
- `FiscalMode` (0/1) ↔ наш `fiscal_mode` ('prod'/'test')
- `useecheckmegovua` ↔ наш `national_check_enabled`
- `Offline` ↔ наш `offline_enabled`
- `UseACSKTSPserver` ↔ наш `tsp_enabled`

### Майбутні розширення (наступні спринти, ще НЕ в міграції)

Visit/print/UI налаштування — Python-side, не блокують Rust driver. Додамо коли прийде Sprint по друкарській формі і UI.

| Поле | Тип | WebCheck ключ | Призначення |
|---|---|---|---|
| `log_enabled` | INTEGER | `LogOn` | Запис protocol trace для цього FN |
| `auto_offline_on` | INTEGER | `AutomatOfflineOn` | Автоматичний перехід в офлайн при втраті зв'язку |
| `print_paper_width_mm` | INTEGER | `Rb80`/`Rb57` | Ширина чека (80 або 57 мм) |
| `export_length` | INTEGER | `ExportLength` | Довжина символів в рядку для TXT-експорту |
| `auto_print_enabled` | INTEGER | `AutomatPrintCheck` | Автодрук одразу після реєстрації |
| `show_print_form` | INTEGER | `ShowPintForm` | Показувати форму друку для SELL/RETURN |
| `show_print_form_x` | INTEGER | `ShowPintFormX` | Показувати форму друку для X-звіту |
| `indicator_y` | INTEGER | `IndicatorY` | Y-координата індикатора в UI |
| `indicator_step_y` | INTEGER | `IndicatorStepY` | Крок між індикаторами |

Експорти форматів (`ToPDF`/`ToTXT`/`ToXML`) — вирішимо чи окремі INTEGER-прапорці, чи одне поле з TEXT-маскою (`'pdf,xml'`).

---

## Таблиця `sidecar_operators` — нова, migration 017

Зберігає JKS credentials **касирів** (операторів) по FN. Модель **1 FN → N касирів** — кожен зі своїм сертифікатом підпису. За аналогією з WebCheck `OPERATORS(ID, OPERATORNAME, KEYPATH, KEYPASS, INN)`.

| Поле | Тип | Default | Призначення |
|---|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | — | Сурогатний ключ |
| `fiscal_number` | TEXT NOT NULL | — | FN до якого прив'язаний оператор |
| `operator_name` | TEXT | NULL | ПІБ касира (відображення в UI/print) |
| `operator_inn` | TEXT NOT NULL | — | **ІПН касира** (10 digits), зазначається в XML як `OI` (опційно) та в логах |
| `jks_path` | TEXT NOT NULL | — | Абсолютний шлях до `.jks` контейнера (або `.zs2`/`.pfx`/`.dat` — формат визначає `prro_crypto::interop::prro::detect_format()`) |
| `jks_password` | TEXT NOT NULL | — | Пароль до контейнера. Зберігається у режимі `xor_soft` (за замовчуванням) як hex XOR-обфускований рядок, або plain text якщо `credentials_mode = "plain"` в sidecar.toml. Ключ обфускації = SHA-256(valid_to + operator_name[1]). |
| `active` | INTEGER NOT NULL | `1` | Активний/вимкнений (без видалення рядка — збереження історії) |
| `created_at` | TEXT | CURRENT_TIMESTAMP | — |
| `updated_at` | TEXT | CURRENT_TIMESTAMP | — |

**Індекси:**
- `ix_sidecar_operators_fn` (fiscal_number) — швидкий lookup операторів FN
- `ix_sidecar_operators_active` (active) — активні оператори

**Admin CLI:**
```bash
prro_admin add_operator \
  --fn FN001 \
  --jks /path/to/key.jks \
  --jks-password "..." \
  --operator-inn 1111111111 \
  --operator-name "Сідоренко В.Л."

prro_admin list_operators --fn FN001
prro_admin deactivate_operator --id 42
prro_admin reactivate_operator --id 42
```

**Runtime flow в sidecar:**
1. Startup: `SELECT * FROM sidecar_operators WHERE active = 1` → для кожного FN тримаємо in-memory список активних ключів
2. Перший активний оператор FN → завантажуємо JKS → тримаємо `LoadedKey` у пам'яті
3. При зміні active operator (через `prro_admin`) — HUP або auto-detect через SIGUSR1 / reload endpoint

**Захист пароля (XOR-soft):**
- Runtime потребує оригінальний пароль для розблокування JKS (hash не підходить)
- Обфускація: `hex(password XOR SHA-256(cert_valid_to + operator_name[1]))` — не plain text у БД
- Ключ не зберігається окремо — виводиться з полів самого запису
- `credentials_mode = "plain"` в sidecar.toml — opt-out для міграції з WebCheck або debug

---

## Таблиця `operator_certs` — кеш public сертифікатів (існуюча, migration 015 — без змін)

**Довідково** — налаштовується автоматично через `services/cert_provisioning.py`, оновлюється `services/cert_watch.py`.

| Поле | Тип | Призначення |
|---|---|---|
| `fiscal_number` | TEXT PK | FN |
| `cert_fingerprint` | TEXT | SHA-256 hex над DER байтами |
| `ski_hex` | TEXT | Subject Key Identifier (64 chars) |
| `cert_der` | BLOB | Сам сертифікат (DER) |
| `subject_dn` | TEXT | Subject DN (CN/O/OU/...) |
| `issuer_dn` | TEXT | Issuer DN (CA) |
| `valid_from` | TIMESTAMP | Початок дії сертифікату |
| `valid_to` | TIMESTAMP | Кінець дії сертифікату |
| `fetched_at` | TIMESTAMP | Коли кеш зберігся |
| `source` | TEXT CHECK ('container','cmp','manual') | Звідки отриманий |
| `last_refresh_at` | TIMESTAMP | Останнє оновлення через CMP |

**Зв'язок з `sidecar_operators`:** link через `fiscal_number`. При додаванні оператора через `prro_admin add_operator`:
1. Відкриваємо JKS з паролем
2. Виймаємо cert DER
3. INSERT/UPDATE в `operator_certs` з `source='container'`
4. INSERT в `sidecar_operators`

Sidecar runtime робить JOIN:
```sql
SELECT so.jks_path, so.jks_password, oc.valid_to, fn.tax_number, fn.fiscal_mode
  FROM sidecar_operators so
  JOIN fiscal_number_config fn USING (fiscal_number)
  LEFT JOIN operator_certs oc USING (fiscal_number)
 WHERE so.fiscal_number = ? AND so.active = 1
 ORDER BY so.created_at ASC
 LIMIT 1;
```

Якщо `oc.valid_to < now() + 14 days` — sidecar додає `cert_expires_in_N_days` warning в кожну response.

---

## Зміни runtime логіки

### Python side

**До міграції 017:** `config.defaults.tax_number` (глобал з `config.yaml`) використовується для всіх FN.

**Після міграції 017:** `WritePathWorker.tax_number` стає per-FN. Контейнер `runtime/container.py` читає `fiscal_number_config.tax_number` при створенні воркера для кожного FN. Глобал `defaults.tax_number` залишається як fallback якщо в БД `''`.

Плану повної міграції yaml → DB для решти налаштувань (ingress, metrics, alerts, runtime) — **окремий спринт після пілоту**.

### Rust side

Sidecar читає per-FN налаштування з БД при кожному запиті (чи кешує з TTL=60s):
- `tax_number` → XML `<DAT TN="...">` і перевірка `license.tin == fn.tax_number`
- `fiscal_mode` → вибір gRPC channel з пула
- `national_check_enabled` → додавання `<L>ERECEIPT/...</L>` тегів
- `offline_enabled` → rejection якщо `GO_OFFLINE` при `0`
- `tsp_enabled` → виклик `CmsSigner::sign_with_tst()` замість `sign_with()`

---

## Зведена таблиця нових полів

**Додаємо зараз (migration 017):**

- `fiscal_number_config.tax_number`
- `fiscal_number_config.fiscal_mode`
- `fiscal_number_config.national_check_enabled`
- `fiscal_number_config.offline_enabled`
- `fiscal_number_config.tsp_enabled`
- нова таблиця `sidecar_operators` (9 полів + 2 індекси)

**Плануємо додати пізніше (окремий спринт):**

- `fiscal_number_config.log_enabled`
- `fiscal_number_config.auto_offline_on`
- `fiscal_number_config.print_paper_width_mm`
- `fiscal_number_config.export_length`
- `fiscal_number_config.auto_print_enabled`
- `fiscal_number_config.show_print_form`
- `fiscal_number_config.show_print_form_x`
- `fiscal_number_config.indicator_y`
- `fiscal_number_config.indicator_step_y`
- експорти форматів (pdf/txt/xml) — форма tbd
- можливо per-FN `inn_payer` якщо виявиться що платник може відрізнятись від власника

---

## Відкриті питання

1. Чи зберігати **обгортку для DPAPI/Keyring** в `security.credentials_mode` одразу, чи додати пізніше по запиту? (Default plain text — погоджено.)
2. Для `tsp_enabled` — звідки брати URL ACSK TSP сервера? Per-FN чи глобально з `ca_endpoints`/`cert_provisioning_config`? (Пропозиція: брати з `cert_provisioning_config` через CA mapping — глобально, не per-FN.)
3. Чи потрібно окреме поле `org_name` / `org_address` для друку реквізитів на чеках? WebCheck має `NorgT`/`AtorgT` у FormNewPro. (Пропозиція: **так, додати в `017`** як TEXT NULL — не критично, але полегшить print form у майбутньому.)

---

## Посилання

- [ADR-004 Rust Fiscal Driver](./ADR-004-rust-fiscal-driver.md)
- [Multi-Protocol PRRO Gateway — загальна архітектура](./Multi-Protocol_PRRO_Gateway.md)
- Existing migrations: `sql/011_per_fn_config.sql`, `sql/015_operator_certs.sql`, `sql/016_ca_endpoints.sql`
- WebCheck reference: `docs/webcheck_reverse/WebCheckMain/WebCheck/FormSettings.cs` (UI), `docs/webcheck_reverse/WebCheckMain/WebCheck/CreateDB.cs:223` (OPERATORS table)
