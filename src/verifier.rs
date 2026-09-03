// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Verifier & NCA surface (R28/R33/R38): the findings workflow with
//! correction buffers, the accreditation gate over an offline verifier
//! register, site-visit modalities, and attestation sign-off.

use ed25519_dalek::{Signer, Verifier};
use serde::{Deserialize, Serialize};

use crate::calendar::{civil_from_days, days_from_epoch, parse_iso};
use crate::domain::errors::DomainError;
use crate::provenance::AuditChain;

// ---------------------------------------------------------------------------
// R38 — site-visit modality (travels with the dossier)
// ---------------------------------------------------------------------------

/// The verifier's site-visit modality per DR (EU) 2025/2551 Art 14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VisitModality {
    /// On-site visit.
    Physical,
    /// Virtual visit, permitted only with a prior NAB waiver approval.
    VirtualApproved,
    /// Desk assessment, visit waived.
    WaivedDesk,
}

// ---------------------------------------------------------------------------
// R28 — findings workflow
// ---------------------------------------------------------------------------

/// Lifecycle state of a non-conformity finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingStatus {
    /// Recorded, not yet routed.
    Open,
    /// Correction request routed to the originating mill.
    CorrectionRequested,
    /// Mill resubmitted the evidence.
    Resubmitted,
    /// Verifier accepted the correction.
    Closed,
    /// Verifier rejected the resubmission.
    Rejected,
}

/// A non-conformity finding on a dossier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable finding identifier.
    pub id: String,
    /// Dossier the finding applies to.
    pub dossier_id: String,
    /// Severity class (e.g. `MAJOR`, `MINOR`).
    pub severity: String,
    /// Human description of the non-conformity.
    pub description: String,
    /// Lifecycle state.
    pub status: FindingStatus,
    /// Correction buffer countdown: working/deadline info (default 15 days
    /// ahead of the September 30 deadline while August–September audits run).
    pub correction_buffer_days: u32,
}

/// Open a finding and immediately route its correction request.
///
/// R28: open + route is one atomic step — the returned finding is already in
/// [`FindingStatus::CorrectionRequested`] (never a dangling `Open`), because a
/// finding that exists without a routed correction request is not a state the
/// DR (EU) 2025/2551 workflow recognizes.
///
/// # Errors
///
/// [`DomainError::Storage`] when `correction_buffer_days` is zero — a
/// correction request without a countdown is refused at open.
pub fn open_finding(
    id: &str,
    dossier_id: &str,
    severity: &str,
    description: &str,
    correction_buffer_days: u32,
) -> Result<Finding, DomainError> {
    if correction_buffer_days == 0 {
        return Err(DomainError::Storage(format!(
            "finding `{id}`: correction buffer must be at least 1 day (R28)"
        )));
    }
    Ok(Finding {
        id: id.to_string(),
        dossier_id: dossier_id.to_string(),
        severity: severity.to_string(),
        description: description.to_string(),
        status: FindingStatus::CorrectionRequested,
        correction_buffer_days,
    })
}

/// Advance a finding through its lifecycle.
///
/// R28 lifecycle (open + route is atomic, so `Open` has no outgoing edge the
/// caller can reach except via a fresh [`open_finding`]):
///
/// - `CorrectionRequested` → `Resubmitted`
/// - `Resubmitted` → `Closed` | `Rejected`
/// - `Rejected` → `CorrectionRequested` (re-request)
/// - `Closed` is terminal
///
/// The input finding is never mutated; a clone with the new status is
/// returned.
///
/// # Errors
///
/// [`DomainError::Storage`] on an illegal transition (e.g. closing a
/// finding that was never resubmitted); the message names both states.
pub fn transition_finding(finding: &Finding, to: FindingStatus) -> Result<Finding, DomainError> {
    let from = finding.status;
    let legal = matches!(
        (from, to),
        (
            FindingStatus::CorrectionRequested,
            FindingStatus::Resubmitted
        ) | (FindingStatus::Resubmitted, FindingStatus::Closed)
            | (FindingStatus::Resubmitted, FindingStatus::Rejected)
            | (FindingStatus::Rejected, FindingStatus::CorrectionRequested)
    );
    if !legal {
        return Err(DomainError::Storage(format!(
            "illegal finding transition {from:?} -> {to:?} on `{}` (R28 lifecycle)",
            finding.id
        )));
    }
    let mut next = finding.clone();
    next.status = to;
    Ok(next)
}

