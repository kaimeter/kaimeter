// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Integration tests for the `verifier` module (R28/R33/R38).
//!
//! Written FIRST (RED) per the TDD contract. These pin:
//! - the findings lifecycle with correction-request routing and the 15-day
//!   correction buffer (R28 — a product default; NOT the Art 22(2a) holding
//!   rule, which has no 15-day buffer), under DR (EU) 2025/2551 verification by accredited bodies;
//! - the accreditation gate over the offline verifier register: registration,
//!   unexpired accreditation, and scope covering every dossier activity group
//!   (R33 — DR (EU) 2025/2551, NAB accreditation per ISO/IEC 17029);
//! - the Ed25519 attestation sign-off and its offline verification, bound to
//!   the dossier's audit-chain root (R10 + R28).

use ed25519_dalek::{SigningKey, VerifyingKey};
use kaimeter_core::domain::errors::DomainError;
use kaimeter_core::provenance::AuditChain;
use kaimeter_core::verifier::{
    accreditation_gate, correction_deadline, open_finding, sign_attestation, transition_finding,
    verify_attestation, ActivityGroup, Attestation, Finding, FindingStatus, VerifierRecord,
    VisitModality,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The offline register (R33): one verifier, V1, NAB-accredited for groups I
/// and IV until 2027-12-31. In production this register is the versioned
/// table shipped in the artifact; NAB IDs enter it only after verification.
fn register() -> Vec<VerifierRecord> {
    vec![VerifierRecord {
        verifier_id: "V1".to_string(),
        name: "Keldrion Accreditation BV".to_string(),
        nab_id: "NL-NAB-17029".to_string(),
        nab_country: "NL".to_string(),
        activity_groups: vec![ActivityGroup::I, ActivityGroup::Iv],
        accredited_until: "2027-12-31".to_string(),
    }]
}

/// The same register, but V1's accreditation lapsed the day before the test's
/// "today" (2026-09-02).
fn register_expired() -> Vec<VerifierRecord> {
    let mut register = register();
    register[0].accredited_until = "2026-08-31".to_string();
    register
}

/// An unsigned attestation by V1 over `dossier_hash`.
fn unsigned_attestation(dossier_hash: &str) -> Attestation {
    Attestation {
        verifier_id: "V1".to_string(),
        dossier_hash: dossier_hash.to_string(),
        signed_at_utc: "2026-09-02T10:00:00Z".to_string(),
        visit_modality: VisitModality::Physical,
        signature_hex: String::new(),
    }
}

// ---------------------------------------------------------------------------
// R28 — findings lifecycle
// ---------------------------------------------------------------------------

/// R28 / DR (EU) 2025/2551: opening a finding routes the correction request to
/// the originating mill atomically — the stored status is CorrectionRequested
/// (never a dangling Open) — then resubmission closes the loop.
#[test]
fn finding_lifecycle_happy_path() {
    let finding = open_finding("F-1", "D-1", "MAJOR", "emissions data unsupported", 15)
        .expect("open + route must succeed with a positive buffer");
    assert_eq!(finding.id, "F-1");
    assert_eq!(finding.dossier_id, "D-1");
    assert_eq!(finding.severity, "MAJOR");
    assert_eq!(finding.correction_buffer_days, 15);
    assert_eq!(finding.status, FindingStatus::CorrectionRequested);

    let resubmitted = transition_finding(&finding, FindingStatus::Resubmitted)
        .expect("CorrectionRequested -> Resubmitted is legal");
    assert_eq!(resubmitted.status, FindingStatus::Resubmitted);

    let closed = transition_finding(&resubmitted, FindingStatus::Closed)
        .expect("Resubmitted -> Closed is legal");
    assert_eq!(closed.status, FindingStatus::Closed);
    // The input finding is never mutated (clone-with-new-status contract).
    assert_eq!(finding.status, FindingStatus::CorrectionRequested);
}

/// R28 lifecycle: Resubmitted -> Rejected -> CorrectionRequested (re-request)
/// is legal; skipping straight from Open to Closed is not; Closed is
/// terminal; a zero-day buffer is refused at open.
#[test]
fn finding_rejects_and_reissues() {
    let finding = open_finding("F-2", "D-1", "MINOR", "missing meter calibration", 15)
        .expect("open must succeed");

    // Illegal: an `Open` finding can never skip straight to Closed. (`Open`
    // has no outgoing edge — `open_finding` routes atomically to
    // CorrectionRequested, so a lingering `Open` is constructed here only to
    // pin the rejected transition.)
    let orphan = Finding {
        id: "F-0".to_string(),
        dossier_id: "D-1".to_string(),
        severity: "MINOR".to_string(),
        description: "stale open finding".to_string(),
        status: FindingStatus::Open,
        correction_buffer_days: 15,
    };
    assert!(matches!(
        transition_finding(&orphan, FindingStatus::Closed),
        Err(DomainError::Storage(msg))
            if msg.contains("Open") && msg.contains("Closed")
    ));
    // Likewise the routed finding cannot jump to Closed without a resubmission.
    assert!(
        transition_finding(&finding, FindingStatus::Closed).is_err(),
        "CorrectionRequested -> Closed must be illegal"
    );

    let resubmitted = transition_finding(&finding, FindingStatus::Resubmitted).expect("resubmit");
    let rejected =
        transition_finding(&resubmitted, FindingStatus::Rejected).expect("reject is legal");
    let re_requested = transition_finding(&rejected, FindingStatus::CorrectionRequested)
        .expect("Rejected -> CorrectionRequested (re-request) is legal");
    assert_eq!(re_requested.status, FindingStatus::CorrectionRequested);

    // Closed is terminal: no transition out of it.
    let closed = transition_finding(&re_requested, FindingStatus::Resubmitted).expect("resubmit");
    let closed = transition_finding(&closed, FindingStatus::Closed).expect("close");
    assert!(transition_finding(&closed, FindingStatus::Resubmitted).is_err());

    // R28: a correction request without a countdown is refused at open.
    assert!(matches!(
        open_finding("F-3", "D-1", "MAJOR", "zero buffer", 0),
        Err(DomainError::Storage(_))
    ));
}

/// R28: the correction-buffer deadline is `requested + buffer days` as an ISO
/// date. The 15-day default is a PRODUCT decision for the verifier correction
/// countdown while August–September audits run ahead of the September 30
/// deadline — it is NOT the Art 22(2a) certificate-holding rule, which has no
/// 15-day buffer (R24).
#[test]
fn correction_buffer_is_15_days_by_default() {
    // 2027-08-16 + 15 days -> 2027-08-31.
    assert_eq!(
        correction_deadline("2027-08-16", 15).expect("deadline"),
        "2027-08-31"
    );
    // The buffer crosses the month end: 2027-08-25 + 15 -> 2027-09-09.
    assert_eq!(
        correction_deadline("2027-08-25", 15).expect("deadline"),
        "2027-09-09"
    );
    // A malformed requested date is rejected as an invalid import date.
    assert!(matches!(
        correction_deadline("2027-13-40", 15),
        Err(DomainError::InvalidImportDate(bad)) if bad == "2027-13-40"
    ));
}

// ---------------------------------------------------------------------------
// R33 — accreditation gate
// ---------------------------------------------------------------------------

/// R33 (DR (EU) 2025/2551): attestations are accepted only from verifiers in
/// the offline register whose accreditation is current on the attestation date
/// and whose accreditation scope covers EVERY dossier activity group. The
/// register shipped in the artifact is the NAB-ID source of truth — mismatched
/// or unverified NAB IDs never enter it, so membership is the gate.
#[test]
fn accreditation_gate_scopes_and_expiry() {
    let register = register();
    let attestation = unsigned_attestation("deadbeef");

    // In scope, accreditation current on 2026-09-02 -> accepted, and the
    // matched register record is returned.
    let record = accreditation_gate(&attestation, &[ActivityGroup::I], &register, "2026-09-02")
        .expect("in-scope, current accreditation must pass");
    assert_eq!(record.verifier_id, "V1");

    // Scope gap: the dossier touches group LII, outside V1's accreditation.
    assert!(matches!(
        accreditation_gate(
            &attestation,
            &[ActivityGroup::I, ActivityGroup::Lii],
            &register,
            "2026-09-02"
        ),
        Err(DomainError::AccreditationMismatch(msg)) if msg.contains("scope missing group")
    ));

    // Expiry: accredited_until 2026-08-31 is strictly before 2026-09-02.
    let expired = register_expired();
    assert!(matches!(
        accreditation_gate(&attestation, &[ActivityGroup::I], &expired, "2026-09-02"),
        Err(DomainError::AccreditationMismatch(msg)) if msg.contains("accreditation expired")
    ));

    // Unknown verifier: not in the register -> rejected.
    let mut unknown = attestation.clone();
    unknown.verifier_id = "VX".to_string();
    assert!(matches!(
        accreditation_gate(&unknown, &[ActivityGroup::I], &register, "2026-09-02"),
        Err(DomainError::AccreditationMismatch(msg)) if msg.contains("verifier not registered")
    ));

    // The expiry boundary itself is inclusive: on 2026-08-31 the lapsed
    // verifier would still pass (strictly-before rule).
    assert!(accreditation_gate(&attestation, &[ActivityGroup::I], &expired, "2026-08-31").is_ok());
}

// ---------------------------------------------------------------------------
// R28 — attestation sign-off + offline verification (bound to R10 chain root)
// ---------------------------------------------------------------------------

/// R28 digital sign-off: the attestation carries a detached Ed25519 signature
/// over `dossier_hash|signed_at_utc|verifier_id`, verifiable offline with the
/// verifier's public key.
#[test]
fn attestation_signs_and_verifies_offline() {
    let signing_key = SigningKey::generate(&mut rand_core::OsRng);
    let attestation = unsigned_attestation("abc123");

    let signed = sign_attestation(&attestation, &signing_key).expect("signing must succeed");
    assert!(!signed.signature_hex.is_empty());
    assert_eq!(signed.dossier_hash, attestation.dossier_hash);

    let verifying_key: VerifyingKey = signing_key.verifying_key();
    verify_attestation(&signed, &verifying_key, None).expect("matching key must verify");

    // Tampering with the dossier hash after signing breaks the signature.
    let mut tampered = signed.clone();
    tampered.dossier_hash = "ffffffff".to_string();
    assert!(verify_attestation(&tampered, &verifying_key, None).is_err());

    // A different verifier's key fails verification.
    let other_key = SigningKey::generate(&mut rand_core::OsRng);
    assert!(matches!(
        verify_attestation(&signed, &other_key.verifying_key(), None),
        Err(DomainError::CryptoError(_))
    ));

    // A signature that is not even hex is a crypto error, not a panic.
    let mut corrupt = signed.clone();
    corrupt.signature_hex = "zz-nothex".to_string();
    assert!(matches!(
        verify_attestation(&corrupt, &verifying_key, None),
        Err(DomainError::CryptoError(_))
    ));
}

/// R10 + R28: the attestation binds to the dossier's hash-linked audit-trail
/// root, so offline verification proves the attested dossier is the untampered
/// one — any post-hoc edit breaks the chain and voids the attestation.
#[test]
fn attestation_binds_to_chain_root() {
    let mut chain = AuditChain::new();
    chain.append(
        "2026-09-01T08:00:00Z",
        "mill",
        "EXTRACT_CONFIRM",
        "electricity-kwh",
        "aaaa",
    );
    chain.append(
        "2026-09-01T09:00:00Z",
        "exporter",
        "DEFAULT_FALLBACK",
        "heat-input",
        "bbbb",
    );
    let root = chain.chain_root().to_string();

    let signing_key = SigningKey::generate(&mut rand_core::OsRng);
    let attestation = unsigned_attestation(&root);
    let signed = sign_attestation(&attestation, &signing_key).expect("sign");
    verify_attestation(&signed, &signing_key.verifying_key(), Some(&chain))
        .expect("attestation over the true chain root must verify");

    // A different dossier hash — honestly signed over THAT hash — fails the
    // chain binding: the signature is valid, the dossier is not the one the
    // chain proves.
    let forged_hash = "cafebabe".to_string();
    let forged = unsigned_attestation(&forged_hash);
    let forged_signed = sign_attestation(&forged, &signing_key).expect("sign");
    assert!(matches!(
        verify_attestation(&forged_signed, &signing_key.verifying_key(), Some(&chain)),
        Err(DomainError::ChainBroken(0))
    ));
}
