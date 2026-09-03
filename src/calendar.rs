// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Deadline calendar (R14), quarterly holding monitor (R24) and the annual
//! declaration amendment window (R34).
//!
//! All deadlines are Brussels time: CET (UTC+1) in winter, CEST (UTC+2) in
//! summer. The tracking year is the calendar year.
//!
//! The date/quarter core below (DST resolution, quarter arithmetic) is frozen
//! and tested; the deadline tables and crossing rule build on it.

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;

// ---------------------------------------------------------------------------
// Date core (frozen): ISO parsing, day-of-week, EU DST rules
// ---------------------------------------------------------------------------

/// Parse an ISO-8601 `YYYY-MM-DD` date into `(year, month, day)`.
///
/// # Errors
///
/// [`DomainError::InvalidImportDate`] when the string is not a real
/// calendar date in `YYYY-MM-DD` form.
pub fn parse_iso(s: &str) -> Result<(i32, u32, u32), DomainError> {
    crate::domain::types::parse_iso_date_pub(s)
        .ok_or_else(|| DomainError::InvalidImportDate(s.to_string()))
}

/// Days from 1970-01-01 (Howard Hinnant's `days_from_civil`), valid for the
/// whole proleptic Gregorian range we care about.
#[must_use]
pub fn days_from_epoch(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(if m <= 2 { y - 1 } else { y });
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (i64::from(m) + 9) % 12; // Mar=0 .. Feb=11
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_epoch`] (Hinnant's `civil_from_days`).
#[must_use]
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

/// ISO weekday (Monday = 1 .. Sunday = 7) for a civil date.
#[must_use]
pub fn iso_weekday(y: i32, m: u32, d: u32) -> u32 {
    let days = days_from_epoch(y, m, d);
    // 1970-01-01 was a Thursday (ISO 4).
    (mod_i64(days + 3, 7) + 1) as u32
}

/// Last Sunday of the given month (used by the EU DST rule).
#[must_use]
pub fn last_sunday(y: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (y + 1, 1)
    } else {
        (y, month + 1)
    };
    let last_day = days_from_epoch(ny, nm, 1) - 1;
    let (ly, lm, ld) = civil_from_days(last_day);
    let wd = iso_weekday(ly, lm, ld);
    ld - (wd % 7) // back from Sunday(7) stays, Monday(1) goes back 1 ...
}

fn mod_i64(a: i64, b: i64) -> i64 {
    ((a % b) + b) % b
}

/// Brussels UTC offset in hours for a given date: +2 during CEST (summer),
/// +1 during CET (winter). Per EU rules CEST begins 01:00 UTC on the last
/// Sunday of March and ends 01:00 UTC on the last Sunday of October.
#[must_use]
pub fn brussels_offset_hours(y: i32, m: u32, d: u32) -> i32 {
    let day = days_from_epoch(y, m, d);
    let dst_start = days_from_epoch(y, 3, last_sunday(y, 3));
    let dst_end = days_from_epoch(y, 10, last_sunday(y, 10));
    if day >= dst_start && day < dst_end {
        2
    } else {
        1
    }
}

// ---------------------------------------------------------------------------
// Quarters (R24)
// ---------------------------------------------------------------------------

/// A calendar quarter, the cadence of the certificate-holding duty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Quarter {
    /// Calendar year.
    pub year: i32,
    /// Quarter number, 1..=4.
    pub q: u32,
}

impl Quarter {
    /// Construct and range-check a quarter.
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidQuarter`] when `q` is outside 1..=4.
    pub fn new(year: i32, q: u32) -> Result<Self, DomainError> {
        if !(1..=4).contains(&q) {
            return Err(DomainError::InvalidQuarter(year, q));
        }
        Ok(Self { year, q })
    }

    /// The quarter containing an ISO date.
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidImportDate`] when `iso` does not parse.
    pub fn from_iso(iso: &str) -> Result<Self, DomainError> {
        let (y, m, _) = parse_iso(iso)?;
        Ok(Self {
            year: y,
            q: (m - 1) / 3 + 1,
        })
    }

    /// The last calendar day of the quarter (the position-check date),
    /// ISO `YYYY-MM-DD`.
    #[must_use]
    pub fn end_iso(&self) -> String {
        let (m, d) = match self.q {
            1 => (3, 31),
            2 => (6, 30),
            3 => (9, 30),
            _ => (12, 31),
        };
        format!("{:04}-{:02}-{:02}", self.year, m, d)
    }

    /// The first calendar day of the quarter, ISO `YYYY-MM-DD`.
    #[must_use]
    pub fn start_iso(&self) -> String {
        format!("{:04}-{:02}-01", self.year, self.q * 3 - 2)
    }

    /// The next quarter in sequence (crossing year boundary).
    #[must_use]
    pub fn following(&self) -> Self {
        if self.q == 4 {
            Self {
                year: self.year + 1,
                q: 1,
            }
        } else {
            Self {
                year: self.year,
                q: self.q + 1,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Deadline kinds (R14) — agent: build the deadline tables on the core above
// ---------------------------------------------------------------------------

/// The fixed CBAM obligation dates Kaimeter tracks (R14/R24/R34).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeadlineKind {
    /// February 1st, 2027 — certificate sales start.
    CertificateSalesStart,
    /// September 30th — annual declaration AND certificate surrender.
    DeclarationAndSurrender,
    /// Quarter-end holding-position check (Mar 31 / Jun 30 / Sep 30 / Dec 31).
    QuarterlyHoldingCheck,
    /// October 31st — buyback request deadline of each surrender year.
    BuybackRequest,
    /// November 1st — certificates from year N-2 cancelled without compensation.
    CertificateCancellation,
    /// November 30th — close of the annual-declaration amendment window.
    AmendmentWindowClose,
}

/// One concrete deadline instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deadline {
    /// Which obligation this is.
    pub kind: DeadlineKind,
    /// Calendar date, ISO `YYYY-MM-DD`.
    pub date_iso: String,
    /// Brussels UTC offset in effect on that date (1 or 2).
    pub brussels_offset_hours: i32,
    /// i18n key for the human label.
    pub label_key: &'static str,
}

/// Build the full deadline calendar for one calendar year.
///
/// Includes the four quarterly holding checks when the year is 2027 or later
/// (the holding duty starts with the Q1 2027 quarter-end; certificates first
/// go on sale February 1st, 2027 — no holding duty exists in 2026).
///
/// # Errors
///
/// [`DomainError::InvalidImportDate`] never for fixed dates, but kept for the
/// date-core plumbing.
pub fn deadlines_for_year(year: i32) -> Result<Vec<Deadline>, DomainError> {
    // Certificates first go on sale 2027-02-01 (R14): no sales-start entry in
    // any other year, and no obligation of any kind exists in 2026.
    const FIRST_SALES_YEAR: i32 = 2027;
    let mut out: Vec<Deadline> = Vec::new();

    if year == FIRST_SALES_YEAR {
        out.push(deadline(
            DeadlineKind::CertificateSalesStart,
            year,
            2,
            1,
            "calendar.sales_start",
        ));
    }

    if year >= FIRST_SALES_YEAR {
        // Quarterly holding-position checks (R24, Art 22(2)); the duty starts
        // with the Q1 2027 quarter-end.
        for (m, d) in [(3, 31), (6, 30), (9, 30), (12, 31)] {
            out.push(deadline(
                DeadlineKind::QuarterlyHoldingCheck,
                year,
                m,
                d,
                "calendar.holding_check",
            ));
        }
        // First declaration AND surrender 2027-09-30, annual thereafter
        // (R14, Arts 6(5)/23).
        out.push(deadline(
            DeadlineKind::DeclarationAndSurrender,
            year,
            9,
            30,
            "calendar.declaration_surrender",
        ));
        // Buyback requests close October 31 of each surrender year (R24, Art 23).
        out.push(deadline(
            DeadlineKind::BuybackRequest,
            year,
            10,
            31,
            "calendar.buyback",
        ));
        // Year N−2 certificates cancelled without compensation (R24, Art 24).
        out.push(deadline(
            DeadlineKind::CertificateCancellation,
            year,
            11,
            1,
            "calendar.cancellation",
        ));
        // Amendment window closes November 30 of the declaration year
        // (R34, Art 6(5) as amended by Reg (EU) 2025/2083).
        out.push(deadline(
            DeadlineKind::AmendmentWindowClose,
            year,
            11,
            30,
            "calendar.amendment_close",
        ));
    }

    // Deterministic order: by date, then by label key for same-date entries
    // (Sep 30 carries both a holding check and the declaration/surrender).
    out.sort_by(|a, b| {
        a.date_iso
            .cmp(&b.date_iso)
            .then(a.label_key.cmp(b.label_key))
    });
    Ok(out)
}

/// Build one fixed-date [`Deadline`], resolving the Brussels offset from the
/// frozen date core.
fn deadline(kind: DeadlineKind, y: i32, m: u32, d: u32, label_key: &'static str) -> Deadline {
    Deadline {
        kind,
        date_iso: format!("{y:04}-{m:02}-{d:02}"),
        brussels_offset_hours: brussels_offset_hours(y, m, d),
        label_key,
    }
}

/// The quarter by which a declarant crossing the 50 t de-minimis line in
/// `crossed` must be in compliance with the 50 % holding obligation:
/// the end of the quarter FOLLOWING the crossing quarter (Art 22(2a)).
///
/// Note: the enacted Art 22(2a) ("...shall comply with the obligation laid
/// down in paragraph 2 by the end of the quarter following that in which the
/// single mass-based threshold is exceeded") contains NO 15-day buffer. That
/// figure belongs exclusively to R28's product-design verifier-correction
/// countdown and must not be applied to the holding duty.
///
/// # Errors
///
/// [`DomainError::InvalidQuarter`] propagated from [`Quarter::new`].
pub fn crossing_compliance_deadline(crossed: &Quarter) -> Result<Quarter, DomainError> {
    Ok(crossed.following())
}

/// Amendment-window state for a declaration of `declaration_year` on `today_iso`.
///
/// Returns `Open` before September 30 (initial filing still possible),
/// `AmendmentsOpen` from September 30 through November 30 (R34: corrected
/// declarations may be filed up to November 30 of the declaration year), and
/// `Closed` afterwards.
///
/// # Errors
///
/// [`DomainError::InvalidImportDate`] when `today_iso` does not parse.
pub fn amendment_window(
    declaration_year: i32,
    today_iso: &str,
) -> Result<AmendmentWindow, DomainError> {
    let (y, m, d) = parse_iso(today_iso)?;
    let today = days_from_epoch(y, m, d);
    let filing = days_from_epoch(declaration_year, 9, 30);
    let amendment_close = days_from_epoch(declaration_year, 11, 30);
    if today < filing {
        Ok(AmendmentWindow::Open)
    } else if today <= amendment_close {
        Ok(AmendmentWindow::AmendmentsOpen)
    } else {
        Ok(AmendmentWindow::Closed)
    }
}

/// State of the annual-declaration amendment window on a given day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AmendmentWindow {
    /// Before September 30: initial filing period.
    Open,
    /// September 30 – November 30: corrections accepted.
    AmendmentsOpen,
    /// After November 30: the declaration year is locked.
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brussels_dst_rules_are_pinned() {
        // Last Sunday of March 2027 is March 28; October 31 in 2027.
        assert_eq!(last_sunday(2027, 3), 28);
        assert_eq!(last_sunday(2027, 10), 31);
        assert_eq!(
            brussels_offset_hours(2027, 3, 27),
            1,
            "still CET before DST"
        );
        assert_eq!(
            brussels_offset_hours(2027, 3, 28),
            2,
            "CEST from last Sun Mar"
        );
        assert_eq!(
            brussels_offset_hours(2027, 10, 30),
            2,
            "still CEST before end"
        );
        assert_eq!(
            brussels_offset_hours(2027, 10, 31),
            1,
            "CET from last Sun Oct"
        );
        assert_eq!(
            brussels_offset_hours(2027, 2, 1),
            1,
            "certificate sales day is CET"
        );
        assert_eq!(brussels_offset_hours(2030, 7, 15), 2);
    }

    #[test]
    fn weekday_and_epoch_round_trip() {
        assert_eq!(iso_weekday(2026, 9, 30), 3, "Sep 30 2026 is a Wednesday");
        assert_eq!(days_from_epoch(1970, 1, 1), 0);
        assert_eq!(days_from_epoch(2026, 9, 30), 20_726);
        for &(y, m, d) in &[
            (2026, 9, 30),
            (2027, 2, 1),
            (2000, 2, 29),
            (2027, 11, 30),
            (1999, 12, 31),
        ] {
            assert_eq!(civil_from_days(days_from_epoch(y, m, d)), (y, m, d));
        }
    }

    #[test]
    fn quarters_round_trip_and_follow() {
        let q1 = Quarter::new(2027, 1).expect("q1");
        assert_eq!(q1.start_iso(), "2027-01-01");
        assert_eq!(q1.end_iso(), "2027-03-31");
        let q4 = Quarter::new(2027, 4).expect("q4");
        assert_eq!(q4.end_iso(), "2027-12-31");
        assert_eq!(q4.following(), Quarter::new(2028, 1).expect("2028 q1"));
        assert_eq!(
            Quarter::from_iso("2027-09-30").expect("from iso"),
            Quarter::new(2027, 3).expect("q3")
        );
        assert!(Quarter::new(2027, 5).is_err());
        assert!(Quarter::new(2027, 0).is_err());
    }

    #[test]
    fn iso_parser_rejects_impossible_dates() {
        assert!(parse_iso("2026-02-30").is_err());
        assert!(parse_iso("2026-13-01").is_err());
        assert!(parse_iso("2026-3-15").is_err());
        assert!(parse_iso("2026-02-29").is_err(), "2026 not a leap year");
        assert!(parse_iso("2024-02-29").is_ok());
    }
}
