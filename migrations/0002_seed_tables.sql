-- SPDX-License-Identifier: Apache-2.0
-- Copyright 2026 Keldrion, LLC and contributors
-- Migration 0002: CBAM reference tables.
--
-- PLACEHOLDER DATA NOTICE
-- -----------------------
-- Every intensity value below is 0.0: a structural placeholder. NO official
-- EU default values are reproduced or invented here. Official Commission
-- implementing data will replace these rows in a later migration; the seed
-- exists so lookups, mark-ups, and completeness flows are exercisable.
-- The mark-up PERCENTAGES, however, are regulatory pins (2026 +10 %,
-- 2027 +20 %, 2028+ +30 %, fertilisers +1 %) and are tested for consistency
-- against src/domain/markups.rs.

CREATE TABLE IF NOT EXISTS cn_codes (
    code        TEXT PRIMARY KEY,          -- 8-digit Combined Nomenclature
    description TEXT NOT NULL,
    sector      TEXT NOT NULL CHECK (sector IN
                  ('STEEL','ALUMINIUM','CEMENT','FERTILISERS','HYDROGEN','ELECTRICITY'))
);

CREATE TABLE IF NOT EXISTS default_values (
    cn_code                TEXT NOT NULL REFERENCES cn_codes(code),
    production_route       TEXT NOT NULL,
    direct_tco2e_per_t     REAL NOT NULL DEFAULT 0.0 CHECK (direct_tco2e_per_t   >= 0),
    indirect_tco2e_per_t   REAL NOT NULL DEFAULT 0.0 CHECK (indirect_tco2e_per_t >= 0),
    markup_2026_percent    REAL NOT NULL,
    markup_2027_percent    REAL NOT NULL,
    markup_2028_percent    REAL NOT NULL,
    PRIMARY KEY (cn_code, production_route)
);

CREATE TABLE IF NOT EXISTS installations (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    address           TEXT NOT NULL,
    production_routes TEXT NOT NULL        -- comma-separated route ids
);

CREATE TABLE IF NOT EXISTS consignments (
    id                          INTEGER PRIMARY KEY,
    cn_code                     TEXT NOT NULL REFERENCES cn_codes(code),
    net_mass_kg                 REAL NOT NULL CHECK (net_mass_kg >= 0),
    country_of_origin           TEXT NOT NULL,
    production_country          TEXT NOT NULL,
    installation_id             TEXT NOT NULL REFERENCES installations(id),
    import_date                 TEXT NOT NULL,  -- ISO-8601 YYYY-MM-DD
    determination_basis         TEXT NOT NULL CHECK (determination_basis IN ('ACTUAL','DEFAULT')),
    carbon_price_eur_per_tco2e  REAL,
    carbon_price_country        TEXT
);

CREATE INDEX IF NOT EXISTS idx_consignments_cn_code ON consignments(cn_code);
CREATE INDEX IF NOT EXISTS idx_consignments_import_date ON consignments(import_date);

-- ---------------------------------------------------------------------------
-- PLACEHOLDER SEED (structural only; all intensities 0.0)
-- ---------------------------------------------------------------------------

INSERT OR IGNORE INTO cn_codes (code, description, sector) VALUES
    ('73181500', 'Threaded studs and similar products, of iron or steel', 'STEEL'),
    ('76041010', 'Aluminium bars, rods and profiles', 'ALUMINIUM'),
    ('31021000', 'Mineral or chemical nitrogenous fertilisers', 'FERTILISERS');

INSERT OR IGNORE INTO default_values
    (cn_code, production_route, direct_tco2e_per_t, indirect_tco2e_per_t,
     markup_2026_percent, markup_2027_percent, markup_2028_percent)
VALUES
    -- Steel fasteners, electric-furnace route (PLACEHOLDER 0.0)
    ('73181500', 'EF',        0.0, 0.0, 10.0, 20.0, 30.0),
    -- Aluminium: primary vs recycled routes (PLACEHOLDER 0.0)
    ('76041010', 'PRIMARY',   0.0, 0.0, 10.0, 20.0, 30.0),
    ('76041010', 'RECYCLED',  0.0, 0.0, 10.0, 20.0, 30.0),
    -- Fertilisers carry the +1 % branch in every year (PLACEHOLDER 0.0)
    ('31021000', 'NATURAL_GAS', 0.0, 0.0, 1.0, 1.0, 1.0);

INSERT OR IGNORE INTO installations (id, name, address, production_routes) VALUES
    ('INST-DE-001', 'Placeholder Stahlwerk Nord', 'Musterstrasse 1, Hamburg, DE', 'EF'),
    ('INST-FR-001', 'Placeholder Aluminium Sud', 'Rue Exemple 2, Lyon, FR', 'PRIMARY,RECYCLED');
