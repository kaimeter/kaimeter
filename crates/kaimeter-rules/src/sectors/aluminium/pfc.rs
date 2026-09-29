//! Perfluorocarbon emissions from anode effects.
//!
//! Primary aluminium smelting releases CF4 and C2F6 during anode effects.
//! The slope method uses the anode effect minutes per cell-day and a
//! technology-specific slope emission factor; the overvoltage method uses
//! the anode effect overvoltage per cell and the current efficiency. Both
//! convert the two gases to `CO2e` with the GWP table.
//!
//! @legal  IR (EU) 2025/2547, Annex II, sections B.7.1-B.7.3
//! @source <http://data.europa.eu/eli/reg_impl/2025/2547/oj>
//! @since  bundle 2026.2.0

use crate::error::RuleError;
use crate::fixed::{Fixed, SCALE};
use crate::parameters::GwpTable;

/// Thousand, the divisor converting kilograms of CF4 to tonnes.
const THOUSAND: Fixed = Fixed::from_scaled(1000 * SCALE);

/// Global warming potentials for the PFC species.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gwp {
    /// CF4, t `CO2e` per t.
    pub cf4: Fixed,
    /// C2F6, t `CO2e` per t.
    pub c2f6: Fixed,
}

impl Gwp {
    /// Reads the PFC global warming potentials from a bundle GWP table.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::MissingParameter`] when CF4 or C2F6 is absent
    /// from the table, and parse errors for malformed table values.
    pub fn from_table(table: &GwpTable) -> Result<Self, RuleError> {
        Ok(Self {
            cf4: gwp_value(table, "CF4")?,
            c2f6: gwp_value(table, "C2F6")?,
        })
    }
}

/// Reads one GWP value from a bundle table.
fn gwp_value(table: &GwpTable, substance: &str) -> Result<Fixed, RuleError> {
    let row = table
        .rows
        .iter()
        .find(|row| row.substance == substance)
        .ok_or(RuleError::MissingParameter)?;
    row.gwp100.parse()
}

/// Slope-method activity data for one reporting period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlopeInput {
    /// Anode effect minutes per cell-day, AEM.
    pub anode_effect_minutes_per_cell_day: Fixed,
    /// Slope emission factor, (kg CF4 / t Al) / (AE-min / cell-day), SEF.
    pub slope_emission_factor_cf4: Fixed,
    /// Weight fraction of C2F6 per CF4, F.
    pub weight_fraction_c2f6_cf4: Fixed,
    /// Primary aluminium produced, t.
    pub production_t: Fixed,
}

/// PFC emissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PfcEmissions {
    /// CF4 emissions, t.
    pub cf4_t: Fixed,
    /// C2F6 emissions, t.
    pub c2f6_t: Fixed,
    /// PFC emissions, t `CO2e`.
    pub pfc_tco2e: Fixed,
}

/// Computes PFC emissions with the slope method.
///
/// # Errors
///
/// Returns [`RuleError::Overflow`] when the arithmetic leaves the scaled
/// range.
pub fn slope(input: &SlopeInput, gwp: &Gwp) -> Result<PfcEmissions, RuleError> {
    let cf4_t = input
        .anode_effect_minutes_per_cell_day
        .try_mul(input.slope_emission_factor_cf4)?
        .try_mul(input.production_t)?
        .try_div(THOUSAND)?;
    let c2f6_t = cf4_t.try_mul(input.weight_fraction_c2f6_cf4)?;
    let cf4_co2e = cf4_t.try_mul(gwp.cf4)?;
    let c2f6_co2e = c2f6_t.try_mul(gwp.c2f6)?;
    Ok(PfcEmissions {
        cf4_t,
        c2f6_t,
        pfc_tco2e: cf4_co2e.try_add(c2f6_co2e)?,
    })
}

/// Overvoltage-method activity data for one reporting period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OvervoltageInput {
    /// Anode effect overvoltage per cell, AEO, mV.
    pub anode_effect_overvoltage_mv: Fixed,
    /// Average current efficiency of aluminium production, CE, per cent.
    pub current_efficiency_percent: Fixed,
    /// Overvoltage coefficient, OVC, (kg CF4 / t Al) / mV.
    pub overvoltage_coefficient_cf4: Fixed,
    /// Weight fraction of C2F6 per CF4, F.
    pub weight_fraction_c2f6_cf4: Fixed,
    /// Primary aluminium produced, t.
    pub production_t: Fixed,
}

