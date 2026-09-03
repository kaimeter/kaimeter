// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Exports & packs (R9/R21/R30): declaration-ready files, one-click
//! compliant export with per-field masking and a self-audit preview, sealed
//! data packs (Merkle root, W3C Verifiable Presentation JSON-LD and
//! VC-JWT), fully offline verification, and the pre-flight schema validator.
//!
//! # Determinism note
//!
//! `serde_json::Value` objects are B-tree backed (the `preserve_order`
//! feature is not enabled), so JSON key order here is alphabetical, not
//! insertion order. Nothing in this module relies on key order and callers
//! must not either; signatures are always taken over canonical
//! serialization of the typed [`PackContent`] struct (whose field order is
//! fixed by the struct definition), never over hand-built `Value` maps.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::domain::errors::DomainError;

// ---------------------------------------------------------------------------
// R9 — declaration-ready file
// ---------------------------------------------------------------------------

/// One declaration field: a canonical name and its JSON value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclarationField {
    /// Canonical field name (e.g. `cn_code`, `net_mass_kg`).
    pub name: String,
    /// The field value.
    pub value: Value,
}

/// The mandatory annual-declaration field names (R2/R9). The declaration
/// export fails closed when any of these is missing.
pub const REQUIRED_DECLARATION_FIELDS: [&str; 8] = [
    "cn_code",
    "net_mass_kg",
    "country_of_origin",
    "production_country",
    "installation_id",
    "import_date",
    "determination_basis",
    "embedded_emissions_tco2e",
];

/// Build a declaration-ready file (R9): a plain, human-readable JSON object
/// carrying exactly the fields given — no hidden metadata, no telemetry.
///
/// Guarantees:
/// - Fails closed: every name in [`REQUIRED_DECLARATION_FIELDS`] must be
///   present or the export is refused, naming the first missing field
///   (first declaration is due September 30th, 2027 — an incomplete file
///   must never leave the machine).
/// - The output object's key set is EXACTLY the input field names: extra
///   (non-mandatory) fields pass through untouched and NO extra keys are
///   ever added by this function — what the user sees in the preview is
///   byte-for-byte what ships (R21).
/// - JSON key order inside the object is not guaranteed (see the module
///   determinism note); only the key set is contractual.
///
/// # Errors
///
/// [`DomainError::MissingRequiredField`] when any of
/// [`REQUIRED_DECLARATION_FIELDS`] is absent.
pub fn build_declaration(fields: &[DeclarationField]) -> Result<Value, DomainError> {
    for required in REQUIRED_DECLARATION_FIELDS {
        if !fields.iter().any(|field| field.name == required) {
            return Err(DomainError::MissingRequiredField(required.to_string()));
        }
    }
    let mut object = serde_json::Map::new();
    for field in fields {
        object.insert(field.name.clone(), field.value.clone());
    }
    Ok(Value::Object(object))
}

// ---------------------------------------------------------------------------
// R21 — per-field masking + self-audit preview
// ---------------------------------------------------------------------------

/// Per-field masking policy for trader packs (R21): the buyer sees
/// compliance data, never the trading book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FieldMask {
    /// Field ships with its value.
    Keep,
    /// Field is omitted entirely.
    Redact,
    /// Field ships with a non-identifying placeholder.
    Anonymize,
}

/// The single non-identifying placeholder substituted for every
/// `Anonymize`d value: compliance data stays, identity never.
pub const ANONYMIZED_PLACEHOLDER: &str = "ANONYMIZED";

/// The self-audit preview shown before anything leaves the user's control:
/// exactly which fields will be included — and which will not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PreviewReport {
    /// Fields that will leave, in export order.
    pub included: Vec<String>,
    /// Fields that will NOT leave (masked or absent).
    pub excluded: Vec<String>,
}

/// Build the self-audit preview from the field/mask plan. The preview lists
/// EXACTLY the fields present (R21: the provider audits the export
/// themselves before anything leaves their control): `Keep`ed fields are
/// listed as included; `Redact`ed and `Anonymize`d fields are listed as
/// excluded, in export order.
#[must_use]
pub fn preview(fields: &[(String, FieldMask)]) -> PreviewReport {
    let mut report = PreviewReport::default();
    for (name, mask) in fields {
        match mask {
            FieldMask::Keep => report.included.push(name.clone()),
            FieldMask::Redact | FieldMask::Anonymize => report.excluded.push(name.clone()),
        }
    }
    report
}

