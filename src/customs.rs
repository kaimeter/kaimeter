// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Customs procedure (Box 37) rule engine (R15), post-clearance adjustments
//! (R41), outward-processing relief (R44), and origin/military exemptions
//! (R43/R45).
//!
//! The Box 37 classification table below is frozen law (Union Customs Code)
//! and pinned by tests; the lifecycle machinery (IPR discharge tracing,
//! warehousing promotion, OPR math) builds on it.

use serde::{Deserialize, Serialize};

use crate::calendar::parse_iso;
use crate::domain::errors::DomainError;

// ---------------------------------------------------------------------------
// Box 37 classification (frozen; UCC procedure-code semantics)
// ---------------------------------------------------------------------------

/// How a customs procedure attaches CBAM obligations to a consignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CbamStatus {
    /// Released for free circulation (40 00, 40 71): CBAM_LIABLE.
    Liable,
    /// Customs warehousing / free zone (71 00): CBAM_DEFERRED — no liability,
    /// no net-mass tracking until released by a 40 71 declaration.
    Deferred,
    /// Inward processing (51 00): tracked until IPR discharge.
    IprTracked,
    /// Outward processing export (61 21) / re-import (40 21): OPR assessment.
    OprTracked,
    /// Returned Union goods (F-codes, UCC Art 203) and other exclusions:
    /// re-imported unchanged goods are not third-country imports.
    Excluded,
}

/// Classify a Box 37 procedure code into its CBAM status.
///
/// The code may carry the two-letter additional code separated by space,
/// slash or nothing (`"40 00"`, `"4000"`, `"40/00"` are all accepted).
///
/// # Errors
///
/// [`DomainError::UnknownProcedureCode`] when the code is not a supported
/// 4-digit procedure.
pub fn classify(procedure_code: &str) -> Result<CbamStatus, DomainError> {
    let digits: String = procedure_code
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    match digits.as_str() {
        "4000" => Ok(CbamStatus::Liable),
        "4071" => Ok(CbamStatus::Liable), // release from warehousing: promotion
        "7100" => Ok(CbamStatus::Deferred),
        "5100" => Ok(CbamStatus::IprTracked),
        "6121" | "4021" => Ok(CbamStatus::OprTracked),
        _ => {
            // Returned-Union-goods F-codes (UCC Art 203): 40 51..40 55 family
            // carry an F additional code; treat the documented F-family as
            // excluded. Any other code is unsupported.
            let has_f = procedure_code.contains('F') || procedure_code.contains('f');
            if has_f && digits.starts_with("40") {
                Ok(CbamStatus::Excluded)
            } else {
                Err(DomainError::UnknownProcedureCode(
                    procedure_code.to_string(),
                ))
            }
        }
    }
}

/// True when the status counts toward the 50 t de-minimis net-mass tracking.
/// Deferred, excluded and in-processing regimes do not count (R15); exempt
/// origins never count either (R43/R45).
#[must_use]
pub fn counts_toward_net_mass(status: CbamStatus, origin_exempt: bool) -> bool {
    if origin_exempt {
        return false;
    }
    matches!(status, CbamStatus::Liable)
}

// ---------------------------------------------------------------------------
// Warehousing promotion (R15): 71 00 CBAM_DEFERRED -> 40 71 CBAM_LIABLE
// ---------------------------------------------------------------------------

/// A consignment record held in customs warehousing or a free zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeferredRecord {
    /// Stable consignment identifier.
    pub consignment_id: String,
    /// 8-digit CN code.
    pub cn_code: String,
    /// Net mass in kilograms (warehouse quantity).
    pub net_mass_kg: f64,
    /// ISO-3166 alpha-2 country of origin.
    pub country_of_origin: String,
    /// Date the goods entered the warehousing regime, ISO `YYYY-MM-DD`.
    pub entry_date: String,
    /// Current status — always `Deferred` at construction.
    pub status: CbamStatus,
}

