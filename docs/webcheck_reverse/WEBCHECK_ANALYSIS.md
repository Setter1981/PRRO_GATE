# WebCheck PRRO32 — Reverse Engineering Analysis

**Джерело:** `C:\Program Files (x86)\WebCheck\PRRO32\`
**Технологія:** .NET Framework 4.x (32-bit), VB.NET → C# (декомпіль ILSpy 8.1)
**Дата аналізу:** 2026-04-17

---

## Архітектура

```
Web Check.exe          ← GUI-оболонка (WinForms), 563K
WebCheck.dll           ← Основна бізнес-логіка (2.7MB, ~150 класів)
WebCheck0-30.dll       ← 31 копія з різними MD5 — механізм ліцензування/мультиекземпляр
WebCheckServer.dll     ← COM Add-in сервер (108K) — bridge для 1C/ERP
WebCheckServer.exe     ← Host-процес для COM-сервера
TaxGrpc.dll            ← gRPC-клієнт для ДПС фіскального сервера (47K)
EUSignCP.dll           ← ЕЦП (кваліфікований підпис, ІІТ EUSign)
CSPBase/CSPExtension   ← ACSK-конектори (ключ-носії)
NCMGryada301.dll       ← Підтримка апаратного РРО Гряда 301
posnet.dll             ← Принтер Posnet
```

---

## Транспорт до ДПС (TaxGrpc.dll)

### Endpoints
- **Production:** `prro.tax.gov.ua:443`
- **Test:** `cabinet.tax.gov.ua:9443`

### Протокол
gRPC over TLS (SSL з сертифікатом `C:\ProgramData\WebCheck\prro-tax-gov-ua-chain.pem`)

### Методи (ChkIncomeService)
| gRPC метод | Призначення |
|-----------|-------------|
| `sendChk` | Надіслати чек (API v1) |
| `sendChkV2` | Надіслати чек (API v2) |
| `ping` | Перевірка з'єднання |
| `lastChk` | Отримати останній чек |

### Check запит (protobuf)
```protobuf
message Check {
  int64 DateTime;
  bytes CheckSign;       // підписаний XML файл
  string RroFn;          // фіскальний номер РРО
  string IdCancel;       // UUID для скасування
  string IdOffline;      // UUID офлайн-номера
  int32 LocalNumber;     // локальний номер чека
  Type CheckType;        // тип (0=Sell, etc.)
}

message CheckResponse {
  string Id;             // фіскальний номер (від ДПС)
  Status status;         // стан
  string ErrorMessage;
  bytes IdSign;          // підпис ID (base64)
  bytes DataSign;        // підпис даних (base64)
}
```

---

## COM API (WebCheckServer.dll)

**ProgId:** `AddIn.vk_WebCheckServer`
**GUID:** `{DE535CB8-E05F-4348-9178-EE95B3E55FF5}`
**Інтерфейс:** `IInitDone` + `ILanguageExtender` (1C-сумісний)

### Методи COM-об'єкту
```
SetP / GetP           — get/set властивостей (FN, etc.)
Init / Done           — ініціалізація/завершення
OpenShift             — відкриття зміни
CloseShift            — закриття зміни (Z-звіт)
ProcessCheck          — провести чек (продаж/повернення)
PrintByCheckFn        — друк чека за фіскальним номером
PrintXReport          — X-звіт
GetDataKKT            — отримати дані РРО
GetCurrentStatus      — поточний статус
CashInOutcome         — службове внесення/видача
DeviceTest            — тест пристрою
GetDescription / GetAdditionalActions / GetVersion
```

---

## XML-протокол (старий, DPS v2.1.x)

**Кодування:** Windows-1251 (cp1251) — явно `Encoding.GetEncoding(1251)`

### Структура чека
```xml
<RQ V='1'>
  <DAT FN='...' TN='...' ZN='' DI='N' V='1'>
    <C T='0'>           <!-- T: 0=Sell, 1=Return, 2=Service, 8=ShiftOpen, 108=ShiftOpen-v2 -->
      <P N='1' C='0' NM='Товар' Price='...' Amount='...' SM='...' TX='А'/>
      <D N='2' TY='0' PR='10.00' SM='...'/>   <!-- знижка -->
      <M N='3' NM='ГОТІВКА' SM='...' T='0'/>  <!-- оплата -->
      <E N='4' SM='...' FN='...' NO='...' TS='...'>
        <TX TX='А' TXPR='20.00' TXSM='...' TXAL='0' TXTY='0'/>
      </E>
    </C>
    <TS>20240101120000</TS>
  </DAT>
  mmmaaaccc   <!-- замінюється MAC-підписом -->
