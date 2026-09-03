// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Frozen domain types for the `core` layer.
//!
//! These are the shapes every other module consumes; changes here are
//! breaking by definition. Validation happens in constructors/`validate`
//! so an instance of these types is always well-formed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::markups::MarkupYear;

/// CBAM sector a CN code belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Sector {
    /// Iron and steel goods.
    Steel,
    /// Aluminium goods.
    Aluminium,
    /// Cement clinker / cement.
    Cement,
    /// Fertilisers (nitrogen products), +1 % mark-up branch.
    Fertilisers,
    /// Hydrogen.
    Hydrogen,
    /// Electricity (imported electricity regime).
    Electricity,
}

impl Sector {
    /// Canonical persisted string (`SCREAMING_SNAKE_CASE`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Steel => "STEEL",
            Self::Aluminium => "ALUMINIUM",
            Self::Cement => "CEMENT",
            Self::Fertilisers => "FERTILISERS",
            Self::Hydrogen => "HYDROGEN",
            Self::Electricity => "ELECTRICITY",
        }
    }

    /// Parse the canonical persisted string.
    ///
    /// # Errors
    ///
    /// [`DomainError::UnknownSector`] when `s` is not a known sector.
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s.trim() {
            "STEEL" => Ok(Self::Steel),
            "ALUMINIUM" => Ok(Self::Aluminium),
            "CEMENT" => Ok(Self::Cement),
            "FERTILISERS" => Ok(Self::Fertilisers),
            "HYDROGEN" => Ok(Self::Hydrogen),
            "ELECTRICITY" => Ok(Self::Electricity),
            other => Err(DomainError::UnknownSector(other.to_string())),
        }
    }
}

impl std::fmt::Display for Sector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An 8-digit Combined Nomenclature code with description and sector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CnCode {
    code: String,
    description: String,
    sector: Sector,
}

impl CnCode {
    /// Construct a validated CN code.
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidCnCode`] unless `code` is exactly 8 ASCII digits.
    pub fn new(code: &str, description: &str, sector: Sector) -> Result<Self, DomainError> {
        let valid = code.len() == 8 && code.bytes().all(|b| b.is_ascii_digit());
        if !valid {
            return Err(DomainError::InvalidCnCode(code.to_string()));
        }
        Ok(Self {
            code: code.to_string(),
            description: description.to_string(),
            sector,
        })
    }

    /// The 8-digit code string.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Human-readable description.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The CBAM sector this code belongs to.
    #[must_use]
    pub fn sector(&self) -> Sector {
        self.sector
    }
}

/// Basis on which embedded emissions are determined for a consignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeterminationBasis {
    /// Actual installation-specific emissions.
    Actual,
    /// CBAM default values.
    Default,
}

impl DeterminationBasis {
    /// Canonical persisted string.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Actual => "ACTUAL",
            Self::Default => "DEFAULT",
        }
    }
}

impl std::str::FromStr for DeterminationBasis {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "ACTUAL" => Ok(Self::Actual),
            "DEFAULT" => Ok(Self::Default),
            other => Err(DomainError::InvalidDeterminationBasis(other.to_string())),
        }
    }
}

/// A production installation registered under CBAM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installation {
    /// Stable installation identifier.
    pub id: String,
    /// Operator-visible name.
    pub name: String,
    /// Physical address.
    pub address: String,
    /// Production routes available at the installation.
    pub production_routes: Vec<String>,
}

/// One imported consignment (the CBAM unit of account).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Consignment {
    /// 8-digit combined nomenclature code of the goods.
    pub cn_code: String,
    /// Net mass in kilograms.
    pub net_mass_kg: f64,
    /// ISO-3166 alpha-2 country of origin.
    pub country_of_origin: String,
    /// Country where the production took place (may differ from origin).
    pub production_country: String,
    /// Identifier of the producing installation.
    pub installation_id: String,
    /// Import date, ISO-8601 `YYYY-MM-DD`.
    pub import_date: String,
    /// How embedded emissions are determined.
    pub determination_basis: DeterminationBasis,
    /// Effective carbon price paid, EUR per tCO2e, if any.
    pub carbon_price_eur_per_tco2e: Option<f64>,
    /// Country in which the carbon price was paid, if any.
    pub carbon_price_country: Option<String>,
}

impl Consignment {
    /// Validate the consignment's invariants.
    ///
    /// # Errors
    ///
    /// [`DomainError::NegativeMass`] for negative mass;
    /// [`DomainError::InvalidImportDate`] for a non-ISO `import_date`.
    pub fn validate(&self) -> Result<(), DomainError> {
        if !(self.net_mass_kg.is_finite() && self.net_mass_kg >= 0.0) {
            return Err(DomainError::NegativeMass(self.net_mass_kg));
        }
        if parse_iso_date(&self.import_date).is_none() {
            return Err(DomainError::InvalidImportDate(self.import_date.clone()));
        }
        Ok(())
    }

    /// Calendar year of the import date (the mark-up schedule year).
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidImportDate`] if the date was never validated.
    pub fn year(&self) -> Result<i32, DomainError> {
        parse_iso_date(&self.import_date)
            .map(|(y, _, _)| y)
            .ok_or_else(|| DomainError::InvalidImportDate(self.import_date.clone()))
    }
}