/// Promote a warehousing record to CBAM_LIABLE via a secondary customs
/// declaration (40 71), locking the tax-point date to the release date.
///
/// # Errors
///
/// [`DomainError::InvalidPromotion`] when the record is not `Deferred`.
pub fn promote_deferred(
    record: &DeferredRecord,
    release_date_iso: &str,
) -> Result<LiableRecord, DomainError> {
    // R15 (UCC Arts 215/237): only a CBAM_DEFERRED warehousing record may be
    // promoted by the secondary (40 71) declaration; anything else is an
    // invalid promotion.
    if record.status != CbamStatus::Deferred {
        return Err(DomainError::InvalidPromotion(record.consignment_id.clone()));
    }
    // CBAM attaches on release for free circulation, so the release date IS
    // the tax point — it must be a real calendar date before it is locked.
    parse_iso(release_date_iso)?;
    Ok(LiableRecord {
        consignment_id: record.consignment_id.clone(),
        cn_code: record.cn_code.clone(),
        net_mass_kg: record.net_mass_kg,
        country_of_origin: record.country_of_origin.clone(),
        tax_point_date: release_date_iso.to_string(),
        // The DeferredRecord carries no exemption flags: exempt consignments
        // (R43/R45) never enter warehousing tracking at all, so everything
        // promoted out of it is liable without exemption.
        origin_exempt: false,
    })
}

/// A consignment carrying live CBAM obligations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiableRecord {
    /// Stable consignment identifier.
    pub consignment_id: String,
    /// 8-digit CN code.
    pub cn_code: String,
    /// Net mass in kilograms subject to CBAM.
    pub net_mass_kg: f64,
    /// ISO-3166 alpha-2 country of origin.
    pub country_of_origin: String,
    /// The date CBAM obligations attach (release for free circulation),
    /// ISO `YYYY-MM-DD` — the locked tax-point date.
    pub tax_point_date: String,
    /// Exemption flags applied at promotion/classification time.
    pub origin_exempt: bool,
}

// ---------------------------------------------------------------------------
// IPR discharge tracing (R15): 51 00 -> free circulation with yield ratios
// ---------------------------------------------------------------------------

/// An inward-processing import record awaiting discharge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IprImport {
    /// Import declaration identifier (the precursor import record).
    pub declaration_id: String,
    /// 8-digit CN code of the imported precursor.
    pub cn_code: String,
    /// Net mass in kilograms imported under IPR.
    pub net_mass_kg: f64,
    /// Country of origin of the precursor.
    pub country_of_origin: String,
}

/// Discharge of processed IPR goods into free circulation (UCC Art 223
/// supports equivalent-goods substitution: the discharge links back to the
/// specific declaration whose mass carries the yield-adjusted liability).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IprDischarge {
    /// The import declaration carrying the liability.
    pub declaration_id: String,
    /// Mass of precursor goods deemed released for free circulation, kg
    /// (transformation yield ratio applied back to the import record).
    pub discharged_mass_kg: f64,
    /// Whether equivalent goods were substituted for the processed ones.
    pub equivalent_goods: bool,
    /// Release date, ISO `YYYY-MM-DD` (the tax-point date).
    pub release_date: String,
}

/// Apply an IPR discharge to an import record, returning the now-liable mass.
///
/// The discharged mass may not exceed the imported mass; the yield ratio is
/// resolved by the caller into discharged precursor mass.
///
/// # Errors
///
/// [`DomainError::DischargeExceedsImport`] when discharging more mass than
/// was imported; [`DomainError::InvalidImportDate`] on a bad release date.
pub fn apply_ipr_discharge(
    import: &IprImport,
    discharge: &IprDischarge,
) -> Result<LiableRecord, DomainError> {
    // R15 (UCC Art 223): the discharged precursor mass may never exceed the
    // imported precursor mass. The caller has already applied the
    // transformation yield ratio back to the import record, so
    // `discharged_mass_kg` is precursor mass, not processed-goods mass.
    if discharge.discharged_mass_kg > import.net_mass_kg {
        return Err(DomainError::DischargeExceedsImport {
            declaration_id: discharge.declaration_id.clone(),
            imported_kg: import.net_mass_kg,
            discharged_kg: discharge.discharged_mass_kg,
        });
    }
    // The discharge release date is the tax point.
    parse_iso(&discharge.release_date)?;
    // The `equivalent_goods` flag (Art 223 substitution) is carried for audit
    // on the discharge record itself — it links the discharge back to this
    // specific import declaration and never changes the mass math.
    Ok(LiableRecord {
        consignment_id: discharge.declaration_id.clone(),
        cn_code: import.cn_code.clone(),
        net_mass_kg: discharge.discharged_mass_kg,
        country_of_origin: import.country_of_origin.clone(),
        tax_point_date: discharge.release_date.clone(),
        origin_exempt: false,
    })
}

