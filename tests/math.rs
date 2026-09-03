// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Integration tests for the `math` module.
//!
//! Written FIRST (RED): they pin the CBAM factor schedule (Art 10a(1a) ETS
//! Directive), the Formula A/B toggle (R7), the Annex II indirect-scope
//! table and Annex IV complex-goods equation (R3), the default-path mark-up
//! application (R4, IR 2025/2621), and the 50 t de-minimis latch (R1,
//! Reg (EU) 2025/2083 Art 2a + Annex VII pt 1). Implementation follows to
//! turn them GREEN.

use kaimeter_core::domain::errors::DomainError;
use kaimeter_core::domain::types::{Consignment, DeterminationBasis, Sector};
use kaimeter_core::math::{
    cbam_factor, complex_goods_emissions, consignment_emissions_actual,
    consignment_emissions_default, gross_exposure, indirect_scope, net_exposure, ComplexGoodsInput,
    DeMinimisTracker, Formula, IndirectScope, PrecursorInput, DE_MINIMIS_THRESHOLD_TONNES,
};

/// Absolute tolerance for the pinned worked examples (contract: 1e-9).
const EPS: f64 = 1e-9;

fn approx(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < EPS,
        "expected {expected}, got {actual}"
    );
}

/// Relative tolerance for the property test (large magnitudes: compare
/// within float noise, not absolutely).
fn approx_rel(actual: f64, expected: f64) {
    let scale = expected.abs().max(actual.abs()).max(1.0);
    assert!(
        (actual - expected).abs() <= 1e-12 * scale,
        "expected {expected}, got {actual}"
    );
}

/// A steel consignment released for free circulation in 2026.
fn steel_consignment_2026(net_mass_kg: f64) -> Consignment {
    Consignment {
        cn_code: "73181500".to_string(),
        net_mass_kg,
        country_of_origin: "CN".to_string(),
        production_country: "CN".to_string(),
        installation_id: "INST-CN-001".to_string(),
        import_date: "2026-03-15".to_string(),
        determination_basis: DeterminationBasis::Default,
        carbon_price_eur_per_tco2e: None,
        carbon_price_country: None,
    }
}

fn precursor(
    cn: &str,
    mass_per_t: f64,
    embedded: f64,
    indirect: f64,
    eligible: bool,
) -> PrecursorInput {
    PrecursorInput {
        cn_code: cn.to_string(),
        mass_per_t_output: mass_per_t,
        embedded_tco2e_per_t: embedded,
        indirect_tco2e_per_t: indirect,
        production_country_unknown: false,
        indirect_scope_eligible: eligible,
    }
}

// ---------------------------------------------------------------------------
// CBAM factor schedule (R7, Art 10a(1a) ETS Directive)
// ---------------------------------------------------------------------------

#[test]
fn regression_cbam_factor_schedule_is_pinned() {
    // REGULATORY PIN — Art 10a(1a) ETS Directive (free-allocation phase-out,
    // SEFA method per IR 2025/2620). These values are law; a Phase-5
    // adoption changes this data table, never silently the code.
    let schedule = [
        (2026, 0.025), // 2026: 2.5 %
        (2027, 0.05),  // 2027: 5 %
        (2028, 0.10),  // 2028: 10 %
        (2029, 0.225), // 2029: 22.5 %
        (2030, 0.485), // 2030: 48.5 %
        (2031, 0.61),  // 2031: 61 %
        (2032, 0.735), // 2032: 73.5 %
        (2033, 0.86),  // 2033: 86 %
        (2034, 1.0),   // 2034: 100 % — fully phased in
        (2035, 1.0),   // every later year stays at 100 %
        (2050, 1.0),
    ];
    for (year, factor) in schedule {
        approx(cbam_factor(year).expect("year in schedule"), factor);
    }
    // The schedule starts in 2026; earlier years are a domain error.
    for year in [1999, 2025] {
        assert!(matches!(
            cbam_factor(year),
            Err(DomainError::CbamFactorYearOutOfRange(y)) if y == year
        ));
    }
}

// ---------------------------------------------------------------------------
// Gross exposure and the Formula A/B toggle (R7, Art 9 deduction order)
// ---------------------------------------------------------------------------

