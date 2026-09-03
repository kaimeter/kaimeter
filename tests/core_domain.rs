// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Integration tests for the `core` domain module.
//!
//! These tests are the executable specification: written FIRST (RED), they
//! pin the regulatory mark-up schedule (2026 +10%, 2027 +20%, 2028+ +30%,
//! fertilisers +1%), unit normalization round-trips, seeded lookups, and
//! dossier completeness flags. Implementation follows to turn them GREEN.

use std::collections::BTreeMap;
use std::path::Path;

use kaimeter_core::db::{SqliteStorage, Storage};
use kaimeter_core::domain::{
    errors::DomainError,
    lookup::Lookup,
    markups::{self, MarkupYear},
    types::{
        CnCode, Consignment, DefaultValue, DeterminationBasis, Dossier, DossierClass, EnergyRecord,
        MaterialRecord, ProductionRecord, Sector,
    },
    units,
};
use kaimeter_core::i18n::I18n;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Open a scratch SQLite database, run migrations, and build a [`Lookup`]
/// over the seeded tables.
fn seeded(tag: &str) -> (SqliteStorage, Lookup) {
    let dir = std::env::temp_dir().join(format!("kaimeter-core-domain-it-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let storage = SqliteStorage::open(&dir.join("test.db")).expect("open");
    storage.migrate().expect("migrate");
    let lookup = Lookup::from_storage(&storage).expect("lookup from storage");
    (storage, lookup)
}

/// Build a `DefaultValue` for CN 73181500 (steel fasteners, route `EF`) with
/// the given emission intensities and an empty mark-up map.
fn steel_default(direct: f64, indirect: f64) -> DefaultValue {
    DefaultValue {
        cn_code: CnCode::new("73181500", "hex nuts of iron or steel", Sector::Steel)
            .expect("valid cn"),
        production_route: "EF".to_string(),
        direct_tco2e_per_t: direct,
        indirect_tco2e_per_t: indirect,
        markups: BTreeMap::new(),
    }
}

/// A minimal valid consignment for dossier tests.
fn sample_consignment() -> Consignment {
    Consignment {
        cn_code: "73181500".to_string(),
        net_mass_kg: 1_000.0,
        country_of_origin: "CN".to_string(),
        production_country: "DE".to_string(),
        installation_id: "INST-DE-001".to_string(),
        import_date: "2026-03-15".to_string(),
        determination_basis: DeterminationBasis::Default,
        carbon_price_eur_per_tco2e: None,
        carbon_price_country: None,
    }
}

// ---------------------------------------------------------------------------
// 1. Unit normalization (kWh <-> MWh, kg <-> tonnes)
// ---------------------------------------------------------------------------

#[test]
fn kwh_to_mwh_and_back_is_lossless() {
    assert_eq!(units::kwh_to_mwh(2_500.0), 2.5);
    assert_eq!(units::mwh_to_kwh(2.5), 2_500.0);
    assert_eq!(units::KWH_PER_MWH, 1000.0);
    assert_eq!(units::KG_PER_TONNE, 1000.0);
}

#[test]
fn kg_to_tonnes_and_back_is_lossless() {
    assert_eq!(units::kg_to_tonnes(1_234.0), 1.234);
    assert_eq!(units::tonnes_to_kg(1.234), 1_234.0);
}

#[test]
fn unit_round_trip_sample_never_loses_precision() {
    for kwh in [0.0, 1.0, 7.0, 999.0, 123_456.0, 1e9] {
        assert_eq!(units::mwh_to_kwh(units::kwh_to_mwh(kwh)), kwh, "kwh {kwh}");
    }
    for kg in [0.0, 0.5, 3.25, 987_654.0, 1e9] {
        assert_eq!(units::tonnes_to_kg(units::kg_to_tonnes(kg)), kg, "kg {kg}");
    }
}

#[test]
fn energy_record_normalizes_kwh_and_mwh() {
    let from_kwh = EnergyRecord::from_kwh(4_200.0).expect("valid energy");
    assert!((from_kwh.mwh() - 4.2).abs() < 1e-9);
    assert_eq!(from_kwh.kwh(), 4_200.0);

    let from_mwh = EnergyRecord::from_mwh(4.2).expect("valid energy");
    assert_eq!(from_mwh.kwh(), 4_200.0);
    assert_eq!(
        from_mwh, from_kwh,
        "both units normalize to the same record"
    );
}

#[test]
fn negative_energy_is_rejected() {
    assert!(matches!(
        EnergyRecord::from_kwh(-1.0),
        Err(DomainError::NegativeEnergy(_))
    ));
    assert!(matches!(
        EnergyRecord::from_mwh(-0.001),
        Err(DomainError::NegativeEnergy(_))
    ));
}

// ---------------------------------------------------------------------------
// 2. Domain type invariants
// ---------------------------------------------------------------------------

#[test]
fn cn_code_must_be_eight_digits() {
    let cn = CnCode::new("73181500", "hex nuts of iron or steel", Sector::Steel).expect("valid");
    assert_eq!(cn.code(), "73181500");
    assert_eq!(cn.sector(), Sector::Steel);

    assert!(matches!(
        CnCode::new("731815", "too short", Sector::Steel),
        Err(DomainError::InvalidCnCode(_))
    ));
    assert!(matches!(
        CnCode::new("73181A00", "not all digits", Sector::Steel),
        Err(DomainError::InvalidCnCode(_))
    ));
}

#[test]
fn consignment_validation_rejects_bad_data() {
    assert!(sample_consignment().validate().is_ok());

    let mut bad = sample_consignment();
    bad.net_mass_kg = -5.0;
    assert!(matches!(bad.validate(), Err(DomainError::NegativeMass(_))));

    let mut bad = sample_consignment();
    bad.import_date = "15/03/2026".to_string();
    assert!(matches!(
        bad.validate(),
        Err(DomainError::InvalidImportDate(_))
    ));
}

#[test]
fn consignment_year_parses_import_date() {
    assert_eq!(sample_consignment().year().expect("year"), 2026);
}

// ---------------------------------------------------------------------------
// 3. Phased mark-up table — REGULATORY PINS
// ---------------------------------------------------------------------------

/// Regression pin: if the 2026 mark-up is not +10%, this test FAILS.
#[test]
fn regression_markup_2026_is_exactly_10_percent() {
    assert_eq!(markups::markup_percent(2026, &Sector::Steel).unwrap(), 10.0);
    assert_eq!(
        markups::markup_percent(2026, &Sector::Aluminium).unwrap(),
        10.0
    );
    assert!(
        (markups::markup_factor(2026, &Sector::Steel).unwrap() - 1.10).abs() < 1e-12,
        "2026 factor must be 1.10"
    );

    let dv = steel_default(2.0, 1.0);
    let out = markups::apply(&dv, 2026).expect("apply 2026");
    assert!((out.direct_tco2e_per_t - 2.2).abs() < 1e-9);
    assert!((out.indirect_tco2e_per_t - 1.1).abs() < 1e-9);
}

/// Pins the full schedule exactly as stated:
/// 2026 +10%, 2027 +20%, 2028+ +30%, fertilisers +1% (all years).
#[test]
fn markup_schedule_is_pinned_exactly() {
    for sector in [
        Sector::Steel,
        Sector::Aluminium,
        Sector::Cement,
        Sector::Hydrogen,
    ] {
        assert_eq!(markups::markup_percent(2026, &sector).unwrap(), 10.0);
        assert_eq!(markups::markup_percent(2027, &sector).unwrap(), 20.0);
        assert_eq!(markups::markup_percent(2028, &sector).unwrap(), 30.0);
        assert_eq!(
            markups::markup_percent(2030, &sector).unwrap(),
            30.0,
            "2028+ stays +30%"
        );
    }
    for year in [2026, 2027, 2028, 2035] {
        assert_eq!(
            markups::markup_percent(year, &Sector::Fertilisers).unwrap(),
            1.0,
            "fertilisers are +1% in every year"
        );
    }
}

#[test]
fn markup_apply_uses_fertiliser_rate_and_keeps_markups_map() {
    let fert = DefaultValue {
        cn_code: CnCode::new(
            "31021000",
            "urea, whether or not in aqueous solution",
            Sector::Fertilisers,
        )
        .expect("valid cn"),
        production_route: "NATURAL_GAS".to_string(),
        direct_tco2e_per_t: 1.0,
        indirect_tco2e_per_t: 0.5,
        markups: BTreeMap::new(),
    };
    let out = markups::apply(&fert, 2026).expect("apply");
    assert!((out.direct_tco2e_per_t - 1.01).abs() < 1e-9);
    assert!((out.indirect_tco2e_per_t - 0.505).abs() < 1e-9);
    assert_eq!(out.production_route, "NATURAL_GAS");
}

#[test]
fn markup_before_2026_is_rejected() {
    assert!(matches!(
        markups::markup_percent(2025, &Sector::Steel),
        Err(DomainError::MarkupYearOutOfRange(2025))
    ));
    assert!(markups::apply(&steel_default(1.0, 1.0), 2020).is_err());
}

#[test]
fn markup_year_bucket_maps_calendar_years() {
    assert_eq!(
        MarkupYear::from_calendar_year(2026),
        Some(MarkupYear::Y2026)
    );
    assert_eq!(
        MarkupYear::from_calendar_year(2027),
        Some(MarkupYear::Y2027)
    );
    assert_eq!(
        MarkupYear::from_calendar_year(2028),
        Some(MarkupYear::Y2028Plus)
    );
    assert_eq!(
        MarkupYear::from_calendar_year(2099),
        Some(MarkupYear::Y2028Plus)
    );
    assert_eq!(MarkupYear::from_calendar_year(2025), None);
}

// ---------------------------------------------------------------------------
// 4. Seeded tables -> in-memory lookup
// ---------------------------------------------------------------------------

#[test]
fn migration_0002_creates_and_seeds_tables() {
    let (storage, lookup) = seeded("seed");
    assert_eq!(storage.schema_version().expect("version"), 3);

    let codes = storage
        .query_rows("SELECT code FROM cn_codes ORDER BY code", &[])
        .expect("select codes");
    assert_eq!(codes.len(), 3, "three placeholder CN codes seeded");

    let defaults = storage
        .query_rows(
            "SELECT cn_code, production_route FROM default_values ORDER BY cn_code",
            &[],
        )
        .expect("select defaults");
    assert_eq!(defaults.len(), 4, "EF; PRIMARY+RECYCLED; NATURAL_GAS");

    let installs = storage
        .query_rows("SELECT id FROM installations ORDER BY id", &[])
        .expect("select installs");
    assert_eq!(
        installs.len(),
        3,
        "two seeded + UNMAPPED (migration 0003 FK target for bulk imports)"
    );

    // Lookup exposes the seeded reference data in memory.
    assert!(lookup.cn_code("73181500").is_some());
    assert!(lookup.cn_code("99999999").is_none());
    assert!(lookup.installation("INST-DE-001").is_some());
}

#[test]
fn lookup_resolves_defaults_by_route() {
    let (_, lookup) = seeded("route");

    let ef = lookup
        .default_for_route("73181500", "EF")
        .expect("steel EF default");
    assert_eq!(ef.production_route, "EF");
    assert_eq!(ef.cn_code.sector(), Sector::Steel);
    assert_eq!(ef.direct_tco2e_per_t, 0.0, "placeholder seed value");
    assert_eq!(ef.indirect_tco2e_per_t, 0.0, "placeholder seed value");

    // Aluminium has two seeded routes.
    assert!(lookup.default_for_route("76041010", "PRIMARY").is_ok());
    assert!(lookup.default_for_route("76041010", "RECYCLED").is_ok());
}

#[test]
fn lookup_seed_carries_regulatory_markup_percentages() {
    let (_, lookup) = seeded("markups");

    let ef = lookup.default_for_route("73181500", "EF").expect("steel");
    assert_eq!(ef.markups.get(&MarkupYear::Y2026), Some(&10.0));
    assert_eq!(ef.markups.get(&MarkupYear::Y2027), Some(&20.0));
    assert_eq!(ef.markups.get(&MarkupYear::Y2028Plus), Some(&30.0));

    let fert = lookup
        .default_for_route("31021000", "NATURAL_GAS")
        .expect("fertiliser");
    assert_eq!(fert.markups.get(&MarkupYear::Y2026), Some(&1.0));
    assert_eq!(fert.markups.get(&MarkupYear::Y2028Plus), Some(&1.0));
}

/// The mark-up columns in the seeded DB must agree with the authoritative
/// schedule in `markups.rs` — no drift between SQL snapshot and code.
#[test]
fn seeded_markup_columns_match_regulatory_schedule() {
    let (storage, _) = seeded("consistency");
    let rows = storage
        .query_rows(
            "SELECT d.cn_code, c.sector, d.markup_2026_percent, d.markup_2027_percent, \
             d.markup_2028_percent FROM default_values d JOIN cn_codes c ON c.code = d.cn_code",
            &[],
        )
        .expect("select");
    assert!(!rows.is_empty());
    for row in rows {
        let sector = Sector::parse(row[1].as_deref().expect("sector")).expect("sector");
        for (col, year) in [(2, 2026), (3, 2027), (4, 2028)] {
            let db_pct: f64 = row[col]
                .as_deref()
                .expect("markup col")
                .parse()
                .expect("markup float");
            let reg_pct = markups::markup_percent(year, &sector).expect("schedule");
            assert!(
                (db_pct - reg_pct).abs() < 1e-9,
                "db {db_pct} != regulatory {reg_pct} for {sector:?} {year}"
            );
        }
    }
}

#[test]
fn lookup_missing_route_and_cn_are_graceful_errors() {
    let (_, lookup) = seeded("graceful");

    assert!(matches!(
        lookup.default_for_route("73181500", "BLAST_FURNACE"),
        Err(DomainError::NoDefaultForRoute { cn, route })
            if cn == "73181500" && route == "BLAST_FURNACE"
    ));
    assert!(matches!(
        lookup.default_for_route("99999999", "EF"),
        Err(DomainError::NoDefaultForCnCode(c)) if c == "99999999"
    ));
    assert!(lookup.defaults_for_cn("76041010").len() == 2);
    assert!(lookup.defaults_for_cn("99999999").is_empty());
}

// ---------------------------------------------------------------------------
// 5. Dossier completeness (three document classes, per R23)
// ---------------------------------------------------------------------------

#[test]
fn dossier_missing_energy_class_is_flagged() {
    let dossier = Dossier::new(sample_consignment())
        .with_materials(vec![MaterialRecord {
            cn_code: "73181500".to_string(),
            net_mass_kg: 1_000.0,
            production_route: Some("EF".to_string()),
        }])
        .with_production(vec![ProductionRecord {
            installation_id: "INST-DE-001".to_string(),
            production_route: "EF".to_string(),
        }]);

    let c = dossier.completeness();
    assert!(!c.complete, "missing the energy/fuel class must be flagged");
    assert_eq!(c.missing, vec![DossierClass::EnergyFuel]);
    assert!(!dossier.is_complete());
}

#[test]
fn dossier_fresh_is_missing_all_three_classes() {
    let dossier = Dossier::new(sample_consignment());
    let c = dossier.completeness();
    assert!(!c.complete);
    assert_eq!(
        c.missing,
        vec![
            DossierClass::EnergyFuel,
            DossierClass::Materials,
            DossierClass::Production
        ]
    );
}

#[test]
fn dossier_with_all_three_classes_is_complete() {
    let dossier = Dossier::new(sample_consignment())
        .with_energy(EnergyRecord::from_mwh(12.5).expect("valid"))
        .with_materials(vec![MaterialRecord {
            cn_code: "73181500".to_string(),
            net_mass_kg: 1_000.0,
            production_route: Some("EF".to_string()),
        }])
        .with_production(vec![ProductionRecord {
            installation_id: "INST-DE-001".to_string(),
            production_route: "EF".to_string(),
        }]);
    let c = dossier.completeness();
    assert!(c.complete);
    assert!(c.missing.is_empty());
    assert!(dossier.is_complete());
}

#[test]
fn dossier_empty_materials_count_as_missing() {
    let dossier = Dossier::new(sample_consignment())
        .with_energy(EnergyRecord::from_kwh(1_000.0).expect("valid"))
        .with_materials(Vec::new())
        .with_production(vec![ProductionRecord {
            installation_id: "INST-DE-001".to_string(),
            production_route: "EF".to_string(),
        }]);
    assert_eq!(
        dossier.completeness().missing,
        vec![DossierClass::Materials]
    );
}

// ---------------------------------------------------------------------------
// 6. Error messages are wired to i18n keys (en + zh-CN)
// ---------------------------------------------------------------------------

#[test]
fn every_domain_error_has_locale_keys_in_en_and_zh_cn() {
    let i18n = I18n::load(Path::new("locales")).expect("load repo locales");
    let samples: Vec<DomainError> = vec![
        DomainError::InvalidCnCode("123".to_string()),
        DomainError::NegativeMass(-1.0),
        DomainError::NegativeEnergy(-1.0),
        DomainError::InvalidImportDate("x".to_string()),
        DomainError::NoDefaultForCnCode("99999999".to_string()),
        DomainError::NoDefaultForRoute {
            cn: "73181500".to_string(),
            route: "BLAST_FURNACE".to_string(),
        },
        DomainError::MarkupYearOutOfRange(2025),
        DomainError::UnknownSector(" Widgets".to_string()),
        DomainError::Storage("boom".to_string()),
    ];
    for err in &samples {
        for locale in ["en", "zh-CN"] {
            let msg = i18n.t(locale, err.i18n_key());
            assert!(msg.is_ok(), "key {} missing in {locale}", err.i18n_key());
            assert!(!msg.expect("msg").trim().is_empty());
        }
    }
}

#[test]
fn determination_basis_round_trips_str() {
    assert_eq!(DeterminationBasis::Actual.as_str(), "ACTUAL");
    assert_eq!(DeterminationBasis::Default.as_str(), "DEFAULT");
    assert_eq!(
        "ACTUAL".parse::<DeterminationBasis>().expect("parse"),
        DeterminationBasis::Actual
    );
    assert!("MONTHLY".parse::<DeterminationBasis>().is_err());
}