// ---------------------------------------------------------------------------
// Post-clearance adjustment (R41, UCC Art 48 / CBAM Art 19)
// ---------------------------------------------------------------------------

/// A post-clearance revision of an already-cleared customs entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostClearanceRevision {
    /// The consignment being revised.
    pub consignment_id: String,
    /// Original (as-cleared) net mass, kg.
    pub original_net_mass_kg: f64,
    /// Revised net mass, kg.
    pub revised_net_mass_kg: f64,
    /// Effective date of the revision, ISO `YYYY-MM-DD`.
    pub revision_date: String,
    /// Reason code for the audit trail.
    pub reason: String,
}

/// Recompute the liability delta implied by a post-clearance revision.
///
/// Returns the mass delta (positive = more liable mass, negative = less) in
/// kilograms; the caller recomputes emissions and certificate obligations
/// and records the delta on the audit trail.
///
/// # Errors
///
/// [`DomainError::NegativeMass`] when the revised mass is negative.
pub fn post_clearance_delta(revision: &PostClearanceRevision) -> Result<f64, DomainError> {
    // R41 (CBAM Art 19 re-evaluation): a revised entry can never declare a
    // negative mass.
    if revision.revised_net_mass_kg < 0.0 {
        return Err(DomainError::NegativeMass(revision.revised_net_mass_kg));
    }
    // UCC Art 48: a post-clearance revision is a dated customs act; the date
    // must be a real calendar day.
    parse_iso(&revision.revision_date)?;
    // Positive delta = additional liable mass, negative = over-declaration
    // corrected; the caller recomputes emissions/obligations and records the
    // delta on the audit trail.
    Ok(revision.revised_net_mass_kg - revision.original_net_mass_kg)
}

// ---------------------------------------------------------------------------
// Outward processing relief (R44, UCC Art 259 / CBAM Art 2(2))
// ---------------------------------------------------------------------------

/// An outward-processing assessment: goods exported for processing abroad
/// (61 21) and re-imported (40 21) are assessed on the offshore value-added
/// processing only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OprAssessment {
    /// Embedded emissions of the exported EU precursor (baseline), tCO2e.
    pub exported_baseline_tco2e: f64,
    /// Emissions of the offshore processing operations, tCO2e.
    pub offshore_processing_tco2e: f64,
    /// Link back to the export declaration for traceability.
    pub export_declaration_id: String,
}

/// Net CBAM-liable emissions for an OPR re-import: offshore delta only
/// (offshore processing emissions minus the exported EU precursor baseline,
/// floored at zero — the EU segment never adds liability via this route).
#[must_use]
pub fn opr_net_emissions(assessment: &OprAssessment) -> f64 {
    (assessment.offshore_processing_tco2e - assessment.exported_baseline_tco2e).max(0.0)
}

// ---------------------------------------------------------------------------
// Origin exemptions (R43 linked markets, R45 military use)
// ---------------------------------------------------------------------------

/// Origin-exemption flags carried on a consignment. Exempt consignments never
/// count toward net mass or obligations; eligibility is data, not code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginExemptions {
    /// Goods from third countries whose emissions trading systems are fully
    /// linked to the EU ETS (Art 2(4), incl. linked-market electricity).
    pub ets_linked_market: bool,
    /// Goods intended for the activities of military forces (Art 2(3)(c),
    /// evidenced per the customs regime).
    pub military_use: bool,
}

impl OriginExemptions {
    /// No exemption claimed.
    #[must_use]
    pub fn none() -> Self {
        Self {
            ets_linked_market: false,
            military_use: false,
        }
    }

