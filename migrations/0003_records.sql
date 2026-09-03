-- SPDX-License-Identifier: Apache-2.0
-- Copyright 2026 Keldrion, LLC and contributors
-- Migration 0003: records, provenance, and compliance state (0.3.0–0.9.0 slices).
--
-- Reference tables (0002) stay plaintext — they are public law data.
-- Customer records below carry a `payload_sealed` NULLABLE column: when the
-- vault is enabled (R22), the row's domain payload is stored there as an
-- AES-256-GCM SealedPayload JSON envelope and the plaintext mirror columns
-- stay NULL. Purge (R27) hard-deletes rows past their retention horizon.

-- ---------------------------------------------------------------------------
-- Consignments: CBAM status lifecycle (R15 customs), exemptions (R43/R45),
-- liability tags (R46), retention (R27).
-- ---------------------------------------------------------------------------

ALTER TABLE consignments ADD COLUMN status TEXT NOT NULL DEFAULT 'LIABLE'
    CHECK (status IN ('LIABLE','DEFERRED','IPR_TRACKED','OPR_TRACKED','EXCLUDED'));
ALTER TABLE consignments ADD COLUMN tax_point_date TEXT;             -- locked on 40 71 promotion
ALTER TABLE consignments ADD COLUMN origin_exempt_ets_link INTEGER NOT NULL DEFAULT 0;  -- R43
ALTER TABLE consignments ADD COLUMN origin_exempt_military INTEGER NOT NULL DEFAULT 0;  -- R45
ALTER TABLE consignments ADD COLUMN liability_tag TEXT NOT NULL DEFAULT 'NONE'
    CHECK (liability_tag IN ('NONE','JOINT_AND_SEVERAL'));           -- R46
ALTER TABLE consignments ADD COLUMN declarant_eori TEXT;             -- workspace owner (ICR mode, R25)
ALTER TABLE consignments ADD COLUMN retention_purge_after TEXT;      -- R27 horizon
ALTER TABLE consignments ADD COLUMN payload_sealed TEXT;             -- vault envelope (R22)

CREATE INDEX IF NOT EXISTS idx_consignments_status ON consignments(status);
CREATE INDEX IF NOT EXISTS idx_consignments_eori ON consignments(declarant_eori);

