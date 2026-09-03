// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Contract tests for the `customs` module: the Box 37 lifecycle (R15),
//! IPR discharge tracing (UCC Art 223), and post-clearance adjustments
//! (R41, UCC Art 48 / CBAM Art 19).

use kaimeter_core::customs::{
    apply_ipr_discharge, classify, counts_toward_net_mass, post_clearance_delta, promote_deferred,
    CbamStatus, DeferredRecord, IprDischarge, IprImport, LiableRecord, PostClearanceRevision,
};
use kaimeter_core::domain::DomainError;

/// R15 / UCC Arts 215-237: goods under customs warehousing (71 00) are
/// CBAM_DEFERRED — no liability attaches while warehoused. CBAM attaches only
/// on release for free circulation by the secondary declaration (40 71); the
/// promotion locks the tax-point date to that release date and carries the
/// warehoused mass through unchanged.
///
/// REGULATORY PIN: CBAM_DEFERRED -> CBAM_LIABLE, tax point = release date.
#[test]
fn warehouse_promotion_locks_tax_point() {
    // Precondition: 71 00 is the deferred regime.
    assert!(matches!(classify("71 00"), Ok(CbamStatus::Deferred)));
    // The promoting secondary declaration is itself free circulation.
    assert!(matches!(classify("40 71"), Ok(CbamStatus::Liable)));

    let record = DeferredRecord {
        consignment_id: "CONS-1".to_string(),
        cn_code: "72083900".to_string(),
        net_mass_kg: 12_500.0,
        country_of_origin: "CN".to_string(),
        entry_date: "2026-05-11".to_string(),
        status: CbamStatus::Deferred,
    };

    // Malformed release date is rejected before any promotion happens.
    assert!(matches!(
        promote_deferred(&record, "2026-13-40"),
        Err(DomainError::InvalidImportDate(d)) if d == "2026-13-40"
    ));

    let liable = promote_deferred(&record, "2026-09-01").expect("promotion succeeds");
    assert_eq!(
        liable,
        LiableRecord {
            consignment_id: "CONS-1".to_string(),
            cn_code: "72083900".to_string(),
            net_mass_kg: 12_500.0,
            country_of_origin: "CN".to_string(),
            tax_point_date: "2026-09-01".to_string(),
            origin_exempt: false,
        },
        "tax point locked to the 40 71 release date; mass carried through"
    );
}

/// R15 / UCC Art 215: only records actually in the warehousing state
/// (CBAM_DEFERRED) may be promoted. A promotion attempt on any other status
/// is rejected with [`DomainError::InvalidPromotion`], naming the consignment.
#[test]
fn promotion_of_non_deferred_record_rejected() {
    let mut record = DeferredRecord {
        consignment_id: "CONS-2".to_string(),
        cn_code: "85030010".to_string(),
        net_mass_kg: 400.0,
        country_of_origin: "TR".to_string(),
        entry_date: "2026-06-01".to_string(),
        status: CbamStatus::Deferred,
    };
    record.status = CbamStatus::Liable; // already released: nothing to promote

    assert!(matches!(
        promote_deferred(&record, "2026-09-01"),
        Err(DomainError::InvalidPromotion(id)) if id == "CONS-2"
    ));
}

/// R15 / UCC Art 223: when IPR goods enter free circulation on discharge,
/// liability attaches to the ORIGINAL precursor import record with the
/// transformation yield ratio already applied back by the caller; the tax
/// point is the discharge release date. The equivalent-goods substitution
/// flag is carried for audit only — it links the discharge to the specific
/// import declaration and never changes the mass math.
#[test]
fn ipr_discharge_yields_liable_mass() {
    // Malformed release date is rejected.
    let import = IprImport {
        declaration_id: "IMPA-9".to_string(),
        cn_code: "76011000".to_string(),
        net_mass_kg: 1_000.0,
        country_of_origin: "IN".to_string(),
    };
    let bad_date = IprDischarge {
        declaration_id: "IMPA-9".to_string(),
        discharged_mass_kg: 800.0,
        equivalent_goods: false,
        release_date: "15/03/2027".to_string(),
    };
    assert!(matches!(
        apply_ipr_discharge(&import, &bad_date),
        Err(DomainError::InvalidImportDate(d)) if d == "15/03/2027"
    ));

    let discharge = IprDischarge {
        declaration_id: "IMPA-9".to_string(),
        discharged_mass_kg: 800.0,
        equivalent_goods: false,
        release_date: "2027-03-15".to_string(),
    };
    let liable = apply_ipr_discharge(&import, &discharge).expect("discharge succeeds");
    assert_eq!(
        liable,
        LiableRecord {
            consignment_id: "IMPA-9".to_string(),
            cn_code: "76011000".to_string(),
            net_mass_kg: 800.0,
            country_of_origin: "IN".to_string(),
            tax_point_date: "2027-03-15".to_string(),
            origin_exempt: false,
        },
        "discharged mass (yield-adjusted by caller) is the liable mass"
    );

    // Substitution under Art 223 (equivalent goods) links the discharge to the
    // same declaration but never changes the math.
    let substituted = IprDischarge {
        equivalent_goods: true,
        ..discharge.clone()
    };
    let liable_substituted = apply_ipr_discharge(&import, &substituted).expect("discharge");
    assert_eq!(liable, liable_substituted, "equivalent_goods is audit-only");
}