/// Apply a masking plan to fields, dropping `Redact`ed entries and
/// replacing `Anonymize`d values with the single non-identifying
/// [`ANONYMIZED_PLACEHOLDER`] — compliance data stays, identity never.
/// `Keep`ed fields and fields with no mask entry pass through unchanged;
/// the original field order is preserved.
#[must_use]
pub fn apply_masks(
    fields: &[DeclarationField],
    masks: &[(String, FieldMask)],
) -> Vec<DeclarationField> {
    fields
        .iter()
        .filter_map(|field| {
            let mask = masks
                .iter()
                .find(|(name, _)| name == &field.name)
                .map(|(_, mask)| *mask);
            match mask {
                Some(FieldMask::Redact) => None,
                Some(FieldMask::Anonymize) => Some(DeclarationField {
                    name: field.name.clone(),
                    value: Value::String(ANONYMIZED_PLACEHOLDER.to_string()),
                }),
                Some(FieldMask::Keep) | None => Some(field.clone()),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Sealed pack — Merkle commitment (R21)
// ---------------------------------------------------------------------------

/// Leaf hash convention (documented, pinned by tests): the input string is
/// hashed as raw bytes with a `0x00` domain prefix —
/// `SHA-256(0x00 || leaf.as_bytes())`.
fn leaf_hash(leaf: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([0x00]);
    hasher.update(leaf.as_bytes());
    hasher.finalize().into()
}

/// Internal node hash convention: `SHA-256(0x01 || left || right)` over the
/// 32 RAW bytes of each child hash (RFC 6962-style, Bitcoin-style
/// duplication for odd tails).
fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([0x01]);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// Fold one level of the tree: pair nodes left-to-right; an odd tail node
/// is promoted by duplication (paired with itself). `level` is never empty.
fn pair_up(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    level
        .chunks(2)
        .map(|pair| {
            let left = &pair[0]; // chunks(2) always yields a non-empty pair
            let right = pair.get(1).unwrap_or(left); // promotion duplicates the tail
            node_hash(left, right)
        })
        .collect()
}

/// Compute the Merkle root over leaf hashes (SHA-256, RFC 6962-style:
/// leaf = `SHA-256(0x00 || leaf.as_bytes())`, node =
/// `SHA-256(0x01 || left_raw || right_raw)`, an odd tail node promoted by
/// duplication at each level). Empty input has the SHA-256 of the empty
/// string as root (`e3b0c442…b855`, pinned by tests).
#[must_use]
pub fn merkle_root(leaves: &[String]) -> String {
    if leaves.is_empty() {
        return hex::encode(Sha256::digest(b""));
    }
    let mut level: Vec<[u8; 32]> = leaves.iter().map(|leaf| leaf_hash(leaf)).collect();
    while level.len() > 1 {
        level = pair_up(&level);
    }
    hex::encode(level[0])
}

/// An inclusion proof for one leaf: sibling hashes from leaf to root with
/// their side (true = sibling is on the right).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleProof {
    /// Index of the proven leaf.
    pub leaf_index: usize,
    /// (sibling hash, sibling_is_right) pairs, leaf level first.
    pub siblings: Vec<(String, bool)>,
    /// The root the proof commits to.
    pub root: String,
}

/// Produce the inclusion proof for `leaf_index` under the same convention
/// as [`merkle_root`] (promoted tail nodes are their own right sibling).
///
/// # Errors
///
/// [`DomainError::Storage`] when the index is out of range.
pub fn merkle_proof(leaves: &[String], leaf_index: usize) -> Result<MerkleProof, DomainError> {
    if leaf_index >= leaves.len() {
        return Err(DomainError::Storage(format!(
            "merkle proof index {leaf_index} out of range for {} leaves",
            leaves.len()
        )));
    }
    let mut level: Vec<[u8; 32]> = leaves.iter().map(|leaf| leaf_hash(leaf)).collect();
    let mut index = leaf_index;
    let mut siblings = Vec::new();
    while level.len() > 1 {
        let sibling_is_right = index % 2 == 0;
        let sibling_index = if sibling_is_right {
            index + 1
        } else {
            index - 1
        };
        // A promoted (odd-tail) node is its own right sibling.
        let sibling = level.get(sibling_index).unwrap_or(&level[index]);
        siblings.push((hex::encode(sibling), sibling_is_right));
        level = pair_up(&level);
        index /= 2;
    }
    Ok(MerkleProof {
        leaf_index,
        siblings,
        root: hex::encode(level[0]),
    })
}

/// Verify an inclusion proof against a leaf and its claimed root: recompute
/// upward from the leaf hash, folding each sibling per its side flag, and
/// compare against `proof.root`. Entirely offline (R21/R22).
#[must_use]
pub fn verify_inclusion(leaf: &str, proof: &MerkleProof) -> bool {
    let mut current = leaf_hash(leaf);
    for (sibling_hex, sibling_is_right) in &proof.siblings {
        let sibling = match hex::decode(sibling_hex) {
            Ok(bytes) => match <[u8; 32]>::try_from(bytes) {
                Ok(sibling) => sibling,
                Err(_) => return false,
            },
            Err(_) => return false,
        };
        current = if *sibling_is_right {
            node_hash(&current, &sibling)
        } else {
            node_hash(&sibling, &current)
        };
    }
    hex::encode(current) == proof.root
}

// ---------------------------------------------------------------------------
// Sealed pack — content + VP serialization (R21)
// ---------------------------------------------------------------------------

/// The sealed pack's compliance payload: emissions values + an anonymized
/// installation ID — no supplier identity, no pricing (blind pass-through).
/// For CN-side exports the pack reduces to a single emission factor per CN
/// code — no bill-of-materials or margin exposure at any point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackContent {
    /// Anonymized installation identifier.
    pub installation_ref: String,
    /// 8-digit CN code the pack covers.
    pub cn_code: String,
    /// The (single) emission factor for the CN code, tCO2e per tonne.
    pub emission_factor_tco2e_per_t: f64,
    /// Embedded emissions actually shipped with the pack, tCO2e.
    pub embedded_emissions_tco2e: f64,
    /// Merkle root of the production log (verifiers confirm the data
    /// derives from the audited record without seeing the record).
    pub production_log_merkle_root: String,
    /// Pack issue date, ISO `YYYY-MM-DD`.
    pub issued_iso: String,
    /// Validity end — stale packs die (R47 workflow).
    pub valid_until_iso: Option<String>,
}

/// A sealed pack: content plus its Ed25519 signature block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SealedPack {
    /// The compliance payload.
    pub content: PackContent,
    /// The signer's public key, hex.
    pub public_key_hex: String,
    /// Signature over the canonical JSON of `content`.
    pub signature_hex: String,
}

/// The canonical bytes every pack signature commits to: `serde_json::to_vec`
/// of the typed [`PackContent`] struct. Serialization of a struct is
/// deterministic (fixed field order), so both signer and verifier derive
/// byte-identical input offline. Note this is canonicalization over the
/// serialized content struct — NOT full URDNA2015 RDF normalization (the
/// pragmatic Kaimeter JSON-LD profile, documented on [`to_vp_json_ld`]).
fn canonical_content(content: &PackContent) -> Result<Vec<u8>, DomainError> {
    serde_json::to_vec(content)
        .map_err(|e| DomainError::CryptoError(format!("canonical content serialization: {e}")))
}

/// Decode a hex-encoded Ed25519 public key.
fn verifying_key_from_hex(public_key_hex: &str) -> Result<VerifyingKey, DomainError> {
    let bytes = hex::decode(public_key_hex)
        .map_err(|e| DomainError::CryptoError(format!("public key is not hex: {e}")))?;
    let array: [u8; 32] = bytes.try_into().map_err(|malformed: Vec<u8>| {
        DomainError::CryptoError(format!(
            "public key must be 32 bytes, got {}",
            malformed.len()
        ))
    })?;
    VerifyingKey::from_bytes(&array)
        .map_err(|e| DomainError::CryptoError(format!("malformed Ed25519 public key: {e}")))
}

/// Decode a hex-encoded Ed25519 signature.
fn signature_from_hex(signature_hex: &str) -> Result<Signature, DomainError> {
    let bytes = hex::decode(signature_hex)
        .map_err(|e| DomainError::CryptoError(format!("signature is not hex: {e}")))?;
    let array: [u8; ed25519_dalek::SIGNATURE_LENGTH] =
        bytes.try_into().map_err(|malformed: Vec<u8>| {
            DomainError::CryptoError(format!(
                "signature must be {} bytes, got {}",
                ed25519_dalek::SIGNATURE_LENGTH,
                malformed.len()
            ))
        })?;
    Ok(Signature::from_bytes(&array))
}

/// Convenience alias so callers can map seal errors onto the domain error.
pub type SealedPackErr = DomainError;

/// Seal a pack with an Ed25519 key (R21): the signature commits to the
/// canonical JSON of the content (see [`canonical_content`]); the pack
/// embeds the signer's public key so any verifier's own copy can check it
/// entirely offline — no Kaimeter server in the loop (R21/R22).
///
/// # Errors
///
/// [`DomainError::CryptoError`] on canonicalization failure (cannot occur
/// with a valid [`PackContent`]; kept for signature stability).
pub fn seal_pack(
    content: PackContent,
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<SealedPack, SealedPackErr> {
    let bytes = canonical_content(&content)?;
    let signature = signing_key.sign(&bytes);
    Ok(SealedPack {
        content,
        public_key_hex: hex::encode(signing_key.verifying_key().as_bytes()),
        signature_hex: hex::encode(signature.to_bytes()),
    })
}

/// Verify a sealed pack offline: signature over the canonical content JSON.
/// Returns the verified content.
///
/// # Errors
///
/// [`DomainError::CryptoError`] on a bad signature or malformed key.
pub fn verify_sealed_pack(pack: &SealedPack) -> Result<PackContent, DomainError> {
    let verifying_key = verifying_key_from_hex(&pack.public_key_hex)?;
    let bytes = canonical_content(&pack.content)?;
    let signature = signature_from_hex(&pack.signature_hex)?;
    verifying_key
        .verify(&bytes, &signature)
        .map_err(|e| DomainError::CryptoError(format!("sealed pack signature invalid: {e}")))?;
    Ok(pack.content.clone())
}

/// Serialize a sealed pack as a W3C Verifiable Presentation (JSON-LD) with
/// an Ed25519 `DataIntegrityProof` — verifiers validate integrity
/// programmatically, entirely offline.
///
/// Kaimeter pragmatic profile: the proof's `proofValue` is the Ed25519
/// signature over the canonical serialization of the content struct (see
/// [`canonical_content`]) — i.e. canonicalization is over the serialized
/// content struct, NOT full URDNA2015 RDF normalization. The signer is
/// anchored by `verificationMethod: did:key:<public_key_hex>` so the
/// verifier needs nothing but the VP itself.
#[must_use]
pub fn to_vp_json_ld(pack: &SealedPack) -> Value {
    serde_json::json!({
        "@context": ["https://www.w3.org/2018/credentials/v1"],
        "type": ["VerifiablePresentation"],
        "verifiableCredential": {
            "type": ["VerifiableCredential"],
            "credentialSubject": pack.content,
            "proof": {
                "type": "DataIntegrityProof",
                "cryptosuite": "eddsa-rdfc-2022",
                "verificationMethod": format!("did:key:{}", pack.public_key_hex),
                "proofValue": pack.signature_hex,
            }
        }
    })
}

/// Parse + verify a VP JSON-LD offline (proof over the canonical credential
/// payload; the signer's key is recovered from the VP's own
/// `verificationMethod`). Returns the verified pack content.
///
/// # Errors
///
/// [`DomainError::CryptoError`] on a bad proof; [`DomainError::SchemaViolation`]
/// when the JSON-LD shape is wrong.
pub fn verify_vp_json_ld(vp: &Value) -> Result<PackContent, DomainError> {
    let missing =
        |what: &str| DomainError::SchemaViolation(format!("VP JSON-LD is missing `{what}`"));
    vp.get("@context").ok_or_else(|| missing("@context"))?;
    vp.get("type").ok_or_else(|| missing("type"))?;
    let vc = vp
        .get("verifiableCredential")
        .ok_or_else(|| missing("verifiableCredential"))?;
    vc.get("type").ok_or_else(|| missing("credential type"))?;
    let subject = vc
        .get("credentialSubject")
        .ok_or_else(|| missing("credentialSubject"))?;
    let content: PackContent = serde_json::from_value(subject.clone()).map_err(|e| {
        DomainError::SchemaViolation(format!("credentialSubject is not a pack content: {e}"))
    })?;
    let proof = vc.get("proof").ok_or_else(|| missing("proof"))?;
    proof.get("type").ok_or_else(|| missing("proof.type"))?;
    proof
        .get("cryptosuite")
        .ok_or_else(|| missing("proof.cryptosuite"))?;
    let method = proof
        .get("verificationMethod")
        .and_then(Value::as_str)
        .ok_or_else(|| missing("proof.verificationMethod"))?;
    let public_key_hex = method.strip_prefix("did:key:").ok_or_else(|| {
        DomainError::SchemaViolation(
            "proof.verificationMethod must be `did:key:<public_key_hex>`".to_string(),
        )
    })?;
    let verifying_key = verifying_key_from_hex(public_key_hex)?;
    let proof_value = proof
        .get("proofValue")
        .and_then(Value::as_str)
        .ok_or_else(|| missing("proof.proofValue"))?;
    let signature = signature_from_hex(proof_value)?;
    let bytes = canonical_content(&content)?;
    verifying_key
        .verify(&bytes, &signature)
        .map_err(|e| DomainError::CryptoError(format!("VP proof invalid: {e}")))?;
    Ok(content)
}

/// Serialize a sealed pack as VC-JWT (RFC 7519, EdDSA/Ed25519 per RFC 8037)
/// — low-spec machines pick fast JWT verification.
///
/// Kaimeter compact profile: the third segment is the pack's existing
/// Ed25519 signature over the canonical content JSON (a detached-payload
/// analogue) rather than a fresh signature over `header.payload`, because
/// [`SealedPack`] deliberately carries no private key — JWT framing is a
/// re-serialization of an existing seal, never a re-signing. The signer's
/// key travels in the JOSE header as an RFC 8037 OKP `jwk` so
/// [`verify_vc_jwt`] stays entirely offline from the token alone. Still
/// tamper-evident: any payload byte flipped changes the content the
/// signature commits to.
///
/// # Errors
///
/// [`DomainError::CryptoError`] on canonicalization failure (cannot occur
/// with a valid [`PackContent`]; kept for signature stability).
pub fn to_vc_jwt(pack: &SealedPack) -> Result<String, DomainError> {
    let public_key = hex::decode(&pack.public_key_hex)
        .map_err(|e| DomainError::CryptoError(format!("pack public key is not hex: {e}")))?;
    let header = serde_json::json!({
        "alg": "EdDSA",
        "typ": "JWT",
        "jwk": { "kty": "OKP", "crv": "Ed25519", "x": URL_SAFE_NO_PAD.encode(public_key) },
    });
    let payload = serde_json::json!({
        "iss": "kaimeter-pack",
        "vc": { "credentialSubject": pack.content },
    });
    let header_b64 = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&header)
            .map_err(|e| DomainError::CryptoError(format!("VC-JWT header serialization: {e}")))?,
    );
    let payload_b64 = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&payload)
            .map_err(|e| DomainError::CryptoError(format!("VC-JWT payload serialization: {e}")))?,
    );
    // Reuse the pack's seal: the same signature bytes that commit to the
    // canonical content, base64url'd into the compact third segment.
    let seal = signature_from_hex(&pack.signature_hex)?;
    let signature_b64 = URL_SAFE_NO_PAD.encode(seal.to_bytes());
    Ok(format!("{header_b64}.{payload_b64}.{signature_b64}"))
}

