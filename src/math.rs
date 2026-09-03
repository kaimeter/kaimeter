// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Exposure math (R3/R7): embedded emissions on the actual and default
//! paths, complex-goods precursor aggregation, gross/net exposure with the
//! Formula A/B toggle, the CBAM factor schedule, and the 50 t de-minimis
//! tracker (R1).
//!
//! Pure functions, no I/O. Compute in full precision (f64) internally;
//! round only at display/export.

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::domain::errors::DomainError;
use crate::domain::markups;
use crate::domain::types::{CnCode, Consignment, DefaultValue, DeterminationBasis, Sector};

// ---------------------------------------------------------------------------
// CBAM factor schedule (R7, Art 10a(1a) ETS Directive)
// ---------------------------------------------------------------------------

/// CBAM factor (the obligation share) for a calendar year, as a fraction of
/// embedded emissions: 2.5% (2026), 5% (2027), 10% (2028), 22.5% (2029),
/// 48.5% (2030), 61% (2031), 73.5% (2032), 86% (2033), 100% (2034 onward).
///
/// REGULATORY PIN — values are law (Art 10a(1a)); the schedule ships as a
/// data table so a Phase-5 adoption re-parameterizes data, not code.
///
/// # Errors
///
/// [`DomainError::CbamFactorYearOutOfRange`] before 2026.
pub fn cbam_factor(year: i32) -> Result<f64, DomainError> {
    // REGULATORY PIN — Art 10a(1a) ETS Directive. Each arm is one year of
    // the free-allocation phase-out; a Phase-5 adoption re-parameterizes
    // this table (data), it does not change the formula.
    match year {
        2026 => Ok(0.025), // 2026: 2.5 %
        2027 => Ok(0.05),  // 2027: 5 %
        2028 => Ok(0.10),  // 2028: 10 %
        2029 => Ok(0.225), // 2029: 22.5 %
        2030 => Ok(0.485), // 2030: 48.5 %
        2031 => Ok(0.61),  // 2031: 61 %
        2032 => Ok(0.735), // 2032: 73.5 %
        2033 => Ok(0.86),  // 2033: 86 %
        // 2034 and every later year: fully phased in at 100 %.
        y if y >= 2034 => Ok(1.0),
        _ => Err(DomainError::CbamFactorYearOutOfRange(year)),
    }
}

// ---------------------------------------------------------------------------
// Formula A/B toggle (R7, Art 9 deduction order)
// ---------------------------------------------------------------------------

/// The Art 9 carbon-price deduction order, parameterized as data so the
/// adopted implementing act flips it with no code change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Formula {
    /// Formula A: `(emissions × factor × price) − carbon_paid`.
    A,
    /// Formula B: `((emissions × price) − carbon_paid) × factor`.
    B,
}

/// Gross exposure (R7): `emissions × ets_price − carbon_price_paid`.
///
/// # Errors
///
/// [`DomainError::InvalidEtsPrice`] for a negative or non-finite price.
pub fn gross_exposure(
    emissions_tco2e: f64,
    ets_price_eur: f64,
    carbon_price_paid_eur: f64,
) -> Result<f64, DomainError> {
    validate_ets_price(ets_price_eur)?;
    Ok(emissions_tco2e * ets_price_eur - carbon_price_paid_eur)
}

/// Net certificate obligation (R7): the exposure after the CBAM factor and
/// the Art 9 carbon-price deduction, in the chosen deduction order.
///
/// # Errors
///
/// [`DomainError::InvalidEtsPrice`] for a negative or non-finite price.
pub fn net_exposure(
    emissions_tco2e: f64,
    ets_price_eur: f64,
    carbon_price_paid_eur: f64,
    cbam_factor: f64,
    formula: Formula,
) -> Result<f64, DomainError> {
    validate_ets_price(ets_price_eur)?;
    Ok(match formula {
        // Formula A: the factor bites the obligation first, then the full
        // carbon price already paid is deducted.
        Formula::A => emissions_tco2e * cbam_factor * ets_price_eur - carbon_price_paid_eur,
        // Formula B: the deduction happens gross, then the factor scales
        // the remainder.
        Formula::B => (emissions_tco2e * ets_price_eur - carbon_price_paid_eur) * cbam_factor,
    })
}