/// Energy/fuel document class (first dossier class per R23).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnergyRecord {
    kwh: f64,
}

impl EnergyRecord {
    /// Construct from kilowatt-hours, normalized internally to MWh.
    ///
    /// # Errors
    ///
    /// [`DomainError::NegativeEnergy`] for negative input or NaN.
    pub fn from_kwh(kwh: f64) -> Result<Self, DomainError> {
        Self::from_mwh(crate::domain::units::kwh_to_mwh(kwh))
    }

    /// Construct from megawatt-hours.
    ///
    /// # Errors
    ///
    /// [`DomainError::NegativeEnergy`] for negative input or NaN.
    pub fn from_mwh(mwh: f64) -> Result<Self, DomainError> {
        if !(mwh.is_finite() && mwh >= 0.0) {
            return Err(DomainError::NegativeEnergy(mwh));
        }
        Ok(Self {
            kwh: crate::domain::units::mwh_to_kwh(mwh),
        })
    }

    /// Energy in kilowatt-hours.
    #[must_use]
    pub fn kwh(&self) -> f64 {
        self.kwh
    }

    /// Energy in megawatt-hours (canonical unit).
    #[must_use]
    pub fn mwh(&self) -> f64 {
        crate::domain::units::kwh_to_mwh(self.kwh)
    }
}

/// Materials document class (second dossier class per R23).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialRecord {
    /// CN code of the material/good.
    pub cn_code: String,
    /// Net mass in kilograms.
    pub net_mass_kg: f64,
    /// Production route, if known.
    pub production_route: Option<String>,
}

/// Production document class (third dossier class per R23).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionRecord {
    /// Producing installation identifier.
    pub installation_id: String,
    /// Production route applied at the installation.
    pub production_route: String,
}

/// A CBAM default value for one (CN code, production route) pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefaultValue {
    /// The CN code the default belongs to.
    pub cn_code: CnCode,
    /// Production route identifier (e.g. `EF`, `PRIMARY`, `NATURAL_GAS`).
    pub production_route: String,
    /// Direct emissions, tCO2e per tonne of goods.
    pub direct_tco2e_per_t: f64,
    /// Indirect emissions, tCO2e per tonne of goods.
    pub indirect_tco2e_per_t: f64,
    /// Phased mark-up percentages by schedule bucket (see `markups`).
    pub markups: BTreeMap<MarkupYear, f64>,
}

