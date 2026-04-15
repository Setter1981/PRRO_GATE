# Offline SHIFT_CLOSE Decision

## 1. Problem Statement

Проєкт уже підтримує `OFFLINE_LOCAL_ACK` для офлайн-документів та ручний `OfflineSyncService`, але контур закриття зміни/дня залишається неповним.

Потрібно відповісти на два різні питання:

1. Чи можна в нашій моделі дозволити `SHIFT_CLOSE` під час `OFFLINE`?
2. Чи є `SHIFT_CLOSE` самодостатньою доменною операцією, чи він має розглядатися разом із `Z_REPORT` / щоденним фіскальним звітним чеком?

Це decision-документ, не implementation plan у коді.

## 2. What Current Code Does Today

### 2.1 Write-path

- `SHIFT_CLOSE` зараз **не підтримується** в offline path:
  [src/prro_gateway/services/write_path.py](/mnt/d/PRRO_GATE/src/prro_gateway/services/write_path.py:734)
- До offline path включено `Z_REPORT`, але не `SHIFT_CLOSE`:
  [src/prro_gateway/services/write_path.py](/mnt/d/PRRO_GATE/src/prro_gateway/services/write_path.py:734)
- Водночас `_sync_shift_close_locked()` уже вважає `OFFLINE_LOCAL_ACK` достатнім для локального переведення зміни в `CLOSED`:
  [src/prro_gateway/services/write_path.py](/mnt/d/PRRO_GATE/src/prro_gateway/services/write_path.py:537)

Це означає:

- сьогодні `offline SHIFT_CLOSE` фактично не запускається;
- але якщо просто додати `SHIFT_CLOSE` в `_operation_supports_offline()`, зміна почне локально закриватися на `OFFLINE_LOCAL_ACK`, що є небезпечною напівреалізацією.

### 2.2 Transport semantics

- Для Checkbox `SHIFT_CLOSE` мапиться на `POST /shifts/close`:
  [src/prro_gateway/transports/checkbox_rest.py](/mnt/d/PRRO_GATE/src/prro_gateway/transports/checkbox_rest.py:336)
- `status='CLOSED'` для `SHIFT_CLOSE` мапиться в `DocumentState.ACK`:
  [src/prro_gateway/transports/checkbox_rest.py](/mnt/d/PRRO_GATE/src/prro_gateway/transports/checkbox_rest.py:413)
- `Z_REPORT` у канонічній моделі існує, але поточний Checkbox transport його **не реалізує**:
  [src/prro_gateway/transports/checkbox_rest.py](/mnt/d/PRRO_GATE/src/prro_gateway/transports/checkbox_rest.py:364)

### 2.3 Shift persistence model

- Модель `ShiftRecord` уже має поля:
  - `close_document_id`
  - `z_report_document_id`
  [src/prro_gateway/models/storage.py](/mnt/d/PRRO_GATE/src/prro_gateway/models/storage.py:105)
- Але `ShiftRepository` зараз керує майже тільки `state` і не наповнює ці link-поля:
  [src/prro_gateway/repositories/shifts.py](/mnt/d/PRRO_GATE/src/prro_gateway/repositories/shifts.py:10)

Висновок: доменна модель натякає на зв'язок `shift close` і `z-report`, але реалізація цього ще не доведена.

## 3. What Current Project Docs Say

- План проєкту фіксує:
  - `SHIFT_CLOSE` closes the active shift after fiscal success
  - `Z-report/fiscal daily report must respect pending/offline document constraints`
  [docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md](/mnt/d/PRRO_GATE/docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md:222)
- У Sprint scope вже записано, що треба блокувати `Z-report/shift close` там, де є pending offline docs:
  [docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md](/mnt/d/PRRO_GATE/docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md:422)
- Acceptance snapshot теж фіксує gap `SHIFT-CLOSE-01`:
  [docs/ACCEPTANCE_COVERAGE_SNAPSHOT.md](/mnt/d/PRRO_GATE/docs/ACCEPTANCE_COVERAGE_SNAPSHOT.md:132)
- `LEGAL_INVARIANTS.md` формулює, що фіскальний денний звіт і закриття зміни не повинні завершуватись при наявності непереданих офлайн-документів:
  [docs/LEGAL_INVARIANTS.md](/mnt/d/PRRO_GATE/docs/LEGAL_INVARIANTS.md:103)

### Terminology drift

У `LEGAL_INVARIANTS.md` ще використовується `OFFLINE_PENDING_SYNC`, тоді як реальний код уже перейшов на `OFFLINE_LOCAL_ACK`.