// ---------------------------------------------------------------------------
// Embedded emissions (R3)
// ---------------------------------------------------------------------------

/// Which Annex II indirect-emissions scope applies to a sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IndirectScope {
    /// Cement and fertilisers: indirect emissions included.
    Included,
    /// Iron & steel: direct-only EXCEPT indirect from agglomerated iron ore
    /// (sinter/pellet) precursors — modeled by including the precursor's
    /// indirect share via [`PrecursorInput`] entries.
    SteelOrePrecursor,
    /// Aluminium and hydrogen: direct-only.
    DirectOnly,
}

/// The Annex II indirect-emissions scope for a sector (electricity is its
/// own regime; treat it as direct-only here).
#[must_use]
pub fn indirect_scope(sector: Sector) -> IndirectScope {
    // Annex II indirect-emissions scope table (R3).
    match sector {
        Sector::Cement | Sector::Fertilisers => IndirectScope::Included,
        Sector::Steel => IndirectScope::SteelOrePrecursor,
        Sector::Aluminium | Sector::Hydrogen | Sector::Electricity => IndirectScope::DirectOnly,
    }
}

/// One precursor input on a complex good's bill of materials (R3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrecursorInput {
    /// Precursor CN code (may itself be a CBAM good — wire rod into drawn
    /// wire, billet into extrusions).
    pub cn_code: String,
    /// Precursor mass consumed per tonne of finished output, tonnes
    /// (yield loss excluded from both this and the denominator).
    pub mass_per_t_output: f64,
    /// Embedded emissions attributed to the precursor, tCO2e per tonne of
    /// precursor (actual if verified, else default + mark-up for its node).
    pub embedded_tco2e_per_t: f64,
    /// Indirect share of the precursor's embedded emissions, tCO2e/t — only
    /// counted for scopes that include it (see
    /// [`PrecursorInput::indirect_scope_eligible`] for the steel carve-out).
    pub indirect_tco2e_per_t: f64,
    /// Whether this precursor is an agglomerated-iron-ore input (sinter or
    /// pellet): only then does its indirect share count under the steel
    /// sector's Annex II carve-out (`SteelOrePrecursor` scope). Ignored for
    /// `Included` (indirect always counts) and `DirectOnly` (never counts).
    pub indirect_scope_eligible: bool,
    /// Whether the precursor's production country is unknown: it then takes
    /// the highest-intensity third-country default (no gaming the lookup).
    pub production_country_unknown: bool,
}

/// Complex-goods embedded emissions per net tonne of finished output (R3,
/// Annex IV activity-level equation): attributed own-production emissions
/// plus precursor emissions, divided by the quantity of goods produced.
/// In-process recycled scrap is excluded from BOTH precursor mass and the
/// output denominator (no double-counting).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ComplexGoodsInput {
    /// Own production-process emissions attributed to the good, tCO2e.
    pub own_emissions_tco2e: f64,
    /// Finished output quantity, net tonnes (the activity denominator).
    pub output_tonnes: f64,
}

/// Compute embedded emissions per net tonne of finished output for a
/// complex good with its bill of precursor inputs.
///
/// # Errors
///
/// [`DomainError::NegativeMass`] when the output quantity is not positive.
pub fn complex_goods_emissions(
    own: &ComplexGoodsInput,
    precursors: &[PrecursorInput],
    scope: IndirectScope,
) -> Result<f64, DomainError> {
    // The activity denominator is the finished-output quantity: it must be
    // strictly positive (Annex IV divides by the quantity of goods
    // produced). In-process recycled scrap is excluded from BOTH precursor
    // mass and this denominator by the caller (R3 documentation invariant).
    if !(own.output_tonnes.is_finite() && own.output_tonnes > 0.0) {
        return Err(DomainError::NegativeMass(own.output_tonnes));
    }
    let mut total_tco2e = own.own_emissions_tco2e;
    for precursor in precursors {
        total_tco2e += precursor.mass_per_t_output * precursor.embedded_tco2e_per_t;
        let indirect_in_scope = scope == IndirectScope::Included
            || (scope == IndirectScope::SteelOrePrecursor && precursor.indirect_scope_eligible);
        if indirect_in_scope {
            total_tco2e += precursor.mass_per_t_output * precursor.indirect_tco2e_per_t;
        }
    }
    Ok(total_tco2e / own.output_tonnes)
}