#[test]
fn gross_exposure_pinned() {
    // R7: gross = emissions × ETS price − carbon price already paid.
    // Worked example: 100 tCO2e × €80 − €500 = €7,500.
    approx(gross_exposure(100.0, 80.0, 500.0).expect("gross"), 7500.0);

    // The ETS price must be finite and non-negative (R7: offline manual
    // entry must never silently poison the projection).
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        // NaN != NaN, so the guard compares NaN-aware.
        let echoes_bad = |x: f64| x == bad || (x.is_nan() && bad.is_nan());
        assert!(matches!(
            gross_exposure(100.0, bad, 0.0),
            Err(DomainError::InvalidEtsPrice(x)) if echoes_bad(x)
        ));
        assert!(matches!(
            net_exposure(100.0, bad, 0.0, 0.025, Formula::A),
            Err(DomainError::InvalidEtsPrice(x)) if echoes_bad(x)
        ));
    }
}

#[test]
fn formula_a_and_b_worked_examples_pinned() {
    // R7 — the Art 9 carbon-price deduction order is a parameterized toggle:
    //   A = (emissions × factor × price) − carbon_paid
    //   B = ((emissions × price) − carbon_paid) × factor
    // Worked example: 100 tCO2e, €80, €500 already paid, 2026 factor 0.025.
    let (e, p, c, f) = (100.0, 80.0, 500.0, 0.025);

    // A: the factor bites first, then the full carbon paid is deducted:
    // 100 × 0.025 × 80 − 500 = 200 − 500 = −300.0 (the obligation share is
    // smaller than the carbon price already paid — no refund is projected).
    approx(
        net_exposure(e, p, c, f, Formula::A).expect("formula A"),
        e * f * p - c,
    );
    // B: the deduction happens gross, then the factor scales the remainder:
    // ((100 × 80) − 500) × 0.025 = 7,500 × 0.025 = 187.5.
    approx(
        net_exposure(e, p, c, f, Formula::B).expect("formula B"),
        (e * p - c) * f,
    );
    // Deduction-order identity (locks which operand the factor hits):
    // A − B = carbon_paid × (factor − 1) = 500 × (−0.975) = −487.5.
    approx(
        net_exposure(e, p, c, f, Formula::A).expect("A")
            - net_exposure(e, p, c, f, Formula::B).expect("B"),
        c * (f - 1.0),
    );
    // With nothing paid abroad, the two orders are algebraically identical.
    approx(
        net_exposure(e, p, 0.0, f, Formula::A).expect("A, nothing paid"),
        net_exposure(e, p, 0.0, f, Formula::B).expect("B, nothing paid"),
    );
    approx(net_exposure(e, p, 0.0, f, Formula::A).expect("A"), 200.0);
}

#[test]
fn formula_a_and_b_coincide_when_no_carbon_was_paid() {
    // R7 property: the orders differ only in WHERE the paid carbon price is
    // deducted relative to the CBAM factor; with carbon_paid = 0 they agree
    // for arbitrary inputs (fixed tuples, no rng).
    let cases = [
        (100.0, 80.0, 0.025),     // 2026 steel, pinned example scale
        (1234.5, 67.89, 0.485),   // 2030 factor
        (0.001, 90.0, 1.0),       // 2034+ (fully phased in)
        (700_000.0, 55.5, 0.735), // 2032 factor, large consignment year
    ];
    for (e, p, f) in cases {
        let a = net_exposure(e, p, 0.0, f, Formula::A).expect("A");
        let b = net_exposure(e, p, 0.0, f, Formula::B).expect("B");
        approx_rel(a, e * p * f);
        approx_rel(a, b);
    }
}

// ---------------------------------------------------------------------------
// Annex II indirect scope + Annex IV complex goods (R3)
// ---------------------------------------------------------------------------

#[test]
fn indirect_scope_table_pinned() {
    // Annex II: cement and fertilisers include indirect emissions;
    // iron & steel is direct-only EXCEPT indirect from agglomerated iron ore
    // (sinter/pellet) precursors (modeled via PrecursorInput eligibility);
    // aluminium and hydrogen are direct-only. Electricity is its own regime
    // (grid factor / PPA per Guidance Doc 3) and is treated direct-only here.
    assert_eq!(indirect_scope(Sector::Cement), IndirectScope::Included);
    assert_eq!(indirect_scope(Sector::Fertilisers), IndirectScope::Included);
    assert_eq!(
        indirect_scope(Sector::Steel),
        IndirectScope::SteelOrePrecursor
    );
    assert_eq!(indirect_scope(Sector::Aluminium), IndirectScope::DirectOnly);
    assert_eq!(indirect_scope(Sector::Hydrogen), IndirectScope::DirectOnly);
    assert_eq!(
        indirect_scope(Sector::Electricity),
        IndirectScope::DirectOnly
    );
}

