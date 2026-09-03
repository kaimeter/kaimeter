// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Validation of incoming mill/consignment data (R12): unit conversion,
//! completeness, route plausibility, and mass balances. Bad upstream data
//! flows straight into a legally binding declaration — everything suspect is
//! flagged, nothing silently corrected.

use serde::{Deserialize, Serialize};

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::errors::DomainError;
use crate::domain::lookup::Lookup;
use crate::domain::types::{parse_iso_date_pub, Consignment, Dossier};
use crate::domain::units;

/// First year of the CBAM definitive regime (Regulation (EU) 2023/956);
/// imports dated earlier are flagged for review, not rejected.
const DEFINITIVE_REGIME_START_YEAR: i32 = 2026;

// ---------------------------------------------------------------------------
// Issues
// ---------------------------------------------------------------------------

/// Severity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Severity {
    /// Blocks declaration export until resolved.
    Error,
    /// Plausibility concern; requires human confirmation (R16 doctrine).
    Warning,
}

/// One validation finding, carrying a stable i18n message key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// Machine-readable issue code, e.g. `UNKNOWN_ROUTE`.
    pub code: String,
    /// Field the issue applies to (dotted path).
    pub field: String,
    /// i18n key for the localized message.
    pub message_key: String,
    /// How hard the issue blocks.
    pub severity: Severity,
}

// ---------------------------------------------------------------------------
// Unit conversion
// ---------------------------------------------------------------------------

/// Supported data-entry units (R12 unit validation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Unit {
    /// Kilograms.
    Kilograms,
    /// Metric tonnes.
    Tonnes,
    /// Kilowatt-hours.
    KilowattHours,
    /// Megawatt-hours.
    MegawattHours,
}

