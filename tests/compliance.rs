// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Integration tests for the `compliance` module.
//!
//! These tests are the executable specification, written FIRST (RED): they
//! pin the Art 22(2) 50% quarterly holding duty with the R24 45%/48% alert
//! buffers, the Art 26(1) EUR 100/t penalty and the Art 26(2a) 3-5x band
//! (R32), the Art 26(2) Member-State jurisdiction table (R39), the
//! IR (EU) 2025/2549 guarantee-sufficiency flag (R37), the Arts 11 & 17 NCA
//! response-deadline counter (R40), and the Art 17(8) authorised-declarant
//! status lifecycle (R42).

use kaimeter_core::calendar::Quarter;
use kaimeter_core::compliance::{
    apply_enforcement, assess_holding, filing_allowed, guarantee_sufficient, jurisdiction_penalty,
    shortfall_tco2e, unauthorised_import_penalty, unsurrendered_penalty, AlertLevel,
    AlertThresholds, AuthorisationStatus, EnforcementAction, GuaranteeProjection, HoldingPosition,
    JurisdictionPenalty, NcaCommunication, HOLDING_SHARE, PENALTY_EUR_PER_TCO2E,
};

/// R24 fixtures: basis 1000 tCO2e -> required 500 tCO2e (Art 22(2) share).
fn position(held: f64) -> HoldingPosition {
    HoldingPosition {
        quarter: Quarter::new(2027, 1).expect("Q1 2027"),
        basis_tco2e: 1000.0,
        required_tco2e: 1000.0 * HOLDING_SHARE,
        held_tco2e: held,
        purchased_ytd_tco2e: 0.0,
        cancelled_ytd_tco2e: 0.0,
    }
}

fn thresholds() -> AlertThresholds {
    AlertThresholds::default()
}

