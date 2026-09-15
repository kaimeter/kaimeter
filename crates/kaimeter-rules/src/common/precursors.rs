//! Precursor attribution and Art. 14 averaging.
//!
//! Where a precursor under one CN code arrives from several installations or
//! production periods, its embedded emissions are the mass-weighted average
//! of the supplies (Art. 14(2)). Where the operator can evidence that only a
//! single installation, period or subset was used for a specific production
//! process, that subset is passed and its values are used directly
//! (Art. 14(3)). Precursors produced in the EU or in excluded countries and
//! territories are zero-rated, but their mass still counts.
//!
//! @legal  IR (EU) 2025/2547, Articles 13-14
//! @source <https://taxation-customs.ec.europa.eu/document/download/29b9eec7-1a4b-4eb6-ab85-96a0c9e35fd0_en?filename=Guidance%20No.%203%20-%20CBAM%20methods%20for%20the%20calculation%20of%20emissions%20embedded%20in%20goods.pdf>
//! @since  bundle 2026.2.0

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Origin class of a precursor supply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrecursorOrigin {
    /// Produced in a third country; embedded emissions count.
    ThirdCountry,
    /// Produced in the EU or in an excluded country or territory; zero
    /// embedded emissions are added (Regulation (EU) 2023/956, Annex III).
    EuOrExcluded,
}

/// One consumed precursor supply under one CN code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrecursorSupply {
    /// Specific embedded emissions of the supply, t `CO2e` per t; ignored
    /// for [`PrecursorOrigin::EuOrExcluded`].
    pub see: Fixed,
    /// Consumed quantity of the precursor, t.
    pub quantity_t: Fixed,
    /// Origin class of the supply.
    pub origin: PrecursorOrigin,
}

/// Computes the mass-weighted average SEE of precursor supplies (Art. 14(2)).
///
/// EU and excluded-origin supplies contribute zero emissions but keep their
/// mass in the denominator. Rounding is half to even at the bundle scale.
///
/// # Errors
///
/// Returns [`RuleError::NoPrecursors`] for an empty supply list,
/// [`RuleError::DivisionByZero`] when the total quantity is zero, and
/// [`RuleError::Overflow`] when the arithmetic leaves the scaled range.
pub fn weighted_average(supplies: &[PrecursorSupply]) -> Result<Fixed, RuleError> {
    if supplies.is_empty() {
        return Err(RuleError::NoPrecursors);
    }
    let mut weighted = Fixed::ZERO;
    let mut total = Fixed::ZERO;
    for supply in supplies {
        total = total.try_add(supply.quantity_t)?;
        weighted = weighted.try_add(contribution(supply)?)?;
    }
    weighted.try_div(total)
}

/// Returns the weighted emissions of one supply.
fn contribution(supply: &PrecursorSupply) -> Result<Fixed, RuleError> {
    match supply.origin {
        PrecursorOrigin::ThirdCountry => supply.quantity_t.try_mul(supply.see),
        PrecursorOrigin::EuOrExcluded => Ok(Fixed::ZERO),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(text: &str) -> Fixed {
        text.parse().unwrap()
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
    fn averages_by_mass() {
        let supplies = [third("1.835", "600"), third("1.910", "430")];
        assert_eq!(weighted_average(&supplies).unwrap(), fixed("1.866311"));
    }

    #[test]
    fn keeps_eu_mass_in_the_denominator() {
        let supplies = [third("2.0", "100"), zero_rated("9.9", "100")];
        assert_eq!(weighted_average(&supplies).unwrap(), fixed("1.0"));
    }

    #[test]
    fn passes_a_single_supply_through() {
        let third_country = [third("2.5", "100")];
        assert_eq!(weighted_average(&third_country).unwrap(), fixed("2.5"));
        let eu = [zero_rated("2.5", "100")];
        assert_eq!(weighted_average(&eu).unwrap(), Fixed::ZERO);
    }

    #[test]
    fn zero_rates_every_eu_supply() {
        let supplies = [zero_rated("3.0", "100"), zero_rated("4.0", "50")];
        assert_eq!(weighted_average(&supplies).unwrap(), Fixed::ZERO);
    }

    #[test]
    fn reports_empty_supplies() {
        assert_eq!(weighted_average(&[]), Err(RuleError::NoPrecursors));
    }

    #[test]
    fn reports_zero_mass() {
        let supplies = [third("1.0", "0")];
        assert_eq!(weighted_average(&supplies), Err(RuleError::DivisionByZero));
    }

    #[test]
    fn reports_overflow() {
        let supplies = [PrecursorSupply {
            see: Fixed::from_scaled(i128::MAX),
            quantity_t: Fixed::from_scaled(2_000_000),
            origin: PrecursorOrigin::ThirdCountry,
        }];
        assert_eq!(weighted_average(&supplies), Err(RuleError::Overflow));
    }
}