#[test]
fn complex_goods_yield_example_pinned() {
    // R3 / Annex IV activity-level equation (Guidance Doc 3, section 5.2):
    // total embedded = (precursor inputs' embedded emissions + own
    // production) / net tonnes of finished output. Yield loss: 105 t of
    // precursor consumed per 100 t of goods out — the input/output ratio
    // lives in the precursor masses, the denominator is finished output.
    //
    // DOCUMENTATION INVARIANT (R3): in-process recycled scrap is excluded
    // from BOTH precursor mass and the output denominator (no double
    // counting in the activity denominator) — the caller must hand this
    // function only CBAM-relevant precursor masses and finished-output
    // tonnes; scrap netting happens upstream of this equation.
    let own = ComplexGoodsInput {
        own_emissions_tco2e: 50.0,
        output_tonnes: 100.0,
    };
    let wire_rod = precursor("73181900", 105.0, 2.0, 0.0, false);
    approx(
        complex_goods_emissions(
            &own,
            std::slice::from_ref(&wire_rod),
            IndirectScope::DirectOnly,
        )
        .expect("complex goods"),
        (105.0 * 2.0 + 50.0) / 100.0, // = 2.6 tCO2e per t output
    );

    // Indirect gating: a precursor's indirect share counts only when
    // (scope == Included) or (scope == SteelOrePrecursor AND the precursor
    // is an agglomerated-iron-ore input — sinter/pellet, e.g. CN 2601 12).
    let with_pellet = precursor("26011200", 1.05, 2.0, 0.5, true);
    let both: [PrecursorInput; 2] = [wire_rod.clone(), with_pellet];

    // Included (cement/fertilisers): indirect always adds:
    // (210 + 2.1 + 0.525 + 50) / 100 = 2.62625.
    approx(
        complex_goods_emissions(&own, &both, IndirectScope::Included).expect("included"),
        2.62625,
    );
    // DirectOnly (aluminium/hydrogen): the indirect share never adds:
    // (210 + 2.1 + 50) / 100 = 2.621.
    approx(
        complex_goods_emissions(&own, &both, IndirectScope::DirectOnly).expect("direct only"),
        2.621,
    );
    // SteelOrePrecursor: the indirect share adds ONLY for the eligible
    // (agglomerated-ore) precursor.
    approx(
        complex_goods_emissions(&own, &both, IndirectScope::SteelOrePrecursor)
            .expect("steel, eligible ore"),
        2.62625,
    );
    let no_ore: [PrecursorInput; 2] = [
        wire_rod.clone(),
        precursor("26011200", 1.05, 2.0, 0.5, false),
    ];
    approx(
        complex_goods_emissions(&own, &no_ore, IndirectScope::SteelOrePrecursor)
            .expect("steel, ineligible"),
        2.621,
    );

    // The activity denominator must be positive finished output.
    let zero_out = ComplexGoodsInput {
        own_emissions_tco2e: 50.0,
        output_tonnes: 0.0,
    };
    assert!(matches!(
        complex_goods_emissions(
            &zero_out,
            std::slice::from_ref(&wire_rod),
            IndirectScope::DirectOnly
        ),
        Err(DomainError::NegativeMass(_))
    ));
    let neg_out = ComplexGoodsInput {
        own_emissions_tco2e: 50.0,
        output_tonnes: -5.0,
    };
    assert!(matches!(
        complex_goods_emissions(&neg_out, &[wire_rod], IndirectScope::DirectOnly),
        Err(DomainError::NegativeMass(_))
    ));
}

// ---------------------------------------------------------------------------
// Consignment embedded emissions: default path with mark-up (R4), actual path
// ---------------------------------------------------------------------------

