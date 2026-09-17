//! Primary and secondary aluminium routes.
//!
//! The primary route attributes directly monitored CO2 plus PFC emissions
//! from anode effects to the primary aluminium produced; the secondary route
//! attributes the direct emissions of melting and casting and treats added
//! unwrought aluminium like a precursor.
//!
//! @legal  IR (EU) 2025/2547, Annex I, point 3.17.2.2, and Annex II, section B.7
//! @source <http://data.europa.eu/eli/reg_impl/2025/2547/oj>
//! @since  bundle 2026.2.0

use crate::common::precursors::{self, PrecursorSupply};
use crate::error::RuleError;
use crate::fixed::Fixed;
use crate::sectors::aluminium::pfc::{self, Gwp, OvervoltageInput, SlopeInput};

/// Primary-route emissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimaryEmissions {
    /// CF4 emissions, t.
    pub cf4_t: Fixed,
    /// C2F6 emissions, t.
    pub c2f6_t: Fixed,
    /// PFC emissions, t `CO2e`.
    pub pfc_tco2e: Fixed,
    /// Specific embedded emissions, t `CO2e` per t.
    pub see_tco2e_per_t: Fixed,
}

/// Computes the specific embedded emissions of primary aluminium, slope
/// method.
///
/// The attributed emissions are the directly monitored CO2 plus the PFC
/// emissions, divided by the primary aluminium produced.
///
/// # Errors
///
/// Returns [`RuleError::DivisionByZero`] for zero production and
/// [`RuleError::Overflow`] when the arithmetic leaves the scaled range.
pub fn see_primary_slope(
    input: &SlopeInput,
    direct_co2_t: Fixed,
    gwp: &Gwp,
) -> Result<PrimaryEmissions, RuleError> {
    let pfc = pfc::slope(input, gwp)?;
    let attributed = direct_co2_t.try_add(pfc.pfc_tco2e)?;
    Ok(PrimaryEmissions {
        cf4_t: pfc.cf4_t,
        c2f6_t: pfc.c2f6_t,
        pfc_tco2e: pfc.pfc_tco2e,
        see_tco2e_per_t: attributed.try_div(input.production_t)?,
    })
}

/// Computes the specific embedded emissions of primary aluminium, overvoltage
/// method.
///
/// The attributed emissions are the directly monitored CO2 plus the PFC
/// emissions, divided by the primary aluminium produced.
///
/// # Errors
///
/// Returns [`RuleError::DivisionByZero`] for zero production and
/// [`RuleError::Overflow`] when the arithmetic leaves the scaled range.
pub fn see_primary_overvoltage(
    input: &OvervoltageInput,
    direct_co2_t: Fixed,
    gwp: &Gwp,
) -> Result<PrimaryEmissions, RuleError> {
    let pfc = pfc::overvoltage(input, gwp)?;
    let attributed = direct_co2_t.try_add(pfc.pfc_tco2e)?;
    Ok(PrimaryEmissions {
        cf4_t: pfc.cf4_t,
        c2f6_t: pfc.c2f6_t,
        pfc_tco2e: pfc.pfc_tco2e,
        see_tco2e_per_t: attributed.try_div(input.production_t)?,
    })
}

