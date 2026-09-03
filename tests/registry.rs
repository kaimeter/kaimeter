// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Integration tests for the `registry` bridge module (R15/R36).
//!
//! Written FIRST (RED) per the TDD contract. They pin: bulk SAD/H1 XML and
//! CSV import parsed straight into consignment records (R15 — users never
//! re-key data Kaimeter already holds), the hand-off to the frozen Box 37
//! rule engine in `customs` (R15), Registry operator → installation mapping
//! (R36; Reg (EU) 2023/956 Art 10, IR (EU) 2024/3210 Art 5), and the offline
//! EORI/VIES format validation build note (R14/R15 open data item: validate
//! identifiers before compiling registry upload packs so filings never fail
//! silently on a format check at the Registry).

use kaimeter_core::customs::CbamStatus;
use kaimeter_core::domain::errors::DomainError;
use kaimeter_core::registry::{self, OperatorRecord, SadRow};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// R15 SAD/H1 XML fixture: two `GoodsItem` records using different accepted
/// spellings per field family, with irregular whitespace (customs brokers
/// do not emit tidy XML).
const SAD_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Declaration>
  <FunctionalReference>IMP-2026-0042</FunctionalReference>
  <GoodsItem>
    <CommodityCode>73181500</CommodityCode>
    <GoodsItemNetMass>12000</GoodsItemNetMass>
    <AdditionalProcedure>40 00</AdditionalProcedure>
    <CountryOfOrigin>CN</CountryOfOrigin>
    <AcceptanceDate>2026-03-15</AcceptanceDate>
  </GoodsItem>
  <GoodsItem>
    <GoodsItemCommodityCode>76 04 10 10</GoodsItemCommodityCode>
    <NetMass>500</NetMass>
    <ProcedureCode>71 00</ProcedureCode>
    <OriginCountry>IN</OriginCountry>
    <ClearanceDate>2026-04-01</ClearanceDate>
  </GoodsItem>
</Declaration>"#;

/// The two rows the XML fixture must produce (exact fields).
fn expected_rows() -> Vec<SadRow> {
    vec![
        SadRow {
            cn_code: "73181500".to_string(),
            net_mass_kg: 12_000.0,
            procedure_code: "40 00".to_string(),
            country_of_origin: "CN".to_string(),
            clearance_date: "2026-03-15".to_string(),
            additional_code: None,
        },
        SadRow {
            cn_code: "76041010".to_string(),
            net_mass_kg: 500.0,
            procedure_code: "71 00".to_string(),
            country_of_origin: "IN".to_string(),
            clearance_date: "2026-04-01".to_string(),
            additional_code: None,
        },
    ]
}

// ---------------------------------------------------------------------------
// R15: bulk SAD/H1 import — users never re-key held data
// ---------------------------------------------------------------------------

/// REGULATORY PIN (R15): customs/broker H1 (SAD) XML exports are parsed
/// straight into consignment rows — users never re-key data Kaimeter
/// already holds.
#[test]
fn sad_xml_fixture_parses_into_rows() {
    let rows = registry::parse_sad_xml(SAD_XML).expect("parse SAD XML");
    assert_eq!(rows, expected_rows(), "two GoodsItem records, exact fields");
}