/// The three document classes correlated in a dossier (per R23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DossierClass {
    /// Energy/fuel records.
    EnergyFuel,
    /// Material records.
    Materials,
    /// Production records.
    Production,
}

impl DossierClass {
    /// All three classes in reporting order.
    pub const ALL: [DossierClass; 3] = [
        DossierClass::EnergyFuel,
        DossierClass::Materials,
        DossierClass::Production,
    ];

    /// Stable key fragment for persistence/UI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EnergyFuel => "ENERGY_FUEL",
            Self::Materials => "MATERIALS",
            Self::Production => "PRODUCTION",
        }
    }
}

/// Completeness report for a dossier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completeness {
    /// True when every document class is present and non-empty.
    pub complete: bool,
    /// Classes that are missing or empty, in reporting order.
    pub missing: Vec<DossierClass>,
}

/// A dossier: one consignment correlated with its three document classes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dossier {
    /// The consignment the documents belong to.
    pub consignment: Consignment,
    /// Energy/fuel document class (optional until provided).
    pub energy: Option<EnergyRecord>,
    /// Material documents (empty until provided).
    pub materials: Vec<MaterialRecord>,
    /// Production documents (empty until provided).
    pub production: Vec<ProductionRecord>,
}

impl Dossier {
    /// Start a dossier for a consignment with no documents attached.
    #[must_use]
    pub fn new(consignment: Consignment) -> Self {
        Self {
            consignment,
            energy: None,
            materials: Vec::new(),
            production: Vec::new(),
        }
    }

    /// Attach the energy/fuel document class.
    #[must_use]
    pub fn with_energy(mut self, energy: EnergyRecord) -> Self {
        self.energy = Some(energy);
        self
    }

    /// Attach the materials document class.
    #[must_use]
    pub fn with_materials(mut self, materials: Vec<MaterialRecord>) -> Self {
        self.materials = materials;
        self
    }

    /// Attach the production document class.
    #[must_use]
    pub fn with_production(mut self, production: Vec<ProductionRecord>) -> Self {
        self.production = production;
        self
    }

    /// True when every document class is present and non-empty.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.completeness().complete
    }

    /// Compute which document classes are missing or empty.
    #[must_use]
    pub fn completeness(&self) -> Completeness {
        let missing: Vec<DossierClass> = DossierClass::ALL
            .into_iter()
            .filter(|class| match class {
                DossierClass::EnergyFuel => self.energy.is_none(),
                DossierClass::Materials => self.materials.is_empty(),
                DossierClass::Production => self.production.is_empty(),
            })
            .collect();
        Completeness {
            complete: missing.is_empty(),
            missing,
        }
    }
}