/// The correction-buffer deadline: `requested_iso` + buffer days, ISO date.
///
/// R28: the 15-day buffer is the product default for the verifier correction
/// countdown ahead of the September 30 declaration deadline; it is NOT the
/// Art 22(2a) certificate-holding rule (which has no 15-day buffer, R24).
///
/// # Errors
///
/// [`DomainError::InvalidImportDate`] when `requested_iso` does not parse.
pub fn correction_deadline(requested_iso: &str, buffer_days: u32) -> Result<String, DomainError> {
    let (y, m, d) = parse_iso(requested_iso)?;
    let (dy, dm, dd) = civil_from_days(days_from_epoch(y, m, d) + i64::from(buffer_days));
    Ok(format!("{dy:04}-{dm:02}-{dd:02}"))
}

// ---------------------------------------------------------------------------
// R33 — accreditation gate + offline register
// ---------------------------------------------------------------------------

/// CBAM activity group (DR (EU) 2025/2551): I–VIII plus L/LI/LII.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActivityGroup {
    /// Group I.
    I,
    /// Group II.
    Ii,
    /// Group III.
    Iii,
    /// Group IV.
    Iv,
    /// Group V.
    V,
    /// Group VI.
    Vi,
    /// Group VII.
    Vii,
    /// Group VIII.
    Viii,
    /// Group L.
    L,
    /// Group LI.
    Li,
    /// Group LII.
    Lii,
}

/// One accredited verifier in the offline, versioned register shipped in the
/// artifact (R33).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierRecord {
    /// Verifier identifier.
    pub verifier_id: String,
    /// Legal name.
    pub name: String,
    /// National Accreditation Body identifier.
    pub nab_id: String,
    /// NAB country, ISO-3166 alpha-2.
    pub nab_country: String,
    /// Activity groups the NAB accredited this verifier for.
    pub activity_groups: Vec<ActivityGroup>,
    /// Accreditation valid until, ISO `YYYY-MM-DD`.
    pub accredited_until: String,
}

/// An attestation offered for acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    /// The verifier who signed.
    pub verifier_id: String,
    /// Hash of the dossier being attested (hex).
    pub dossier_hash: String,
    /// Signing timestamp, ISO-8601 UTC.
    pub signed_at_utc: String,
    /// Visit modality evidenced for the audit.
    pub visit_modality: VisitModality,
    /// Detached signature over `dossier_hash|signed_at_utc|verifier_id`.
    pub signature_hex: String,
}

