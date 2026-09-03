// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Persistence seam over the [`crate::db::Storage`] trait (R2/R10/R15/R16/R23–
//! R27/R47). Every statement is written against `migrations/0003_records.sql`
//! exactly and binds only through the trait's `?N` string parameters, so the
//! store is testable against any `Storage` backend (SQLite is the only one
//! shipped).
//!
//! The layer is deliberately thin: domain types are constructed here, but all
//! domain logic (validation, completeness, chain hashing, retention math)
//! stays in its own module and is reused, never re-implemented.

use serde::Serialize;

use crate::db::Storage;
use crate::domain::errors::DomainError;
use crate::domain::types::{
    Consignment, DeterminationBasis, Dossier, EnergyRecord, MaterialRecord, ProductionRecord,
};
use crate::dossier;
use crate::provenance::{self, chain_hash, sha256_hex, Attachment, GENESIS_PREV_HASH};

/// Map a backend failure onto the domain error surface.
fn storage_err(e: crate::db::StorageError) -> DomainError {
    DomainError::Storage(e.to_string())
}

/// Read a non-NULL string cell, failing loudly on a NULL in a NOT NULL slot.
fn cell(row: &[Option<String>], col: usize) -> Result<String, DomainError> {
    row.get(col)
        .cloned()
        .flatten()
        .ok_or_else(|| DomainError::Storage(format!("NULL in column {col} of a record row")))
}

/// Fetch the row id minted by the most recent INSERT on this connection.
/// (The trait's scalar reads are text-typed, so numeric scalars are CAST.)
fn last_insert_rowid(storage: &dyn Storage) -> Result<i64, DomainError> {
    storage
        .query_scalar("SELECT CAST(last_insert_rowid() AS TEXT)", &[])
        .map_err(storage_err)?
        .ok_or_else(|| DomainError::Storage("last_insert_rowid() returned NULL".to_string()))?
        .parse::<i64>()
        .map_err(|e| DomainError::Storage(format!("last_insert_rowid: {e}")))
}

// ---------------------------------------------------------------------------
// Consignments (R15 status lifecycle, R25 declarant workspace, R27 retention)
// ---------------------------------------------------------------------------

/// A consignment row joined with its CBAM lifecycle columns.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredConsignment {
    /// The database row id.
    pub row_id: i64,
    /// The domain consignment payload.
    pub consignment: Consignment,
    /// CBAM status token (`LIABLE`, `DEFERRED`, ... — R15).
    pub status: String,
    /// Liability tag (`NONE` or `JOINT_AND_SEVERAL` — R46).
    pub liability_tag: String,
}

/// Insert one consignment record with its status token and workspace owner.
///
/// The retention horizon column is computed from
/// [`crate::provenance::retention_expiry`] (R27: Dec 31 of year + 4). The
/// vault envelope column stays NULL until the vault feature seals payloads.
///
/// # Errors
///
/// [`DomainError::InvalidImportDate`] when the consignment's import date was
/// never validated; [`DomainError::Storage`] on backend failure.
pub fn insert_consignment(
    storage: &dyn Storage,
    consignment: &Consignment,
    status: &str,
    eori: Option<&str>,
) -> Result<i64, DomainError> {
    // The retention horizon is derived from the declaration (import) year.
    let purge_after = provenance::retention_expiry(consignment.year()?).purge_after_iso;

    // Nullable columns are composed into the statement (the trait binds only
    // non-NULL strings); everything else is a fixed ?N parameter.
    let mut columns: Vec<&str> = vec![
        "cn_code",
        "net_mass_kg",
        "country_of_origin",
        "production_country",
        "installation_id",
        "import_date",
        "determination_basis",
        "status",
        "liability_tag",
        "retention_purge_after",
    ];
    let mut params: Vec<String> = vec![
        consignment.cn_code.trim().to_string(),
        consignment.net_mass_kg.to_string(),
        consignment.country_of_origin.trim().to_string(),
        consignment.production_country.trim().to_string(),
        consignment.installation_id.trim().to_string(),
        consignment.import_date.trim().to_string(),
        consignment.determination_basis.as_str().to_string(),
        status.trim().to_string(),
        "NONE".to_string(), // R46 default; joint-and-several is set via a later update
        purge_after,
    ];
    if let Some(price) = consignment.carbon_price_eur_per_tco2e {
        columns.push("carbon_price_eur_per_tco2e");
        params.push(price.to_string());
    }
    if let Some(country) = &consignment.carbon_price_country {
        columns.push("carbon_price_country");
        params.push(country.trim().to_string());
    }
    if let Some(eori) = eori {
        columns.push("declarant_eori");
        params.push(eori.trim().to_string());
    }
    let placeholders: Vec<String> = (1..=params.len()).map(|n| format!("?{n}")).collect();
    let sql = format!(
        "INSERT INTO consignments ({}) VALUES ({})",
        columns.join(", "),
        placeholders.join(", ")
    );
    let refs: Vec<&str> = params.iter().map(String::as_str).collect();
    storage.execute(&sql, &refs).map_err(storage_err)?;
    last_insert_rowid(storage)
}