/// Computes PFC emissions with the overvoltage method.
///
/// Method B of Annex II, section B.7.2: the CF4 emissions are the overvoltage
/// coefficient times the ratio of the anode effect overvoltage to the current
/// efficiency, times production; C2F6 follows from the weight fraction, and
/// both gases convert to `CO2e` with the GWP table (Equations 24-26).
///
/// # Errors
///
/// Returns [`RuleError::DivisionByZero`] for zero current efficiency and
/// [`RuleError::Overflow`] when the arithmetic leaves the scaled range.
pub fn overvoltage(input: &OvervoltageInput, gwp: &Gwp) -> Result<PfcEmissions, RuleError> {
    let cf4_t = input
        .overvoltage_coefficient_cf4
        .try_mul(
            input
                .anode_effect_overvoltage_mv
                .try_div(input.current_efficiency_percent)?,
        )?
        .try_mul(input.production_t)?
        .try_div(THOUSAND)?;
    let c2f6_t = cf4_t.try_mul(input.weight_fraction_c2f6_cf4)?;
    let cf4_co2e = cf4_t.try_mul(gwp.cf4)?;
    let c2f6_co2e = c2f6_t.try_mul(gwp.c2f6)?;
    Ok(PfcEmissions {
        cf4_t,
        c2f6_t,
        pfc_tco2e: cf4_co2e.try_add(c2f6_co2e)?,
    })
}

#[cfg(test)]
mod tests {
    use crate::parameters::{GwpRow, GwpTable};
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    fn fixed(text: &str) -> Fixed {
        text.parse().unwrap()
    }

    fn gwp_row(substance: &str, value: &str) -> GwpRow {
        GwpRow {
            substance: String::from(substance),
            gwp100: String::from(value),
            legal: String::from("test"),
            source: String::from("https://example.test"),
            retrieved: String::from("2026-09-16"),
        }
    }

    fn gwp_table(rows: Vec<GwpRow>) -> GwpTable {
        GwpTable {
            table: String::from("test"),
            legal: String::from("test"),
            source: String::from("https://example.test"),
            retrieved: String::from("2026-09-16"),
            rows,
        }
    }

    fn appendix_b_input() -> SlopeInput {
        SlopeInput {
            anode_effect_minutes_per_cell_day: fixed("0.25"),
            slope_emission_factor_cf4: fixed("0.143"),
            weight_fraction_c2f6_cf4: fixed("0.121"),
            production_t: fixed("100000"),
        }
    }

    fn appendix_b_gwp() -> Gwp {
        Gwp {
            cf4: fixed("6630"),
            c2f6: fixed("11100"),
        }
    }

    #[test]
    fn scales_the_appendix_b_activity_data() {
        let emissions = slope(&appendix_b_input(), &appendix_b_gwp()).unwrap();
        assert_eq!(emissions.cf4_t, fixed("3.575"));
        assert_eq!(emissions.c2f6_t, fixed("0.432575"));
        assert_eq!(emissions.pfc_tco2e, fixed("28503.8325"));
    }

    #[test]
    fn reads_gwp_values_from_a_table() {
        let table = gwp_table(vec![gwp_row("C2F6", "11100"), gwp_row("CF4", "6630")]);
        assert_eq!(Gwp::from_table(&table).unwrap(), appendix_b_gwp());
    }

    #[test]
    fn reports_missing_or_malformed_gwp_values() {
        let missing = gwp_table(vec![gwp_row("CF4", "6630")]);
        assert_eq!(Gwp::from_table(&missing), Err(RuleError::MissingParameter));
        let malformed = gwp_table(vec![gwp_row("CF4", "6630"), gwp_row("C2F6", "many")]);
        assert_eq!(Gwp::from_table(&malformed), Err(RuleError::InvalidDecimal));
    }

    #[test]
    fn reports_overflow() {
        let input = SlopeInput {
            anode_effect_minutes_per_cell_day: Fixed::from_scaled(i128::MAX),
            slope_emission_factor_cf4: fixed("2"),
            weight_fraction_c2f6_cf4: Fixed::ZERO,
            production_t: fixed("1"),
        };
        assert_eq!(slope(&input, &appendix_b_gwp()), Err(RuleError::Overflow));
    }

    fn overvoltage_input() -> OvervoltageInput {
        OvervoltageInput {
            anode_effect_overvoltage_mv: fixed("2.5"),
            current_efficiency_percent: fixed("96"),
            overvoltage_coefficient_cf4: fixed("1.16"),
            weight_fraction_c2f6_cf4: fixed("0.121"),
            production_t: fixed("100000"),
        }
    }

    #[test]
    fn scales_the_overvoltage_activity_data() {
        let emissions = overvoltage(&overvoltage_input(), &appendix_b_gwp()).unwrap();
        assert_eq!(emissions.cf4_t, fixed("3.0209"));
        assert_eq!(emissions.c2f6_t, fixed("0.365529"));
        assert_eq!(emissions.pfc_tco2e, fixed("24085.9389"));
    }

    #[test]
    fn reports_zero_current_efficiency() {
        let input = OvervoltageInput {
            current_efficiency_percent: Fixed::ZERO,
            ..overvoltage_input()
        };
        assert_eq!(
            overvoltage(&input, &appendix_b_gwp()),
            Err(RuleError::DivisionByZero)
        );
    }

    #[test]
    fn reports_overflow_for_overvoltage() {
        let input = OvervoltageInput {
            overvoltage_coefficient_cf4: Fixed::from_scaled(i128::MAX),
            ..overvoltage_input()
        };
        assert_eq!(
            overvoltage(&input, &appendix_b_gwp()),
            Err(RuleError::Overflow)
        );
    }
}