/// Parse + verify a VC-JWT offline. Returns the verified pack content.
///
/// # Errors
///
/// [`DomainError::CryptoError`] on a bad signature;
/// [`DomainError::SchemaViolation`] on a malformed JWT (wrong shape, or a
/// non-`EdDSA` `alg` — the profile is EdDSA-only per the R25 build note).
pub fn verify_vc_jwt(jwt: &str) -> Result<PackContent, DomainError> {
    let shape = |what: String| DomainError::SchemaViolation(what);
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return Err(shape(format!(
            "VC-JWT must have exactly 3 dot-separated parts, got {}",
            parts.len()
        )));
    }
    let header: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(parts[0])
            .map_err(|e| shape(format!("VC-JWT header is not base64url: {e}")))?,
    )
    .map_err(|e| shape(format!("VC-JWT header is not JSON: {e}")))?;
    let alg = header
        .get("alg")
        .and_then(Value::as_str)
        .ok_or_else(|| shape("VC-JWT header is missing `alg`".to_string()))?;
    if alg != "EdDSA" {
        return Err(shape(format!(
            "unsupported alg `{alg}`: the VC-JWT profile is EdDSA-only"
        )));
    }
    let jwk_x = header
        .get("jwk")
        .and_then(|jwk| jwk.get("x"))
        .and_then(Value::as_str)
        .ok_or_else(|| shape("VC-JWT header is missing `jwk.x`".to_string()))?;
    let key_bytes = URL_SAFE_NO_PAD
        .decode(jwk_x)
        .map_err(|e| DomainError::CryptoError(format!("VC-JWT jwk.x is not base64url: {e}")))?;
    let key_array: [u8; 32] = key_bytes.try_into().map_err(|malformed: Vec<u8>| {
        DomainError::CryptoError(format!(
            "VC-JWT jwk.x must be 32 bytes, got {}",
            malformed.len()
        ))
    })?;
    let verifying_key = VerifyingKey::from_bytes(&key_array)
        .map_err(|e| DomainError::CryptoError(format!("malformed VC-JWT key: {e}")))?;

    let payload: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|e| shape(format!("VC-JWT payload is not base64url: {e}")))?,
    )
    .map_err(|e| shape(format!("VC-JWT payload is not JSON: {e}")))?;
    let subject = payload
        .get("vc")
        .and_then(|vc| vc.get("credentialSubject"))
        .ok_or_else(|| shape("VC-JWT payload is missing `vc.credentialSubject`".to_string()))?;
    let content: PackContent = serde_json::from_value(subject.clone())
        .map_err(|e| shape(format!("vc.credentialSubject is not a pack content: {e}")))?;

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|e| DomainError::CryptoError(format!("VC-JWT signature is not base64url: {e}")))?;
    let sig_array: [u8; ed25519_dalek::SIGNATURE_LENGTH] =
        sig_bytes.try_into().map_err(|malformed: Vec<u8>| {
            DomainError::CryptoError(format!(
                "VC-JWT signature must be {} bytes, got {}",
                ed25519_dalek::SIGNATURE_LENGTH,
                malformed.len()
            ))
        })?;
    let signature = Signature::from_bytes(&sig_array);
    let bytes = canonical_content(&content)?;
    verifying_key
        .verify(&bytes, &signature)
        .map_err(|e| DomainError::CryptoError(format!("VC-JWT signature invalid: {e}")))?;
    Ok(content)
}