fn assert_f64(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// R24 — quarterly certificate-position monitor (Art 22(2) as amended)
// ---------------------------------------------------------------------------

/// Art 22(2) CBAM Reg as amended by Reg (EU) 2025/2083: the quarterly
/// certificate-holding duty is 50% of the embedded-emissions basis
/// (reduced from the original 80% proposal trajectory). R24 pins the share
/// as a named constant so no code path can drift from the statute.
#[test]
fn holding_share_is_50_percent() {
    assert!(
        (HOLDING_SHARE - 0.5).abs() < f64::EPSILON,
        "Art 22(2) as amended by 2025/2083 pins the holding share at 50% (was 80%)"
    );
    // The default position fixture derives its requirement from the share.
    assert_f64(position(0.0).required_tco2e, 500.0);
}

/// R24 default alert buffers: warn at 45%, critical at 48% OF THE REQUIRED
/// position (the buffers sit BELOW the 50% duty). Bands, best to worst:
/// coverage >= 1.0 -> Ok; >= 0.48 -> Warn; >= 0.45 -> Critical; below ->
/// Shortfall. A position at exactly 48% of the requirement must already
/// trigger the early warning (Art 22(2) as amended by Reg (EU) 2025/2083).
#[test]
fn coverage_bands_pinned() {
    // 500/500 = 1.0 -> covered, no alert.
    assert_eq!(
        assess_holding(&position(500.0), &thresholds()),
        AlertLevel::Ok
    );
    // 250/500 = 0.50 -> inside the warn buffer (>= 0.48).
    assert_eq!(
        assess_holding(&position(250.0), &thresholds()),
        AlertLevel::Warn
    );
    // 240/500 = 0.48 -> exactly on the critical boundary: still Warn.
    assert_eq!(
        assess_holding(&position(240.0), &thresholds()),
        AlertLevel::Warn
    );
    // 235/500 = 0.47 -> inside the critical buffer (>= 0.45, < 0.48).
    assert_eq!(
        assess_holding(&position(235.0), &thresholds()),
        AlertLevel::Critical
    );
    // 224/500 = 0.448 -> below the 0.45 floor: outright Shortfall.
    assert_eq!(
        assess_holding(&position(224.0), &thresholds()),
        AlertLevel::Shortfall
    );
}

/// R24 shortfall projection: how many more certificates must be held to
/// reach the Art 22(2) requirement (0.0 when covered).
#[test]
fn shortfall_pinned() {
    // required 500, held 300 -> 200 more needed.
    assert_f64(shortfall_tco2e(&position(300.0)), 200.0);
    // Covered position -> no shortfall.
    assert_f64(shortfall_tco2e(&position(500.0)), 0.0);
    assert_f64(shortfall_tco2e(&position(600.0)), 0.0);
}

// ---------------------------------------------------------------------------
// R32 — penalty exposure engine (Art 26(1), Art 26(2a))
// ---------------------------------------------------------------------------

/// Art 26(1) CBAM Reg as amended by Reg (EU) 2025/2083: EUR 100 per tonne
/// CO2e of certificates not surrendered, index-linked to EU consumer
/// prices. Penalties NEVER release the underlying surrender obligation.
#[test]
fn art26_penalty_pinned() {
    assert_f64(PENALTY_EUR_PER_TCO2E, 100.0);

    // 10 t unsurrendered at the base rate -> EUR 1000.
    let base = unsurrendered_penalty(10.0, 1.0).expect("base rate");
    assert_f64(base.rate_eur_per_tco2e, 100.0);
    assert_f64(base.total_eur, 1000.0);
    assert!(!base.reduction_applied, "Art 26(1) has no reduction");
    assert!(
        base.surrender_obligation_remains,
        "Art 26(1) penalties never release the surrender obligation"
    );

    // Index-linked: factor 1.05 -> EUR 105/t -> EUR 1050 for 10 t.
    let indexed = unsurrendered_penalty(10.0, 1.05).expect("indexed rate");
    assert_f64(indexed.rate_eur_per_tco2e, 105.0);
    assert_f64(indexed.total_eur, 1050.0);
    assert!(indexed.surrender_obligation_remains);

    // The index factor must be finite and strictly positive.
    assert!(
        unsurrendered_penalty(10.0, -1.0).is_err(),
        "negative index factor is rejected"
    );
    assert!(unsurrendered_penalty(10.0, 0.0).is_err());
    assert!(unsurrendered_penalty(10.0, f64::NAN).is_err());
}

/// Art 26(2a) CBAM Reg: importing above the 50 t threshold without
/// authorised-declarant status carries 3-5x the Art 26(1) rate; the penalty
/// is reducible where the excess is <= 10% of the threshold (floored at the
/// 3x rate).
#[test]
fn art26_2a_multiplier_range_pinned() {
    // Lower bound of the band: 3x EUR 100 = EUR 300/t.
    let at_3 = unauthorised_import_penalty(10.0, 3.0, false).expect("3x");
    assert_f64(at_3.rate_eur_per_tco2e, 300.0);
    assert_f64(at_3.total_eur, 3000.0);
    assert!(!at_3.reduction_applied);
    assert!(at_3.surrender_obligation_remains);

    // Upper bound of the band: 5x EUR 100 = EUR 500/t.
    let at_5 = unauthorised_import_penalty(10.0, 5.0, false).expect("5x");
    assert_f64(at_5.rate_eur_per_tco2e, 500.0);
    assert_f64(at_5.total_eur, 5000.0);

    // Outside 3.0..=5.0 the multiplier is illegal.
    assert!(unauthorised_import_penalty(10.0, 2.9, false).is_err());
    assert!(unauthorised_import_penalty(10.0, 5.1, false).is_err());
    assert!(unauthorised_import_penalty(10.0, f64::NAN, false).is_err());

    // The reducible case (excess <= 10% of the threshold) floors the
    // multiplier at 3.0 and flags the reduction.
    let reduced = unauthorised_import_penalty(10.0, 5.0, true).expect("reduced");
    assert_f64(reduced.rate_eur_per_tco2e, 300.0);
    assert_f64(reduced.total_eur, 3000.0);
    assert!(reduced.reduction_applied);
    assert!(
        reduced.surrender_obligation_remains,
        "even the reduced Art 26(2a) penalty never releases the surrender obligation"
    );
}

// ---------------------------------------------------------------------------
// R39 — Member-State penalty variations (Art 26(2))
// ---------------------------------------------------------------------------

/// Art 26(2) CBAM Reg: penalties for other non-compliance are set by each
/// Member State. The jurisdiction table resolves the national framework per
/// country, case-insensitively, and rejects unknown jurisdictions.
#[test]
fn jurisdiction_table_lookup() {
    let table = vec![
        JurisdictionPenalty {
            country: "DE".into(),
            authority: "DEHSt".into(),
            framework: "DE national enforcement under Art 26(2)".into(),
            fine_range_eur: Some((50_000.0, 500_000.0)),
        },
        JurisdictionPenalty {
            country: "FR".into(),
            authority: "DGEC".into(),
            framework: "FR national enforcement under Art 26(2)".into(),
            fine_range_eur: None,
        },
    ];

    let de = jurisdiction_penalty(&table, "DE").expect("DE row");
    assert_eq!(de.authority, "DEHSt");
    assert_eq!(
        jurisdiction_penalty(&table, "de")
            .expect("lowercase de")
            .country,
        "DE",
        "country match is case-insensitive"
    );

    let fr = jurisdiction_penalty(&table, "FR").expect("FR row");
    assert_eq!(fr.authority, "DGEC");
    assert_eq!(
        jurisdiction_penalty(&table, "fr")
            .expect("lowercase fr")
            .country,
        "FR"
    );

    // Unknown jurisdiction: error names the country.
    let err = jurisdiction_penalty(&table, "XX").expect_err("unknown country");
    assert!(
        err.to_string().contains("XX"),
        "the error must name the missing country, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// R37 — NCA financial guarantee projection (IR (EU) 2025/2549, Art 4-6)
// ---------------------------------------------------------------------------

/// IR (EU) 2025/2549, Art 4-6: declarants not established for the two prior
/// financial years must lodge a financial security covering the projected
/// certificate-surrender exposure; sufficiency is boundary-inclusive.
#[test]
fn guarantee_sufficiency_flag() {
    let short = GuaranteeProjection {
        year1_exposure_eur: 6_000.0,
        year2_exposure_eur: 6_000.0,
        required_eur: 12_000.0,
        lodged_eur: 10_000.0,
    };
    assert!(!guarantee_sufficient(&short), "10k lodged vs 12k required");

    let exact = GuaranteeProjection {
        lodged_eur: 12_000.0,
        ..short
    };
    assert!(
        guarantee_sufficient(&exact),
        "lodged == required is sufficient (boundary inclusive)"
    );

    let over = GuaranteeProjection {
        lodged_eur: 15_000.0,
        ..short
    };
    assert!(guarantee_sufficient(&over));
}

// ---------------------------------------------------------------------------
// R40 — NCA communications log (Arts 11 & 17)
// ---------------------------------------------------------------------------

/// Arts 11 & 17 CBAM Reg: NCA information requests and hearing notices
/// carry a response deadline; the counter counts calendar days from today
/// to the deadline, negative once overdue, and rejects unparseable dates.
#[test]
fn nca_deadline_counter() {
    let notice = NcaCommunication {
        id: "NCA-2027-0842".into(),
        notice_kind: "INFORMATION_REQUEST".into(),
        received_iso: "2027-08-01".into(),
        respond_by_iso: "2027-08-29".into(),
    };

    // Received day: 28 days remaining.
    assert_eq!(
        notice.days_remaining("2027-08-01").expect("day of receipt"),
        28
    );
    // Deadline day: 0 (due today).
    assert_eq!(
        notice.days_remaining("2027-08-29").expect("deadline day"),
        0
    );
    // Day after: overdue (-1).
    assert_eq!(
        notice
            .days_remaining("2027-08-30")
            .expect("day after deadline"),
        -1
    );

    // Unparseable dates are errors, never silent miscounts.
    assert!(notice.days_remaining("2027-13-01").is_err());
    assert!(notice.days_remaining("not-a-date").is_err());

    // A malformed stored deadline must also surface as an error.
    let broken = NcaCommunication {
        respond_by_iso: "2027-02-30".into(),
        ..notice.clone()
    };
    assert!(broken.days_remaining("2027-08-01").is_err());
}

// ---------------------------------------------------------------------------
// R42 — authorised-declarant status lifecycle (Art 17(8))
// ---------------------------------------------------------------------------

/// Art 17(8) CBAM Reg: the authorised-declarant status state machine.
/// Legal transitions: Active+Suspend -> Suspended, Suspended+Revoke ->
/// Revoked, Active+Revoke -> Revoked, Suspended+Reinstate -> Active.
/// Everything else is an illegal transition. Filing is allowed only in the
/// Active state — suspension/revocation freezes filings.
#[test]
fn status_lifecycle_freezes_filings() {
    use AuthorisationStatus::{Active, Revoked, Suspended};
    use EnforcementAction::{Reinstate, Revoke, Suspend};

    // The legal transition matrix.
    assert_eq!(
        apply_enforcement(Active, Suspend).expect("legal"),
        Suspended
    );
    assert_eq!(
        apply_enforcement(Suspended, Revoke).expect("legal"),
        Revoked
    );
    assert_eq!(apply_enforcement(Active, Revoke).expect("legal"), Revoked);
    assert_eq!(
        apply_enforcement(Suspended, Reinstate).expect("legal"),
        Active
    );

    // Every illegal transition errors.
    for (current, action) in [
        (Active, Reinstate),
        (Revoked, Reinstate),
        (Suspended, Suspend),
        (Revoked, Suspend),
    ] {
        assert!(
            apply_enforcement(current, action).is_err(),
            "expected {current:?} + {action:?} to be an illegal transition"
        );
    }

    // Filings only flow while Active; suspension/revocation freezes them.
    assert!(filing_allowed(Active));
    assert!(!filing_allowed(Suspended));
    assert!(!filing_allowed(Revoked));
}
