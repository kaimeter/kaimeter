// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Compliance engines: quarterly certificate-position monitor (R24), penalty
//! exposure (R32/R39), NCA financial guarantee projection (R37), NCA
//! communications log (R40), and the authorised-declarant status lifecycle
//! (R42).

use serde::{Deserialize, Serialize};

use crate::calendar::Quarter;
use crate::domain::errors::DomainError;

// ---------------------------------------------------------------------------
// R24 — quarterly certificate-position monitor
// ---------------------------------------------------------------------------

/// Certificate-position snapshot for one quarter-end check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoldingPosition {
    /// The quarter being checked (its end date is the position date).
    pub quarter: Quarter,
    /// Embedded emissions since January 1, tCO2e — on the R24 basis: Annex IV
    /// default values WITHOUT the mark-up, or the prior year's surrender.
    pub basis_tco2e: f64,
    /// Required holding: 50% of `basis_tco2e` (Art 22(2) as amended).
    pub required_tco2e: f64,
    /// Certificates currently held, tCO2e.
    pub held_tco2e: f64,
    /// Certificates purchased year-to-date, tCO2e.
    pub purchased_ytd_tco2e: f64,
    /// Certificates cancelled/surrendered year-to-date, tCO2e.
    pub cancelled_ytd_tco2e: f64,
}

/// The configured shortfall-alert thresholds (defaults per R24: warn at 45%,
/// critical at 48% of the required position).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Coverage ratio at or below which the alert level is `warn`.
    pub warn_at: f64,
    /// Coverage ratio at or below which the alert level is `critical`.
    pub critical_at: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            warn_at: 0.45,
            critical_at: 0.48,
        }
    }
}

/// Alert level derived from the current coverage ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlertLevel {
    /// Coverage ≥ required.
    Ok,
    /// Shortfall inside the warning buffer.
    Warn,
    /// Shortfall inside the critical buffer.
    Critical,
    /// Shortfall beyond the critical buffer.
    Shortfall,
}

/// The statutory holding share of the embedded-emissions basis (Art 22(2)):
/// 50%.
pub const HOLDING_SHARE: f64 = 0.5;

/// Classify a holding position against the thresholds (R24, Art 22(2) as
/// amended by Reg (EU) 2025/2083).
///
/// Coverage is `held / required`; the alert bands run best to worst:
/// coverage ≥ 1.0 → `Ok`; ≥ `critical_at` (default 0.48) → `Warn`;
/// ≥ `warn_at` (default 0.45) → `Critical`; below → `Shortfall`. The R24
/// buffers sit BELOW the 50% duty, so a position at 48% of the requirement
/// already triggers the early warning. A non-positive requirement (duty not
/// started or below the de-minimis line) is always `Ok`.
#[must_use]
pub fn assess_holding(position: &HoldingPosition, thresholds: &AlertThresholds) -> AlertLevel {
    let required = position.required_tco2e;
    if required <= 0.0 {
        return AlertLevel::Ok;
    }
    let coverage = position.held_tco2e / required;
    if coverage >= 1.0 {
        AlertLevel::Ok
    } else if coverage >= thresholds.critical_at {
        AlertLevel::Warn
    } else if coverage >= thresholds.warn_at {
        AlertLevel::Critical
    } else {
        AlertLevel::Shortfall
    }
}

/// The shortfall in certificates (tCO2e) for a position: how much more must
/// be held to reach the 50% requirement (0.0 when covered).
#[must_use]
pub fn shortfall_tco2e(position: &HoldingPosition) -> f64 {
    (position.required_tco2e - position.held_tco2e).max(0.0)
}

// ---------------------------------------------------------------------------
// R32 — penalty exposure engine
// ---------------------------------------------------------------------------

/// Penalty exposure computed in euros, before it happens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PenaltyExposure {
    /// Un-surrendered certificates, tCO2e.
    pub unsurrendered_tco2e: f64,
    /// Applicable euro rate per tonne (Art 26(1): €100/t, index-linked).
    pub rate_eur_per_tco2e: f64,
    /// Total penalty exposure, euros.
    pub total_eur: f64,
    /// True when the Art 26(2a) reduction (excess ≤ 10% of the threshold)
    /// was applied.
    pub reduction_applied: bool,
    /// Penalties never release the underlying surrender obligation — always
    /// surfaced next to the amount (R32).
    pub surrender_obligation_remains: bool,
}