Це **переважно термінологічний drift**, а не окрема нова state-machine, але документ треба оновити в майбутньому.

## 4. Official / Legal Sources Reviewed

### 4.1 Official sources

- Наказ Мінфіну №317 / offline range contour:
  https://zakon.rada.gov.ua/go/z0636-20
- Офіційна брошура ДПС про ПРРО та offline режим:
  https://kyiv.tax.gov.ua/data/material/000/349/444462/broshura_2.pdf
- Роз'яснення ДПС Закарпаття, опубліковано **30 січня 2024**:
  https://zak.tax.gov.ua/media-ark/news-ark/750776.html
- ЗІР ДПС:
  https://zir.tax.gov.ua/main/bz/view/?id=39842&src=ques

### 4.2 Facts established from official sources

Факт:

- в офіційному матеріалі ДПС сказано, що якщо під час offline настає час формування `z-звіту`, йому також присвоюється фіскальний номер із діапазону; усі такі документи після відновлення зв'язку надсилаються пакетом і зберігаються до підтвердження прийняття;
- у роз'ясненні ДПС від **30 січня 2024** сказано: якщо при закритті зміни в `Z-звіті` не відображаються offline-операції, операцію треба скасувати, переконатися, що дані передані на фіскальний сервер, і повторити;
- у ЗІР зафіксовано, що якщо ПРРО був переведений у `offline`, закриття робочої зміни на інших пристроях заборонене.

Inference:

- офіційний контур допускає offline close-of-day щонайменше через `Z_REPORT`;
- при цьому close/day-close повинен залишатися пов'язаним з тим самим пристроєм/екземпляром ПРРО і з коректною доставкою offline backlog;
- пряма теза `offline SHIFT_CLOSE завжди дозволений` з цих джерел не випливає.

## 5. Architectural Decision

### 5.1 Main decision

**Цільова підтримка offline close-of-day потрібна. Але її не можна реалізовувати як ізольовану пряму операцію шляхом простого додавання `SHIFT_CLOSE` до `_operation_supports_offline()`.**

Це небезпечно, бо:

- поточний код одразу переводитиме зміну в `CLOSED` на `OFFLINE_LOCAL_ACK`;
- у системі ще немає повністю реалізованого `Z_REPORT` contour для Checkbox;
- це створить юридично сумнівний стан: зміна локально закрита, а денний фіскальний контур не доведений до DPS-підтвердження.

### 5.2 Recommended domain interpretation

Для нашого ядра потрібно розрізняти:

- `SHIFT_CLOSE`:
  backend-specific operational close request;
- `Z_REPORT`:
  юридично значущий daily fiscal report / close-of-day document.

**Рекомендована модель: close-of-day — це compound contour, у якому `Z_REPORT` є головним фіскальним документом, а `SHIFT_CLOSE` є transport/backend action only where the backend requires it.**

Для поточного Checkbox contour:

- `POST /shifts/close` слід вважати операційним close call;
- юридично безпечне offline close-of-day не можна будувати лише на ньому;
- offline day-close має бути прив'язаний до `Z_REPORT` і до подальшої впорядкованої доставки offline backlog.

## 6. Recommended State Machine

### 6.1 Minimal safe recommendation

Не додавати нові `DocumentState` на цьому етапі.

Використовувати на рівні документів уже наявні стани:

- `OFFLINE_LOCAL_ACK`
- `SENT`
- `KVT1`
- `KVT2`
- `ACK`
- `REJECTED`
- `REQUIRES_MANUAL_RECONCILIATION`

### 6.2 Shift lifecycle recommendation

Для shift lifecycle мінімально безпечна інтерпретація така:

- `OPENED`
- `CLOSING`
- `CLOSED`

Але `CLOSING` має стати змістовним, а не лише проміжним async-online станом.

`CLOSING` повинний охоплювати:

- online close sent but not finally acknowledged;
- offline day-close created locally (`Z_REPORT` у `OFFLINE_LOCAL_ACK`);
- будь-який case, де зміна вже не повинна приймати нові операції, але close/day-close ще не завершено фіскально.

### 6.3 Linkage requirement

Shift record повинен бути зв'язаний з документами закриття:

- `close_document_id`
- `z_report_document_id`

Поки ці зв'язки не наповнюються і не використовуються, contour close-of-day вважається неповним.

## 7. Required Guards

### 7.1 Guards that should exist eventually

1. `SHIFT_CLOSE` online must be blocked if:
   - є pending offline documents того самого `fiscal_number`;
   - або вже існує unresolved close/day-close contour попередньої зміни.