    /// True when any exemption applies.
    #[must_use]
    pub fn is_exempt(&self) -> bool {
        self.ets_linked_market || self.military_use
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REGULATORY PIN: the Box 37 -> CBAM status table (R15).
    #[test]
    fn box37_classification_table_is_pinned() {
        assert_eq!(classify("40 00").unwrap(), CbamStatus::Liable);
        assert_eq!(classify("4000").unwrap(), CbamStatus::Liable);
        assert_eq!(
            classify("40/71").unwrap(),
            CbamStatus::Liable,
            "40 71 release from warehousing"
        );
        assert_eq!(classify("71 00").unwrap(), CbamStatus::Deferred);
        assert_eq!(classify("51 00").unwrap(), CbamStatus::IprTracked);
        assert_eq!(classify("61 21").unwrap(), CbamStatus::OprTracked);
        assert_eq!(classify("40 21").unwrap(), CbamStatus::OprTracked);
        assert_eq!(
            classify("40 51 F").unwrap(),
            CbamStatus::Excluded,
            "returned Union goods (UCC Art 203)"
        );
    }

    #[test]
    fn unknown_procedures_are_rejected() {
        assert!(matches!(
            classify("99 99"),
            Err(DomainError::UnknownProcedureCode(c)) if c == "99 99"
        ));
        assert!(classify("garbage").is_err());
    }

    /// REGULATORY PIN: only free-circulation goods count toward net mass;
    /// exempt origins never count (R15/R43/R45).
    #[test]
    fn net_mass_tracking_filter() {
        assert!(counts_toward_net_mass(CbamStatus::Liable, false));
        assert!(
            !counts_toward_net_mass(CbamStatus::Liable, true),
            "exempt origin"
        );
        assert!(
            !counts_toward_net_mass(CbamStatus::Deferred, false),
            "71 00 deferred"
        );
        assert!(
            !counts_toward_net_mass(CbamStatus::IprTracked, false),
            "51 00 in processing"
        );
        assert!(!counts_toward_net_mass(CbamStatus::OprTracked, false));
        assert!(
            !counts_toward_net_mass(CbamStatus::Excluded, false),
            "F-code returned goods"
        );
    }

    /// REGULATORY PIN: OPR assesses the offshore value-added only (R44).
    #[test]
    fn opr_assesses_offshore_delta_only() {
        let a = OprAssessment {
            exported_baseline_tco2e: 5.0,
            offshore_processing_tco2e: 2.0,
            export_declaration_id: "EXP-1".into(),
        };
        assert_eq!(
            opr_net_emissions(&a),
            0.0,
            "EU baseline is never added back"
        );
        let b = OprAssessment {
            exported_baseline_tco2e: 2.0,
            offshore_processing_tco2e: 5.0,
            export_declaration_id: "EXP-2".into(),
        };
        assert!((opr_net_emissions(&b) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn origin_exemption_flags_or_together() {
        assert!(!OriginExemptions::none().is_exempt());
        assert!(OriginExemptions {
            ets_linked_market: true,
            military_use: false,
        }
        .is_exempt());
        assert!(OriginExemptions {
            ets_linked_market: false,
            military_use: true,
        }
        .is_exempt());
    }

    /// Boundary of UCC Art 223 (R15): discharging the FULL imported mass is
    /// legal — only over-discharge is rejected.
    #[test]
    fn full_ipr_discharge_is_allowed() {
        let import = IprImport {
            declaration_id: "IMPA-11".into(),
            cn_code: "76011000".into(),
            net_mass_kg: 1_000.0,
            country_of_origin: "IN".into(),
        };
        let discharge = IprDischarge {
            declaration_id: "IMPA-11".into(),
            discharged_mass_kg: 1_000.0,
            equivalent_goods: false,
            release_date: "2026-09-15".into(),
        };
        let liable = apply_ipr_discharge(&import, &discharge).expect("full discharge");
        assert_eq!(liable.net_mass_kg, 1_000.0);
        assert_eq!(liable.consignment_id, "IMPA-11");
    }

    /// Boundary of R41: a revision that changes nothing yields a zero delta
    /// (still a valid, dated customs act).
    #[test]
    fn no_change_revision_yields_zero_delta() {
        let same = PostClearanceRevision {
            consignment_id: "CONS-5".into(),
            original_net_mass_kg: 500.0,
            revised_net_mass_kg: 500.0,
            revision_date: "2027-01-20".into(),
            reason: "clerical confirmation".into(),
        };
        assert_eq!(post_clearance_delta(&same).expect("zero delta"), 0.0);
    }
}