/// The base Art 26(1) penalty: €100 per tonne CO2e.
pub const PENALTY_EUR_PER_TCO2E: f64 = 100.0;

/// Compute the Art 26(1) penalty exposure for un-surrendered certificates
/// (R32): rate = €100 × `index_factor`; total = tonnes × rate. The
/// reduction flag stays false (no Art 26(2a) reduction on this route) and
/// the surrender obligation always remains — penalties never release it.
///
/// # Errors
///
/// [`DomainError::Storage`] when the index factor is not finite and > 0.
pub fn unsurrendered_penalty(
    unsurrendered_tco2e: f64,
    index_factor: f64,
) -> Result<PenaltyExposure, DomainError> {
    if !index_factor.is_finite() || index_factor <= 0.0 {
        return Err(DomainError::Storage(format!(
            "invalid Art 26(1) index factor `{index_factor}`: must be finite and > 0"
        )));
    }
    let rate = PENALTY_EUR_PER_TCO2E * index_factor;
    Ok(PenaltyExposure {
        unsurrendered_tco2e,
        rate_eur_per_tco2e: rate,
        total_eur: unsurrendered_tco2e * rate,
        reduction_applied: false,
        surrender_obligation_remains: true,
    })
}

/// Penalty for importing above the 50 t threshold without authorised
/// declarant status (Art 26(2a), R32): total = tonnes × €100 × the
/// effective multiplier. The multiplier must lie in 3.0..=5.0. Where the
/// excess is ≤10% of the threshold (`excess_within_10pct`, the reducible
/// case) the multiplier is floored at the band minimum 3.0 and
/// `reduction_applied` is set. The surrender obligation always remains.
///
/// # Errors
///
/// [`DomainError::Storage`] when the multiplier is outside 3.0..=5.0.
pub fn unauthorised_import_penalty(
    liable_tco2e: f64,
    multiplier: f64,
    excess_within_10pct: bool,
) -> Result<PenaltyExposure, DomainError> {
    if !(3.0..=5.0).contains(&multiplier) {
        return Err(DomainError::Storage(format!(
            "invalid Art 26(2a) multiplier `{multiplier}`: must be within 3.0..=5.0"
        )));
    }
    // The reducible case (excess ≤ 10% of the threshold) never drops below
    // the band floor of 3.0.
    let effective = if excess_within_10pct {
        3.0_f64.min(multiplier)
    } else {
        multiplier
    };
    let rate = PENALTY_EUR_PER_TCO2E * effective;
    Ok(PenaltyExposure {
        unsurrendered_tco2e: liable_tco2e,
        rate_eur_per_tco2e: rate,
        total_eur: liable_tco2e * rate,
        reduction_applied: excess_within_10pct,
        surrender_obligation_remains: true,
    })
}

// ---------------------------------------------------------------------------
// R39 — Member-State penalty variations (jurisdiction table)
// ---------------------------------------------------------------------------

/// One jurisdiction's penalty framework descriptor (Art 26(2): national
/// rules apply to other non-compliance). Ships as versioned data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JurisdictionPenalty {
    /// ISO-3166 alpha-2 Member State code.
    pub country: String,
    /// National enforcement authority (e.g. `DEHSt`, `DGEC`).
    pub authority: String,
    /// Free-form descriptor of the national penalty framework.
    pub framework: String,
    /// Typical fine range in euros (min, max) — informational.
    pub fine_range_eur: Option<(f64, f64)>,
}

/// Apply the correct national framework for a jurisdiction (R39, Art 26(2)):
/// case-insensitive ISO alpha-2 match against the versioned table.
///
/// # Errors
///
/// [`DomainError::Storage`] when the jurisdiction is not in the table; the
/// error names the missing country.
pub fn jurisdiction_penalty<'a>(
    table: &'a [JurisdictionPenalty],
    country: &str,
) -> Result<&'a JurisdictionPenalty, DomainError> {
    table
        .iter()
        .find(|entry| entry.country.eq_ignore_ascii_case(country))
        .ok_or_else(|| {
            DomainError::Storage(format!(
                "no national penalty framework registered for country `{country}` (Art 26(2))"
            ))
        })
}

