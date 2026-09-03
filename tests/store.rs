// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Persistence-store contract tests (`src/store.rs` over the `Storage` trait).
//!
//! Pins the schema usage contract of `migrations/0003_records.sql`:
//! consignment records with status/eori/retention (R15/R25/R27), the
//! hash-chained audit trail (R10), human-verified attachments (R16),
//! dossier completeness (R23), certificate positions (R24), role
//! persistence (R47), the ETS price cache (R7/R14), declaration files
//! (R9/R30), and the queued data-request outbox (R11/R36).

use kaimeter_core::db::{SqliteStorage, Storage};
use kaimeter_core::domain::types::{
    Consignment, DeterminationBasis, EnergyRecord, MaterialRecord, ProductionRecord,
};
use kaimeter_core::dossier;
use kaimeter_core::provenance::{self};
use kaimeter_core::roles::{self, Role, RoleSelection};
use kaimeter_core::store;

fn storage(tag: &str) -> SqliteStorage {
    let dir = std::env::temp_dir().join(format!("kaimeter-store-test-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let storage = SqliteStorage::open(&dir.join("test.db")).expect("open db");
    storage.migrate().expect("migrate");
    storage
}

fn consignment(cn: &str, kg: f64, date: &str) -> Consignment {
    Consignment {
        cn_code: cn.to_string(),
        net_mass_kg: kg,
        country_of_origin: "CN".to_string(),
        production_country: "DE".to_string(),
        installation_id: "INST-DE-001".to_string(),
        import_date: date.to_string(),
        determination_basis: DeterminationBasis::Default,
        carbon_price_eur_per_tco2e: None,
        carbon_price_country: None,
    }
}

// ---------------------------------------------------------------------------
// Consignment records (R15 status lifecycle, R25 declarant workspace, R27
// retention horizon)
// ---------------------------------------------------------------------------

#[test]
fn consignment_round_trip_with_status_eori_and_retention() {
    let storage = storage("round-trip");
    let c2026 = consignment("73181500", 1_000.0, "2026-03-15");
    let id = store::insert_consignment(&storage, &c2026, "LIABLE", Some("DE12345678"))
        .expect("insert 2026");
    assert!(id > 0, "row id returned");

    let c2027 = consignment("76041010", 2_000.0, "2027-06-01");
    let id27 = store::insert_consignment(&storage, &c2027, "DEFERRED", Some("FR12345678901"))
        .expect("insert 2027");

    // Year filter (R14 declaration year = import year).
    let listed = store::list_consignments(&storage, 2026, None).expect("list 2026");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].row_id, id);
    assert_eq!(listed[0].consignment, c2026);
    assert_eq!(listed[0].status, "LIABLE");
    assert_eq!(listed[0].liability_tag, "NONE", "default tag is NONE (R46)");
    // Serde contract: StoredConsignment is serializable for the API layer.
    serde_json::to_string(&listed[0]).expect("serialize");

    // EORI filter (R25 ICR mode: rows belong to the workspace owner).
    let mine = store::list_consignments(&storage, 2026, Some("DE12345678")).expect("eori filter");
    assert_eq!(mine.len(), 1);
    assert!(
        store::list_consignments(&storage, 2026, Some("FR999999999"))
            .expect("other eori")
            .is_empty()
    );

    // Retention column is set to Dec 31 of declaration_year + 4 (R27/Art 14).
    let purge = storage
        .query_scalar(
            "SELECT retention_purge_after FROM consignments WHERE id = ?1",
            &[&id.to_string()],
        )
        .expect("retention cell");
    assert_eq!(
        purge.as_deref(),
        Some(provenance::retention_expiry(2026).purge_after_iso.as_str()),
        "retention horizon = 2030-12-31 for a 2026 import"
    );
    assert_eq!(id27, id + 1, "row ids increment");
}

#[test]
fn consignment_insert_carries_optional_carbon_price_columns() {
    let storage = storage("carbon-price");
    let mut c = consignment("31021000", 500.0, "2026-05-20");
    c.carbon_price_eur_per_tco2e = Some(12.5);
    c.carbon_price_country = Some("GB".to_string());
    store::insert_consignment(&storage, &c, "LIABLE", None).expect("insert");
    let listed = store::list_consignments(&storage, 2026, None).expect("list");
    assert_eq!(listed[0].consignment.carbon_price_eur_per_tco2e, Some(12.5));
    assert_eq!(
        listed[0].consignment.carbon_price_country.as_deref(),
        Some("GB")
    );
}

