// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Integration tests for the `liability` module.
//!
//! These tests are the executable specification: written FIRST (RED), they
//! pin per-EORI physical tenant isolation (R25), the master rollup over the
//! 50 t de-minimis line (R25), separable site boundaries in group rollups
//! (R26), and ICR joint-and-several liability tagging (R46, Art 5(2) & 26).
//! Implementation follows to turn them GREEN.

use std::path::PathBuf;

use kaimeter_core::db::Storage;
use kaimeter_core::domain::errors::DomainError;
use kaimeter_core::liability::{
    aggregate_liability_mass_kg, group_rollup, liability_tag, open_workspace, rollup_workspaces,
    workspace_db_path, DeclarantRollup, GroupHierarchy, LiabilityTag, SiteRecord,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Fresh scratch data directory per test tag.
fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kaimeter-liability-it-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir scratch dir");
    dir
}

/// Build one production site for a group.
fn site(installation_id: &str, country: &str) -> SiteRecord {
    SiteRecord {
        installation_id: installation_id.to_string(),
        name: format!("site {installation_id}"),
        country: country.to_string(),
    }
}

/// Two-declarant group hierarchy: INST-CN-01 and INST-CN-02.
fn cn_group() -> GroupHierarchy {
    GroupHierarchy {
        group_id: "G-CN-1".to_string(),
        sites: vec![site("INST-CN-01", "CN"), site("INST-CN-02", "CN")],
    }
}

/// Collect every file under `dir` recursively (used to prove a bad EORI never
/// became a filename anywhere on disk).
fn all_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match std::fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// ---------------------------------------------------------------------------
// R25 — per-EORI physical tenant isolation
// ---------------------------------------------------------------------------

/// R25: tenant isolation is PHYSICAL — one SQLite file per EORI under
/// `db/tenants/`, so one ICR never mixes two clients' data. A marker row
/// written into workspace A's database must be invisible from workspace B,
/// and the two databases must live at distinct paths.
#[test]
fn tenant_databases_are_physically_separate() {
    let data_dir = scratch_dir("tenant-isolation");

    let workspace_a = open_workspace(&data_dir, "DE12345678").expect("workspace A opens");
    let workspace_b = open_workspace(&data_dir, "NL12345678").expect("workspace B opens");

    workspace_a
        .execute(
            "INSERT INTO settings (key, value) VALUES ('marker', 'A')",
            &[],
        )
        .expect("insert marker into workspace A");

    // Workspace B must NOT see workspace A's marker row.
    let seen_from_b = workspace_b
        .query_scalar("SELECT value FROM settings WHERE key = 'marker'", &[])
        .expect("query marker from workspace B");
    assert_eq!(seen_from_b, None, "R25: workspace B must not see A's data");

    // Workspace A still sees its own marker (sanity).
    let seen_from_a = workspace_a
        .query_scalar("SELECT value FROM settings WHERE key = 'marker'", &[])
        .expect("query marker from workspace A");
    assert_eq!(seen_from_a.as_deref(), Some("A"));

    // The two databases are distinct physical files under db/tenants/.
    let path_a = workspace_db_path(&data_dir, "DE12345678").expect("path A");
    let path_b = workspace_db_path(&data_dir, "NL12345678").expect("path B");
    assert_ne!(path_a, path_b, "R25: each EORI gets its own .db file");
    assert!(
        path_a.starts_with(data_dir.join("tenants")),
        "R25: databases live under db/tenants/ (physical segregation)"
    );
    assert_eq!(
        path_a.file_name().and_then(|n| n.to_str()),
        Some("DE12345678.db"),
        "R25: file name is the EORI"
    );
    assert!(path_a.is_file(), "open_workspace must have created A's db");
    assert!(path_b.is_file(), "open_workspace must have created B's db");
}

/// R25: EORI format validation runs BEFORE path construction — a bad EORI is
/// rejected as a Storage error and must never become a filename on disk.
#[test]
fn bad_eori_never_becomes_a_filename() {
    let data_dir = scratch_dir("bad-eori");

    let err = workspace_db_path(&data_dir, "de!!")
        .expect_err("R25: invalid EORI must fail validation first");
    assert!(
        matches!(err, DomainError::Storage(_)),
        "validation failure maps into DomainError::Storage"
    );

    // open_workspace rejects the same EORI before creating anything.
    let open_err = match open_workspace(&data_dir, "de!!") {
        Err(e) => e,
        Ok(_) => panic!("open_workspace must reject a bad EORI before opening"),
    };
    assert!(matches!(open_err, DomainError::Storage(_)));

    // Nothing containing the bad EORI ever landed on disk.
    for file in all_files(&data_dir) {
        assert!(
            !file.to_string_lossy().contains("de!!"),
            "R25: a bad EORI must never become a filename, found {file:?}"
        );
    }
}