/// Total embedded emissions for one consignment on the DEFAULT path:
/// (default direct + in-scope indirect per tonne, with the year's mark-up
/// applied) × net tonnes.
///
/// # Errors
///
/// Lookup/mark-up errors propagate; [`DomainError::NegativeMass`] when the
/// consignment mass is negative.
pub fn consignment_emissions_default(
    consignment: &Consignment,
    default_direct_tco2e_per_t: f64,
    default_indirect_tco2e_per_t: f64,
) -> Result<f64, DomainError> {
    if !valid_mass(consignment.net_mass_kg) {
        return Err(DomainError::NegativeMass(consignment.net_mass_kg));
    }
    let year = consignment.year()?;
    let sector = sector_for_cn(&consignment.cn_code)?;
    let default = DefaultValue {
        cn_code: CnCode::new(&consignment.cn_code, "", sector)?,
        production_route: String::new(),
        direct_tco2e_per_t: default_direct_tco2e_per_t,
        indirect_tco2e_per_t: default_indirect_tco2e_per_t,
        markups: BTreeMap::new(),
    };
    // R4: apply the year's mark-up to both components (fertilisers carry the
    // flat +1 % branch); years before 2026 are rejected here.
    let marked_up = markups::apply(&default, year)?;
    // Additive on both components — whether the sector's Annex II scope
    // admits an indirect share at all is the caller's decision (validate/web).
    let net_tonnes = consignment.net_mass_kg / 1000.0;
    Ok((marked_up.direct_tco2e_per_t + marked_up.indirect_tco2e_per_t) * net_tonnes)
}

/// Total embedded emissions for one consignment on the ACTUAL path:
/// verifier-approved installation-specific intensity × net tonnes.
///
/// # Errors
///
/// [`DomainError::NegativeMass`] when the consignment mass is negative.
pub fn consignment_emissions_actual(
    consignment: &Consignment,
    actual_tco2e_per_t: f64,
) -> Result<f64, DomainError> {
    if !valid_mass(consignment.net_mass_kg) {
        return Err(DomainError::NegativeMass(consignment.net_mass_kg));
    }
    // A negative (or non-finite) intensity is as meaningless as a negative
    // mass — reject it on the same error variant the contract pins.
    if !(actual_tco2e_per_t.is_finite() && actual_tco2e_per_t >= 0.0) {
        return Err(DomainError::NegativeMass(actual_tco2e_per_t));
    }
    Ok(actual_tco2e_per_t * consignment.net_mass_kg / 1000.0)
}

/// The data path actually used by a consignment's computation (R8 flag).
#[must_use]
pub fn used_basis(consignment: &Consignment) -> DeterminationBasis {
    consignment.determination_basis
}

// ---------------------------------------------------------------------------
// 50 t de-minimis tracker (R1)
// ---------------------------------------------------------------------------

/// Calendar-year-to-date net-mass tracker per declarant, aggregated across
/// ALL CBAM goods (R1, Art 2a + Annex VII pt 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeMinimisTracker {
    ytd_net_mass_kg: f64,
    threshold_kg: f64,
    crossed: bool,
}

/// The statutory de-minimis threshold: 50 t per declarant per calendar year.
pub const DE_MINIMIS_THRESHOLD_TONNES: f64 = 50.0;