// ---------------------------------------------------------------------------
// Audit trail (R10): append-only hash chain over the DB
// ---------------------------------------------------------------------------

#[test]
fn audit_chain_append_verify_root_and_tamper_detection() {
    let storage = storage("audit");
    // REGULATORY PIN (R10): the chain starts at the genesis prev-hash and
    // every appended link recomputes from the stored row.
    store::append_audit(
        &storage,
        "2026-09-01T10:00:00Z",
        "api",
        "consignment.created",
        "consignment:1",
        &provenance::sha256_hex(b"payload-1"),
    )
    .expect("append 1");
    store::append_audit(
        &storage,
        "2026-09-01T10:05:00Z",
        "api",
        "consignment.created",
        "consignment:2",
        &provenance::sha256_hex(b"payload-2"),
    )
    .expect("append 2");
    let third = store::append_audit(
        &storage,
        "2026-09-01T10:07:00Z",
        "mill:user",
        "extraction_confirmed",
        "consignment:2",
        &provenance::sha256_hex(b"payload-3"),
    )
    .expect("append 3");
    assert_eq!(third, 2, "sequence numbers are zero-based");

    store::verify_audit(&storage).expect("intact chain");

    // The root is the LAST event's hash, straight from the DB.
    let root = store::audit_root(&storage).expect("root");
    let last = storage
        .query_scalar(
            "SELECT hash FROM audit_events ORDER BY seq DESC LIMIT 1",
            &[],
        )
        .expect("last hash");
    assert_eq!(root, last.expect("some hash"));

    // Tamper with a mid-chain payload_hash: verification fails exactly there.
    storage
        .execute(
            "UPDATE audit_events SET payload_hash = ?1 WHERE seq = 1",
            &[&provenance::sha256_hex(b"tampered")],
        )
        .expect("tamper update");
    match store::verify_audit(&storage) {
        Err(kaimeter_core::domain::errors::DomainError::ChainBroken(seq)) => assert_eq!(seq, 1),
        other => panic!("expected ChainBroken(1), got {other:?}"),
    }
}

#[test]
fn audit_root_of_empty_chain_is_genesis() {
    let storage = storage("audit-genesis");
    assert_eq!(
        store::audit_root(&storage).expect("root"),
        provenance::GENESIS_PREV_HASH
    );
    store::verify_audit(&storage).expect("empty chain verifies");
}

// ---------------------------------------------------------------------------
// Attachments (R16): the human-verification gate, then metadata-only rows
// ---------------------------------------------------------------------------

#[test]
fn attachment_gate_blocks_unverified_and_round_trips_verified() {
    let storage = storage("attachments");
    // R16: without human verification nothing is retained — the gate lives in
    // `new_attachment` and the store only ever persists its output.
    let unverified = provenance::new_attachment(
        "att-1",
        "invoice.pdf",
        "application/pdf",
        b"bytes",
        false,
        "",
    );
    assert!(matches!(
        unverified,
        Err(kaimeter_core::domain::errors::DomainError::HumanVerificationRequired(_))
    ));

    let att = provenance::new_attachment(
        "att-2",
        "invoice.pdf",
        "application/pdf",
        b"document-bytes",
        true,
        "mass and date verified by operator",
    )
    .expect("verified attachment");
    store::add_attachment(&storage, &att, "consignment:1").expect("persist");

    let listed = store::list_attachments(&storage, "consignment:1").expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], att, "round-trips id/filename/mime/hash/note");
    assert_eq!(listed[0].sha256, provenance::sha256_hex(b"document-bytes"));
    assert!(store::list_attachments(&storage, "consignment:2")
        .expect("other subject")
        .is_empty());
}

// ---------------------------------------------------------------------------
// Dossiers (R23): per-class upserts + the completeness flag
// ---------------------------------------------------------------------------