/// R25: the master-importer view rolls up total net mass across ALL connected
/// ICR workspaces, so the 50 t de-minimis line is watched ONCE — 10 t + 20 t +
/// 25 t = 55 t crosses the line although each declarant alone would not.
#[test]
fn rollup_watches_50t_once() {
    let rolls = [
        DeclarantRollup {
            eori: "DE12345678".to_string(),
            ytd_net_mass_kg: 10_000.0,
            open_dossiers: 1,
        },
        DeclarantRollup {
            eori: "NL12345678".to_string(),
            ytd_net_mass_kg: 20_000.0,
            open_dossiers: 0,
        },
        DeclarantRollup {
            eori: "PL12345678".to_string(),
            ytd_net_mass_kg: 25_000.0,
            open_dossiers: 2,
        },
    ];
    let total = rollup_workspaces(&rolls);
    assert!(
        close(total, 55_000.0),
        "R25: master view aggregates across workspaces, got {total}"
    );
    assert!(close(rollup_workspaces(&[]), 0.0), "no workspaces, no mass");
}

// ---------------------------------------------------------------------------
// R26 — group & multi-installation hierarchy
// ---------------------------------------------------------------------------

/// R26: each installation ID is the verifier-facing unit; group rollups never
/// blur site boundaries. The pairing is returned per site in group order;
/// unknown, duplicate, and missing site ids are all rejected.
#[test]
fn group_rollup_keeps_site_boundaries() {
    let group = cn_group();

    // Happy path: values pair with sites in group.sites order.
    let values = [("INST-CN-01", 120.0), ("INST-CN-02", 80.0)];
    let paired = group_rollup(&group, &values).expect("R26: per-site pairing");
    assert_eq!(
        paired,
        vec![("INST-CN-01", 120.0), ("INST-CN-02", 80.0)],
        "R26: pairing follows group.sites order"
    );

    // Order of input values must not matter.
    let shuffled = [("INST-CN-02", 80.0), ("INST-CN-01", 120.0)];
    let paired = group_rollup(&group, &shuffled).expect("R26: input order irrelevant");
    assert_eq!(
        paired,
        vec![("INST-CN-01", 120.0), ("INST-CN-02", 80.0)],
        "R26: output order is group.sites order"
    );

    // Unknown site id (not in group.sites) -> Err.
    let unknown = [
        ("INST-CN-01", 1.0),
        ("INST-CN-02", 1.0),
        ("INST-XX-99", 1.0),
    ];
    let err = group_rollup(&group, &unknown).expect_err("R26: unknown site rejected");
    assert!(matches!(err, DomainError::Storage(_)));

    // Duplicate site id in per_site_values -> Err.
    let duplicated = [
        ("INST-CN-01", 1.0),
        ("INST-CN-01", 2.0),
        ("INST-CN-02", 1.0),
    ];
    let err = group_rollup(&group, &duplicated).expect_err("R26: duplicate site rejected");
    assert!(matches!(err, DomainError::Storage(_)));

    // Missing site (in group but not in values) -> Err NAMING the site.
    let missing = [("INST-CN-01", 1.0)];
    let err = group_rollup(&group, &missing).expect_err("R26: missing site rejected");
    match err {
        DomainError::Storage(msg) => assert!(
            msg.contains("INST-CN-02"),
            "R26: missing-site error must name the site, got: {msg}"
        ),
        other => panic!("expected DomainError::Storage, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// R46 — ICR joint-and-several liability tagging (Art 5(2) & 26)
// ---------------------------------------------------------------------------

/// R46 (Art 5(2) & 26): an ICR filing for an importer that LACKS
/// authorised-declarant status carries joint and several liability; a
/// represented importer holding the status carries none. The dashboard
/// aggregate sums ONLY the joint-and-several mass.
#[test]
fn joint_and_several_tagging_and_aggregate() {
    assert_eq!(
        liability_tag(false),
        LiabilityTag::JointAndSeveral,
        "R46: no authorised status -> joint and several liability"
    );
    assert_eq!(
        liability_tag(true),
        LiabilityTag::None,
        "R46: authorised status -> no liability tag"
    );

    let masses = [
        (LiabilityTag::None, 100.0),
        (LiabilityTag::JointAndSeveral, 250.0),
        (LiabilityTag::JointAndSeveral, 50.0),
    ];
    let aggregate = aggregate_liability_mass_kg(&masses);
    assert!(
        close(aggregate, 300.0),
        "R46: aggregate counts joint-and-several mass only, got {aggregate}"
    );
    assert!(
        close(aggregate_liability_mass_kg(&[]), 0.0),
        "empty ledger aggregates to zero"
    );
    assert!(
        close(
            aggregate_liability_mass_kg(&[(LiabilityTag::None, 500.0)]),
            0.0
        ),
        "R46: untagged mass never enters the exposure aggregate"
    );
}
