# RS-3 A2.1b-core — досье ревью (PR #125)

**Дата:** 2026-06-10
**Объект:** ветка `feat/rs3-a2-1b-core` (`23eafc6..c9c8c83`, 11 коммитов) — dormant inline-оркестратор `services/write_path/inline.rs`.
**Фикс-коммиты по итогам:** `1f03117` (review fixes), `cdb3fb2` (audit fix).
**Прогоны:** 1338 passed на `c9c8c83`; 1343 passed на `1f03117`; merge-gate на `cdb3fb2` — см. PR.

## Процесс (честно)

Пять линз структурированного ревью (operator-contract fidelity · inbox-lifecycle · HTTP-таксономия · test adequacy · A2.4/A2.2-readiness) отработали полностью; шестая (audit-forensics) и **все** агенты состязательной верификации упали на сессионных лимитах. Верификация всех существенных находок проведена вручную в основном контексте (Fable 5), включая независимый fresh-eyes проход, который нашёл дефект в одном из собственных фиксов (см. AUD-1). Полные сырые результаты линз: артефакт workflow `wf_8a8eff4b-2cb`.

## Вердикты линз (сводки)

- **Contract fidelity**: «залоченные решения исполнены верно; HIGH/MED расхождений нет». Поправка 1 (атомарность pre-acquire) **доказана** one-tx; Z-арм fail-closed во всех профилях сборки (501-return безусловен от гейта, `debug_assert` — только test-tripwire); поправки 3/4/5 исполнены точно.
- **Inbox-lifecycle**: полное перечисление Err-выходов `run()` — все терминализируют корректно; double-terminalise невозможен (грепом доказано: стадии, кроме acquire/finalize, не трогают inbox).
- **HTTP-таксономия**: все эмиттируемые коды round-trip'ятся в свои классы; `_=>500` fallback для Internal-кодов — осознанный и запиннен fence-тестом.
- **Test adequacy (adversarial)**: 11 интеграционных тестов честные (без unit-шорткатов), но 8 арм-ов без integration-покрытия; аудиты ассертились только COUNT'ом.
- **A2.4-readiness**: API-форма пригодна для мульти-FN биндинга (per-call deps + существующий bindings-registry); порядок проверок в `run()` корректен; RETURN проходит весь тракт.

## Реестр находок и диспозиция

### Исправлено в `1f03117`

| ID | Sev | Находка | Фикс |
|---|---|---|---|
| OCF-2 / F3 / HTTP-2 | LOW | Terminalise-сбои глотали причину (`map_err(\|_\|…)`); код `REPLAY_LEDGER_DRIFT` переиспользован не по смыслу; упавший Critical-audit Noop — молча | `tracing::error!` с цепочкой в обоих хелперах + Noop; новый fenced-код `INBOX_TERMINALISE_FAILED` (500) |
| OCF-1 / A24-1 | MED | Inline-ACK без `fiscal_ts` — расхождение конверта первый-проход↔replay | Best-effort чтение `first_kvt1_at` после advance (деградация в None); пин `fiscal_ts.is_some()` |
| OCF-4 | LOW | `_` catch-all в `terminal_to_outcome`: будущий терминальный `DocState` молча ушёл бы в 202 | Исчерпывающий match по 13 вариантам |
| HTTP-1 | LOW | `Z_SURFACE_NOT_READY` тремя несвязанными литералами; fixed-литералы `code_of` без fence | Z-арм через `code_of(&fe)`; fence-тест на 5 fixed-литералов |
| A24-3 | LOW | Оба `#![allow(dead_code)]` устарели; заметка inline_map ложная | Сняты; вскрытое поле `observed` теперь трейсится |
| TA-1 | HIGH | confirm-Drift арм — ноль покрытия при тривиальной инжекции; единственный пин #8-расхождения (doc SENT + inbox REJECTED) | Интеграционный тест |
| TA-2 | MED | `resolve_against_ledger`/Noop ни разу не исполнялись против реальной БД (decision e не запиннен) | Noop-тест: DONE-строка + ACK-леджер → возврат истины + Critical-audit + чужая строка не тронута |
| TA-6 (часть) | LOW | BuildReject не покрыт | Тест: hash-mismatch → pre-acquire REJECT + `PAYLOAD_HASH_MISMATCH`, 0 doc-строк |
| A24-4 | LOW | RETURN в подписанном скоупе, но без покрытия | Тест RETURN→ACK |
| TA-4 / TA-7 | MED/INFO | Аудиты только COUNT'ом; success-статусы inbox не ассертились | Пины payload-кода и `attempt_no` (GOTCHA-захват); inbox DONE после ACK, PROCESSING после Hold/offline |

