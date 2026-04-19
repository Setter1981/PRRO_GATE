-- CA endpoint registry for cert_provisioning multi-URL retry.
--
-- Populated from the Ukrainian qualified-trust-service-provider list at
-- docs.art-zvit.com.ua/appendix_ya.htm (cross-checked 2026-04-16 against
-- dstucrypt/agent osplus.ini conventions). The IIT proprietary wire
-- protocol used by every listed CMP endpoint is identical — only the
-- host differs. `issuer_pattern` is a case-insensitive substring test
-- against the cert's issuer DN used to prioritise the correct CA on
-- lookups; if absent (ZS2 without embedded cert), the service iterates
-- all enabled endpoints by ascending priority until one returns a cert.

CREATE TABLE ca_endpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    cmp_url TEXT,
    tsp_url TEXT,
    ocsp_url TEXT,
    issuer_pattern TEXT,
    priority INTEGER NOT NULL DEFAULT 100,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX ix_ca_endpoints_enabled_priority
    ON ca_endpoints(enabled, priority);

-- ДПС (ca.tax.gov.ua / acskidd.gov.ua) — default for most ПРРО operators
INSERT INTO ca_endpoints (name, cmp_url, tsp_url, ocsp_url, issuer_pattern, priority) VALUES
    ('acskidd',
     'http://acskidd.gov.ua/services/cmp/',
     'http://acskidd.gov.ua/services/tsp/',
     'http://acskidd.gov.ua/services/ocsp/',
     'податкова',
     10);

-- АЦСК "Україна" (uakey.com.ua) — commercial CA, frequent fallback
INSERT INTO ca_endpoints (name, cmp_url, tsp_url, ocsp_url, issuer_pattern, priority) VALUES
    ('uakey',
     'http://uakey.com.ua/services/cmp/',
     'http://uakey.com.ua/services/tsp/',
     'http://uakey.com.ua/services/ocsp/',
     'україна',
     20);

-- АЦСК ПриватБанк (acsk.privatbank.ua)
INSERT INTO ca_endpoints (name, cmp_url, tsp_url, ocsp_url, issuer_pattern, priority) VALUES
    ('privatbank',
     'http://acsk.privatbank.ua/services/cmp/',
     'http://acsk.privatbank.ua/services/tsp/',
     'http://acsk.privatbank.ua/services/ocsp/',
     'приватбанк',
     30);

-- MasterKey (cmp.masterkey.ua)
INSERT INTO ca_endpoints (name, cmp_url, tsp_url, ocsp_url, issuer_pattern, priority) VALUES
    ('masterkey',
     'http://cmp.masterkey.ua/services/cmp/',
     'http://tsp.masterkey.ua/services/tsp/',
     'http://masterkey.ua/services/ocsp/',
     'masterkey',
     40);

-- CA ІнформЮст (ca.informjust.ua) — Ministry of Justice
INSERT INTO ca_endpoints (name, cmp_url, tsp_url, ocsp_url, issuer_pattern, priority) VALUES
    ('informjust',
     'http://ca.informjust.ua/services/cmp/',
     'http://ca.informjust.ua/services/tsp/',
     'http://ca.informjust.ua/services/ocsp/',
     'юст',
     50);

-- ЦСК УкрСиббанк (csk.ukrsibbank.com)
INSERT INTO ca_endpoints (name, cmp_url, tsp_url, ocsp_url, issuer_pattern, priority) VALUES
    ('ukrsibbank',
     'http://csk.ukrsibbank.com/services/cmp/',
     'http://csk.ukrsibbank.com/services/tsp/',
     'http://csk.ukrsibbank.com/services/ocsp/',
     'укрсиббанк',
     60);

-- CSK Укрзалізниця (csk.uz.gov.ua)
INSERT INTO ca_endpoints (name, cmp_url, tsp_url, ocsp_url, issuer_pattern, priority) VALUES
    ('csk_uz',
     'http://csk.uz.gov.ua/services/cmp/',
     'http://csk.uz.gov.ua/services/tsp/',
     'http://csk.uz.gov.ua/services/ocsp/',
     'укрзалізниц',
     70);

-- АЦСК Енергореєстр (acsk.er.gov.ua)
INSERT INTO ca_endpoints (name, cmp_url, tsp_url, ocsp_url, issuer_pattern, priority) VALUES
    ('acsk_er',
     'http://acsk.er.gov.ua/services/cmp/',
     NULL,
     NULL,
     'енергетич',
     80);

-- CA НБУ (canbu.bank.gov.ua) — TSP/OCSP only, no CMP in reference list
INSERT INTO ca_endpoints (name, cmp_url, tsp_url, ocsp_url, issuer_pattern, priority) VALUES
    ('ca_nbu',
     NULL,
     'http://canbu.bank.gov.ua/services/tsp/',
     'http://canbu.bank.gov.ua/services/ocsp/',
     'національний банк',
     90);
