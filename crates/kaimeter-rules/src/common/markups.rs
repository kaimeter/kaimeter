//! Sector-dependent default-value mark-ups.
//!
//! Default values carry a mark-up that rises over 2026-2028 for most
//! sectors; fertilisers carry a flat one per cent in every year.
//!
//! @legal  IR (EU) 2025/2621, Article 4(2), as corrected by IR (EU) 2026/1740
//! @source <http://data.europa.eu/eli/reg_impl/2025/2621/oj>
//! @since  bundle 2026.2.0

use crate::common::Sector;
use crate::common::period::ReportingPeriod;
use crate::fixed::Fixed;

/// One per cent, the fertiliser mark-up in every year.
const FERTILISER_MARKUP: Fixed = Fixed::from_scaled(10_000);

/// Ten per cent, the 2026 mark-up for the standard schedule.
const MARKUP_2026: Fixed = Fixed::from_scaled(100_000);

/// Twenty per cent, the 2027 mark-up for the standard schedule.
const MARKUP_2027: Fixed = Fixed::from_scaled(200_000);

/// Thirty per cent, the mark-up from 2028 for the standard schedule.
const MARKUP_2028: Fixed = Fixed::from_scaled(300_000);

/// Returns the default-value mark-up for a sector and reporting period.
///
/// The standard schedule applies to cement, iron and steel, aluminium and
/// hydrogen: ten per cent in 2026, twenty in 2027 and thirty from 2028.
/// Fertilisers carry one per cent in every year.
#[must_use]
pub fn markup(sector: Sector, period: ReportingPeriod) -> Fixed {
    if sector == Sector::Fertilisers {
        return FERTILISER_MARKUP;
    }
    match period.year() {
        2026 => MARKUP_2026,
        2027 => MARKUP_2027,
        _ => MARKUP_2028,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn period(year: u16) -> ReportingPeriod {
        ReportingPeriod::new(year).unwrap()
    }

    #[test]
    fn applies_ten_twenty_thirty_to_the_standard_schedule() {
        let sectors = [
            Sector::Cement,
            Sector::IronAndSteel,
            Sector::Aluminium,
            Sector::Hydrogen,
        ];
        for sector in sectors {
            assert_eq!(markup(sector, period(2026)), MARKUP_2026, "{sector:?}");
            assert_eq!(markup(sector, period(2027)), MARKUP_2027, "{sector:?}");
            assert_eq!(markup(sector, period(2028)), MARKUP_2028, "{sector:?}");
            assert_eq!(markup(sector, period(2035)), MARKUP_2028, "{sector:?}");
        }
    }

    #[test]
    fn keeps_the_fertiliser_markup_at_one_percent() {
        for year in [2026, 2027, 2028, 2035] {
            assert_eq!(
                markup(Sector::Fertilisers, period(year)),
                FERTILISER_MARKUP,
                "{year}"
            );
        }
    }
}