/// Accept an attestation only when (a) the verifier is registered, (b) the
/// accreditation has not expired on the attestation date, and (c) the
/// accreditation scope covers ALL the dossier's activity groups. Non-EU
/// bodies qualify only via NABs accepting third-country applicants — the
/// register is the data source; mismatched or unverified NAB IDs reject.
///
/// NAB-ID handling (R33): the `register` IS the offline, versioned register
/// shipped in the artifact. Verifier and NAB identifiers enter it only after
/// verification against the NAB / CBAM Registry data at artifact-build time,
/// so "found in this register" already implies a verified NAB ID — no
/// separate NAB check happens here, and any mismatched or unverified ID
/// never reaches this gate.
///
/// Expiry is strict: `accredited_until < today` rejects; the last accredited
/// day itself still passes.
///
/// # Errors
///
/// [`DomainError::AccreditationMismatch`] with the rejection reason
/// (`verifier not registered`, `accreditation expired`, `scope missing group
/// …`); [`DomainError::InvalidImportDate`] when `today_iso` or the record's
/// `accredited_until` does not parse.
pub fn accreditation_gate<'a>(
    attestation: &Attestation,
    dossier_activity_groups: &[ActivityGroup],
    register: &'a [VerifierRecord],
    today_iso: &str,
) -> Result<&'a VerifierRecord, DomainError> {
    // (a) registration: the offline register is the only acceptance source.
    let record = register
        .iter()
        .find(|r| r.verifier_id == attestation.verifier_id)
        .ok_or_else(|| {
            DomainError::AccreditationMismatch(format!(
                "verifier not registered: {}",
                attestation.verifier_id
            ))
        })?;

    // (b) accreditation current on the attestation date (strictly before
    // rejects; the last accredited day passes).
    let (ty, tm, td) = parse_iso(today_iso)?;
    let (uy, um, ud) = parse_iso(&record.accredited_until)?;
    if days_from_epoch(uy, um, ud) < days_from_epoch(ty, tm, td) {
        return Err(DomainError::AccreditationMismatch(format!(
            "accreditation expired {}: {} lapsed {}",
            attestation.verifier_id, record.verifier_id, record.accredited_until
        )));
    }

    // (c) scope covers EVERY dossier activity group.
    for group in dossier_activity_groups {
        if !record.activity_groups.contains(group) {
            return Err(DomainError::AccreditationMismatch(format!(
                "scope missing group {group:?}: verifier {} not accredited for it",
                record.verifier_id
            )));
        }
    }

    Ok(record)
}

/// The detached signature message: `dossier_hash|signed_at_utc|verifier_id`.
fn attestation_message(attestation: &Attestation) -> String {
    format!(
        "{}|{}|{}",
        attestation.dossier_hash, attestation.signed_at_utc, attestation.verifier_id
    )
}