2. `Z_REPORT` online must be blocked if:
   - offline backlog, який має потрапити до денного контуру, ще не доставлений або не узгоджений з close-of-day semantics.

3. `SHIFT_OPEN` must be blocked if:
   - попередня зміна знаходиться в `CLOSING`;
   - linked close/day-close documents не мають фінального допустимого стану.

4. Close/day-close during or after offline must remain single-device / single-channel scoped:
   - не можна допускати закриття зміни на іншому пристрої/іншому каналі під час unresolved offline contour.

### 7.2 Immediate safety boundary before full contour

Поки compound contour не реалізований:

- не слід відкривати в runtime **standalone direct** `offline SHIFT_CLOSE`, який локально закриває зміну без повного day-close contour;
- `SHIFT_CLOSE` / `Z_REPORT` online слід блокувати при наявності `OFFLINE_LOCAL_ACK` backlog для того ж `fiscal_number`, де це юридично необхідно.

Це проміжна, але безпечна позиція.

## 8. Recovery / Restart Semantics

Після restart система повинна відновлювати не лише active shift, а й pending close-of-day contour.

Мінімальні вимоги:

1. Якщо `shift.state == CLOSING`, новий `SHIFT_OPEN` не дозволяється автоматично.
2. Якщо linked `z_report_document_id` у `OFFLINE_LOCAL_ACK`, документ повинен проходити через `OfflineSyncService` у строгому порядку після раніше створених offline документів.
3. Якщо `close_document_id` або `z_report_document_id` у `SENT/KVT1/KVT2`, контур завершується через reconciliation, а не новим write-path документом.
4. Лише після фінально допустимого результату close-of-day contour зміна переходить у `CLOSED`.

## 9. Risks If We Do Nothing

1. Хтось може спростити реалізацію і просто додати `SHIFT_CLOSE` до offline-supported operations, після чого зміна буде локально закриватись на `OFFLINE_LOCAL_ACK` без повного close-of-day contour.
2. `SHIFT_CLOSE` і `Z_REPORT` залишаться напіврозірваними доменами, хоча модель shifts уже має `z_report_document_id`.
3. Після offline activity можна буде закривати/відкривати зміну без чіткої гарантії, що денний фіскальний контур завершений правильно.
4. Документація й код продовжать розходитися: docs already expect legal blockers, code still lacks full implementation.

## 10. Recommended Bounded Implementation Sequence

### Step A — legal blocker first

Зробити вузький guard:

- блокувати `SHIFT_CLOSE` та `Z_REPORT` там, де є pending `OFFLINE_LOCAL_ACK` backlog для відповідного `fiscal_number`, якщо повний close-of-day contour ще не реалізований;
- додати окремий canonical error code для legal blocker.

### Step B — shift/document linkage

Довести linkage:

- почати реально заповнювати `close_document_id` і `z_report_document_id`;
- зробити `CLOSING` відновлюваним, а не merely cosmetic.

### Step C — online `Z_REPORT` support for Checkbox

Додати реальний transport contour для `Z_REPORT` у Checkbox.

Поки цього немає, повний day-close contour для Checkbox вважається неповним.

### Step D — offline `Z_REPORT` contour

Реалізувати offline `Z_REPORT` як документ close-of-day:

- local creation with `OFFLINE_LOCAL_ACK`;
- strict sync ordering after earlier offline docs;
- recovery-safe finalization to `ACK` / manual path.

### Step E — offline/day-close orchestration

Після цього реалізувати керований day-close contour, який підтримує offline close-of-day у повній моделі.

Він може бути оформлений як higher-level operator action:

- `close_day`

яка оркеструє:

- `Z_REPORT`
- backend-specific `SHIFT_CLOSE`

в потрібному порядку для конкретного transport/backend.

## 11. Final Recommendation

**Safest implementable interpretation:**

- продукт повинен підтримати offline close-of-day;
- але `offline SHIFT_CLOSE` не слід відкривати як окрему пряму операцію в поточній архітектурі;
- offline close-of-day потрібно моделювати через `Z_REPORT`-centric contour;
- до реалізації цього contour система має:
  - не відкривати standalone direct offline `SHIFT_CLOSE`;
  - додати legal blockers для `SHIFT_CLOSE` / `Z_REPORT` при pending offline backlog;
  - довести linkage між shift і close/day-close documents.

Це рішення є консервативним, але воно мінімізує ризик юридично хибного `shift closed` без завершеного фіскального денного контуру.