#[test]
fn dossier_upsert_tracks_completeness_flag() {
    let storage = storage("dossier");
    let id = store::insert_consignment(
        &storage,
        &consignment("73181500", 100.0, "2026-02-01"),
        "LIABLE",
        None,
    )
    .expect("insert");

    assert!(
        store::get_dossier(&storage, id)
            .expect("get missing")
            .is_none(),
        "no dossier row before the first upsert"
    );

    store::upsert_dossier(&storage, id, "energy", r#"{"kwh":1200.0}"#).expect("energy");
    let (complete, doc) = store::get_dossier(&storage, id).expect("get").expect("row");
    assert!(!complete, "one class is not a complete dossier (R23)");
    assert_eq!(doc["energy"]["kwh"], 1200.0);

    store::upsert_dossier(
        &storage,
        id,
        "materials",
        r#"[{"cn_code":"73181500","net_mass_kg":10.0}]"#,
    )
    .expect("materials");
    let (complete, _) = store::get_dossier(&storage, id).expect("get").expect("row");
    assert!(!complete);

    store::upsert_dossier(
        &storage,
        id,
        "production",
        r#"[{"installation_id":"INST-DE-001","production_route":"EF"}]"#,
    )
    .expect("production");
    let (complete, doc) = store::get_dossier(&storage, id).expect("get").expect("row");
    assert!(complete, "all three classes present (R23)");
    assert_eq!(doc["materials"].as_array().map(Vec::len), Some(1));

    // The balance column (R35) stores without touching the completeness flag.
    store::upsert_dossier(
        &storage,
        id,
        "balance",
        r#"{"heat_flows":[],"waste_gas_flows":[]}"#,
    )
    .expect("balance");
    let (still_complete, doc) = store::get_dossier(&storage, id).expect("get").expect("row");
    assert!(still_complete);
    assert!(doc["balance"].is_object());

    // Rejected field names never reach SQL.
    assert!(store::upsert_dossier(&storage, id, "nickname", "{}").is_err());

    // The stored classes deserialize back into the domain dossier types and
    // agree with `dossier::completeness`.
    let energy: EnergyRecord = serde_json::from_value(doc["energy"].clone()).expect("energy");
    let materials: Vec<MaterialRecord> =
        serde_json::from_value(doc["materials"].clone()).expect("materials");
    let production: Vec<ProductionRecord> =
        serde_json::from_value(doc["production"].clone()).expect("production");
    let d = dossier::assemble(consignment("73181500", 100.0, "2026-02-01"))
        .with_energy(energy)
        .with_materials(materials)
        .with_production(production);
    assert!(dossier::completeness(&d).complete);
}

// ---------------------------------------------------------------------------
// Certificate events (R24): year-scoped SUM by kind
// ---------------------------------------------------------------------------

#[test]
fn certificate_position_sums_by_year() {
    let storage = storage("certs");
    store::add_certificate_event(&storage, "PURCHASED", 100.0, Some(80.0), "2027-02-10")
        .expect("purchase 1");
    store::add_certificate_event(&storage, "PURCHASED", 50.0, Some(81.0), "2027-03-01")
        .expect("purchase 2");
    store::add_certificate_event(&storage, "CANCELLED", 20.0, None, "2027-03-15").expect("cancel");
    store::add_certificate_event(&storage, "SURRENDERED", 60.0, None, "2027-09-30")
        .expect("surrender");
    // A 2026 purchase must not leak into the 2027 position.
    store::add_certificate_event(&storage, "PURCHASED", 999.0, None, "2026-12-31")
        .expect("2026 purchase");

    let (purchased, cancelled, surrendered) =
        store::certificate_position(&storage, 2027).expect("position");
    assert!((purchased - 150.0).abs() < 1e-9);
    assert!((cancelled - 20.0).abs() < 1e-9);
    assert!((surrendered - 60.0).abs() < 1e-9);

    let (p2026, _, _) = store::certificate_position(&storage, 2026).expect("2026");
    assert!((p2026 - 999.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Role selection (R47): persisted through the settings table
// ---------------------------------------------------------------------------

#[test]
fn role_selection_persists_through_settings() {
    let storage = storage("role");
    assert!(store::get_role(&storage).expect("empty role").is_none());

    let selection = RoleSelection::first_run(Role::Exporter);
    store::set_role(&storage, &roles::persist(&selection)).expect("persist role");
    let stored = store::get_role(&storage)
        .expect("stored role")
        .expect("some");
    let restored = roles::restore(&stored).expect("restore");
    assert_eq!(restored.active(), Role::Exporter);

    // A second write overwrites (roles stay resettable, R47).
    let mut updated = selection;
    updated.add_role(Role::Verifier);
    store::set_role(&storage, &roles::persist(&updated)).expect("overwrite role");
    let stored = store::get_role(&storage).expect("stored").expect("some");
    assert_eq!(roles::restore(&stored).expect("restore").roles().len(), 2);
}

// ---------------------------------------------------------------------------
// ETS price cache (R7/R14): single-row upsert
// ---------------------------------------------------------------------------

#[test]
fn price_cache_upsert_overwrites() {
    let storage = storage("price");
    assert!(store::get_price(&storage).expect("empty cache").is_none());

    store::set_price(&storage, 75.36, "2026-04-07", true, false).expect("seed price");
    let (eur, as_of, manual, stale) = store::get_price(&storage).expect("price").expect("entry");
    assert!((eur - 75.36).abs() < 1e-12);
    assert_eq!(as_of, "2026-04-07");
    assert!(manual, "manual entry flag (R7/R22)");
    assert!(!stale);

    // The id=1 row is updated in place, never duplicated.
    store::set_price(&storage, 80.0, "2027-01-04", false, true).expect("refresh");
    let (eur, as_of, manual, stale) = store::get_price(&storage).expect("price").expect("entry");
    assert!((eur - 80.0).abs() < 1e-12);
    assert_eq!(as_of, "2027-01-04");
    assert!(!manual);
    assert!(stale, "staleness flag survives the round trip");
    let rows = storage
        .query_scalar("SELECT CAST(COUNT(*) AS TEXT) FROM ets_price_cache", &[])
        .expect("count");
    assert_eq!(rows.as_deref(), Some("1"));
}

// ---------------------------------------------------------------------------
// Declarations (R9/R30): file + schema version + chain root at submission
// ---------------------------------------------------------------------------

#[test]
fn declaration_save_persists_file_and_root() {
    let storage = storage("declaration");
    store::append_audit(
        &storage,
        "2027-09-30T12:00:00Z",
        "api",
        "declaration.exported",
        "decl-2026",
        "p",
    )
    .expect("audit event");
    let root = store::audit_root(&storage).expect("root");
    store::save_declaration(
        &storage,
        "decl-2026",
        2026,
        "2027.1",
        r#"{"declaration_year":2026,"consignments":[]}"#,
        &root,
    )
    .expect("save");

    let joined = storage
        .query_scalar(
            "SELECT declaration_year || '|' || schema_version || '|' || chain_root \
             FROM declarations WHERE id = 'decl-2026'",
            &[],
        )
        .expect("row")
        .expect("declaration row");
    let mut parts = joined.split('|');
    assert_eq!(parts.next(), Some("2026"));
    assert_eq!(parts.next(), Some("2027.1"));
    let chain = parts.next().expect("chain root");
    assert_eq!(chain, root, "the audit root at submission is frozen (R10)");
}

// ---------------------------------------------------------------------------
// Authorisation status + data-request outbox (R42/R11/R36)
// ---------------------------------------------------------------------------

#[test]
fn authorisation_status_round_trips() {
    let storage = storage("auth");
    assert!(store::get_authorisation(&storage, "DE12345678")
        .expect("none")
        .is_none());
    store::set_authorisation(&storage, "DE12345678", "SUSPENDED").expect("set");
    assert_eq!(
        store::get_authorisation(&storage, "DE12345678").expect("get"),
        Some("SUSPENDED".to_string())
    );
    store::set_authorisation(&storage, "DE12345678", "ACTIVE").expect("update");
    assert_eq!(
        store::get_authorisation(&storage, "DE12345678").expect("get"),
        Some("ACTIVE".to_string())
    );
}

#[test]
fn data_requests_queue_and_drain() {
    let storage = storage("requests");
    store::add_data_request(&storage, "r1", "zh-CN", "某钢铁厂", r#"["73181500"]"#)
        .expect("queue 1");
    store::add_data_request(&storage, "r2", "en", "mill", r#"["76041010"]"#).expect("queue 2");

    let queued = store::queued_requests(&storage).expect("queued");
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[0].id, "r1");
    assert_eq!(queued[0].locale, "zh-CN");
    assert_eq!(queued[0].recipient, "某钢铁厂");
    assert_eq!(queued[0].cn_codes_json, r#"["73181500"]"#);
    assert!(queued[0].queued);

    store::mark_sent(&storage, "r1").expect("mark sent");
    let queued = store::queued_requests(&storage).expect("queued after drain");
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].id, "r2");
}