/// Parse an ISO-8601 `YYYY-MM-DD` date into `(year, month, day)`.
fn parse_iso_date(s: &str) -> Option<(i32, u32, u32)> {
    let mut parts = s.split('-');
    let y = parts.next()?;
    let m = parts.next()?;
    let d = parts.next()?;
    if parts.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    if !(y
        .bytes()
        .chain(m.bytes())
        .chain(d.bytes())
        .all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let year: i32 = y.parse().ok()?;
    let month: u32 = m.parse().ok()?;
    let day: u32 = d.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > max_day {
        return None;
    }
    Some((year, month, day))
}

/// Crate-facing accessor for the ISO `YYYY-MM-DD` parser (`calendar` and
/// persistence layers reuse the same strict grammar).
pub fn parse_iso_date_pub(s: &str) -> Option<(i32, u32, u32)> {
    parse_iso_date(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_round_trips_through_canonical_strings() {
        for sector in [
            Sector::Steel,
            Sector::Aluminium,
            Sector::Cement,
            Sector::Fertilisers,
            Sector::Hydrogen,
            Sector::Electricity,
        ] {
            assert_eq!(Sector::parse(sector.as_str()).expect("parse"), sector);
        }
        assert!(matches!(
            Sector::parse(" Widgets"),
            Err(DomainError::UnknownSector(s)) if s == "Widgets"
        ));
    }

    #[test]
    fn cn_code_rejects_non_eight_digit_codes() {
        for bad in ["", "123", "1234567", "123456789", "73181A00", "7318 500"] {
            assert!(
                CnCode::new(bad, "d", Sector::Steel).is_err(),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn consignment_rejects_bad_mass_and_dates() {
        let mut c = sample();
        c.net_mass_kg = -1.0;
        assert!(matches!(c.validate(), Err(DomainError::NegativeMass(_))));

        let mut c = sample();
        c.import_date = "2026-02-30".to_string(); // Feb 30 does not exist
        assert!(matches!(
            c.validate(),
            Err(DomainError::InvalidImportDate(_))
        ));

        let mut c = sample();
        c.import_date = "2026-13-01".to_string();
        assert!(matches!(
            c.validate(),
            Err(DomainError::InvalidImportDate(_))
        ));
    }

    #[test]
    fn leap_years_handled_in_date_validation() {
        let mut ok = sample();
        ok.import_date = "2024-02-29".to_string();
        assert!(ok.validate().is_ok());

        let mut bad = sample();
        bad.import_date = "2026-02-29".to_string(); // 2026 is not a leap year
        assert!(bad.validate().is_err());
    }

    #[test]
    fn iso_date_parser_rejects_garbage() {
        for bad in [
            "",
            "2026",
            "2026-03",
            "15/03/2026",
            "2026-3-15",
            "abcd-03-15",
            "2026-03-15T00:00",
        ] {
            assert!(parse_iso_date(bad).is_none(), "{bad} should not parse");
        }
        assert_eq!(parse_iso_date("2026-03-15"), Some((2026, 3, 15)));
    }

    #[test]
    fn dossier_completeness_tracks_all_three_classes() {
        let mut d = Dossier::new(sample());
        assert_eq!(
            d.completeness().missing,
            vec![
                DossierClass::EnergyFuel,
                DossierClass::Materials,
                DossierClass::Production
            ]
        );

        d = d.with_energy(EnergyRecord::from_mwh(1.0).expect("energy"));
        assert_eq!(
            d.completeness().missing,
            vec![DossierClass::Materials, DossierClass::Production]
        );

        d = d.with_materials(vec![MaterialRecord {
            cn_code: "73181500".into(),
            net_mass_kg: 10.0,
            production_route: None,
        }]);
        assert_eq!(d.completeness().missing, vec![DossierClass::Production]);

        d = d.with_production(vec![ProductionRecord {
            installation_id: "I1".into(),
            production_route: "EF".into(),
        }]);
        assert!(d.is_complete());

        // Zero values still count as present (presence, not magnitude).
        d = d.with_materials(Vec::new());
        assert!(!d.is_complete());
        assert!(d
            .with_materials(vec![MaterialRecord {
                cn_code: "73181500".into(),
                net_mass_kg: 0.0,
                production_route: None,
            }])
            .is_complete());
    }

    #[test]
    fn energy_record_accepts_zero_and_rejects_nan() {
        assert!(EnergyRecord::from_kwh(0.0).is_ok());
        assert!(EnergyRecord::from_mwh(0.0).is_ok());
        assert!(matches!(
            EnergyRecord::from_kwh(f64::NAN),
            Err(DomainError::NegativeEnergy(_))
        ));
        assert!(EnergyRecord::from_mwh(f64::INFINITY).is_err());
    }

    fn sample() -> Consignment {
        Consignment {
            cn_code: "73181500".to_string(),
            net_mass_kg: 1_000.0,
            country_of_origin: "CN".to_string(),
            production_country: "DE".to_string(),
            installation_id: "INST-DE-001".to_string(),
            import_date: "2026-03-15".to_string(),
            determination_basis: DeterminationBasis::Default,
            carbon_price_eur_per_tco2e: None,
            carbon_price_country: None,
        }
    }
}
