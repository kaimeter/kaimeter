// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Integration tests for the `calendar` module (R14/R24/R34).
//!
//! These tests are the executable specification, written FIRST (RED): they
//! pin the obligation dates of the deadline calendar, the Art 22(2a)
//! threshold-crossing rule, and the Art 6(5) amendment-window boundaries.
//! Implementation follows to turn them GREEN.

use kaimeter_core::calendar::{
    amendment_window, crossing_compliance_deadline, deadlines_for_year, AmendmentWindow, Deadline,
    DeadlineKind, Quarter,
};

fn dates(kinds: &[Deadline]) -> Vec<(&'static str, String)> {
    kinds
        .iter()
        .map(|d| (d.label_key, d.date_iso.clone()))
        .collect()
}

/// R14 (Reg (EU) 2023/956 as amended by Reg (EU) 2025/2083): the exact 2027
/// obligation set — certificate sales start February 1st (Art 22 as amended);
/// the four quarterly holding-position checks start with the Q1 2027
/// quarter-end (Art 22(2)); the first declaration AND certificate surrender
/// fall on September 30th (Art 6(5)/Art 23); buyback requests close October
/// 31st (Art 23); year N−2 certificates are cancelled November 1st (Art 24);
/// the amendment window closes November 30th (Art 6(5) as amended). All
/// dates sorted, each carrying its Brussels CET/CEST offset.
#[test]
fn deadline_calendar_2027_is_pinned() {
    let all = deadlines_for_year(2027).expect("fixed dates never fail");
    assert_eq!(
        dates(&all),
        vec![
            ("calendar.sales_start", "2027-02-01".to_string()),
            ("calendar.holding_check", "2027-03-31".to_string()),
            ("calendar.holding_check", "2027-06-30".to_string()),
            ("calendar.declaration_surrender", "2027-09-30".to_string()),
            ("calendar.holding_check", "2027-09-30".to_string()),
            ("calendar.buyback", "2027-10-31".to_string()),
            ("calendar.cancellation", "2027-11-01".to_string()),
            ("calendar.amendment_close", "2027-11-30".to_string()),
            ("calendar.holding_check", "2027-12-31".to_string()),
        ],
        "pinned 2027 deadline set per R14/R24/R34"
    );

    let kinds: Vec<(String, DeadlineKind)> =
        all.iter().map(|d| (d.date_iso.clone(), d.kind)).collect();
    assert!(kinds.contains(&(
        "2027-02-01".to_string(),
        DeadlineKind::CertificateSalesStart
    )));
    assert!(kinds.contains(&(
        "2027-09-30".to_string(),
        DeadlineKind::DeclarationAndSurrender
    )));
    assert_eq!(
        kinds
            .iter()
            .filter(|(_, k)| *k == DeadlineKind::QuarterlyHoldingCheck)
            .count(),
        4,
        "Mar 31 / Jun 30 / Sep 30 / Dec 31 holding checks (Art 22(2))"
    );
    assert!(kinds.contains(&("2027-10-31".to_string(), DeadlineKind::BuybackRequest)));
    assert!(kinds.contains(&(
        "2027-11-01".to_string(),
        DeadlineKind::CertificateCancellation
    )));
    assert!(kinds.contains(&("2027-11-30".to_string(), DeadlineKind::AmendmentWindowClose)));

    // Brussels offsets pinned: Feb 1 is CET; Mar 31 through Sep 30 are CEST;
    // Oct 31 2027 is the day CEST ends, so CET from that day on.
    let offsets: Vec<(String, i32)> = all
        .iter()
        .map(|d| (d.date_iso.clone(), d.brussels_offset_hours))
        .collect();
    assert_eq!(
        offsets,
        vec![
            ("2027-02-01".to_string(), 1),
            ("2027-03-31".to_string(), 2),
            ("2027-06-30".to_string(), 2),
            ("2027-09-30".to_string(), 2),
            ("2027-09-30".to_string(), 2),
            ("2027-10-31".to_string(), 1),
            ("2027-11-01".to_string(), 1),
            ("2027-11-30".to_string(), 1),
            ("2027-12-31".to_string(), 1),
        ]
    );
}