/// Sign an attestation with an Ed25519 signing key (digital sign-off
/// attached to the dossier).
///
/// R28: the accredited verifier's sign-off is a detached Ed25519 signature
/// over `dossier_hash|signed_at_utc|verifier_id`, hex-encoded into
/// `signature_hex` so it travels inside the attestation and verifies offline.
///
/// # Errors
///
/// [`DomainError::CryptoError`] on key/signature failure (cannot occur with
/// a valid [`ed25519_dalek::SigningKey`]; kept for signature stability).
pub fn sign_attestation(
    attestation: &Attestation,
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<Attestation, DomainError> {
    let message = attestation_message(attestation);
    let signature = signing_key.sign(message.as_bytes());
    let mut signed = attestation.clone();
    signed.signature_hex = hex::encode(signature.to_bytes());
    Ok(signed)
}

/// Verify an attestation's detached signature against the verifier's public
/// key, and (optionally, when provided) the dossier's audit-chain root.
///
/// R28 + R10: offline verification — first the detached Ed25519 signature
/// over `dossier_hash|signed_at_utc|verifier_id`, then (when a chain is
/// supplied) the binding of `dossier_hash` to the chain root, proving the
/// attested dossier is the untampered, hash-linked one.
///
/// # Errors
///
/// [`DomainError::CryptoError`] on non-hex / malformed / bad signature;
/// [`DomainError::ChainBroken`] (position `0` — the root, not a specific
/// event) when the chain root does not match `dossier_hash`.
pub fn verify_attestation(
    attestation: &Attestation,
    verifying_key: &ed25519_dalek::VerifyingKey,
    chain: Option<&AuditChain>,
) -> Result<(), DomainError> {
    let message = attestation_message(attestation);
    let sig_bytes = hex::decode(&attestation.signature_hex)
        .map_err(|e| DomainError::CryptoError(format!("attestation signature is not hex: {e}")))?;
    let sig_array: [u8; ed25519_dalek::SIGNATURE_LENGTH] =
        sig_bytes.try_into().map_err(|malformed: Vec<u8>| {
            DomainError::CryptoError(format!(
                "attestation signature must be {} bytes, got {}",
                ed25519_dalek::SIGNATURE_LENGTH,
                malformed.len()
            ))
        })?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|e| DomainError::CryptoError(format!("attestation signature invalid: {e}")))?;

    if let Some(chain) = chain {
        if attestation.dossier_hash != chain.chain_root() {
            return Err(DomainError::ChainBroken(0));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests — complement the contract tests in tests/verifier.rs
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A finding pre-set to `status` (bypassing `open_finding`, which routes
    /// atomically to `CorrectionRequested`).
    fn finding_in(status: FindingStatus) -> Finding {
        Finding {
            id: "F-T".to_string(),
            dossier_id: "D-T".to_string(),
            severity: "MAJOR".to_string(),
            description: "unit fixture".to_string(),
            status,
            correction_buffer_days: 15,
        }
    }

    /// The full R28 transition matrix: only the four edges named in the
    /// lifecycle are legal; every other (from, to) pair is a Storage error
    /// naming both states.
    #[test]
    fn transition_matrix_is_exact() {
        let all = [
            FindingStatus::Open,
            FindingStatus::CorrectionRequested,
            FindingStatus::Resubmitted,
            FindingStatus::Closed,
            FindingStatus::Rejected,
        ];
        for from in all {
            for to in all {
                let result = transition_finding(&finding_in(from), to);
                let legal = matches!(
                    (from, to),
                    (
                        FindingStatus::CorrectionRequested,
                        FindingStatus::Resubmitted
                    ) | (FindingStatus::Resubmitted, FindingStatus::Closed)
                        | (FindingStatus::Resubmitted, FindingStatus::Rejected)
                        | (FindingStatus::Rejected, FindingStatus::CorrectionRequested)
                );
                if legal {
                    assert_eq!(result.expect("legal edge").status, to);
                } else {
                    let err = result.expect_err("illegal edge must be refused");
                    match err {
                        DomainError::Storage(msg) => {
                            assert!(
                                msg.contains(&format!("{from:?}"))
                                    && msg.contains(&format!("{to:?}")),
                                "error must name both states: {msg}"
                            );
                        }
                        other => panic!("expected Storage error, got {other:?}"),
                    }
                }
            }
        }
    }

    /// R28: the countdown arithmetic crosses leap days and year ends.
    #[test]
    fn deadline_crosses_leap_day_and_year_end() {
        // 2028 is a leap year: 02-25 + 15 = 03-11.
        assert_eq!(correction_deadline("2028-02-25", 15).unwrap(), "2028-03-11");
        // Year end: 2027-12-25 + 15 = 2028-01-09.
        assert_eq!(correction_deadline("2027-12-25", 15).unwrap(), "2028-01-09");
    }

    /// R33: an empty dossier activity-group list is vacuously in scope.
    #[test]
    fn gate_accepts_empty_group_list() {
        let register = vec![VerifierRecord {
            verifier_id: "V1".to_string(),
            name: "V".to_string(),
            nab_id: "NAB-1".to_string(),
            nab_country: "NL".to_string(),
            activity_groups: vec![ActivityGroup::I],
            accredited_until: "2027-12-31".to_string(),
        }];
        let attestation = Attestation {
            verifier_id: "V1".to_string(),
            dossier_hash: "hash".to_string(),
            signed_at_utc: "2026-09-02T00:00:00Z".to_string(),
            visit_modality: VisitModality::WaivedDesk,
            signature_hex: String::new(),
        };
        assert!(
            accreditation_gate(&attestation, &[], &register, "2026-09-02").is_ok(),
            "no groups to cover -> no scope violation"
        );
    }

    /// R28: Ed25519 is deterministic — signing the same attestation twice
    /// yields the identical detached signature.
    #[test]
    fn signing_is_deterministic() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let attestation = Attestation {
            verifier_id: "V1".to_string(),
            dossier_hash: "hash".to_string(),
            signed_at_utc: "2026-09-02T00:00:00Z".to_string(),
            visit_modality: VisitModality::Physical,
            signature_hex: String::new(),
        };
        let first = sign_attestation(&attestation, &key).unwrap();
        let second = sign_attestation(&attestation, &key).unwrap();
        assert_eq!(first.signature_hex, second.signature_hex);
        assert!(verify_attestation(&first, &key.verifying_key(), None).is_ok());
    }
}