</RQ>
```

### Зміна відкриття
- `T='8'` (API v1) або `T='108'` (API v2) для `<C>` (CloseShift)

---

## Підпис (EUSignCP.dll)

- Ліб: ІІТ EUSign CP (.NET wrapper)
- Налаштування: `My Documents\WebCheck\` (settings.ini)
- OCSP/TSP/CMP — окремо конфігуруються через `TypServera`
- `MAC` = SHA-based chain: кожен чек має посилання на попередній MAC (`SubstitutePreviousMAC`)
- Ключ-носії: FileSystem, PKCS11, IDCard, Almaz 1C, Crystal 1

---

## Офлайн режим

### Ліміти (захардкоджені + конфіг)
```
LimitTimeMoon = 168 год (= 7 днів, стала константа)
TimeLimOffline = 10080 - OfflineTime хвилин
```

- `OfflineMax` (INI): макс кількість офлайн-номерів (default 500, max 2000)
- `OfflineMin` (INI): мінімальний поріг залишку (default 50)
- Демо-версія (FN=7000000512): офлайн вимкнено
- Безкоштовна версія: офлайн вимкнено
- Автоматичний офлайн: `AutomatOfflineOn`

### Офлайн UUID
- Окремий пул UUID-ів, отриманих від ДПС заздалегідь
- Зберігаються локально в SQLite
- Кожен офлайн-чек отримує один UUID з пулу (`NumbersOfflineUse.OfflineID()`)

---

## Локальна база даних (SQLite)

### Основна таблиця: `ksef`
```sql
SELECT checkidficscal, shiftid FROM ksef WHERE shiftid = ?
SELECT * FROM ksef WHERE ... -- MaxID для LocalNumber
```

**Поля:** `checkidficscal` (фіскальний номер), `shiftid`

### Конфіг
- INI-файли: `My Documents\WebCheck\settings.ini`
- Per-FN секція: `OfflineMax`, `OfflineMin`, `OfflineTime`, `SRC`
- `Archive\settings.ini` для архіву

---

## WebCheck0-30.dll — множинні екземпляри

31 унікальних DLL (~2.7MB кожен, різний MD5), ймовірно:
- **Версія А:** кожна DLL — окремий ліцензований екземпляр для конкретного FN (дозволяє паралельну роботу 31 РРО)
- **Версія Б:** ротаційна схема захисту коду (різна обфускація одного і того ж коду)

Обидва пояснення сумісні — скоріше за все A: один екземпляр = один фіскальний номер.

---

## Ключові спостереження для нашого Gateway

| Аспект | WebCheck | Наш Gateway |
|--------|----------|-------------|
| Транспорт до ДПС | gRPC `sendChkV2` | HTTP через sidecar/passthrough |
| XML-кодування | cp1251 | UTF-8 |
| Підпис | EUSign (COM) | Rust sidecar |
| Офлайн UUID | Пул від ДПС, SQLite | Аналогічно |
| MAC-chain | `SubstitutePreviousMAC` | Аналогічно |
| Зберігання | SQLite (`ksef`) | SQLite (`fiscal_documents`) |
| COM-bridge | `AddIn.vk_WebCheckServer` | REST/XML-RPC/Maria |

### Важливо для сумісності
1. **gRPC `sendChkV2`** — це той метод, який треба підтримати якщо хочемо прямий gRPC транспорт
2. **cp1251** — XML перед відправкою кодується в Windows-1251, не UTF-8
3. **`C:\ProgramData\WebCheck\prro-tax-gov-ua-chain.pem`** — TLS-сертифікат ДПС
4. **`CloseOffline10()`** — перевіряє чи ДПС вже закрила офлайн-сесію
5. **`T='108'`** для відкриття зміни в API v2 (замість `T='8'`)

---

## Файли в каталозі аналізу

```
docs/webcheck_reverse/
├── WEBCHECK_ANALYSIS.md     ← цей файл
├── TaxGrpc/                 ← gRPC client + protobuf-generated classes
│   ├── TaxGrpc/Client.cs    ← основний клієнт
│   ├── TaxGrpc/Answer.cs    ← відповідь ДПС
│   └── Com.Programika.Rro.Ws.Chk/  ← protobuf types
├── WebCheckServer/          ← COM Add-in server
│   └── WebCheckServer/vk_WebCheckServer.cs  ← COM interface
├── WebCheckExe/             ← GUI shell
│   └── WebCheck/Wcwc.cs     ← запуск
└── WebCheckMain/            ← основна логіка (~150 файлів)
    └── WebCheck/
        ├── ClassFiscal.cs   ← ЯДРО: ProcessCheck, ReportZ (3832 рядки)
        ├── StringXML.cs     ← побудова XML чеків
        ├── Offlin.cs        ← офлайн режим
        ├── Signature.cs     ← ЕЦП через EUSignCP
        ├── SQLlite.cs       ← SQLite операції
        ├── All.cs           ← глобальний стан, константи
        ├── NumbersOfflineUse.cs  ← офлайн UUID пул
        └── SendingOfflineChecks.cs  ← синхронізація офлайн
```
