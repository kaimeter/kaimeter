// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Phased CBAM mark-up table.
//!
//! Regulatory pins (tests enforce these exactly):
//! - 2026: **+10 %**
//! - 2027: **+20 %**
//! - 2028 onward: **+30 %**
//! - Fertilisers: **+1 %** in every year of the schedule.

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::types::{DefaultValue, Sector};

/// Buckets of the phased mark-up schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarkupYear {
    /// 2026 — first year, +10 %.
    Y2026,
    /// 2027 — +20 %.
    Y2027,
    /// 2028 and later — +30 % (fertilisers +1 %).
    Y2028Plus,
}

impl MarkupYear {
    /// All buckets in ascending order.
    pub const ALL: [MarkupYear; 3] = [MarkupYear::Y2026, MarkupYear::Y2027, MarkupYear::Y2028Plus];

    /// Map a calendar year to its schedule bucket; `None` before 2026.
    #[must_use]
    pub fn from_calendar_year(year: i32) -> Option<Self> {
        match year {
            2026 => Some(Self::Y2026),
            2027 => Some(Self::Y2027),
            y if y >= 2028 => Some(Self::Y2028Plus),
            _ => None,
        }
    }

    /// First calendar year of the bucket.
    #[must_use]
    pub fn first_calendar_year(self) -> i32 {
        match self {
            Self::Y2026 => 2026,
            Self::Y2027 => 2027,
            Self::Y2028Plus => 2028,
        }
    }

    /// Stable key fragment for persistence/UI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Y2026 => "2026",
            Self::Y2027 => "2027",
            Self::Y2028Plus => "2028+",
        }
    }
}

/// Mark-up percentage for `sector` in `year`.
///
/// # Errors
///
/// [`DomainError::MarkupYearOutOfRange`] for years before 2026.
pub fn markup_percent(year: i32, sector: &Sector) -> Result<f64, DomainError> {
    let bucket =
        MarkupYear::from_calendar_year(year).ok_or(DomainError::MarkupYearOutOfRange(year))?;
    let percent = if sector == &Sector::Fertilisers {
        1.0
    } else {
        match bucket {
            MarkupYear::Y2026 => 10.0,
            MarkupYear::Y2027 => 20.0,
            MarkupYear::Y2028Plus => 30.0,
        }
    };
    Ok(percent)
}

/// Multiplier equivalent of [`markup_percent`] (e.g. 2026 → 1.10).
///
/// # Errors
///
/// [`DomainError::MarkupYearOutOfRange`] for years before 2026.
pub fn markup_factor(year: i32, sector: &Sector) -> Result<f64, DomainError> {
    Ok(1.0 + markup_percent(year, sector)? / 100.0)
}

/// Apply the year's mark-up to a [`DefaultValue`], returning a copy with
/// both emission intensities scaled by the mark-up factor.
///
/// # Errors
///
/// [`DomainError::MarkupYearOutOfRange`] for years before 2026.
pub fn apply(default: &DefaultValue, year: i32) -> Result<DefaultValue, DomainError> {
    let factor = markup_factor(year, &default.cn_code.sector())?;
    let mut out = default.clone();
    out.direct_tco2e_per_t *= factor;
    out.indirect_tco2e_per_t *= factor;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::types::{CnCode, DefaultValue};

    fn default_for(sector: Sector) -> DefaultValue {
        DefaultValue {
            cn_code: CnCode::new("99999999", "test", sector).expect("valid cn"),
            production_route: "TEST".to_string(),
            direct_tco2e_per_t: 1.0,
            indirect_tco2e_per_t: 2.0,
            markups: BTreeMap::new(),
        }
    }

    #[test]
    fn schedule_pins_exact_percentages() {
        for sector in [
            Sector::Steel,
            Sector::Aluminium,
            Sector::Cement,
            Sector::Hydrogen,
        ] {
            assert_eq!(markup_percent(2026, &sector).unwrap(), 10.0);
            assert_eq!(markup_percent(2027, &sector).unwrap(), 20.0);
            assert_eq!(markup_percent(2028, &sector).unwrap(), 30.0);
            assert_eq!(markup_percent(2050, &sector).unwrap(), 30.0);
        }
        for year in [2026, 2027, 2028, 2030, 2050] {
            assert_eq!(markup_percent(year, &Sector::Fertilisers).unwrap(), 1.0);
        }
    }

    #[test]
    fn buckets_map_calendar_years() {
        assert_eq!(MarkupYear::from_calendar_year(2025), None);
        assert_eq!(
            MarkupYear::from_calendar_year(2026),
            Some(MarkupYear::Y2026)
        );
        assert_eq!(
            MarkupYear::from_calendar_year(2027),
            Some(MarkupYear::Y2027)
        );
        assert_eq!(
            MarkupYear::from_calendar_year(2028),
            Some(MarkupYear::Y2028Plus)
        );
        assert_eq!(
            MarkupYear::from_calendar_year(2099),
            Some(MarkupYear::Y2028Plus)
        );
    }

    #[test]
    fn factors_and_apply_scale_both_intensities() {
        assert!((markup_factor(2026, &Sector::Steel).unwrap() - 1.10).abs() < 1e-12);
        assert!((markup_factor(2027, &Sector::Steel).unwrap() - 1.20).abs() < 1e-12);
        assert!((markup_factor(2028, &Sector::Steel).unwrap() - 1.30).abs() < 1e-12);

        let dv = default_for(Sector::Steel);
        let out26 = apply(&dv, 2026).expect("apply");
        assert!((out26.direct_tco2e_per_t - 1.1).abs() < 1e-9);
        assert!((out26.indirect_tco2e_per_t - 2.2).abs() < 1e-9);

        let out28 = apply(&dv, 2028).expect("apply");
        assert!((out28.direct_tco2e_per_t - 1.3).abs() < 1e-9);
        assert!((out28.indirect_tco2e_per_t - 2.6).abs() < 1e-9);

        let fert = default_for(Sector::Fertilisers);
        let out = apply(&fert, 2027).expect("apply");
        assert!((out.direct_tco2e_per_t - 1.01).abs() < 1e-9);
        assert!((out.indirect_tco2e_per_t - 2.02).abs() < 1e-9);
    }

    #[test]
    fn years_before_2026_are_rejected() {
        for year in [2020, 2024, 2025] {
            assert!(matches!(
                markup_percent(year, &Sector::Steel),
                Err(DomainError::MarkupYearOutOfRange(y)) if y == year
            ));
        }
        assert!(apply(&default_for(Sector::Steel), 2025).is_err());
    }
}