// ---------------------------------------------------------------------------
// R30 — pre-flight schema validation
// ---------------------------------------------------------------------------

/// A version-tagged schema entry (versioned schemas are retained
/// side-by-side so historical dossiers re-validate against their
/// submission-era schema — R30).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaEntry {
    /// Schema version tag (e.g. `2027.1`).
    pub version: String,
    /// Required top-level fields.
    pub required: Vec<String>,
    /// Primitive types for known fields (`string`, `number`, `boolean`).
    pub types: std::collections::BTreeMap<String, String>,
}

/// One schema violation found at pre-flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    /// The offending field.
    pub field: String,
    /// What is wrong (`MISSING`, `TYPE`).
    pub code: String,
}

/// Validate an export instance against a version-tagged schema BEFORE
/// submission, flagging missing required attributes before an upload can
/// fail (R30). A pragmatic subset: required-field presence + primitive
/// types (`"string"`/`"number"`/`"boolean"`; unknown type names are outside
/// the subset and pass) — the official registry XSD/JSON-Schema remain a 1.0
/// verification item.
///
/// Result contract (API is frozen): `Ok` only ever carries the EMPTY list
/// (the instance is clean); when any violation exists the function returns
/// `Err(DomainError::SchemaViolation)` whose message names the count and
/// every `field=code` pair. Callers needing the structured list read it out
/// of the error message, fix the instance, and re-validate.
///
/// # Errors
///
/// [`DomainError::SchemaViolation`] listing every violation when any exist.
pub fn preflight_validate(
    instance: &Value,
    schema: &SchemaEntry,
) -> Result<Vec<Violation>, DomainError> {
    let mut violations = Vec::new();
    let object = instance.as_object();

    // Required-field presence: a non-object instance fails closed with
    // every required field reported MISSING.
    for required in &schema.required {
        let present = object.is_some_and(|map| map.contains_key(required));
        if !present {
            violations.push(Violation {
                field: required.clone(),
                code: "MISSING".to_string(),
            });
        }
    }

    // Primitive type check: only for fields that ARE present.
    if let Some(map) = object {
        for (field, expected) in &schema.types {
            let matches = match map.get(field) {
                Some(value) => match expected.as_str() {
                    "string" => value.is_string(),
                    "number" => value.is_number(),
                    "boolean" => value.is_boolean(),
                    _ => true, // outside the pragmatic subset
                },
                None => true, // absent fields are flagged MISSING, not TYPE
            };
            if !matches {
                violations.push(Violation {
                    field: field.clone(),
                    code: "TYPE".to_string(),
                });
            }
        }
    }

    if violations.is_empty() {
        Ok(Vec::new())
    } else {
        let detail = violations
            .iter()
            .map(|violation| format!("{}={}", violation.field, violation.code))
            .collect::<Vec<_>>()
            .join(", ");
        Err(DomainError::SchemaViolation(format!(
            "{} pre-flight violation(s): {detail}",
            violations.len()
        )))
    }
}

