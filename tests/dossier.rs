// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Integration tests for the `dossier` module.
//!
//! Written FIRST (RED); implementation follows to turn them GREEN.
//! These tests pin:
//!
//! - R23 — the three-class complete-dossier document set: completeness flags
//!   in reporting order and stable i18n keys per missing class.
//! - R23 (0.9.0) — 数电发票 (e-fapiao) XML-first parsing: when the structured
//!   XML is present it is the source of truth and OCR is skipped entirely.
//! - R35 / Reg (EU) 2023/956 Annex IV Sec 3 / Guidance Doc 3 §4.3 — the
//!   sub-installation heat & waste-gas balance: attributed emissions cancel
//!   exactly (nothing double-counted, nothing omitted), and unmetered
//!   heat flows are flagged.

use kaimeter_core::domain::errors::DomainError;
use kaimeter_core::domain::types::{
    Consignment, DeterminationBasis, DossierClass, EnergyRecord, MaterialRecord, ProductionRecord,
};
use kaimeter_core::dossier::{
    assemble, completeness, missing_class_key, parse_efapiao_xml, reconcile_balance,
    unmetered_flows, BalanceTable, HeatFlow, WasteGasFlow,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A minimal valid consignment for dossier tests.
fn sample_consignment() -> Consignment {
    Consignment {
        cn_code: "73181500".to_string(),
        net_mass_kg: 1_000.0,
        country_of_origin: "CN".to_string(),
        production_country: "CN".to_string(),
        installation_id: "INST-42".to_string(),
        import_date: "2026-07-20".to_string(),
        determination_basis: DeterminationBasis::Default,
        carbon_price_eur_per_tco2e: None,
        carbon_price_country: None,
    }
}

/// The golden e-fapiao fixture (samples/energy-bills/e-fapiao-sample.xml),
/// compiled in so the test never depends on the working directory.
const EFAPIAO_SAMPLE: &str = include_str!("../samples/energy-bills/e-fapiao-sample.xml");

// ---------------------------------------------------------------------------
// R23 — three-class completeness
// ---------------------------------------------------------------------------

/// R23: a dossier is only complete with all three document sets; the wizard
/// flags whichever class is missing, in reporting order, with stable i18n
/// keys.
#[test]
fn three_class_completeness_end_to_end() {
    // Freshly assembled: all three classes missing, in reporting order.
    let dossier = assemble(sample_consignment());
    let report = completeness(&dossier);
    assert!(!report.complete);
    assert_eq!(
        report.missing,
        vec![
            DossierClass::EnergyFuel,
            DossierClass::Materials,
            DossierClass::Production
        ],
        "R23: missing classes are flagged in reporting order"
    );
    assert_eq!(
        missing_class_key(DossierClass::EnergyFuel),
        "dossier.missing.energy_fuel"
    );
    assert_eq!(
        missing_class_key(DossierClass::Materials),
        "dossier.missing.materials"
    );
    assert_eq!(
        missing_class_key(DossierClass::Production),
        "dossier.missing.production"
    );

    // Energy attached: only the other two classes remain flagged.
    let energy = EnergyRecord::from_kwh(4_200.0).expect("valid energy record");
    let dossier = dossier.with_energy(energy);
    let report = completeness(&dossier);
    assert_eq!(
        report.missing,
        vec![DossierClass::Materials, DossierClass::Production]
    );

    // Materials attached: production remains flagged.
    let materials = vec![MaterialRecord {
        cn_code: "72071200".to_string(),
        net_mass_kg: 950.0,
        production_route: Some("EF".to_string()),
    }];
    let dossier = dossier.with_materials(materials);
    let report = completeness(&dossier);
    assert_eq!(report.missing, vec![DossierClass::Production]);

    // Production attached: the dossier is complete.
    let production = vec![ProductionRecord {
        installation_id: "INST-42".to_string(),
        production_route: "EF".to_string(),
    }];
    let dossier = dossier.with_production(production);
    let report = completeness(&dossier);
    assert!(report.complete, "R23: all three classes present");
    assert!(report.missing.is_empty());
}

// ---------------------------------------------------------------------------
// R23 (0.9.0) — 数电发票 XML-first parsing
// ---------------------------------------------------------------------------

/// R23: when the signed structured XML is present it parses deterministically
/// (OCR skipped entirely) — the golden fixture resolves to its exact fields.
#[test]
fn efapiao_fixture_parses() {
    let fields = parse_efapiao_xml(EFAPIAO_SAMPLE).expect("golden fixture must parse");
    assert_eq!(fields.invoice_number, "24312000000123456789");
    assert_eq!(fields.issue_date, "2026-07-15");
    assert_eq!(fields.seller_name, "某钢铁有限公司");
    assert!((fields.electricity_kwh - 4200.0).abs() < 1e-9);
    assert!((fields.amount_cny - 3360.0).abs() < 1e-9);
    assert!(fields.tax_authority_signed);
}

/// R23: malformed input (empty document, missing mandatory fields, bad date,
/// non-numeric quantity) must fail with [`DomainError::RegistryParseError`],
/// never with a panic or silently-zero fields.
#[test]
fn efapiao_rejects_malformed() {
    // Empty input.
    match parse_efapiao_xml("") {
        Err(DomainError::RegistryParseError(_)) => {}
        other => panic!("empty XML must be RegistryParseError, got {other:?}"),
    }
    // Missing InvoiceNumber.
    let no_number =
        EFAPIAO_SAMPLE.replace("<InvoiceNumber>24312000000123456789</InvoiceNumber>", "");
    assert!(matches!(
        parse_efapiao_xml(&no_number),
        Err(DomainError::RegistryParseError(_))
    ));
    // IssueDate not YYYY-MM-DD.
    let bad_date = EFAPIAO_SAMPLE.replace("2026-07-15", "2026/07/15");
    assert!(matches!(
        parse_efapiao_xml(&bad_date),
        Err(DomainError::RegistryParseError(_))
    ));
    // Non-numeric Quantity.
    let bad_quantity = EFAPIAO_SAMPLE.replace("4200.00", "four thousand two hundred");
    assert!(matches!(
        parse_efapiao_xml(&bad_quantity),
        Err(DomainError::RegistryParseError(_))
    ));
}

/// R23: several invoice line items (e.g. 电费 + 基本电费) sum into one
/// electricity quantity and one total amount.
#[test]
fn efapiao_sums_multiple_items() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<EInvoice>
  <!-- two settlement lines on one invoice -->
  <InvoiceNumber>24312000000123456790</InvoiceNumber>
  <IssueDate>2026-08-01</IssueDate>
  <SellerName>某供电有限公司</SellerName>
  <Items>
    <Item><Name>电费</Name><Quantity unit="kWh">4200.00</Quantity><Amount>3360.00</Amount></Item>
    <Item><Name>基本电费</Name><Quantity unit="kWh">800.50</Quantity><Amount>640.40</Amount></Item>
  </Items>
  <TaxAuthoritySignature present="true"/>
</EInvoice>
"#;
    let fields = parse_efapiao_xml(xml).expect("two-item invoice must parse");
    assert!((fields.electricity_kwh - 5000.5).abs() < 1e-9);
    assert!((fields.amount_cny - 4000.4).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// R35 — sub-installation heat & waste-gas balance
// ---------------------------------------------------------------------------

/// R35 / Annex IV Sec 3: one physical S1→S2 transfer is two balance records —
/// +attributed_tco2e on the exporting side, −attributed_tco2e on the
/// importing side — so the sum cancels to exactly 0.0.
#[test]
fn heat_balance_reconciles_to_zero() {
    let table = BalanceTable {
        heat_flows: vec![
            HeatFlow {
                source: "S1".to_string(),
                destination: "S2".to_string(),
                q_net_mwh: 50.0,
                attributed_tco2e: 12.5,
                metered: true,
            },
            HeatFlow {
                source: "S1".to_string(),
                destination: "S2".to_string(),
                q_net_mwh: 50.0,
                attributed_tco2e: -12.5,
                metered: true,
            },
        ],
        waste_gas_flows: Vec::new(),
    };
    let residual = reconcile_balance(&table).expect("balanced table reconciles");
    assert_eq!(residual, 0.0, "R35: attributed emissions cancel exactly");
}

/// R35: a broken balance (omitted or double-counted tonnes) is flagged with
/// the residual in the error message — nothing double-counted, nothing
/// omitted.
#[test]
fn broken_balance_is_flagged() {
    let table = BalanceTable {
        heat_flows: vec![
            HeatFlow {
                source: "S1".to_string(),
                destination: "S2".to_string(),
                q_net_mwh: 50.0,
                attributed_tco2e: 12.5,
                metered: true,
            },
            HeatFlow {
                source: "S1".to_string(),
                destination: "S2".to_string(),
                q_net_mwh: 50.0,
                attributed_tco2e: -12.0,
                metered: true,
            },
        ],
        waste_gas_flows: Vec::new(),
    };
    match reconcile_balance(&table) {
        Err(DomainError::Storage(msg)) => {
            assert!(
                msg.contains("balance residual"),
                "message names the residual: {msg}"
            );
            assert!(
                msg.contains("0.5"),
                "message carries the 0.5 tCO2e residual: {msg}"
            );
        }
        other => panic!("broken balance must be a Storage error, got {other:?}"),
    }
}

/// R35: waste-gas transfers participate in the same reconciliation as heat
/// flows — both must cancel to zero together.
#[test]
fn waste_gas_flows_participate_in_balance() {
    let table = BalanceTable {
        heat_flows: vec![
            HeatFlow {
                source: "S1".to_string(),
                destination: "S2".to_string(),
                q_net_mwh: 40.0,
                attributed_tco2e: 10.0,
                metered: true,
            },
            HeatFlow {
                source: "S1".to_string(),
                destination: "S2".to_string(),
                q_net_mwh: 40.0,
                attributed_tco2e: -10.0,
                metered: true,
            },
        ],
        waste_gas_flows: vec![
            WasteGasFlow {
                source: "S2".to_string(),
                destination: "S3".to_string(),
                volume_knm3: 1_200.0,
                attributed_tco2e: 2.0,
            },
            WasteGasFlow {
                source: "S2".to_string(),
                destination: "S3".to_string(),
                volume_knm3: 1_200.0,
                attributed_tco2e: -2.0,
            },
        ],
    };
    let residual = reconcile_balance(&table).expect("balanced table reconciles");
    assert_eq!(residual, 0.0, "R35: heat and waste-gas cancel together");
}

/// R35: the balance table carries per-flow metered quantity — unmetered heat
/// flows are listed by index (heat flows first, waste-gas flows continuing
/// the numbering). Waste-gas flows carry no `metered` field, so they are
/// never listed.
#[test]
fn unmetered_flows_are_listed() {
    let table = BalanceTable {
        heat_flows: vec![
            // Index 0: metered — must NOT be listed.
            HeatFlow {
                source: "S1".to_string(),
                destination: "S2".to_string(),
                q_net_mwh: 30.0,
                attributed_tco2e: 7.5,
                metered: true,
            },
            // Index 1: unmetered estimate — must be flagged.
            HeatFlow {
                source: "S3".to_string(),
                destination: "S1".to_string(),
                q_net_mwh: 12.0,
                attributed_tco2e: 3.0,
                metered: false,
            },
        ],
        waste_gas_flows: vec![WasteGasFlow {
            source: "S2".to_string(),
            destination: "S3".to_string(),
            volume_knm3: 900.0,
            attributed_tco2e: 1.5,
        }],
    };
    assert_eq!(
        unmetered_flows(&table),
        vec![1],
        "R35: only the unmetered heat flow is listed, by its index"
    );

    // An all-metered table lists nothing.
    let clean = BalanceTable {
        heat_flows: vec![HeatFlow {
            source: "S1".to_string(),
            destination: "S2".to_string(),
            q_net_mwh: 30.0,
            attributed_tco2e: 7.5,
            metered: true,
        }],
        waste_gas_flows: table.waste_gas_flows.clone(),
    };
    assert!(
        unmetered_flows(&clean).is_empty(),
        "R35: metered heat flows and waste-gas flows are never listed"
    );
}