### Исправлено в `cdb3fb2` (находка fresh-eyes аудита)

| ID | Sev | Находка | Фикс |
|---|---|---|---|
| **AUD-1** | **MED (fiscal-truth)** | Фикс OCF-3 из `1f03117` терминализировал inbox на ЛЮБОЙ Err леджер-резолвера, включая «истина неизвестна». Doc, durably висящий в `Sent`, позже доконвергирует до ACK через boot-probe — а REJECTED inbox шорт-катит replay в `Failed` ДО джойна с леджером ⇒ шлюз replay'ил бы «Failed» про фискализированный чек | `is_terminal_ledger_verdict`: терминализация только при позитивном терминальном вердикте (DpsRejected / `SHIFT_MANUAL_RECON`); unknown-drift оставляет PROCESSING (+warn-trace). Юнит-пин ×4 |

Производная OCF-3/F2 (терминализация нашего лиза при терминальном вердикте резолвера) — реализована в `1f03117`, уточнена в `cdb3fb2`.

### Подтверждено, задокументировано (фикс невозможен или не здесь)

| ID | Sev | Находка | Диспозиция |
|---|---|---|---|
| F1 | MED→LOW | DB-fault во время pre-acquire terminalise оставляет строку NEW при возвращённом Err — буква obligation невыполнима в момент отказа (в падающую БД нельзя записать терминализацию) | Громкий trace + код-коммент; **скоуп B1: реапер обязан покрывать NEW-строки без fiscal_documents** |
| TA-3 | MED | Арм dispatch-Refused (`INLINE_DISPATCH_REFUSED`, единственный inline-owned 503) не инжектируем интеграционно — нужен mid-flight flip ноды между acquire и dispatch | Known-untested; проводка идентична доказанным арм-ам; покрыть stage-stub'ом в слое 2 тест-плана |
| F6 / HTTP-5 | LOW | Terminalise-everything ⇒ повтор ключа после infra-500/503 даёт 422 INBOX_REJECTED (re-key обязателен) | Контракт POS-шима — задокументировать в A2.4; `busy_timeout 5s` поглощает обычный contention |

### Forward-looking (контракты для последующих кусков — не дефекты ветки)

| ID | Куда | Суть |
|---|---|---|
| OCF-5 / TA-5 / F5 | **B1 / ops-loop** | Конвергенция online-`Sent` (Hold-202/drift) существует только на boot; ops-loop обязан re-probe'ить online Sent-доки; `inline::run` не годится как вход реапера (всё NEW-gated) — B1 нужен свой вход |
| OCF-6 / A24-2 | **A2.4** | Гейт/`sign_ctx`/`fn_sign` не привязаны к `row.fiscal_number` типом — биндинг обязан FN-bound newtype + FN→deps map |
| OCF-7 | — | `OFFLINE_NODE_ONLINE_RACE` разделён тремя режимами (имя описывает один) — приемлемо, режимы-сироты unreachable |
| HTTP-4 | — | Handler-side тест не расширен новым 422-кодом — избыточно (fence идёт через ту же функцию) |

### Архитектурные находки аудита (вне реестра линз)

См. `docs/superpowers/plans/2026-06-10-architecture-audit-and-test-plan.md` (часть 1):
двойной DPS round-trip на онлайн-чек (исследовать протокольную эквивалентность `sendChk.data_sign ≡ lastChk.data_sign` — seam `advance_to_ack` готов к однострочной оптимизации); `run()` ~540 строк — извлечь онлайн-хвост перед A2.2; аудит-гигиена.

## Итоговый вердикт

**SAFE после фиксов.** Все HIGH/MED закрыты фиксами+тестами либо явно посажены в скоуп B1/A2.2/A2.4 с обоснованием. Залоченный операторский контракт (Q1–Q4, поправки 1–5) исполнен дословно — подтверждено линзой contract-fidelity построчно. Merge-gate: полный прогон на `cdb3fb2` + знак оператора.
