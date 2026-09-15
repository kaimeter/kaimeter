//! Primary and secondary aluminium routes.
//!
//! The primary route attributes directly monitored CO2 plus PFC emissions
//! from anode effects to the primary aluminium produced; the secondary route
//! arrives with v0.2.
//!
//! @legal  IR (EU) 2025/2547, Annex II, section B.7
//! @source <http://data.europa.eu/eli/reg_impl/2025/2547/oj>
//! @since  bundle 2026.2.0

use crate::error::RuleError;
use crate::fixed::Fixed;
use crate::sectors::aluminium::pfc::{self, Gwp, SlopeInput};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
