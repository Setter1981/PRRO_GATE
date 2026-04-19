# Security Policy

## Призначення

`prro_crypto` — це спеціалізований pure-Rust криптографічний та CMS/interop стек для українських прикладних workflow, насамперед PRRO/фіскального підпису та суміжних сценаріїв.

Бібліотека **не позиціонується** як універсальна заміна всьому українському PKI/crypto software. Підтримка, аудит і безпекові гарантії стосуються лише задокументованого функціоналу та задокументованого threat model.

## Підтримуваний security scope

До security-sensitive scope входять:

- DSTU 4145 signing / verify
- GOST 34.311-95 hashing
- Kupyna-256/512 (DSTU 7564) hashing
- CMS/CAdES signing pipeline (auto-detect profile from certificate)
- container import для задокументованих форматів
- certificate/SPKI parsing на публічних boundary
- envelope decrypt path у межах поточного threat model
- Python bindings, які експонують публічний API

## Threat model

Поточний threat model такий:

- бібліотека повинна коректно обробляти **недовірений зовнішній ввід** без panic, silent truncation або неявного downgrade поведінки;
- бібліотека повинна відхиляти невалідні public keys / malformed compressed points / некоректні ASN.1 boundary cases на публічних шляхах;
- secret-bearing операції повинні уникати очевидних secret-dependent API/algorithmic path там, де це практично можливо в поточному scope;
- secret material не повинно навмисно експонуватись через `Debug` або залишатись у пам'яті довше, ніж потрібно, **у межах контрольованих типів**. Зауваження: `Scalar` є `Copy` і не може мати `Drop`-based auto-wipe; секретні `Scalar` temporaries зачищуються вручним `.zeroize()` у `sign()` та `ecdh_zz()`, але compiler-inserted copies на стеку (register spills, inlining) знаходяться поза нашим контролем. PBKDF2/HMAC внутрішні буфери (`pbe.rs`) зачищуються best-effort.

## Що не гарантується

Поточна версія **не гарантує**:

- універсальну сумісність з усіма історичними або vendor-specific українськими PKI артефактами;
- повний PKI lifecycle;
- transport/security semantics поза межами самого crate;
- захист від компрометації хоста, процесу, Python runtime або операційної системи;
- формальну side-channel proof-гарантію для всіх low-level алгоритмічних шляхів;
- повну верифікацію цілісності всіх контейнерних форматів у всіх режимах.

## Відомі компроміси

На поточний момент відомі й свідомо задокументовані такі компроміси:

- **Key-6 MAC не верифікується.**
  Бібліотека парсить формат і далі покладається на внутрішню ASN.1/DER-consistency перевірку для відсікання некоректного вмісту. Це не є повною integrity verification.

- **PFX `macData` наразі ігнорується.**
  Це означає, що import не слід трактувати як повну outer-container integrity verification.

- **`pubkey_dstu_pb_257()` використовує variable-time wNAF.**
  Обчислення `Q = -d·G` проходить через variable-time множення (wNAF таблиці). Це є вимірюваний timing side-channel по приватному скаляру `d` протягом одного виклику. Функція призначена для onboarding/provisioning (один раз за lifetime ключа), не для signing hot path. Constant-time coverage scope обмежений тільки **signing** (`rand_e × G`) та **ECDH** (`d·h × Q`) шляхами.

- **`parse_ocsp_status()` не прив'язує відповідь до конкретного серійного номера.**
  Парсер бере перший `SingleResponse` з `BasicOCSPResponse` без порівняння `CertID.serialNumber`. У multi-response або reordered сценарії caller може отримати статус не того сертифіката. Для production cert-watch рекомендується додатково перевіряти serial на стороні Python-caller'а.

- **Verify-path кеші мають configurable cap (default 256).**
  `verify()` кешує: (a) validated pubkeys (HashSet, 33 байти на запис), (b) precomputed EC point tables (HashMap, ~1.2 KB на запис). Обидва кеші обмежені — при досягненні cap очищуються повністю. Default 512 записів = ~600 KB max. Для серверних deployment'ів де через `verify()` проходять тисячі різних pubkeys, cap слід збільшити через `set_verify_cache_capacity(n)` на старті процесу. Без цього — cache miss кожні 512 ключів (re-validation ~150µs per key, не crash). Кеші містять тільки **публічні** дані (compressed pubkey + precomputed multiples публічної точки), секретний матеріал в кешах відсутній.

Ці обмеження прийняті для поточного продуктового scope і threat model. Якщо threat model еволюціонує до сценаріїв, де атакувальник може цілеспрямовано підміняти контейнерні байти на диску або в каналі доставки, ці механізми мають бути реалізовані повністю.

## Secure usage guidelines

Рекомендується:

- використовувати лише публічні checked/fallible API для недовіреного вводу;
- не трактувати успішний import контейнера як повний доказ його криптографічної цілісності, якщо йдеться про Key-6 або PFX outer MAC;
- не логувати приватні ключі, DER blobs, decrypted container payloads або інші секретні артефакти;
- оновлювати crate разом із regression tests та зафіксованими platform/feature combinations;
- використовувати бібліотеку в межах задокументованого support scope.

## Security reporting

Якщо ви знайшли security issue, будь ласка, не відкривайте публічний issue до узгодження fix/release.

У security report бажано включити:

- короткий опис проблеми;
- impact;
- minimal reproduction;
- affected API / format / feature flag;
- версію crate;
- платформу та архітектуру.

## Support statement

`prro_crypto` слід оцінювати як спеціалізований production-oriented компонент для конкретних українських workflow, а не як загальний криптографічний toolkit широкого призначення.