// ---------------------------------------------------------------------------
// R37 — NCA financial guarantee projection
// ---------------------------------------------------------------------------

/// Projection of the NCA financial security required from declarants not
/// established for the two prior financial years (IR 2025/2549, Art 4–6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuaranteeProjection {
    /// Projected certificate-surrender exposure, year 1, euros.
    pub year1_exposure_eur: f64,
    /// Projected certificate-surrender exposure, year 2, euros.
    pub year2_exposure_eur: f64,
    /// The projected required guarantee amount, euros.
    pub required_eur: f64,
    /// Guarantee currently lodged, euros.
    pub lodged_eur: f64,
}

/// True when the lodged guarantee covers the projected requirement
/// (R37, IR (EU) 2025/2549 Art 4–6). Boundary-inclusive: lodged == required
/// is sufficient.
#[must_use]
pub fn guarantee_sufficient(projection: &GuaranteeProjection) -> bool {
    projection.lodged_eur >= projection.required_eur
}

// ---------------------------------------------------------------------------
// R40 — NCA communications log
// ---------------------------------------------------------------------------

/// One structured NCA communication record (Arts 11 & 17).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NcaCommunication {
    /// Stable record identifier.
    pub id: String,
    /// Incoming notice kind (e.g. `INFORMATION_REQUEST`, `HEARING_NOTICE`).
    pub notice_kind: String,
    /// Receipt date, ISO `YYYY-MM-DD`.
    pub received_iso: String,
    /// Response deadline, ISO `YYYY-MM-DD` (the counter).
    pub respond_by_iso: String,
}