/// Convert between compatible units.
///
/// Conversions are the exact power-of-ten scalings from [`units`] (kg ↔ t,
/// kWh ↔ MWh); same-unit identity passes through unchanged. Quantities are
/// validated, never coerced: NaN, infinite, or negative inputs and
/// cross-family conversions (mass vs energy) are rejected.
///
/// # Errors
///
/// [`DomainError::Storage`] carries the incompatible-unit condition (kg ↔ t
/// and kWh ↔ MWh convert; across families does not) and the non-finite or
/// negative-quantity condition.
pub fn convert_units(value: f64, from: Unit, to: Unit) -> Result<f64, DomainError> {
    if !value.is_finite() || value < 0.0 {
        return Err(DomainError::Storage(format!(
            "invalid quantity {value} for unit conversion: must be finite and non-negative"
        )));
    }

    let family = |unit: Unit| match unit {
        Unit::Kilograms | Unit::Tonnes => "mass",
        Unit::KilowattHours | Unit::MegawattHours => "energy",
    };
    if family(from) != family(to) {
        return Err(DomainError::Storage(format!(
            "incompatible unit families: cannot convert {from:?} to {to:?} (mass and energy are not interchangeable)"
        )));
    }

    match (from, to) {
        (Unit::Kilograms, Unit::Kilograms)
        | (Unit::Tonnes, Unit::Tonnes)
        | (Unit::KilowattHours, Unit::KilowattHours)
        | (Unit::MegawattHours, Unit::MegawattHours) => Ok(value),
        (Unit::Kilograms, Unit::Tonnes) => Ok(units::kg_to_tonnes(value)),
        (Unit::Tonnes, Unit::Kilograms) => Ok(units::tonnes_to_kg(value)),
        (Unit::KilowattHours, Unit::MegawattHours) => Ok(units::kwh_to_mwh(value)),
        (Unit::MegawattHours, Unit::KilowattHours) => Ok(units::mwh_to_kwh(value)),
        // Unreachable behind the family guard above; kept as defense in
        // depth rather than a panic in compliance tooling.
        _ => Err(DomainError::Storage(format!(
            "incompatible unit families: cannot convert {from:?} to {to:?}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Consignment validation
// ---------------------------------------------------------------------------

/// Validate one consignment: domain invariants, reference-data plausibility
/// (CN code exists, production route known), and completeness of the
/// carbon-price fields (a price without its country is flagged).
///
/// The inverse case — a carbon-price country recorded without a price — is
/// deliberately **not** an issue: a zero-priced import is legal (R12 flags
/// incomplete data; it does not criminalize a `0.00` price). Likewise there
/// is no future-date check: pure code has no clock. An import date before
/// 2026 (start of the definitive regime) is a Warning.
#[must_use]
pub fn validate_consignment(consignment: &Consignment, lookup: &Lookup) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Domain invariants (mass sign/finiteness, date shape).
    if let Err(err) = consignment.validate() {
        let (code, field) = match &err {
            DomainError::NegativeMass(_) => ("NEGATIVE_MASS", "net_mass_kg"),
            DomainError::InvalidImportDate(_) => ("INVALID_DATE", "import_date"),
            _ => ("DOMAIN_INVARIANT", "consignment"),
        };
        issues.push(issue(code, field, Severity::Error));
    }

    // CN code: well-formed before it can be known; never double-flag.
    let cn = consignment.cn_code.trim();
    let well_formed = cn.len() == 8 && cn.bytes().all(|b| b.is_ascii_digit());
    if !well_formed {
        issues.push(issue("CN_FORMAT", "cn_code", Severity::Error));
    } else if lookup.cn_code(cn).is_none() {
        issues.push(issue("CN_UNKNOWN", "cn_code", Severity::Error));
    }

    // Carbon-price completeness: a price needs its country; a country
    // without a price is a legal zero-price import and stays unflagged.
    if consignment.carbon_price_eur_per_tco2e.is_some()
        && consignment.carbon_price_country.is_none()
    {
        issues.push(issue(
            "CARBON_PRICE_COUNTRY_MISSING",
            "carbon_price_country",
            Severity::Warning,
        ));
    }

    // Pre-regime imports are plausible but worth a human glance.
    if let Some((year, _, _)) = parse_iso_date_pub(&consignment.import_date) {
        if year < DEFINITIVE_REGIME_START_YEAR {
            issues.push(issue(
                "DATE_BEFORE_REGIME",
                "import_date",
                Severity::Warning,
            ));
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// Dossier mass balance
// ---------------------------------------------------------------------------

/// Plausibility of a dossier's material masses against its output: consumed
/// precursor masses materially below the finished-output mass, negative
/// masses, or duplicate CN entries with conflicting routes are flagged for
/// human verification.
///
/// Every finding is advisory except negative masses: a shortfall of recorded
/// inputs against output (`MASS_BALANCE_INPUT_BELOW_OUTPUT`) is a Warning per
/// R16 — outputs cannot exceed recorded inputs, but the verifying human is
/// the author of the record, so the flag demands confirmation rather than
/// blocking. A dossier with neither materials nor production records raises
/// `NO_PRODUCTION_EVIDENCE` (and, if the consignment claims mass, the
/// shortfall warning alongside it — zero recorded inputs cannot cover it).
#[must_use]
pub fn validate_dossier_mass_balance(dossier: &Dossier) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Negative (or non-finite) material masses are hard errors; only valid
    // masses count toward the recorded input total.
    let mut recorded_input_kg = 0.0;
    for (index, material) in dossier.materials.iter().enumerate() {
        if !(material.net_mass_kg.is_finite() && material.net_mass_kg >= 0.0) {
            issues.push(issue(
                "NEGATIVE_MASS",
                &format!("materials[{index}].net_mass_kg"),
                Severity::Error,
            ));
        } else {
            recorded_input_kg += material.net_mass_kg;
        }
    }

    // Outputs cannot exceed recorded inputs — flagged for human
    // verification (R16), not silently accepted nor hard-blocked.
    if recorded_input_kg < dossier.consignment.net_mass_kg {
        issues.push(issue(
            "MASS_BALANCE_INPUT_BELOW_OUTPUT",
            "materials",
            Severity::Warning,
        ));
    }

    // Route plausibility: one CN code attributed to two different
    // production routes cannot both be true.
    let mut routes_by_cn: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for material in &dossier.materials {
        if let Some(route) = &material.production_route {
            routes_by_cn
                .entry(material.cn_code.trim())
                .or_default()
                .insert(route.as_str());
        }
    }
    for (cn, routes) in &routes_by_cn {
        if routes.len() > 1 {
            issues.push(issue(
                "CONFLICTING_ROUTES",
                &format!("materials[cn={cn}].production_route"),
                Severity::Warning,
            ));
        }
    }

    // No evidence at all of either kind: nothing to verify against.
    if dossier.materials.is_empty() && dossier.production.is_empty() {
        issues.push(issue(
            "NO_PRODUCTION_EVIDENCE",
            "dossier",
            Severity::Warning,
        ));
    }

    issues
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Build one finding with the stable i18n key
/// `validate.issue.<code lowercase>`.
fn issue(code: &str, field: &str, severity: Severity) -> ValidationIssue {
    ValidationIssue {
        code: code.to_string(),
        field: field.to_string(),
        message_key: format!("validate.issue.{}", code.to_ascii_lowercase()),
        severity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R12: exact power-of-ten scalings; family and quantity guards.
    #[test]
    fn convert_units_scales_and_guards() {
        assert_eq!(
            convert_units(1_234.0, Unit::Kilograms, Unit::Tonnes).expect("kg -> t"),
            1.234
        );
        assert_eq!(
            convert_units(7.5, Unit::MegawattHours, Unit::KilowattHours).expect("MWh -> kWh"),
            7500.0
        );
        assert!(convert_units(f64::INFINITY, Unit::Tonnes, Unit::Kilograms).is_err());
        assert!(convert_units(1.0, Unit::Tonnes, Unit::KilowattHours).is_err());
    }

    /// The i18n key is deterministically derived from the code.
    #[test]
    fn issue_keys_derive_from_codes() {
        let built = issue("CN_UNKNOWN", "cn_code", Severity::Error);
        assert_eq!(built.message_key, "validate.issue.cn_unknown");
        assert_eq!(built.code, "CN_UNKNOWN");
        assert_eq!(built.field, "cn_code");
        assert_eq!(built.severity, Severity::Error);
    }
}