-- ---------------------------------------------------------------------------
-- Audit trail (R10): append-only, hash-chained; the chain row order IS the
-- chain. Attachments (R16) retain the local document hash — never the file.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS audit_events (
    seq          INTEGER PRIMARY KEY,
    ts_utc       TEXT NOT NULL,
    actor        TEXT NOT NULL,
    action       TEXT NOT NULL,
    subject      TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    prev_hash    TEXT NOT NULL,
    hash         TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS attachments (
    id                  TEXT PRIMARY KEY,
    subject             TEXT NOT NULL,        -- consignment/dossier id
    filename            TEXT NOT NULL,
    mime_type           TEXT NOT NULL,
    sha256              TEXT NOT NULL,
    verified_by_human   INTEGER NOT NULL CHECK (verified_by_human IN (0,1)),
    verification_note   TEXT NOT NULL DEFAULT '',
    created_utc         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_attachments_subject ON attachments(subject);

-- ---------------------------------------------------------------------------
-- Dossiers (R23/R35): three document classes + heat/waste-gas balance.
-- Document payloads are JSON (sealed via payload_sealed when the vault is on).
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS dossiers (
    id              TEXT PRIMARY KEY,
    consignment_id  INTEGER NOT NULL REFERENCES consignments(id),
    energy_json     TEXT,      -- EnergyRecord + e-fapiao fields
    materials_json  TEXT,      -- Vec<MaterialRecord> + scrap records
    production_json TEXT,      -- Vec<ProductionRecord> + output tonnes
    balance_json    TEXT,      -- R35 BalanceTable
    complete        INTEGER NOT NULL DEFAULT 0 CHECK (complete IN (0,1)),
    payload_sealed  TEXT,
    updated_utc     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- ---------------------------------------------------------------------------
-- Certificates & compliance (R24/R32/R37/R40/R42).
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS certificate_events (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('PURCHASED','CANCELLED','SURRENDERED','BUYBACK_REQUESTED')),
    tco2e       REAL NOT NULL CHECK (tco2e >= 0),
    price_eur   REAL,
    event_date  TEXT NOT NULL,
    created_utc TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS nca_communications (
    id             TEXT PRIMARY KEY,
    notice_kind    TEXT NOT NULL,
    received_iso   TEXT NOT NULL,
    respond_by_iso TEXT NOT NULL,
    responded_utc  TEXT,
    notes          TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS authorisation_status (
    eori         TEXT PRIMARY KEY,
    status       TEXT NOT NULL CHECK (status IN ('ACTIVE','SUSPENDED','REVOKED')),
    since_utc    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS guarantee (
    eori         TEXT PRIMARY KEY,
    lodged_eur   REAL NOT NULL DEFAULT 0 CHECK (lodged_eur >= 0),
    updated_utc  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- ---------------------------------------------------------------------------
-- Verifier surface (R28/R33/R38) and packs/declarations (R9/R21/R30).
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS findings (
    id                     TEXT PRIMARY KEY,
    dossier_id             TEXT NOT NULL,
    severity               TEXT NOT NULL,
    description            TEXT NOT NULL,
    status                 TEXT NOT NULL CHECK (status IN
                             ('OPEN','CORRECTION_REQUESTED','RESUBMITTED','CLOSED','REJECTED')),
    correction_buffer_days INTEGER NOT NULL CHECK (correction_buffer_days > 0),
    requested_iso          TEXT NOT NULL,
    updated_utc            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS declarations (
    id               TEXT PRIMARY KEY,
    declaration_year INTEGER NOT NULL,
    schema_version   TEXT NOT NULL,              -- R30: validated against its era
    file_json        TEXT NOT NULL,
    chain_root       TEXT NOT NULL,              -- R10: audit root at submission
    submitted_utc    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS sealed_packs (
    id           TEXT PRIMARY KEY,
    cn_code      TEXT NOT NULL,
    pack_json    TEXT NOT NULL,                  -- SealedPack (VP/VC-JWT derive from it)
    issued_iso   TEXT NOT NULL,
    created_utc  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- ---------------------------------------------------------------------------
-- Sync outbox (R7/R11/R36): ETS price cache, localized data requests,
-- registry status snapshots. All degrade offline; queued rows drain on sync.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS ets_price_cache (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    eur_per_tco2e REAL NOT NULL CHECK (eur_per_tco2e >= 0),
    as_of_iso     TEXT NOT NULL,
    manual        INTEGER NOT NULL DEFAULT 0 CHECK (manual IN (0,1)),
    stale         INTEGER NOT NULL DEFAULT 0 CHECK (stale IN (0,1)),
    updated_utc   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS data_requests (
    id          TEXT PRIMARY KEY,
    locale      TEXT NOT NULL CHECK (locale IN ('en','zh-CN')),
    recipient   TEXT NOT NULL,
    cn_codes    TEXT NOT NULL,                   -- JSON array
    queued      INTEGER NOT NULL DEFAULT 1 CHECK (queued IN (0,1)),
    created_utc TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS registry_status (
    subject       TEXT PRIMARY KEY,              -- operator id or EORI
    status        TEXT NOT NULL,
    refreshed_iso TEXT NOT NULL,
    stale         INTEGER NOT NULL DEFAULT 0 CHECK (stale IN (0,1))
);

-- Operator-ID mapping (R36).
CREATE TABLE IF NOT EXISTS operator_records (
    registry_operator_id TEXT PRIMARY KEY,
    installation_id      TEXT NOT NULL,
    status               TEXT NOT NULL CHECK (status IN ('REGISTERED','PENDING','REVOKED','WITHDRAWN')),
    refreshed_iso        TEXT NOT NULL
);

-- FK target for consignments imported in bulk (SAD rows) or synced from the
-- wizard before an installation is mapped (R36 mapping happens later).
-- production_routes carries a non-empty placeholder (the lookup layer
-- rejects empty reference cells); "-" matches no real route.
INSERT OR IGNORE INTO installations (id, name, address, production_routes) VALUES
    ('UNMAPPED', 'Unmapped installation', '—', '-');
