// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Contract tests for `kaimeter::provenance`:
//! attachments (R16), retention/purge (R27, Art 14 CBAM Reg), and the
//! exportable hash-chained audit trail (R10).

use kaimeter_core::domain::DomainError;
use kaimeter_core::provenance::{
    new_attachment, purge_decision, retention_expiry, sha256_hex, AuditChain, PurgeDecision,
    RetentionStatus, GENESIS_PREV_HASH,
};

fn payload(n: u64) -> String {
    sha256_hex(n.to_string().as_bytes())
}

/// R16: fields prefill, but the end-user must verify each entry against the
/// attached source before anything is saved — the verifying human is the
/// author of the record. Without that human verification the attachment is
/// rejected; nothing is retained as provenance.
#[test]
fn attachment_requires_human_verification() {
    let bytes = b"utility-bill-july";

    // Unverified → refused, naming the attachment id.
    let refused = new_attachment(
        "att-1",
        "bill.pdf",
        "application/pdf",
        bytes,
        false,
        "no human check yet",
    );
    assert!(
        matches!(&refused, Err(DomainError::HumanVerificationRequired(id)) if id == "att-1"),
        "R16: no save without human verification, got {refused:?}"
    );

    // Verified → accepted, all fields round-trip, hash is over the raw bytes.
    let att = new_attachment(
        "att-2",
        "bill.pdf",
        "application/pdf",
        bytes,
        true,
        "totals checked against the bill by the declarant",
    )
    .expect("verified attachment is accepted");
    assert_eq!(att.id, "att-2");
    assert_eq!(att.filename, "bill.pdf");
    assert_eq!(att.mime_type, "application/pdf");
    assert_eq!(att.sha256, sha256_hex(bytes));
    assert!(att.verified_by_human);
    assert_eq!(
        att.verification_note,
        "totals checked against the bill by the declarant"
    );
}

/// The attachment hash is a real SHA-256 over the file bytes — pinned to the
/// standard NIST vector for `b"abc"` (R16: only the hash is stored).
#[test]
fn attachment_hash_matches_known_sha256() {
    let att = new_attachment(
        "att-abc",
        "a.txt",
        "text/plain",
        b"abc",
        true,
        "pinned vector",
    )
    .expect("verified");
    assert_eq!(
        att.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// R27 / Art 14 CBAM Reg: records of the information used to calculate
/// embedded emissions are kept for 4 calendar years AFTER the declaration
/// year — the horizon is December 31 of `declaration_year + 4`.
#[test]
fn retention_horizon_is_four_years_after_declaration_year() {
    let y2027 = retention_expiry(2027);
    assert_eq!(
        y2027,
        RetentionStatus {
            declaration_year: 2027,
            purge_after_iso: "2031-12-31".to_string()
        }
    );
    let y2026 = retention_expiry(2026);
    assert_eq!(y2026.declaration_year, 2026);
    assert_eq!(y2026.purge_after_iso, "2030-12-31");
}

/// R27: the record is retained THROUGH December 31 of `declaration_year + 4`;
/// purging starts only strictly after that date. Bad dates are rejected.
#[test]
fn purge_boundary_is_strict() {
    let status = retention_expiry(2027);

    // On the boundary date itself: still retained.
    assert_eq!(
        purge_decision(&status, "2031-12-31").expect("valid date"),
        PurgeDecision::Retain
    );
    // One day past the boundary: purge.
    assert_eq!(
        purge_decision(&status, "2032-01-01").expect("valid date"),
        PurgeDecision::Purge
    );
    // Well inside the window: retained.
    assert_eq!(
        purge_decision(&status, "2030-06-01").expect("valid date"),
        PurgeDecision::Retain
    );
    // Unparseable date → error, never a silent purge decision.
    assert!(matches!(
        purge_decision(&status, "31/12/2031"),
        Err(DomainError::InvalidImportDate(s)) if s == "31/12/2031"
    ));
}

/// R10: the audit trail is exportable — a chain survives a serde_json
/// round-trip and still verifies with an unchanged chain root, so the
/// verifier can check a shipped dossier offline.
#[test]
fn chain_survives_serialization() {
    let mut chain = AuditChain::new();
    chain.append(
        "2026-09-01T10:00:00Z",
        "mill:user",
        "extraction_confirmed",
        "cons-1",
        &payload(1),
    );
    chain.append(
        "2026-09-01T10:05:00Z",
        "mill:user",
        "override",
        "cons-1",
        &payload(2),
    );
    chain.append(
        "2026-09-01T10:07:00Z",
        "core",
        "default_fallback",
        "cons-2",
        &payload(3),
    );

    let root_before = chain.chain_root().to_string();
    assert_ne!(root_before, GENESIS_PREV_HASH);

    // Export as JSON, wipe the original, re-import.
    let exported = serde_json::to_string(&chain).expect("chain serializes");
    let imported: AuditChain = serde_json::from_str(&exported).expect("chain deserializes");

    assert_eq!(imported.events().len(), 3);
    assert!(
        imported.verify().is_ok(),
        "round-tripped chain still verifies"
    );
    assert_eq!(imported.chain_root(), root_before, "chain root unchanged");
    assert_eq!(imported, chain);
}
