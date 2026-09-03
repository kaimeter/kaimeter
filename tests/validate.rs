// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Contract tests for `core::validate` — R12: validate incoming mill data
//! (units, completeness, route plausibility). Bad upstream data flows
//! straight into a legally binding declaration, so these tests pin that
//! everything suspect is flagged and nothing is silently corrected.
//!
//! Written FIRST (RED); implementation follows to turn them GREEN.

use kaimeter_core::db::SqliteStorage;
use kaimeter_core::domain::errors::DomainError;
use kaimeter_core::domain::lookup::Lookup;
use kaimeter_core::domain::types::{Consignment, DeterminationBasis, Dossier, MaterialRecord};
use kaimeter_core::validate::{self, Severity, Unit, ValidationIssue};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Open a scratch SQLite database, run migrations, and build a [`Lookup`]
/// over the seeded reference tables (73181500 STEEL route EF, 76041010
/// ALUMINIUM routes PRIMARY+RECYCLED, 31021000 FERTILISERS route NATURAL_GAS).
fn seeded(tag: &str) -> Lookup {
    let dir = std::env::temp_dir().join(format!("kaimeter-validate-it-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let storage = SqliteStorage::open(&dir.join("test.db")).expect("open");
    storage.migrate().expect("migrate");
    Lookup::from_storage(&storage).expect("lookup from storage")
}

/// A clean consignment for a seeded CN code: valid by construction.
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

/// A materials record of `kg` of the seeded steel CN code.
fn material(kg: f64) -> MaterialRecord {
    MaterialRecord {
        cn_code: "73181500".to_string(),
        net_mass_kg: kg,
        production_route: None,
    }
}

/// True when an issue with `code` is present.
fn has(issues: &[ValidationIssue], code: &str) -> bool {
    issues.iter().any(|i| i.code == code)
}

/// The severity of the issue with `code` (panics in tests if absent).
fn severity_of(issues: &[ValidationIssue], code: &str) -> Severity {
    issues
        .iter()
        .find(|i| i.code == code)
        .map(|i| i.severity)
        .unwrap_or_else(|| panic!("issue {code} not found in {issues:?}"))
}

// ---------------------------------------------------------------------------
// 1. Unit conversion (R12: units must be normalized, never guessed)
// ---------------------------------------------------------------------------

/// R12: incoming quantities in mixed units must convert along exact
/// power-of-ten scalings (1000 kg per t, 1000 kWh per MWh); identity is
/// allowed; cross-family conversions (mass vs energy) and NaN/negative
/// quantities are rejected — never silently coerced into plausibility.
#[test]
fn unit_conversion_matrix_pinned() {
    // Mass: 1000 kg = 1 t; 2.5 t = 2500 kg (KG_PER_TONNE = 1000, exact).
    assert_eq!(
        validate::convert_units(1000.0, Unit::Kilograms, Unit::Tonnes).expect("kg -> t"),
        1.0
    );
    assert_eq!(
        validate::convert_units(2.5, Unit::Tonnes, Unit::Kilograms).expect("t -> kg"),
        2500.0
    );
    // Energy: 4200 kWh = 4.2 MWh; back again (KWH_PER_MWH = 1000, exact).
    assert_eq!(
        validate::convert_units(4200.0, Unit::KilowattHours, Unit::MegawattHours)
            .expect("kWh -> MWh"),
        4.2
    );
    assert_eq!(
        validate::convert_units(4.2, Unit::MegawattHours, Unit::KilowattHours).expect("MWh -> kWh"),
        4200.0
    );
    // Identity conversions are legal within each family.
    for (value, unit) in [
        (5.0, Unit::Kilograms),
        (5.0, Unit::Tonnes),
        (5.0, Unit::KilowattHours),
        (5.0, Unit::MegawattHours),
    ] {
        assert_eq!(
            validate::convert_units(value, unit, unit).expect("identity"),
            value,
            "{unit:?} -> {unit:?}"
        );
    }
    // Zero quantities are valid data (reported, not judged here).
    assert_eq!(
        validate::convert_units(0.0, Unit::Kilograms, Unit::Tonnes).expect("zero"),
        0.0
    );
    // Cross-family: mass and energy are incompatible — flagged, not coerced.
    for (from, to) in [
        (Unit::Kilograms, Unit::KilowattHours),
        (Unit::KilowattHours, Unit::Tonnes),
        (Unit::MegawattHours, Unit::Kilograms),
        (Unit::Tonnes, Unit::MegawattHours),
    ] {
        let result = validate::convert_units(1.0, from, to);
        assert!(
            matches!(&result, Err(DomainError::Storage(msg)) if msg.contains("incompatible unit families")),
            "{from:?} -> {to:?} must be an incompatible-family Storage error, got {result:?}"
        );
    }
    // NaN and negative quantities are rejected outright (R12: bad upstream
    // data is flagged, nothing silently corrected).
    assert!(
        validate::convert_units(f64::NAN, Unit::Kilograms, Unit::Tonnes).is_err(),
        "NaN must not convert"
    );
    assert!(
        validate::convert_units(-1.0, Unit::Kilograms, Unit::Tonnes).is_err(),
        "negative mass must not convert"
    );
    assert!(
        validate::convert_units(-0.5, Unit::KilowattHours, Unit::MegawattHours).is_err(),
        "negative energy must not convert"
    );
}

// ---------------------------------------------------------------------------
// 2. Consignment validation (R12: completeness + reference-data plausibility)
// ---------------------------------------------------------------------------

/// R12: a consignment on a seeded CN code with well-formed fields raises no
/// issues; unknown/malformed CN codes, missing carbon-price country,
/// pre-regime dates, and broken domain invariants are flagged at the right
/// severity.
#[test]
fn consignment_flags_unknown_cn_and_missing_country() {
    let lookup = seeded("consignment");

    // Clean consignment on a seeded CN code: no issues at all.
    let clean = sample_consignment();
    assert!(
        validate::validate_consignment(&clean, &lookup).is_empty(),
        "clean consignment must raise no issues"
    );
    // The other seeded CN codes are equally clean.
    let mut aluminium = sample_consignment();
    aluminium.cn_code = "76041010".to_string();
    assert!(validate::validate_consignment(&aluminium, &lookup).is_empty());

    // Unknown but well-formed CN code -> Error CN_UNKNOWN.
    let mut unknown = sample_consignment();
    unknown.cn_code = "99999999".to_string();
    let issues = validate::validate_consignment(&unknown, &lookup);
    assert!(has(&issues, "CN_UNKNOWN"), "got {issues:?}");
    assert_eq!(severity_of(&issues, "CN_UNKNOWN"), Severity::Error);

    // Malformed CN (not 8 digits) -> Error CN_FORMAT, and never a
    // misleading CN_UNKNOWN on top.
    let mut malformed = sample_consignment();
    malformed.cn_code = "731815".to_string();
    let issues = validate::validate_consignment(&malformed, &lookup);
    assert!(has(&issues, "CN_FORMAT"), "got {issues:?}");
    assert_eq!(severity_of(&issues, "CN_FORMAT"), Severity::Error);
    assert!(!has(&issues, "CN_UNKNOWN"));

    // Carbon price present without its country -> Warning (completeness).
    let mut priced = sample_consignment();
    priced.carbon_price_eur_per_tco2e = Some(2.0);
    let issues = validate::validate_consignment(&priced, &lookup);
    assert!(
        has(&issues, "CARBON_PRICE_COUNTRY_MISSING"),
        "got {issues:?}"
    );
    assert_eq!(
        severity_of(&issues, "CARBON_PRICE_COUNTRY_MISSING"),
        Severity::Warning
    );

    // The inverse is LEGAL: a country recorded with no price means a
    // zero-price import — no CARBON_PRICE_WITHOUT_COUNTRY issue may exist.
    let mut zero_price = sample_consignment();
    zero_price.carbon_price_country = Some("CN".to_string());
    let issues = validate::validate_consignment(&zero_price, &lookup);
    assert!(
        !has(&issues, "CARBON_PRICE_WITHOUT_COUNTRY"),
        "zero-price import is legal: {issues:?}"
    );
    assert!(issues.is_empty());

    // Import date before the definitive regime (2026) -> Warning
    // DATE_BEFORE_REGIME (Reg. (EU) 2023/956: definitive period starts
    // 2026; pure code has no clock, so no future-date check exists).
    let mut early = sample_consignment();
    early.import_date = "2024-01-01".to_string();
    let issues = validate::validate_consignment(&early, &lookup);
    assert!(has(&issues, "DATE_BEFORE_REGIME"), "got {issues:?}");
    assert_eq!(
        severity_of(&issues, "DATE_BEFORE_REGIME"),
        Severity::Warning
    );

    // Negative mass breaks a domain invariant -> Error NEGATIVE_MASS.
    let mut negative = sample_consignment();
    negative.net_mass_kg = -5.0;
    let issues = validate::validate_consignment(&negative, &lookup);
    assert!(has(&issues, "NEGATIVE_MASS"), "got {issues:?}");
    assert_eq!(severity_of(&issues, "NEGATIVE_MASS"), Severity::Error);

    // NaN mass is likewise non-finite -> Error NEGATIVE_MASS.
    let mut nan = sample_consignment();
    nan.net_mass_kg = f64::NAN;
    let issues = validate::validate_consignment(&nan, &lookup);
    assert!(has(&issues, "NEGATIVE_MASS"), "got {issues:?}");

    // Unparseable import date -> Error INVALID_DATE.
    let mut bad_date = sample_consignment();
    bad_date.import_date = "15/03/2026".to_string();
    let issues = validate::validate_consignment(&bad_date, &lookup);
    assert!(has(&issues, "INVALID_DATE"), "got {issues:?}");
    assert_eq!(severity_of(&issues, "INVALID_DATE"), Severity::Error);
}

// ---------------------------------------------------------------------------
// 3. Dossier mass balance (R12 plausibility; R16: the human verifies)
// ---------------------------------------------------------------------------

/// R12 + R16: recorded material inputs materially below the consignment
/// output mass are flagged (outputs cannot exceed inputs) — a Warning for
/// human verification, not a hard error, because the verifying human is the
/// author of the record. Negative material masses are Errors; duplicate CN
/// codes with conflicting production routes are flagged; a dossier with no
/// evidence at all is flagged.
#[test]
fn dossier_mass_balance_flags_input_below_output() {
    let consignment = sample_consignment(); // output: 1000 kg

    // Inputs (500 kg) below output (1000 kg) -> Warning MASS_BALANCE_INPUT_BELOW_OUTPUT.
    let short = Dossier::new(consignment.clone()).with_materials(vec![material(500.0)]);
    let issues = validate::validate_dossier_mass_balance(&short);
    assert!(
        has(&issues, "MASS_BALANCE_INPUT_BELOW_OUTPUT"),
        "got {issues:?}"
    );
    assert_eq!(
        severity_of(&issues, "MASS_BALANCE_INPUT_BELOW_OUTPUT"),
        Severity::Warning
    );

    // Inputs covering the output (600 + 400 = 1000 kg) raise no issue.
    let covered =
        Dossier::new(consignment.clone()).with_materials(vec![material(600.0), material(400.0)]);
    let issues = validate::validate_dossier_mass_balance(&covered);
    assert!(
        !has(&issues, "MASS_BALANCE_INPUT_BELOW_OUTPUT"),
        "balanced inputs must not warn: {issues:?}"
    );
    assert!(issues.is_empty(), "got {issues:?}");

    // A negative material mass is an Error, not a warning (R12).
    let mut negative = Dossier::new(consignment.clone());
    negative.materials = vec![material(-1.0)];
    let issues = validate::validate_dossier_mass_balance(&negative);
    assert!(has(&issues, "NEGATIVE_MASS"), "got {issues:?}");
    assert_eq!(severity_of(&issues, "NEGATIVE_MASS"), Severity::Error);

    // Duplicate CN codes with conflicting production routes -> Warning
    // CONFLICTING_ROUTES (route plausibility, R12).
    let conflicting = Dossier::new(consignment.clone()).with_materials(vec![
        MaterialRecord {
            cn_code: "76041010".to_string(),
            net_mass_kg: 500.0,
            production_route: Some("PRIMARY".to_string()),
        },
        MaterialRecord {
            cn_code: "76041010".to_string(),
            net_mass_kg: 500.0,
            production_route: Some("RECYCLED".to_string()),
        },
    ]);
    let issues = validate::validate_dossier_mass_balance(&conflicting);
    assert!(has(&issues, "CONFLICTING_ROUTES"), "got {issues:?}");
    assert_eq!(
        severity_of(&issues, "CONFLICTING_ROUTES"),
        Severity::Warning
    );

    // The same route repeated on a duplicate CN is NOT a conflict.
    let consistent = Dossier::new(consignment.clone()).with_materials(vec![
        MaterialRecord {
            cn_code: "76041010".to_string(),
            net_mass_kg: 500.0,
            production_route: Some("PRIMARY".to_string()),
        },
        MaterialRecord {
            cn_code: "76041010".to_string(),
            net_mass_kg: 500.0,
            production_route: Some("PRIMARY".to_string()),
        },
    ]);
    let issues = validate::validate_dossier_mass_balance(&consistent);
    assert!(!has(&issues, "CONFLICTING_ROUTES"), "got {issues:?}");

    // Neither materials nor production records at all -> Warning
    // NO_PRODUCTION_EVIDENCE (R12 completeness; R16: the human must supply
    // and verify the evidence).
    let empty = Dossier::new(consignment);
    let issues = validate::validate_dossier_mass_balance(&empty);
    assert!(has(&issues, "NO_PRODUCTION_EVIDENCE"), "got {issues:?}");
    assert_eq!(
        severity_of(&issues, "NO_PRODUCTION_EVIDENCE"),
        Severity::Warning
    );
}

// ---------------------------------------------------------------------------
// 4. Stable i18n message keys (locales render issues, not raw codes)
// ---------------------------------------------------------------------------

/// Every issue emitted by every scenario carries the stable key
/// `validate.issue.<code lowercase>` so localized surfaces can depend on it.
#[test]
fn issues_carry_stable_message_keys() {
    let lookup = seeded("keys");
    let mut collected: Vec<ValidationIssue> = Vec::new();

    // Consignment scenarios: clean, unknown CN, malformed CN, missing
    // carbon-price country, pre-regime date, negative mass, bad date.
    let mut c = sample_consignment();
    collected.extend(validate::validate_consignment(&c, &lookup));
    c.cn_code = "99999999".to_string();
    collected.extend(validate::validate_consignment(&c, &lookup));
    c.cn_code = "731815".to_string();
    collected.extend(validate::validate_consignment(&c, &lookup));
    let mut c = sample_consignment();
    c.carbon_price_eur_per_tco2e = Some(2.0);
    collected.extend(validate::validate_consignment(&c, &lookup));
    let mut c = sample_consignment();
    c.import_date = "2024-01-01".to_string();
    collected.extend(validate::validate_consignment(&c, &lookup));
    let mut c = sample_consignment();
    c.net_mass_kg = -1.0;
    collected.extend(validate::validate_consignment(&c, &lookup));
    let mut c = sample_consignment();
    c.import_date = "not-a-date".to_string();
    collected.extend(validate::validate_consignment(&c, &lookup));

    // Dossier scenarios: short inputs, negative material, conflicting
    // routes, and a completely empty dossier.
    let consignment = sample_consignment();
    collected.extend(validate::validate_dossier_mass_balance(&Dossier::new(
        consignment.clone(),
    )));
    collected.extend(validate::validate_dossier_mass_balance(
        &Dossier::new(consignment.clone()).with_materials(vec![material(500.0)]),
    ));
    let mut negative = Dossier::new(consignment.clone());
    negative.materials = vec![material(-1.0)];
    collected.extend(validate::validate_dossier_mass_balance(&negative));
    collected.extend(validate::validate_dossier_mass_balance(
        &Dossier::new(consignment).with_materials(vec![
            MaterialRecord {
                cn_code: "76041010".to_string(),
                net_mass_kg: 500.0,
                production_route: Some("PRIMARY".to_string()),
            },
            MaterialRecord {
                cn_code: "76041010".to_string(),
                net_mass_kg: 500.0,
                production_route: Some("RECYCLED".to_string()),
            },
        ]),
    ));

    assert!(!collected.is_empty(), "scenarios must emit issues");

    // Every key matches the `validate.issue.*` pattern derived from its code.
    for issue in &collected {
        assert!(
            issue.message_key.starts_with("validate.issue."),
            "key `{}` for code `{}` is off-pattern",
            issue.message_key,
            issue.code
        );
        assert_eq!(
            issue.message_key,
            format!("validate.issue.{}", issue.code.to_lowercase()),
            "key must be `validate.issue.<code lowercase>`"
        );
    }

    // Coverage: every contract code appears somewhere in the scenarios above.
    for code in [
        "NEGATIVE_MASS",
        "INVALID_DATE",
        "CN_FORMAT",
        "CN_UNKNOWN",
        "CARBON_PRICE_COUNTRY_MISSING",
        "DATE_BEFORE_REGIME",
        "MASS_BALANCE_INPUT_BELOW_OUTPUT",
        "CONFLICTING_ROUTES",
        "NO_PRODUCTION_EVIDENCE",
    ] {
        assert!(
            collected.iter().any(|i| i.code == code),
            "scenario set never exercised `{code}`"
        );
    }
}