/// R15 / UCC Art 223: the discharged precursor mass may never exceed the
/// imported precursor mass; over-discharge is rejected with the declaration
/// id and both masses carried in the error for the audit trail.
#[test]
fn ipr_discharge_exceeding_import_rejected() {
    let import = IprImport {
        declaration_id: "IMPA-10".to_string(),
        cn_code: "76011000".to_string(),
        net_mass_kg: 1_000.0,
        country_of_origin: "IN".to_string(),
    };
    let discharge = IprDischarge {
        declaration_id: "IMPA-10".to_string(),
        discharged_mass_kg: 1_001.0,
        equivalent_goods: false,
        release_date: "2027-03-15".to_string(),
    };

    assert!(matches!(
        apply_ipr_discharge(&import, &discharge),
        Err(DomainError::DischargeExceedsImport {
            ref declaration_id,
            imported_kg,
            discharged_kg,
        }) if declaration_id == "IMPA-10"
            && imported_kg == 1_000.0
            && discharged_kg == 1_001.0
    ));
}

/// R41 / UCC Art 48 + CBAM Art 19: post-clearance revisions re-evaluate the
/// surrender duty in BOTH directions — a revised entry may add liable mass
/// (additional certificates owed) or remove it (over-declaration corrected);
/// the delta is returned for the caller to recompute obligations and record
/// on the audit trail. A negative revised mass is rejected.
#[test]
fn post_clearance_revision_flows_both_ways() {
    let up = PostClearanceRevision {
        consignment_id: "CONS-3".to_string(),
        original_net_mass_kg: 500.0,
        revised_net_mass_kg: 600.0,
        revision_date: "2027-01-20".to_string(),
        reason: "post-clearance audit: mass understated".to_string(),
    };
    assert_eq!(post_clearance_delta(&up).expect("upward delta"), 100.0);

    let down = PostClearanceRevision {
        consignment_id: "CONS-3".to_string(),
        original_net_mass_kg: 500.0,
        revised_net_mass_kg: 400.0,
        revision_date: "2027-01-20".to_string(),
        reason: "post-clearance audit: mass overstated".to_string(),
    };
    assert_eq!(post_clearance_delta(&down).expect("downward delta"), -100.0);

    let negative = PostClearanceRevision {
        consignment_id: "CONS-3".to_string(),
        original_net_mass_kg: 500.0,
        revised_net_mass_kg: -5.0,
        revision_date: "2027-01-20".to_string(),
        reason: "impossible".to_string(),
    };
    assert!(matches!(
        post_clearance_delta(&negative),
        Err(DomainError::NegativeMass(v)) if v == -5.0
    ));

    let bad_date = PostClearanceRevision {
        consignment_id: "CONS-3".to_string(),
        original_net_mass_kg: 500.0,
        revised_net_mass_kg: 600.0,
        revision_date: "2027-02-30".to_string(),
        reason: "impossible calendar date".to_string(),
    };
    assert!(matches!(
        post_clearance_delta(&bad_date),
        Err(DomainError::InvalidImportDate(d)) if d == "2027-02-30"
    ));
}

/// R15 end-to-end: the full lifecycle chain — a 71 00 warehousing declaration
/// classifies CBAM_DEFERRED, a 40 71 release promotes it to a liable record
/// with the tax point locked, and the promoted record counts toward the 50 t
/// de-minimis net-mass tracking (the warehoused state did not).
#[test]
fn warehouse_chain_classify_promote_then_counts_toward_net_mass() {
    let status = classify("71 00").expect("71 00 is a supported code");
    assert_eq!(status, CbamStatus::Deferred);

    let record = DeferredRecord {
        consignment_id: "CONS-4".to_string(),
        cn_code: "25232900".to_string(),
        net_mass_kg: 60_000.0,
        country_of_origin: "CN".to_string(),
        entry_date: "2026-04-02".to_string(),
        status,
    };
    assert!(
        !counts_toward_net_mass(record.status, false),
        "CBAM_DEFERRED does not count toward net mass while warehoused"
    );

    let liable = promote_deferred(&record, "2026-08-17").expect("40 71 release");
    assert_eq!(liable.tax_point_date, "2026-08-17");
    assert!(
        counts_toward_net_mass(CbamStatus::Liable, liable.origin_exempt),
        "after release for free circulation the mass counts"
    );
}