/// REGULATORY PIN (R15): the CSV flavor of the same bridge parses with and
/// without the optional `additional_code` column, and malformed rows fail
/// naming the 1-based row number (header = row 1) so the broker can fix the
/// exact line.
#[test]
fn sad_csv_fixture_parses_with_and_without_additional_code() {
    let five_col = concat!(
        "cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date\n",
        "73181500,12000,40 00,CN,2026-03-15\n",
        "76041010,500,71 00,IN,2026-04-01\n",
    );
    let rows = registry::parse_sad_csv(five_col).expect("parse 5-column CSV");
    assert_eq!(rows, expected_rows());

    let six_col = concat!(
        "cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date,additional_code\n",
        "73181500,12000,40 00,CN,2026-03-15,F51\n",
        "76041010,500,71 00,IN,2026-04-01,\n",
    );
    let rows = registry::parse_sad_csv(six_col).expect("parse 6-column CSV");
    assert_eq!(rows[0].additional_code.as_deref(), Some("F51"));
    assert_eq!(
        rows[1].additional_code, None,
        "empty cell = no additional code"
    );

    // A wrong header is rejected outright — silent column misalignment
    // would corrupt consignment records.
    let bad_header = "cn,net_mass_kg,procedure_code,country_of_origin,clearance_date\n";
    assert!(matches!(
        registry::parse_sad_csv(bad_header),
        Err(DomainError::RegistryParseError(_))
    ));

    // 6-column header forces every row to carry the 6th column.
    let short_row = concat!(
        "cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date,additional_code\n",
        "73181500,12000,40 00,CN,2026-03-15,F51\n",
        "76041010,500,71 00,IN,2026-04-01\n",
    );
    match registry::parse_sad_csv(short_row) {
        Err(DomainError::RegistryParseError(detail)) => {
            assert!(detail.contains("row 3"), "detail names the row: {detail}");
        }
        other => panic!("expected RegistryParseError naming row 3, got {other:?}"),
    }

    // Malformed mass on the first data row errors naming "row 2"
    // (1-based: the header is row 1).
    let bad_mass = concat!(
        "cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date\n",
        "76041010,twelve,71 00,IN,2026-04-01\n",
        "73181500,12000,40 00,CN,2026-03-15\n",
    );
    match registry::parse_sad_csv(bad_mass) {
        Err(DomainError::RegistryParseError(detail)) => {
            assert!(detail.contains("row 2"), "detail names the row: {detail}");
            assert!(
                detail.contains("net_mass_kg"),
                "detail names the field: {detail}"
            );
        }
        other => panic!("expected RegistryParseError naming row 2, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// R15: classification via the frozen Box 37 rule engine
// ---------------------------------------------------------------------------

/// REGULATORY PIN (R15): imported rows are classified through the frozen
/// `customs::classify` Box 37 table — 40 00 release for free circulation is
/// CBAM_LIABLE and counts toward the 50 t net-mass watch; 71 00 customs
/// warehousing is CBAM_DEFERRED and does not count (UCC Arts 215/237).
/// Origin exemptions (R43/R45) are applied by the caller when known, so the
/// bridge classifies with `origin_exempt = false`.
#[test]
fn classify_imports_uses_frozen_box37() {
    let rows = registry::parse_sad_xml(SAD_XML).expect("parse SAD XML");
    let classified = registry::classify_imports(&rows).expect("classify");

    assert_eq!(classified.len(), 2);
    assert_eq!(classified[0].row, rows[0]);
    assert_eq!(
        classified[0].status,
        CbamStatus::Liable,
        "40 00 -> CBAM_LIABLE"
    );
    assert!(
        classified[0].counts_toward_net_mass,
        "free circulation counts toward the 50 t watch"
    );
    assert_eq!(
        classified[1].status,
        CbamStatus::Deferred,
        "71 00 -> CBAM_DEFERRED (no liability, no tracking)"
    );
    assert!(
        !classified[1].counts_toward_net_mass,
        "warehoused goods are excluded from net-mass tracking"
    );

    // Unsupported Box 37 codes propagate the frozen rule engine's error.
    let unknown = SadRow {
        cn_code: "73181500".to_string(),
        net_mass_kg: 1.0,
        procedure_code: "99 99".to_string(),
        country_of_origin: "CN".to_string(),
        clearance_date: "2026-03-15".to_string(),
        additional_code: None,
    };
    assert!(matches!(
        registry::classify_imports(&[unknown]),
        Err(DomainError::UnknownProcedureCode(_))
    ));
}

// ---------------------------------------------------------------------------
// R14/R15 build note: offline EORI format validation
// ---------------------------------------------------------------------------

/// REGULATORY PIN (R14/R15 build note): EORI identifiers are format-checked
/// offline (generic rule + pinned national formats for the big states)
/// before compiling registry upload packs — prevents silent registry
/// rejections during peak filing windows.
#[test]
fn eori_offline_validation_pins() {
    // DE national format: "DE" + 8..=9 digits.
    assert!(
        registry::validate_eori("DE12345678").is_ok(),
        "DE + 8 digits"
    );
    assert!(
        registry::validate_eori("DE123456789").is_ok(),
        "DE + 9 digits"
    );
    assert!(
        registry::validate_eori("DE1234567").is_err(),
        "DE + 7 digits is not a DE EORI"
    );
    assert!(
        registry::validate_eori("de12345678").is_err(),
        "lowercase prefix rejected"
    );
    assert!(
        registry::validate_eori("D212345678").is_err(),
        "digit in the country prefix rejected"
    );
    // Generic rule for prefixes without a pinned national format:
    // 2 uppercase letters + 1..=15 alphanumeric.
    assert!(
        registry::validate_eori("XXABC123").is_ok(),
        "unpinned prefix passes the generic rule (format cache covers the big states)"
    );
    assert!(registry::validate_eori("PL1234567890AB123").is_ok());
    assert!(
        matches!(
            registry::validate_eori("X1ABC123"),
            Err(DomainError::Storage(_))
        ),
        "non-letter prefix -> Storage error"
    );
    assert!(matches!(
        registry::validate_eori("DE1234"),
        Err(DomainError::Storage(_))
    ));
}

/// REGULATORY PIN (R14/R15 build note): VAT identifiers are format-checked
/// offline against VIES structure rules (the live VIES check is a sync-time
/// feature, never a launch assumption).
#[test]
fn vies_format_pins() {
    // DE: "DE" + exactly 9 digits ("DE136695976" style).
    assert!(registry::validate_vies_format("DE136695976").is_ok());
    assert!(
        registry::validate_vies_format("DE1366959").is_err(),
        "8 digits"
    );
    // NL: 12 characters after the prefix — 9 alphanumeric, then "B", then
    // 2 digits ("NL004495445B01").
    assert!(registry::validate_vies_format("NL004495445B01").is_ok());
    assert!(
        registry::validate_vies_format("NL004495445X01").is_err(),
        "NL body must end B + 2 digits"
    );
    // FR and unpinned prefixes pass the generic rule: 2 letters + 2..=17
    // alphanumeric.
    assert!(registry::validate_vies_format("FR40303265045").is_ok());
    assert!(registry::validate_vies_format("IE6388047V").is_ok());
    assert!(matches!(
        registry::validate_vies_format("D2 36695976"),
        Err(DomainError::Storage(_))
    ));
}

// ---------------------------------------------------------------------------
// R36: Registry operator -> installation mapping (Art 10)
// ---------------------------------------------------------------------------

/// REGULATORY PIN (R36 / Reg (EU) 2023/956 Art 10, IR (EU) 2024/3210
/// Art 5): third-country operator registration records map onto local
/// installations so verifier-approved actuals flow in without re-keying;
/// unknown statuses and empty installation ids are rejected.
#[test]
fn operator_mapping_round_trips() {
    let record = OperatorRecord {
        registry_operator_id: "CN-OPS-000123".to_string(),
        installation_id: String::new(),
        status: "REGISTERED".to_string(),
        refreshed_iso: "2026-08-30".to_string(),
    };

    let mapped = registry::map_operator(&record, "INST-CN-01").expect("map onto installation");
    assert_eq!(mapped.installation_id, "INST-CN-01");
    assert_eq!(
        mapped.registry_operator_id, "CN-OPS-000123",
        "rest preserved"
    );
    assert_eq!(mapped.status, "REGISTERED");
    assert_eq!(mapped.refreshed_iso, "2026-08-30");
    // The source record is not mutated.
    assert_eq!(record.installation_id, "");

    for known in ["REGISTERED", "PENDING", "REVOKED", "WITHDRAWN"] {
        let r = OperatorRecord {
            status: known.to_string(),
            ..record.clone()
        };
        assert!(
            registry::map_operator(&r, "INST-CN-01").is_ok(),
            "{known} is a known status"
        );
    }

    assert!(
        matches!(
            registry::map_operator(&record, ""),
            Err(DomainError::Storage(_))
        ),
        "empty installation id rejected"
    );
    assert!(
        matches!(
            registry::map_operator(&record, "   "),
            Err(DomainError::Storage(_))
        ),
        "whitespace installation id rejected"
    );
    let unknown_status = OperatorRecord {
        status: "UNKNOWN".to_string(),
        ..record.clone()
    };
    assert!(matches!(
        registry::map_operator(&unknown_status, "INST-CN-01"),
        Err(DomainError::Storage(_))
    ));
}