/// R14/R24: certificates first go on sale February 1st, 2027 and the holding
/// duty starts with the Q1 2027 quarter-end — 2026 carries no obligation
/// dates at all.
#[test]
fn no_deadlines_in_2026() {
    let all = deadlines_for_year(2026).expect("fixed dates never fail");
    assert!(
        all.is_empty(),
        "no sales before Feb 2027, no holding in 2026"
    );
}

/// R14/R24/R34: from 2028 the annual cycle repeats — four holding checks,
/// September 30 declaration+surrender, October 31 buyback, November 1
/// cancellation, November 30 amendment close — and the one-off February
/// 2027 sales start does NOT reappear.
#[test]
fn deadlines_2028_repeat_annually() {
    let all = deadlines_for_year(2028).expect("fixed dates never fail");
    let kinds: Vec<DeadlineKind> = all.iter().map(|d| d.kind).collect();
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == DeadlineKind::QuarterlyHoldingCheck)
            .count(),
        4
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == DeadlineKind::DeclarationAndSurrender)
            .count(),
        1,
        "Sep 30 annual declaration and surrender"
    );
    assert!(!kinds.contains(&DeadlineKind::CertificateSalesStart));
    let ds: Vec<String> = all.iter().map(|d| d.date_iso.clone()).collect();
    assert_eq!(
        ds,
        vec![
            "2028-03-31",
            "2028-06-30",
            "2028-09-30",
            "2028-09-30",
            "2028-10-31",
            "2028-11-01",
            "2028-11-30",
            "2028-12-31",
        ]
    );
    assert!(ds.windows(2).all(|w| w[0] <= w[1]), "sorted by date");
}

/// Art 22(2a) CBAM Regulation as amended by Reg (EU) 2025/2083: a declarant
/// crossing the 50 t single mass-based threshold in a quarter must comply
/// with the 50 % holding obligation by the END of the FOLLOWING quarter.
/// The rejected "15-day buffer" reading has no basis in the enacted
/// text — the 15-day
/// figure belongs to R28's product-design verifier-correction countdown only.
#[test]
fn crossing_rule_is_following_quarter_end() {
    let q1_2027 = Quarter::new(2027, 1).expect("q1");
    assert_eq!(
        crossing_compliance_deadline(&q1_2027).expect("valid quarter"),
        Quarter::new(2027, 2).expect("q2"),
        "crossing Q1 2027 -> comply by end of Q2 2027 (Art 22(2a))"
    );
    let q4_2027 = Quarter::new(2027, 4).expect("q4");
    assert_eq!(
        crossing_compliance_deadline(&q4_2027).expect("valid quarter"),
        Quarter::new(2028, 1).expect("2028 q1"),
        "crossing Q4 2027 -> comply by end of Q1 2028 (year rollover)"
    );
    assert_eq!(
        crossing_compliance_deadline(&q1_2027)
            .expect("valid quarter")
            .end_iso(),
        "2027-06-30",
        "the compliance date is the following quarter's end"
    );
}

/// Art 6(5) Reg (EU) 2023/956 as amended by Reg (EU) 2025/2083 (R34): the
/// window is `Open` (initial filing) before September 30 of the declaration
/// year, `AmendmentsOpen` from September 30 through November 30 inclusive,
/// and `Closed` from December 1.
#[test]
fn amendment_window_boundaries() {
    let y = 2027;
    assert_eq!(
        amendment_window(y, "2027-09-29").expect("valid date"),
        AmendmentWindow::Open,
        "Sep 29: initial filing still possible"
    );
    assert_eq!(
        amendment_window(y, "2027-09-30").expect("valid date"),
        AmendmentWindow::AmendmentsOpen,
        "Sep 30: filing date reached, amendments accepted"
    );
    assert_eq!(
        amendment_window(y, "2027-11-30").expect("valid date"),
        AmendmentWindow::AmendmentsOpen,
        "Nov 30 inclusive: last day to file corrections"
    );
    assert_eq!(
        amendment_window(y, "2027-12-01").expect("valid date"),
        AmendmentWindow::Closed,
        "Dec 1: the declaration year is locked"
    );
}