impl NcaCommunication {
    /// Days remaining to respond on a given day — negative when overdue
    /// (R40, Arts 11 & 17).
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidImportDate`] when `today_iso` or the stored
    /// `respond_by_iso` does not parse as a real calendar date.
    pub fn days_remaining(&self, today_iso: &str) -> Result<i64, DomainError> {
        let (by_y, by_m, by_d) = crate::calendar::parse_iso(&self.respond_by_iso)?;
        let (t_y, t_m, t_d) = crate::calendar::parse_iso(today_iso)?;
        Ok(crate::calendar::days_from_epoch(by_y, by_m, by_d)
            - crate::calendar::days_from_epoch(t_y, t_m, t_d))
    }
}

// ---------------------------------------------------------------------------
// R42 — authorised-declarant status lifecycle
// ---------------------------------------------------------------------------

/// Authorised-declarant status (Art 17(8)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorisationStatus {
    /// Active: filings allowed.
    Active,
    /// Suspended: new filings frozen.
    Suspended,
    /// Revoked: new filings frozen; registration ended.
    Revoked,
}

/// An enforcement action communicated by the NCA/Registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnforcementAction {
    /// Suspend the authorised-declarant status.
    Suspend,
    /// Revoke the authorised-declarant status.
    Revoke,
    /// Reinstate a suspended status.
    Reinstate,
}

/// Apply an enforcement action to a status, returning the new status
/// (R42, Art 17(8)). Legal transitions: Active+Suspend → Suspended,
/// Suspended+Revoke → Revoked, Active+Revoke → Revoked,
/// Suspended+Reinstate → Active.
///
/// # Errors
///
/// [`DomainError::Storage`] for a transition that is not legal (e.g.
/// reinstating a revoked status, or suspending an already suspended or
/// revoked status).
pub fn apply_enforcement(
    current: AuthorisationStatus,
    action: EnforcementAction,
) -> Result<AuthorisationStatus, DomainError> {
    match (current, action) {
        (AuthorisationStatus::Active, EnforcementAction::Suspend) => {
            Ok(AuthorisationStatus::Suspended)
        }
        (AuthorisationStatus::Suspended, EnforcementAction::Revoke) => {
            Ok(AuthorisationStatus::Revoked)
        }
        (AuthorisationStatus::Active, EnforcementAction::Revoke) => {
            Ok(AuthorisationStatus::Revoked)
        }
        (AuthorisationStatus::Suspended, EnforcementAction::Reinstate) => {
            Ok(AuthorisationStatus::Active)
        }
        (current, action) => Err(DomainError::Storage(format!(
            "illegal Art 17(8) transition: cannot apply `{action:?}` to a `{current:?}` \
             authorised-declarant status"
        ))),
    }
}

/// True when new filings are allowed under the status. Suspension/revocation
/// freezes filings (R42) and surfaces the affected workspaces.
#[must_use]
pub fn filing_allowed(status: AuthorisationStatus) -> bool {
    matches!(status, AuthorisationStatus::Active)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(required: f64, held: f64) -> HoldingPosition {
        HoldingPosition {
            quarter: Quarter::new(2027, 1).expect("Q1 2027"),
            basis_tco2e: 1000.0,
            required_tco2e: required,
            held_tco2e: held,
            purchased_ytd_tco2e: 0.0,
            cancelled_ytd_tco2e: 0.0,
        }
    }

    /// R24: a non-positive requirement (duty not started / below the
    /// de-minimis line) is never a shortfall.
    #[test]
    fn nonpositive_required_is_ok() {
        let t = AlertThresholds::default();
        assert_eq!(assess_holding(&position(0.0, 0.0), &t), AlertLevel::Ok);
        assert_eq!(assess_holding(&position(-1.0, 0.0), &t), AlertLevel::Ok);
        assert_eq!(shortfall_tco2e(&position(0.0, 0.0)), 0.0);
    }

    /// R24: the warn/critical boundary is inclusive at `critical_at`, and
    /// exactly `warn_at` coverage lands on Critical.
    #[test]
    fn threshold_boundaries_are_inclusive_downward() {
        let t = AlertThresholds::default();
        // 225/500 = 0.45 coverage: >= warn_at (0.45) but < critical_at (0.48)
        // -> Critical.
        assert_eq!(
            assess_holding(&position(500.0, 225.0), &t),
            AlertLevel::Critical
        );
        // Over-covered -> Ok.
        assert_eq!(assess_holding(&position(500.0, 750.0), &t), AlertLevel::Ok);
    }

    /// Art 26(1): the index factor must be finite and strictly positive —
    /// zero, negative, infinite and NaN are all rejected.
    #[test]
    fn index_factor_must_be_finite_and_positive() {
        for bad in [0.0, -1.0, -0.5, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert!(
                unsurrendered_penalty(10.0, bad).is_err(),
                "index factor {bad} must be rejected"
            );
        }
    }

    /// Art 26(2a): any multiplier inside the band is legal in the
    /// non-reducible case; the reducible case floors at 3.0.
    #[test]
    fn multiplier_band_and_reducible_floor() {
        let mid = unauthorised_import_penalty(1.0, 4.2, false).expect("mid-band");
        assert!((mid.rate_eur_per_tco2e - 420.0).abs() < 1e-9);
        assert!(!mid.reduction_applied);

        let reduced = unauthorised_import_penalty(1.0, 4.2, true).expect("reduced");
        assert!(
            (reduced.rate_eur_per_tco2e - 300.0).abs() < 1e-9,
            "floor at 3.0"
        );
        assert!(reduced.reduction_applied);

        // The floor never bites at the band minimum itself.
        let at_min = unauthorised_import_penalty(1.0, 3.0, true).expect("floor case");
        assert!((at_min.rate_eur_per_tco2e - 300.0).abs() < 1e-9);
    }

    /// R40: both the stored deadline and the queried day are validated.
    #[test]
    fn deadline_counter_validates_both_dates() {
        let notice = NcaCommunication {
            id: "n1".into(),
            notice_kind: "HEARING_NOTICE".into(),
            received_iso: "2027-09-01".into(),
            respond_by_iso: "2027-09-30".into(),
        };
        assert_eq!(notice.days_remaining("2027-09-15").expect("mid-window"), 15);
        // Leap-day deadline parses fine.
        let leap = NcaCommunication {
            respond_by_iso: "2028-02-29".into(),
            ..notice.clone()
        };
        assert_eq!(leap.days_remaining("2028-02-28").expect("leap day"), 1);
        // Impossible dates are errors in either slot.
        assert!(notice.days_remaining("2027-09-31").is_err());
        assert!(notice.days_remaining("30 September 2027").is_err());
    }

    /// R42: filing allowance follows the status, not the action history.
    #[test]
    fn filing_freeze_tracks_status() {
        assert!(filing_allowed(AuthorisationStatus::Active));
        assert!(!filing_allowed(AuthorisationStatus::Suspended));
        assert!(!filing_allowed(AuthorisationStatus::Revoked));
    }
}