#[test]
fn consignment_default_path_applies_markup() {
    // R4 (IR 2025/2621, corrected by Reg (EU) 2026/1740): default values
    // carry a +10 % mark-up in 2026 for steel. CN 73181500 (STEEL),
    // 2000 kg = 2 t, direct 2.0 / indirect 1.0 per tonne:
    // (2.0 × 1.1 + 1.0 × 1.1) × 2 = (2.2 + 1.1) × 2 = 6.6 tCO2e.
    let c = steel_consignment_2026(2000.0);
    approx(
        consignment_emissions_default(&c, 2.0, 1.0).expect("default 2026"),
        6.6,
    );

    // Both components are additive — sector scoping (whether indirect
    // belongs in scope at all) is the caller's job (validate/web).
    approx(
        consignment_emissions_default(&c, 2.0, 0.0).expect("direct only"),
        4.4,
    );

    // 2027: +20 % → (2.4 + 1.2) × 2 = 7.2.
    let mut c27 = steel_consignment_2026(2000.0);
    c27.import_date = "2027-03-15".to_string();
    approx(
        consignment_emissions_default(&c27, 2.0, 1.0).expect("default 2027"),
        7.2,
    );

    // Fertilisers branch: flat +1 % in every schedule year (R4).
    let mut fert = steel_consignment_2026(1000.0);
    fert.cn_code = "31021000".to_string(); // nitrogenous fertilisers
    approx(
        consignment_emissions_default(&fert, 2.0, 1.0).expect("fertilisers"),
        (2.0 * 1.01 + 1.0 * 1.01) * 1.0, // = 3.03
    );

    // Mark-ups apply from 2026 — an earlier year is rejected via the
    // mark-ups error.
    let mut old = steel_consignment_2026(2000.0);
    old.import_date = "2025-12-31".to_string();
    assert!(matches!(
        consignment_emissions_default(&old, 2.0, 1.0),
        Err(DomainError::MarkupYearOutOfRange(2025))
    ));

    // Negative mass is a domain error.
    let neg = steel_consignment_2026(-1.0);
    assert!(matches!(
        consignment_emissions_default(&neg, 2.0, 1.0),
        Err(DomainError::NegativeMass(_))
    ));
}

#[test]
fn consignment_actual_path_multiplies_intensity_by_net_tonnes() {
    // R3/R8: verifier-approved installation-specific intensity × net tonnes.
    let c = steel_consignment_2026(2000.0);
    approx(
        consignment_emissions_actual(&c, 2.5).expect("actual"),
        2.5 * 2.0, // = 5.0 tCO2e
    );
    // Zero mass is valid (Consignment::validate allows >= 0).
    let zero = steel_consignment_2026(0.0);
    approx(
        consignment_emissions_actual(&zero, 2.5).expect("zero mass"),
        0.0,
    );
    // A negative intensity is as meaningless as a negative mass here.
    assert!(matches!(
        consignment_emissions_actual(&c, -0.5),
        Err(DomainError::NegativeMass(_))
    ));
    let neg = steel_consignment_2026(-3.0);
    assert!(matches!(
        consignment_emissions_actual(&neg, 2.5),
        Err(DomainError::NegativeMass(_))
    ));
}

// ---------------------------------------------------------------------------
// 50 t de-minimis tracker (R1, Art 2a + Annex VII pt 1)
// ---------------------------------------------------------------------------

#[test]
fn de_minimis_crossing_latches_for_the_year() {
    // R1 (Reg (EU) 2025/2083, Art 2a + Annex VII pt 1): the threshold is
    // 50 t of net mass per declarant per calendar year, AGGREGATED across
    // all CBAM goods. Exceeding it makes the importer liable for ALL tonnes
    // that year — so the crossing latches forever within the tracker.
    //
    // Electricity and hydrogen have NO exemption: those imports are always
    // liable regardless of the aggregate. The tracker is sector-blind — it
    // only tracks the aggregate exemption; always-liable sector handling
    // sits with the caller.
    assert_eq!(DE_MINIMIS_THRESHOLD_TONNES, 50.0);

    let mut t = DeMinimisTracker::new();
    assert!(t.is_exempt(), "a fresh year starts exempt");
    assert!(!t.crossed());
    assert_eq!(t.ytd_net_mass_kg(), 0.0);

    // Just under the threshold: 49,999 kg stays exempt.
    assert!(!t.add(49_999.0), "49,999 kg has not crossed");
    assert!(t.is_exempt());

    // +2 kg → 50,001 kg cumulative: crossed, and liable for ALL tonnes
    // that year — the latch never resets.
    assert!(t.add(2.0), "50,001 kg crosses the 50 t threshold");
    assert!(t.crossed());
    assert!(!t.is_exempt());
    t.add(0.0);
    assert!(t.crossed(), "the latch is permanent for the year");
    assert!(!t.is_exempt());
}

#[test]
fn de_minimis_boundary_is_exactly_fifty_tonnes() {
    // Annex VII pt 1: the exemption holds at "≤ 50 t" — exactly 50,000 kg
    // does NOT cross; only exceeding it does.
    let mut t = DeMinimisTracker::new();
    assert!(!t.add(50_000.0));
    assert!(t.is_exempt());
    assert!(!t.crossed());
    // The next gram tips the year over, forever.
    assert!(t.add(0.001));
    assert!(t.crossed());
    assert!(!t.is_exempt());
}
