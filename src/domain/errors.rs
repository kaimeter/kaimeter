// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Error types for the `core` domain layer.
//!
//! Every variant carries a stable i18n key (`i18n_key`) so user-facing
//! surfaces can render localized messages instead of raw `Display` strings.

/// Domain-layer error.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// A CN code must be exactly 8 ASCII digits.
    #[error("invalid CN code `{0}`: must be exactly 8 digits")]
    InvalidCnCode(String),
    /// Net mass must be zero or positive.
    #[error("negative net mass: {0} kg")]
    NegativeMass(f64),
    /// Energy must be zero or positive.
    #[error("negative energy: {0}")]
    NegativeEnergy(f64),
    /// Import date must be ISO-8601 `YYYY-MM-DD`.
    #[error("invalid import date `{0}`: expected YYYY-MM-DD")]
    InvalidImportDate(String),
    /// Determination basis must be `ACTUAL` or `DEFAULT`.
    #[error("invalid determination basis `{0}`: expected ACTUAL or DEFAULT")]
    InvalidDeterminationBasis(String),
    /// The CN code has no registered default value at all.
    #[error("no default value registered for CN code `{0}`")]
    NoDefaultForCnCode(String),
    /// The CN code exists but has no default for this production route.
    #[error("no default value for CN code `{cn}` on production route `{route}`")]
    NoDefaultForRoute {
        /// The 8-digit CN code that was looked up.
        cn: String,
        /// The production route that had no default.
        route: String,
    },
    /// Mark-ups apply from 2026 onward.
    #[error("mark-up year out of range: {0} (mark-ups apply from 2026)")]
    MarkupYearOutOfRange(i32),
    /// A quarter number outside 1..=4.
    #[error("invalid quarter Q{1} of {0}: quarter must be 1..=4")]
    InvalidQuarter(i32, u32),
    /// The sector name was not recognized.
    #[error("unknown sector `{0}`")]
    UnknownSector(String),
    /// A customs procedure code outside the supported Box 37 set.
    #[error("unknown customs procedure code `{0}`")]
    UnknownProcedureCode(String),
    /// A promotion was attempted on a record not in the deferred state.
    #[error("invalid promotion: consignment `{0}` is not CBAM_DEFERRED")]
    InvalidPromotion(String),
    /// An IPR discharge exceeds the mass of the original import.
    #[error("IPR discharge of {discharged_kg} kg exceeds imported {imported_kg} kg on `{declaration_id}`")]
    DischargeExceedsImport {
        /// The import declaration discharged against.
        declaration_id: String,
        /// Imported precursor mass, kg.
        imported_kg: f64,
        /// Attempted discharge mass, kg.
        discharged_kg: f64,
    },
    /// The ETS price must be a finite, non-negative number.
    #[error("invalid ETS price: {0}")]
    InvalidEtsPrice(f64),
    /// The CBAM factor schedule starts in 2026.
    #[error("CBAM factor year out of range: {0} (schedule starts in 2026)")]
    CbamFactorYearOutOfRange(i32),
    /// A registry import row could not be parsed.
    #[error("registry import parse error: {0}")]
    RegistryParseError(String),
    /// A verifier attestation failed the accreditation gate.
    #[error("accreditation mismatch: {0}")]
    AccreditationMismatch(String),
    /// A required declaration field is missing.
    #[error("missing required declaration field: `{0}`")]
    MissingRequiredField(String),
    /// An export file violates the target schema.
    #[error("schema violation: {0}")]
    SchemaViolation(String),
    /// An attachment-backed field was saved without human verification (R16).
    #[error("human verification required before saving: {0}")]
    HumanVerificationRequired(String),
    /// The hash chain is broken at the given sequence number (R10).
    #[error("audit chain broken at sequence {0}")]
    ChainBroken(u64),
    /// A cryptographic operation failed.
    #[error("crypto error: {0}")]
    CryptoError(String),
    /// A storage/backend failure surfaced while reading reference data.
    #[error("storage error: {0}")]
    Storage(String),
}

impl DomainError {
    /// Stable message key for the i18n layer (see `locales/*.json`).
    pub fn i18n_key(&self) -> &'static str {
        match self {
            Self::InvalidCnCode(_) => "core.error.invalid_cn_code",
            Self::NegativeMass(_) => "core.error.negative_mass",
            Self::NegativeEnergy(_) => "core.error.negative_energy",
            Self::InvalidImportDate(_) => "core.error.invalid_import_date",
            Self::InvalidDeterminationBasis(_) => "core.error.invalid_determination_basis",
            Self::NoDefaultForCnCode(_) => "core.error.no_default_for_cn",
            Self::NoDefaultForRoute { .. } => "core.error.no_default_for_route",
            Self::MarkupYearOutOfRange(_) => "core.error.markup_year_out_of_range",
            Self::InvalidQuarter(_, _) => "core.error.invalid_quarter",
            Self::UnknownSector(_) => "core.error.unknown_sector",
            Self::UnknownProcedureCode(_) => "core.error.unknown_procedure_code",
            Self::InvalidPromotion(_) => "core.error.invalid_promotion",
            Self::DischargeExceedsImport { .. } => "core.error.discharge_exceeds_import",
            Self::InvalidEtsPrice(_) => "core.error.invalid_ets_price",
            Self::CbamFactorYearOutOfRange(_) => "core.error.cbam_factor_year_out_of_range",
            Self::RegistryParseError(_) => "core.error.registry_parse_error",
            Self::AccreditationMismatch(_) => "core.error.accreditation_mismatch",
            Self::MissingRequiredField(_) => "core.error.missing_required_field",
            Self::SchemaViolation(_) => "core.error.schema_violation",
            Self::HumanVerificationRequired(_) => "core.error.human_verification_required",
            Self::ChainBroken(_) => "core.error.chain_broken",
            Self::CryptoError(_) => "core.error.crypto_error",
            Self::Storage(_) => "core.error.storage",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_a_distinct_i18n_key() {
        let errors = [
            DomainError::InvalidCnCode("x".into()),
            DomainError::NegativeMass(-1.0),
            DomainError::NegativeEnergy(-1.0),
            DomainError::InvalidImportDate("x".into()),
            DomainError::InvalidDeterminationBasis("x".into()),
            DomainError::NoDefaultForCnCode("x".into()),
            DomainError::NoDefaultForRoute {
                cn: "c".into(),
                route: "r".into(),
            },
            DomainError::MarkupYearOutOfRange(0),
            DomainError::InvalidQuarter(2027, 5),
            DomainError::UnknownSector("x".into()),
            DomainError::UnknownProcedureCode("99 99".into()),
            DomainError::InvalidPromotion("c1".into()),
            DomainError::DischargeExceedsImport {
                declaration_id: "d".into(),
                imported_kg: 1.0,
                discharged_kg: 2.0,
            },
            DomainError::InvalidEtsPrice(-1.0),
            DomainError::CbamFactorYearOutOfRange(2025),
            DomainError::RegistryParseError("x".into()),
            DomainError::AccreditationMismatch("x".into()),
            DomainError::MissingRequiredField("f".into()),
            DomainError::SchemaViolation("x".into()),
            DomainError::HumanVerificationRequired("a".into()),
            DomainError::ChainBroken(3),
            DomainError::CryptoError("x".into()),
            DomainError::Storage("x".into()),
        ];
        let mut keys: Vec<&str> = errors.iter().map(DomainError::i18n_key).collect();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "i18n keys must be unique per variant");
    }
}