/// Computes the specific embedded emissions of secondary aluminium.
///
/// Secondary melting uses aluminium scrap as its main input, so the
/// attributed emissions are the directly monitored CO2 of melting, scrap
/// pre-treatment, casting and slag recovery. Unwrought aluminium added from
/// other sources is treated like a precursor, and PFC emissions do not arise.
///
/// # Errors
///
/// Returns [`RuleError::DivisionByZero`] for zero production and
/// [`RuleError::Overflow`] when the arithmetic leaves the scaled range.
pub fn see_secondary(
    direct_co2_t: Fixed,
    production_t: Fixed,
    added_unwrought: &[PrecursorSupply],
) -> Result<Fixed, RuleError> {
    precursors::see_complex(direct_co2_t, production_t, added_unwrought)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::precursors::PrecursorOrigin;

    fn fixed(text: &str) -> Fixed {
        text.parse().unwrap()
    }

    fn input() -> SlopeInput {
        SlopeInput {
            anode_effect_minutes_per_cell_day: fixed("0.25"),
            slope_emission_factor_cf4: fixed("0.143"),
            weight_fraction_c2f6_cf4: fixed("0.121"),
            production_t: fixed("100000"),
        }
    }

    fn gwp() -> Gwp {
        Gwp {
            cf4: fixed("6630"),
            c2f6: fixed("11100"),
        }
    }

    #[test]
    fn divides_attributed_emissions_by_production() {
        let emissions = see_primary_slope(&input(), fixed("155000"), &gwp()).unwrap();
        assert_eq!(emissions.cf4_t, fixed("3.575"));
        assert_eq!(emissions.c2f6_t, fixed("0.432575"));
        assert_eq!(emissions.pfc_tco2e, fixed("28503.8325"));
        assert_eq!(emissions.see_tco2e_per_t, fixed("1.835038"));
    }

    #[test]
    fn rounds_the_specific_emissions_half_to_even() {
        let input = SlopeInput {
            anode_effect_minutes_per_cell_day: Fixed::ZERO,
            slope_emission_factor_cf4: Fixed::ZERO,
            weight_fraction_c2f6_cf4: Fixed::ZERO,
            production_t: fixed("2000000"),
        };
        let gwp = Gwp {
            cf4: Fixed::ZERO,
            c2f6: Fixed::ZERO,
        };
        // 3 / 2,000,000 = 0.0000015, and 5 / 2,000,000 = 0.0000025; both
        // round half to even, to 0.000002.
        let odd = see_primary_slope(&input, fixed("3"), &gwp).unwrap();
        assert_eq!(odd.see_tco2e_per_t, fixed("0.000002"));
        let even = see_primary_slope(&input, fixed("5"), &gwp).unwrap();
        assert_eq!(even.see_tco2e_per_t, fixed("0.000002"));
    }

    #[test]
    fn reports_zero_production() {
        let input = SlopeInput {
            production_t: Fixed::ZERO,
            ..input()
        };
        assert_eq!(
            see_primary_slope(&input, fixed("155000"), &gwp()),
            Err(RuleError::DivisionByZero)
        );
    }

    #[test]
    fn reports_overflow() {
        assert_eq!(
            see_primary_slope(&input(), Fixed::from_scaled(i128::MAX), &gwp()),
            Err(RuleError::Overflow)
        );
    }

    fn third(see: &str, quantity: &str) -> PrecursorSupply {
        PrecursorSupply {
            see: fixed(see),
            quantity_t: fixed(quantity),
            origin: PrecursorOrigin::ThirdCountry,
        }
    }

    fn zero_rated(see: &str, quantity: &str) -> PrecursorSupply {
        PrecursorSupply {
            see: fixed(see),
            quantity_t: fixed(quantity),
            origin: PrecursorOrigin::EuOrExcluded,
        }
    }

    #[test]
    fn divides_secondary_direct_emissions_by_production() {
        assert_eq!(
            see_secondary(fixed("40"), fixed("950"), &[]).unwrap(),
            fixed("0.042105")
        );
    }

    #[test]
    fn treats_added_unwrought_aluminium_as_a_precursor() {
        let supplies = [third("1.9", "100"), zero_rated("9.9", "30")];
        assert_eq!(
            see_secondary(fixed("40"), fixed("950"), &supplies).unwrap(),
            fixed("0.242105")
        );
    }

    #[test]
    fn reports_zero_production_for_secondary() {
        assert_eq!(
            see_secondary(fixed("40"), Fixed::ZERO, &[]),
            Err(RuleError::DivisionByZero)
        );
    }

    #[test]
    fn reports_overflow_for_secondary() {
        let supplies = [PrecursorSupply {
            see: Fixed::from_scaled(i128::MAX),
            quantity_t: Fixed::from_scaled(2_000_000),
            origin: PrecursorOrigin::ThirdCountry,
        }];
        assert_eq!(
            see_secondary(Fixed::ZERO, Fixed::ONE, &supplies),
            Err(RuleError::Overflow)
        );
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
    fn divides_overvoltage_attributed_emissions_by_production() {
        let emissions =
            see_primary_overvoltage(&overvoltage_input(), fixed("155000"), &gwp()).unwrap();
        assert_eq!(emissions.cf4_t, fixed("3.0209"));
        assert_eq!(emissions.c2f6_t, fixed("0.365529"));
        assert_eq!(emissions.pfc_tco2e, fixed("24085.9389"));
        assert_eq!(emissions.see_tco2e_per_t, fixed("1.790859"));
    }

    #[test]
    fn reports_zero_production_for_overvoltage() {
        let input = OvervoltageInput {
            production_t: Fixed::ZERO,
            ..overvoltage_input()
        };
        assert_eq!(
            see_primary_overvoltage(&input, fixed("155000"), &gwp()),
            Err(RuleError::DivisionByZero)
        );
    }

    #[test]
    fn reports_overflow_for_overvoltage_see() {
        assert_eq!(
            see_primary_overvoltage(&overvoltage_input(), Fixed::from_scaled(i128::MAX), &gwp()),
            Err(RuleError::Overflow)
        );
    }
}