impl DeMinimisTracker {
    /// A fresh tracker for one declarant's calendar year: empty aggregate,
    /// threshold at the statutory 50 t, exemption not crossed.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ytd_net_mass_kg: 0.0,
            threshold_kg: DE_MINIMIS_THRESHOLD_TONNES * 1000.0,
            crossed: false,
        }
    }

    /// Add one consignment's mass. Returns whether the threshold is now
    /// crossed (a latching flag: once cumulative mass exceeds the threshold,
    /// the exemption is lost for ALL tonnes that year — R1).
    ///
    /// # Sector-blind by design
    ///
    /// Electricity and hydrogen imports enjoy NO de-minimis exemption: they
    /// are always liable regardless of the aggregate. This tracker only
    /// tracks the aggregate exemption and is deliberately sector-blind —
    /// always-liable sector handling sits with the caller.
    ///
    /// Exempt origins (R43/R45) and non-free-circulation regimes must be
    /// filtered by the caller (see `customs::counts_toward_net_mass`).
    pub fn add(&mut self, net_mass_kg: f64) -> bool {
        self.ytd_net_mass_kg += net_mass_kg;
        if !self.crossed && self.ytd_net_mass_kg > self.threshold_kg {
            self.crossed = true;
        }
        self.crossed
    }

    /// Year-to-date aggregated net mass across all CBAM goods, kg.
    #[must_use]
    pub fn ytd_net_mass_kg(&self) -> f64 {
        self.ytd_net_mass_kg
    }

    /// True once the cumulative mass has ever exceeded the threshold:
    /// the exemption is lost for ALL tonnes that year (R1).
    #[must_use]
    pub fn crossed(&self) -> bool {
        self.crossed
    }

    /// True while the declarant is still exempt (≤ 50 t YTD and never crossed).
    #[must_use]
    pub fn is_exempt(&self) -> bool {
        !self.crossed && self.ytd_net_mass_kg <= self.threshold_kg
    }
}

impl Default for DeMinimisTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Mirror of `Consignment::validate`'s mass rule: finite and >= 0.
fn valid_mass(kg: f64) -> bool {
    kg.is_finite() && kg >= 0.0
}

/// The ETS price must be finite and non-negative (R7: manual offline entry
/// must never silently poison the projection).
fn validate_ets_price(ets_price_eur: f64) -> Result<(), DomainError> {
    if ets_price_eur.is_finite() && ets_price_eur >= 0.0 {
        Ok(())
    } else {
        Err(DomainError::InvalidEtsPrice(ets_price_eur))
    }
}

/// The CBAM sector for a CN code, derived from its Annex I grouping by
/// chapter/headings. The seeded `cn_codes` reference table (see
/// `domain::lookup`) remains the authoritative mapping; this pure classifier
/// exists only so the default-path mark-up (R4) can be applied without I/O.
///
/// # Errors
///
/// [`DomainError::UnknownSector`] when the code matches no CBAM chapter.
fn sector_for_cn(code: &str) -> Result<Sector, DomainError> {
    // Order matters only within a chapter family; longest prefixes first.
    const PREFIXES: [(&str, Sector); 9] = [
        ("3102", Sector::Fertilisers), // nitrogenous fertilisers (Annex I: 3102)
        ("3105", Sector::Fertilisers), // other fertilisers (Annex I: 3105)
        ("2523", Sector::Cement),      // cements / clinker (Annex I: 2523)
        ("6810", Sector::Cement),      // articles of cement/concrete (Annex I: 6810)
        ("2716", Sector::Electricity), // electrical energy (Annex I: 2716)
        ("2804", Sector::Hydrogen),    // hydrogen (Annex I: 2804 10 00)
        ("72", Sector::Steel),         // iron & steel (Annex I: chapter 72)
        ("73", Sector::Steel),         // iron & steel articles (Annex I: chapter 73)
        ("76", Sector::Aluminium),     // aluminium (Annex I: chapter 76)
    ];
    for (prefix, sector) in PREFIXES {
        if code.starts_with(prefix) {
            return Ok(sector);
        }
    }
    Err(DomainError::UnknownSector(code.to_string()))
}
