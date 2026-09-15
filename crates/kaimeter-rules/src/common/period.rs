//! Reporting-period determination.
//!
//! The reporting period is the calendar year in which the goods are
//! imported; that year drives the default-value mark-up.
//!
//! @legal  IR (EU) 2025/2547, Article 7
//! @source <http://data.europa.eu/eli/reg_impl/2025/2547/oj>
//! @since  bundle 2026.2.0

use core::fmt;
use core::str::FromStr;

use crate::error::RuleError;

/// A calendar date in the proleptic Gregorian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Date {
    /// Creates a date from its components.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::InvalidDate`] when the components are not a real
    /// calendar date.
    pub fn from_parts(year: u16, month: u8, day: u8) -> Result<Self, RuleError> {
        if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return Err(RuleError::InvalidDate);
        }
        Ok(Self { year, month, day })
    }

    /// Parses a `YYYY-MM-DD` date.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::InvalidDate`] for any other shape or for a
    /// component that is not a real calendar date.
    pub fn from_iso(text: &str) -> Result<Self, RuleError> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(RuleError::InvalidDate);
        }
        let year = parse_field(&bytes[0..4])?;
        let month = parse_u8(&bytes[5..7])?;
        let day = parse_u8(&bytes[8..10])?;
        Self::from_parts(year, month, day)
    }

    /// Returns the calendar year.
    #[must_use]
    pub const fn year(self) -> u16 {
        self.year
    }

    /// Returns the month, `1` to `12`.
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// Returns the day of the month, `1` to `31`.
    #[must_use]
    pub const fn day(self) -> u8 {
        self.day
    }
}

impl FromStr for Date {
    type Err = RuleError;

    /// Parses a `YYYY-MM-DD` date.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::InvalidDate`] for any other shape or for a
    /// component that is not a real calendar date.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_iso(text)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

/// Parses an all-digit field into a `u16`.
fn parse_field(bytes: &[u8]) -> Result<u16, RuleError> {
    let mut value = 0_u16;
    for &byte in bytes {
        let digit = byte
            .checked_sub(b'0')
            .filter(|digit| *digit < 10)
            .ok_or(RuleError::InvalidDate)?;
        value = value * 10 + u16::from(digit);
    }
    Ok(value)
}

/// Parses an all-digit field into a `u8`, rejecting values above 255.
fn parse_u8(bytes: &[u8]) -> Result<u8, RuleError> {
    u8::try_from(parse_field(bytes)?).map_err(|_error| RuleError::InvalidDate)
}

/// Returns the number of days in a month, zero for an invalid month.
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Returns `true` for a leap year of the Gregorian calendar.
fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// The calendar year over which embedded emissions are determined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReportingPeriod(u16);

impl ReportingPeriod {
    /// First calendar year of the definitive period.
    pub const FIRST_YEAR: u16 = 2026;

    /// Creates a reporting period for a calendar year.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::UnsupportedPeriod`] for a year before
    /// [`ReportingPeriod::FIRST_YEAR`].
    pub fn new(year: u16) -> Result<Self, RuleError> {
        if year < Self::FIRST_YEAR {
            return Err(RuleError::UnsupportedPeriod { year });
        }
        Ok(Self(year))
    }

    /// Determines the reporting period containing a date.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::UnsupportedPeriod`] for a date before
    /// [`ReportingPeriod::FIRST_YEAR`].
    pub fn of(date: Date) -> Result<Self, RuleError> {
        Self::new(date.year())
    }

    /// Returns the calendar year.
    #[must_use]
    pub const fn year(self) -> u16 {
        self.0
    }
}

impl fmt::Display for ReportingPeriod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_dates() {
        assert_eq!(
            Date::from_iso("2026-01-01").unwrap(),
            Date::from_parts(2026, 1, 1).unwrap()
        );
        assert_eq!(
            Date::from_iso("2026-12-31").unwrap(),
            Date::from_parts(2026, 12, 31).unwrap()
        );
        assert!(Date::from_iso("2028-02-29").is_ok());
        assert!(Date::from_iso("2024-02-29").is_ok());
    }

    #[test]
    fn rejects_impossible_dates() {
        for text in [
            "",
            "2026",
            "2026-01",
            "2026-1-01",
            "2026-01-1",
            "20260101",
            "2026/01/01",
            "2026-01-01 ",
            " 2026-01-01",
            "2026-01-01x",
            "0000-01-01",
            "2026-00-01",
            "2026-13-01",
            "2026-01-00",
            "2026-01-32",
            "2026-04-31",
            "2026-02-29",
            "2100-02-29",
            "2026-aa-01",
            "abcd-01-01",
        ] {
            assert_eq!(
                Date::from_iso(text),
                Err(RuleError::InvalidDate),
                "{text:?}"
            );
        }
    }

    #[test]
    fn validates_components() {
        assert_eq!(Date::from_parts(2026, 0, 1), Err(RuleError::InvalidDate));
        assert_eq!(Date::from_parts(2026, 13, 1), Err(RuleError::InvalidDate));
        assert_eq!(Date::from_parts(2026, 2, 0), Err(RuleError::InvalidDate));
        assert_eq!(Date::from_parts(2026, 2, 30), Err(RuleError::InvalidDate));
        assert_eq!(Date::from_parts(2026, 1, 32), Err(RuleError::InvalidDate));
        assert_eq!(Date::from_parts(0, 1, 1), Err(RuleError::InvalidDate));
        assert_eq!(Date::from_parts(2100, 2, 29), Err(RuleError::InvalidDate));
        assert_eq!(
            Date::from_parts(2028, 2, 29).unwrap().to_string(),
            "2028-02-29"
        );
    }

    #[test]
    fn orders_dates_chronologically() {
        let first = Date::from_iso("2026-01-01").unwrap();
        let second = Date::from_iso("2026-01-02").unwrap();
        let third = Date::from_iso("2027-01-01").unwrap();
        assert!(first < second);
        assert!(second < third);
    }

    #[test]
    fn displays_and_parses_iso_dates() {
        let date: Date = "2026-01-01".parse().unwrap();
        assert_eq!(date.to_string(), "2026-01-01");
        assert_eq!(date.year(), 2026);
        assert_eq!(date.month(), 1);
        assert_eq!(date.day(), 1);
        assert_eq!(
            "2028-02-29".parse::<Date>().unwrap().to_string(),
            "2028-02-29"
        );
    }

    #[test]
    fn determines_the_reporting_period() {
        let period = ReportingPeriod::of(Date::from_iso("2026-06-15").unwrap()).unwrap();
        assert_eq!(period.year(), 2026);
        assert_eq!(period.to_string(), "2026");
        assert_eq!(ReportingPeriod::new(2026).unwrap().year(), 2026);
        assert!(ReportingPeriod::new(2026).unwrap() < ReportingPeriod::new(2027).unwrap());
    }

    #[test]
    fn rejects_periods_outside_the_bundle() {
        assert_eq!(
            ReportingPeriod::new(2025),
            Err(RuleError::UnsupportedPeriod { year: 2025 })
        );
        assert_eq!(
            ReportingPeriod::of(Date::from_iso("2025-12-31").unwrap()),
            Err(RuleError::UnsupportedPeriod { year: 2025 })
        );
    }
}