/// List the consignment records of one declaration year, optionally scoped to
/// a declarant's EORI (R25 ICR mode), ordered by row id.
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure or a malformed stored row.
pub fn list_consignments(
    storage: &dyn Storage,
    year: i32,
    eori: Option<&str>,
) -> Result<Vec<StoredConsignment>, DomainError> {
    let mut sql = String::from(
        "SELECT id, cn_code, net_mass_kg, country_of_origin, production_country, \
         installation_id, import_date, determination_basis, \
         carbon_price_eur_per_tco2e, carbon_price_country, status, liability_tag \
         FROM consignments WHERE strftime('%Y', import_date) = ?1",
    );
    if eori.is_some() {
        sql.push_str(" AND declarant_eori = ?2");
    }
    sql.push_str(" ORDER BY id");

    let mut params: Vec<String> = vec![format!("{year:04}")];
    if let Some(eori) = eori {
        params.push(eori.trim().to_string());
    }
    let refs: Vec<&str> = params.iter().map(String::as_str).collect();
    let rows = storage.query_rows(&sql, &refs).map_err(storage_err)?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let parse_f64 = |col: usize| -> Result<f64, DomainError> {
            cell(&row, col)?
                .parse::<f64>()
                .map_err(|e| DomainError::Storage(format!("column {col}: {e}")))
        };
        let parse_i64 = |col: usize| -> Result<i64, DomainError> {
            cell(&row, col)?
                .parse::<i64>()
                .map_err(|e| DomainError::Storage(format!("column {col}: {e}")))
        };
        let optional_f64 = |col: usize| -> Result<Option<f64>, DomainError> {
            match row.get(col).and_then(|c| c.as_deref()) {
                None | Some("") => Ok(None),
                Some(text) => text
                    .parse::<f64>()
                    .map(Some)
                    .map_err(|e| DomainError::Storage(format!("column {col}: {e}"))),
            }
        };
        let basis_text = cell(&row, 7)?;
        let determination_basis: DeterminationBasis = basis_text.parse().map_err(|_| {
            DomainError::Storage(format!("unknown determination basis `{basis_text}`"))
        })?;
        let consignment = Consignment {
            cn_code: cell(&row, 1)?,
            net_mass_kg: parse_f64(2)?,
            country_of_origin: cell(&row, 3)?,
            production_country: cell(&row, 4)?,
            installation_id: cell(&row, 5)?,
            import_date: cell(&row, 6)?,
            determination_basis,
            carbon_price_eur_per_tco2e: optional_f64(8)?,
            carbon_price_country: row.get(9).and_then(|c| c.as_deref()).map(str::to_string),
        };
        out.push(StoredConsignment {
            row_id: parse_i64(0)?,
            consignment,
            status: cell(&row, 10)?,
            liability_tag: cell(&row, 11)?,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Audit trail (R10): append-only hash chain persisted in `audit_events`
// ---------------------------------------------------------------------------

/// Append one event to the persisted hash chain: the previous hash and
/// sequence number are read from the last stored row (genesis when empty) so
/// the DB row order IS the chain. Returns the new event's sequence number.
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn append_audit(
    storage: &dyn Storage,
    ts_utc: &str,
    actor: &str,
    action: &str,
    subject: &str,
    payload_hash: &str,
) -> Result<u64, DomainError> {
    let prev_hash = storage
        .query_scalar(
            "SELECT hash FROM audit_events ORDER BY seq DESC LIMIT 1",
            &[],
        )
        .map_err(storage_err)?
        .unwrap_or_else(|| GENESIS_PREV_HASH.to_string());
    let seq: i64 = storage
        .query_scalar(
            "SELECT CAST(COALESCE(MAX(seq), -1) AS TEXT) FROM audit_events",
            &[],
        )
        .map_err(storage_err)?
        .and_then(|s| s.parse().ok())
        .unwrap_or(-1)
        + 1;
    let seq_u = u64::try_from(seq)
        .map_err(|_| DomainError::Storage(format!("negative audit seq {seq}")))?;
    let hash = chain_hash(
        seq_u,
        ts_utc,
        actor,
        action,
        subject,
        payload_hash,
        &prev_hash,
    );
    let params: [String; 8] = [
        seq.to_string(),
        ts_utc.to_string(),
        actor.to_string(),
        action.to_string(),
        subject.to_string(),
        payload_hash.to_string(),
        prev_hash,
        hash,
    ];
    let refs: Vec<&str> = params.iter().map(String::as_str).collect();
    storage
        .execute(
            "INSERT INTO audit_events (seq, ts_utc, actor, action, subject, payload_hash, \
             prev_hash, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &refs,
        )
        .map_err(storage_err)?;
    Ok(seq_u)
}

/// Verify the whole persisted chain end-to-end by recomputing every link via
/// [`chain_hash`] (R10). The empty chain verifies (nothing to tamper with).
///
/// # Errors
///
/// [`DomainError::ChainBroken`] at the first bad sequence number.
pub fn verify_audit(storage: &dyn Storage) -> Result<(), DomainError> {
    let rows = storage
        .query_rows(
            "SELECT seq, ts_utc, actor, action, subject, payload_hash, prev_hash, hash \
             FROM audit_events ORDER BY seq ASC",
            &[],
        )
        .map_err(storage_err)?;
    let mut prev = GENESIS_PREV_HASH.to_string();
    for row in rows {
        let seq: u64 = cell(&row, 0)?
            .parse()
            .map_err(|e| DomainError::Storage(format!("audit seq: {e}")))?;
        let ts = cell(&row, 1)?;
        let actor = cell(&row, 2)?;
        let action = cell(&row, 3)?;
        let subject = cell(&row, 4)?;
        let payload_hash = cell(&row, 5)?;
        let stored_prev = cell(&row, 6)?;
        let stored_hash = cell(&row, 7)?;
        let expected = chain_hash(seq, &ts, &actor, &action, &subject, &payload_hash, &prev);
        if stored_prev != prev || stored_hash != expected {
            return Err(DomainError::ChainBroken(seq));
        }
        prev = stored_hash;
    }
    Ok(())
}

/// The chain root: the last event's hash (`GENESIS_PREV_HASH` when empty).
/// Ships with a declaration so a verifier can prove non-tampering (R10).
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure — a failed read is never
/// masked as the genesis root, which would forge a clean provenance.
pub fn audit_root(storage: &dyn Storage) -> Result<String, DomainError> {
    storage
        .query_scalar(
            "SELECT hash FROM audit_events ORDER BY seq DESC LIMIT 1",
            &[],
        )
        .map_err(storage_err)
        .map(|hash| hash.unwrap_or_else(|| GENESIS_PREV_HASH.to_string()))
}

// ---------------------------------------------------------------------------
// Attachments (R16): the R16 gate fires in `new_attachment`; the store only
// persists its output — hash + metadata, never the document bytes
// ---------------------------------------------------------------------------

/// Persist one verified attachment record against a subject
/// (`consignment:<id>` / dossier id). Accepts nothing unverified by
/// construction: callers must build the record via
/// [`crate::provenance::new_attachment`], which enforces the R16 gate.
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn add_attachment(
    storage: &dyn Storage,
    attachment: &Attachment,
    subject: &str,
) -> Result<(), DomainError> {
    let params: [String; 7] = [
        attachment.id.clone(),
        subject.to_string(),
        attachment.filename.clone(),
        attachment.mime_type.clone(),
        attachment.sha256.clone(),
        if attachment.verified_by_human { 1 } else { 0 }.to_string(),
        attachment.verification_note.clone(),
    ];
    let refs: Vec<&str> = params.iter().map(String::as_str).collect();
    storage
        .execute(
            "INSERT INTO attachments (id, subject, filename, mime_type, sha256, \
             verified_by_human, verification_note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &refs,
        )
        .map_err(storage_err)?;
    Ok(())
}

/// List the attachment records of one subject, oldest first.
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn list_attachments(
    storage: &dyn Storage,
    subject: &str,
) -> Result<Vec<Attachment>, DomainError> {
    let rows = storage
        .query_rows(
            "SELECT id, filename, mime_type, sha256, verified_by_human, verification_note \
             FROM attachments WHERE subject = ?1 ORDER BY created_utc, id",
            &[subject],
        )
        .map_err(storage_err)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(Attachment {
            id: cell(&row, 0)?,
            filename: cell(&row, 1)?,
            mime_type: cell(&row, 2)?,
            sha256: cell(&row, 3)?,
            verified_by_human: cell(&row, 4)? != "0",
            verification_note: cell(&row, 5)?,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Dossiers (R23/R35): per-class JSON upserts + the completeness flag
// ---------------------------------------------------------------------------

/// Map a dossier field name to its column (whitelist: no dynamic SQL from
/// user input).
fn dossier_column(field: &str) -> Option<&'static str> {
    match field {
        "energy" => Some("energy_json"),
        "materials" => Some("materials_json"),
        "production" => Some("production_json"),
        "balance" => Some("balance_json"),
        _ => None,
    }
}

/// Recompute the dossier completeness flag from the stored classes via
/// [`dossier::completeness`] (R23: all three classes present). The balance
/// table (R35) is a fourth, purely additive column that never affects
/// completeness.
fn refresh_dossier_complete(storage: &dyn Storage, consignment_id: i64) -> Result<(), DomainError> {
    let rows = storage
        .query_rows(
            "SELECT energy_json, materials_json, production_json FROM dossiers \
             WHERE consignment_id = ?1",
            &[&consignment_id.to_string()],
        )
        .map_err(storage_err)?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(()); // nothing stored yet — nothing to flag
    };
    let energy: Option<EnergyRecord> = row
        .first()
        .and_then(|c| c.as_deref())
        .and_then(|json| serde_json::from_str(json).ok());
    let materials: Vec<MaterialRecord> = row
        .get(1)
        .and_then(|c| c.as_deref())
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    let production: Vec<ProductionRecord> = row
        .get(2)
        .and_then(|c| c.as_deref())
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    // Completeness is a function of class presence only, so a placeholder
    // consignment keeps the domain function authoritative (never re-derive).
    let dossier = Dossier::new(Consignment {
        cn_code: String::new(),
        net_mass_kg: 0.0,
        country_of_origin: String::new(),
        production_country: String::new(),
        installation_id: String::new(),
        import_date: "1970-01-01".to_string(),
        determination_basis: crate::domain::types::DeterminationBasis::Default,
        carbon_price_eur_per_tco2e: None,
        carbon_price_country: None,
    })
    .with_materials(materials)
    .with_production(production);
    let dossier = match energy {
        Some(energy) => dossier.with_energy(energy),
        None => dossier,
    };
    let complete = if dossier::completeness(&dossier).complete {
        1
    } else {
        0
    };
    storage
        .execute(
            "UPDATE dossiers SET complete = ?1, updated_utc = \
             strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE consignment_id = ?2",
            &[&complete.to_string(), &consignment_id.to_string()],
        )
        .map_err(storage_err)?;
    Ok(())
}

/// Insert or update one dossier document class for a consignment.
///
/// `field` must be one of `energy`, `materials`, `production`, `balance`;
/// `json` is the class payload (see `migrations/0003_records.sql`). After the
/// write the `complete` flag is recomputed from the stored classes.
///
/// # Errors
///
/// [`DomainError::Storage`] for an unknown field name or backend failure.
pub fn upsert_dossier(
    storage: &dyn Storage,
    consignment_id: i64,
    field: &str,
    json: &str,
) -> Result<(), DomainError> {
    let column = dossier_column(field).ok_or_else(|| {
        DomainError::Storage(format!(
            "unknown dossier field `{field}`: expected energy, materials, production or balance"
        ))
    })?;
    let id = consignment_id.to_string();
    let existing = storage
        .query_scalar("SELECT id FROM dossiers WHERE consignment_id = ?1", &[&id])
        .map_err(storage_err)?;
    if existing.is_some() {
        let sql = format!(
            "UPDATE dossiers SET {column} = ?1, updated_utc = \
             strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE consignment_id = ?2"
        );
        storage.execute(&sql, &[json, &id]).map_err(storage_err)?;
    } else {
        // Row id doubles as the dossier identifier (one dossier per
        // consignment); `payload_sealed` stays NULL until the vault is on.
        let dossier_id = format!("dossier-{consignment_id}");
        let sql =
            format!("INSERT INTO dossiers (id, consignment_id, {column}) VALUES (?1, ?2, ?3)");
        storage
            .execute(&sql, &[&dossier_id, &id, json])
            .map_err(storage_err)?;
    }
    refresh_dossier_complete(storage, consignment_id)
}

/// The stored dossier: `(complete, payload)` where the payload object carries
/// up to the four class keys (`energy`, `materials`, `production`, `balance`),
/// `null` for classes never written.
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure or malformed stored JSON.
pub fn get_dossier(
    storage: &dyn Storage,
    consignment_id: i64,
) -> Result<Option<(bool, serde_json::Value)>, DomainError> {
    let rows = storage
        .query_rows(
            "SELECT energy_json, materials_json, production_json, balance_json, complete \
             FROM dossiers WHERE consignment_id = ?1",
            &[&consignment_id.to_string()],
        )
        .map_err(storage_err)?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let parse = |col: usize| -> Result<serde_json::Value, DomainError> {
        match row.get(col).and_then(|c| c.as_deref()) {
            None | Some("") => Ok(serde_json::Value::Null),
            Some(json) => serde_json::from_str(json)
                .map_err(|e| DomainError::Storage(format!("dossier column {col}: {e}"))),
        }
    };
    let payload = serde_json::json!({
        "energy": parse(0)?,
        "materials": parse(1)?,
        "production": parse(2)?,
        "balance": parse(3)?,
    });
    let complete = cell(&row, 4)? != "0";
    Ok(Some((complete, payload)))
}

// ---------------------------------------------------------------------------
// Certificate events (R24): kind-scoped, year-scoped sums
// ---------------------------------------------------------------------------

/// Record one certificate event (`PURCHASED`, `CANCELLED`, `SURRENDERED`,
/// `BUYBACK_REQUESTED`) and return its row id.
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn add_certificate_event(
    storage: &dyn Storage,
    kind: &str,
    tco2e: f64,
    price_eur: Option<f64>,
    event_date: &str,
) -> Result<i64, DomainError> {
    match price_eur {
        Some(price) => {
            let params: [String; 4] = [
                kind.to_string(),
                tco2e.to_string(),
                price.to_string(),
                event_date.to_string(),
            ];
            let refs: Vec<&str> = params.iter().map(String::as_str).collect();
            storage
                .execute(
                    "INSERT INTO certificate_events (kind, tco2e, price_eur, event_date) \
                     VALUES (?1, ?2, ?3, ?4)",
                    &refs,
                )
                .map_err(storage_err)?;
        }
        None => {
            let params: [String; 3] = [kind.to_string(), tco2e.to_string(), event_date.to_string()];
            let refs: Vec<&str> = params.iter().map(String::as_str).collect();
            storage
                .execute(
                    "INSERT INTO certificate_events (kind, tco2e, price_eur, event_date) \
                     VALUES (?1, ?2, NULL, ?3)",
                    &refs,
                )
                .map_err(storage_err)?;
        }
    }
    last_insert_rowid(storage)
}

/// The year's certificate position: `(purchased, cancelled, surrendered)` in
/// tCO2e, summed per kind over events dated inside the calendar year (R24).
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn certificate_position(
    storage: &dyn Storage,
    year: i32,
) -> Result<(f64, f64, f64), DomainError> {
    let sum = |kind: &str| -> Result<f64, DomainError> {
        storage
            .query_scalar(
                "SELECT CAST(COALESCE(SUM(tco2e), 0) AS TEXT) FROM certificate_events \
                 WHERE kind = ?2 AND strftime('%Y', event_date) = ?1",
                &[&format!("{year:04}"), kind],
            )
            .map_err(storage_err)?
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| DomainError::Storage("non-numeric certificate sum".to_string()))
    };
    Ok((sum("PURCHASED")?, sum("CANCELLED")?, sum("SURRENDERED")?))
}

// ---------------------------------------------------------------------------
// Authorised-declarant status (R42) and guarantee hooks
// ---------------------------------------------------------------------------

/// Store the authorised-declarant status for an EORI (R42: `ACTIVE`,
/// `SUSPENDED`, `REVOKED` — the CHECK constraint is the authority).
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn set_authorisation(
    storage: &dyn Storage,
    eori: &str,
    status: &str,
) -> Result<(), DomainError> {
    storage
        .execute(
            "INSERT INTO authorisation_status (eori, status) VALUES (?1, ?2) \
             ON CONFLICT(eori) DO UPDATE SET status = ?2",
            &[eori, status],
        )
        .map_err(storage_err)?;
    Ok(())
}

/// The stored authorised-declarant status for an EORI, if any.
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn get_authorisation(storage: &dyn Storage, eori: &str) -> Result<Option<String>, DomainError> {
    storage
        .query_scalar(
            "SELECT status FROM authorisation_status WHERE eori = ?1",
            &[eori],
        )
        .map_err(storage_err)
}

// ---------------------------------------------------------------------------
// ETS price cache (R7/R14): the single `id = 1` row
// ---------------------------------------------------------------------------

/// Upsert the ETS price cache (the table admits exactly one row, `id = 1`):
/// price, observation date, manual-entry and staleness flags.
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn set_price(
    storage: &dyn Storage,
    eur: f64,
    as_of: &str,
    manual: bool,
    stale: bool,
) -> Result<(), DomainError> {
    let params: [String; 4] = [
        eur.to_string(),
        as_of.to_string(),
        if manual { 1 } else { 0 }.to_string(),
        if stale { 1 } else { 0 }.to_string(),
    ];
    let refs: Vec<&str> = params.iter().map(String::as_str).collect();
    storage
        .execute(
            "INSERT INTO ets_price_cache (id, eur_per_tco2e, as_of_iso, manual, stale) \
             VALUES (1, ?1, ?2, ?3, ?4) \
             ON CONFLICT(id) DO UPDATE SET eur_per_tco2e = ?1, as_of_iso = ?2, \
             manual = ?3, stale = ?4",
            &refs,
        )
        .map_err(storage_err)?;
    Ok(())
}

/// The cached price: `(eur_per_tco2e, as_of_iso, manual, stale)`, or `None`
/// before the first sync or manual entry (R7: projections surface "no price"
/// instead of blocking).
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn get_price(storage: &dyn Storage) -> Result<Option<(f64, String, bool, bool)>, DomainError> {
    let rows = storage
        .query_rows(
            "SELECT eur_per_tco2e, as_of_iso, manual, stale FROM ets_price_cache WHERE id = 1",
            &[],
        )
        .map_err(storage_err)?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let eur: f64 = cell(&row, 0)?
        .parse()
        .map_err(|e| DomainError::Storage(format!("cached price: {e}")))?;
    Ok(Some((
        eur,
        cell(&row, 1)?,
        cell(&row, 2)? != "0",
        cell(&row, 3)? != "0",
    )))
}

// ---------------------------------------------------------------------------
// Data-request outbox (R11/R36)
// ---------------------------------------------------------------------------

/// One queued (or drained) data request row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredDataRequest {
    /// Request identifier.
    pub id: String,
    /// Request locale (`en`, `zh-CN`).
    pub locale: String,
    /// The mill/supplier the request goes to.
    pub recipient: String,
    /// The CN codes covered, as a JSON array string.
    pub cn_codes_json: String,
    /// Outbox state: still queued (unsent).
    pub queued: bool,
}

/// Queue one localized data request for a supplier (R11); the row starts
/// queued and drains via [`mark_sent`].
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn add_data_request(
    storage: &dyn Storage,
    id: &str,
    locale: &str,
    recipient: &str,
    cn_codes_json: &str,
) -> Result<(), DomainError> {
    storage
        .execute(
            "INSERT INTO data_requests (id, locale, recipient, cn_codes) VALUES (?1, ?2, ?3, ?4)",
            &[id, locale, recipient, cn_codes_json],
        )
        .map_err(storage_err)?;
    Ok(())
}

/// Every still-queued request (the offline outbox; R22: nothing blocks).
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn queued_requests(storage: &dyn Storage) -> Result<Vec<StoredDataRequest>, DomainError> {
    let rows = storage
        .query_rows(
            "SELECT id, locale, recipient, cn_codes, queued FROM data_requests \
             WHERE queued = 1 ORDER BY created_utc, id",
            &[],
        )
        .map_err(storage_err)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(StoredDataRequest {
            id: cell(&row, 0)?,
            locale: cell(&row, 1)?,
            recipient: cell(&row, 2)?,
            cn_codes_json: cell(&row, 3)?,
            queued: cell(&row, 4)? != "0",
        });
    }
    Ok(out)
}

/// Mark one request as sent (drains it from the outbox).
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn mark_sent(storage: &dyn Storage, id: &str) -> Result<(), DomainError> {
    storage
        .execute("UPDATE data_requests SET queued = 0 WHERE id = ?1", &[id])
        .map_err(storage_err)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Declarations (R9/R30/R10)
// ---------------------------------------------------------------------------

/// Save a declaration-ready file with its schema version and the audit chain
/// root at submission (R9/R30/R10). Re-saving a declaration id replaces the
/// stored file (amendments, R34) — the submitted-era chain root is preserved
/// in the replaced row only, so amendments mint a NEW id.
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn save_declaration(
    storage: &dyn Storage,
    id: &str,
    year: i32,
    schema_version: &str,
    file_json: &str,
    chain_root: &str,
) -> Result<(), DomainError> {
    let year_str = year.to_string();
    storage
        .execute(
            "INSERT OR REPLACE INTO declarations (id, declaration_year, schema_version, \
             file_json, chain_root) VALUES (?1, ?2, ?3, ?4, ?5)",
            &[id, &year_str, schema_version, file_json, chain_root],
        )
        .map_err(storage_err)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Role selection (R47): settings table, key `role_selection`
// ---------------------------------------------------------------------------

const ROLE_SETTING_KEY: &str = "role_selection";

/// Persist the serialized role selection (`roles::persist` output) to the
/// settings table (R47: stored locally, resettable).
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn set_role(storage: &dyn Storage, role_json: &str) -> Result<(), DomainError> {
    storage
        .execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = \
             strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            &[ROLE_SETTING_KEY, role_json],
        )
        .map_err(storage_err)?;
    Ok(())
}

/// The stored role selection JSON, or `None` before the first run (R47).
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn get_role(storage: &dyn Storage) -> Result<Option<String>, DomainError> {
    storage
        .query_scalar(
            "SELECT value FROM settings WHERE key = ?1",
            &[ROLE_SETTING_KEY],
        )
        .map_err(storage_err)
}

/// Delete the stored role selection (the settings reset, R47).
///
/// # Errors
///
/// [`DomainError::Storage`] on backend failure.
pub fn clear_role(storage: &dyn Storage) -> Result<(), DomainError> {
    storage
        .execute("DELETE FROM settings WHERE key = ?1", &[ROLE_SETTING_KEY])
        .map_err(storage_err)?;
    Ok(())
}

/// Convenience digest for audit payload records — a named call into
/// [`sha256_hex`] so callers do not reach into the provenance module for the
/// common case.
pub fn payload_hash(payload_json: &str) -> String {
    sha256_hex(payload_json.as_bytes())
}