// ---------------------------------------------------------------------------
// Unit tests — complement the contract tests in tests/export.rs
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_declaration_names_collapse_to_one_key() {
        let fields = vec![
            DeclarationField {
                name: "cn_code".to_string(),
                value: Value::String("72081000".to_string()),
            },
            DeclarationField {
                name: "cn_code".to_string(),
                value: Value::String("72085000".to_string()),
            },
        ];
        // Missing mandatory fields still fail closed, even with duplicates.
        let err = build_declaration(&fields).expect_err("incomplete");
        assert!(matches!(err, DomainError::MissingRequiredField(_)));
    }

    #[test]
    fn two_leaf_merkle_matches_convention() {
        let (l0, l1) = (leaf_hash("x"), leaf_hash("y"));
        let expected = hex::encode(node_hash(&l0, &l1));
        assert_eq!(merkle_root(&["x".to_string(), "y".to_string()]), expected);
        let proof = merkle_proof(&["x".to_string(), "y".to_string()], 0).expect("in range");
        assert_eq!(proof.siblings, vec![(hex::encode(l1), true)]);
        assert!(verify_inclusion("x", &proof));
    }

    #[test]
    fn promoted_tail_proof_verifies_for_three_leaves() {
        let leaves = ["a".to_string(), "b".to_string(), "c".to_string()];
        let proof = merkle_proof(&leaves, 2).expect("in range");
        // Index 2 is the odd tail: at the leaf level its own hash is the
        // (right) sibling; one level up (index 1) the sibling is the node
        // on its LEFT.
        let n01 = node_hash(&leaf_hash("a"), &leaf_hash("b"));
        assert_eq!(proof.siblings.len(), 2);
        assert_eq!(proof.siblings[0], (hex::encode(leaf_hash("c")), true));
        assert_eq!(proof.siblings[1], (hex::encode(n01), false));
        assert!(verify_inclusion("c", &proof));
    }

    #[test]
    fn anonymize_placeholder_is_not_identity() {
        let fields = vec![DeclarationField {
            name: "installation_id".to_string(),
            value: Value::String("Keldrion Steel Works Ltd".to_string()),
        }];
        let masked = apply_masks(
            &fields,
            &[("installation_id".to_string(), FieldMask::Anonymize)],
        );
        assert_eq!(masked[0].value, Value::String("ANONYMIZED".to_string()));
    }
}
