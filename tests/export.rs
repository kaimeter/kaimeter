// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Contract tests for `kaimeter::export`:
//! declaration-ready files (R9), one-click compliant export with per-field
//! masking and the self-audit preview (R21), sealed packs (Merkle root,
//! W3C VP JSON-LD, VC-JWT) verified entirely offline (R21/R22), and the
//! pre-flight registry schema validator (R30).

use std::collections::BTreeSet;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::SigningKey;
use kaimeter_core::domain::DomainError;
use kaimeter_core::export::{
    apply_masks, build_declaration, merkle_proof, merkle_root, preflight_validate, preview,
    seal_pack, to_vc_jwt, to_vp_json_ld, verify_inclusion, verify_sealed_pack, verify_vc_jwt,
    verify_vp_json_ld, DeclarationField, FieldMask, PackContent, SchemaEntry, SealedPack,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// One declaration field with a JSON value (test shorthand).
fn df(name: &str, value: Value) -> DeclarationField {
    DeclarationField {
        name: name.to_string(),
        value,
    }
}

/// All 8 mandatory annual-declaration fields (R2/R9), in canonical order.
fn all_declaration_fields() -> Vec<DeclarationField> {
    vec![
        df("cn_code", json!("72081000")),
        df("net_mass_kg", json!(25_000.5)),
        df("country_of_origin", json!("CN")),
        df("production_country", json!("CN")),
        df("installation_id", json!("INSTALL-77")),
        df("import_date", json!("2027-03-01")),
        df("determination_basis", json!("DEFAULT")),
        df("embedded_emissions_tco2e", json!(41.25)),
    ]
}

/// A sealed-pack payload: anonymized installation ref + a single CN factor
/// only — no supplier identity, no pricing (R21 blind pass-through).
fn sample_content() -> PackContent {
    PackContent {
        installation_ref: "ANON-4471".to_string(),
        cn_code: "72081000".to_string(),
        emission_factor_tco2e_per_t: 1.85,
        embedded_emissions_tco2e: 41.25,
        production_log_merkle_root: "ab".repeat(32),
        issued_iso: "2027-03-01".to_string(),
        valid_until_iso: Some("2028-03-01".to_string()),
    }
}

fn sample_pack() -> SealedPack {
    let key = SigningKey::from_bytes(&[42u8; 32]);
    seal_pack(sample_content(), &key).expect("seal")
}

/// Test-side SHA-256 (independent of the module under test).
fn sha(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// The documented Merkle leaf convention: SHA-256(0x00 || leaf bytes).
fn leaf_hash(leaf: &str) -> [u8; 32] {
    let mut input = vec![0x00u8];
    input.extend_from_slice(leaf.as_bytes());
    sha(&input)
}

/// The documented Merkle node convention: SHA-256(0x01 || left || right).
fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut input = vec![0x01u8];
    input.extend_from_slice(left);
    input.extend_from_slice(right);
    sha(&input)
}

// ---------------------------------------------------------------------------
// R9 — declaration-ready file (first declaration September 30th, 2027)
// ---------------------------------------------------------------------------

/// R9: a declaration export fails closed unless every mandatory field is
/// present, and otherwise emits exactly the values given.
#[test]
fn declaration_requires_all_mandatory_fields() {
    // All 8 mandatory fields present -> Ok object carrying the values.
    let ok = build_declaration(&all_declaration_fields()).expect("all fields present");
    assert!(ok.is_object(), "R9: declaration is a JSON object");
    assert_eq!(ok["cn_code"], json!("72081000"));
    assert_eq!(ok["net_mass_kg"], json!(25_000.5));
    assert_eq!(ok["embedded_emissions_tco2e"], json!(41.25));
    assert_eq!(ok["determination_basis"], json!("DEFAULT"));

    // Any mandatory field missing -> Err naming it (fail closed).
    let mut missing_one = all_declaration_fields();
    missing_one.retain(|f| f.name != "embedded_emissions_tco2e");
    let err =
        build_declaration(&missing_one).expect_err("R9: missing mandatory field must fail closed");
    assert!(
        matches!(&err, DomainError::MissingRequiredField(name) if name == "embedded_emissions_tco2e"),
        "R9 (first declaration 2027-09-30): error must name the missing field, got {err:?}"
    );
}

/// R21: the file is plain, human-readable, and free of hidden metadata —
/// the output key set is EXACTLY the input field names, no extras ever.
/// (Key order is not asserted: serde_json does not preserve insertion
/// order unless the `preserve_order` feature is enabled.)
#[test]
fn declaration_has_no_hidden_metadata() {
    let fields = all_declaration_fields();
    let declaration = build_declaration(&fields).expect("complete declaration");
    let expected: BTreeSet<String> = fields.iter().map(|f| f.name.clone()).collect();
    let actual: BTreeSet<String> = declaration
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        actual, expected,
        "R21: no hidden metadata, no dropped fields"
    );

    // Extra (non-mandatory) fields pass through untouched — exactly the
    // given fields, nothing added by the exporter.
    let mut with_extra = fields;
    with_extra.push(df("exporter_note", json!("blast-furnace route")));
    let extended = build_declaration(&with_extra).expect("extra fields allowed");
    let expected_extra: BTreeSet<String> = with_extra.iter().map(|f| f.name.clone()).collect();
    let actual_extra: BTreeSet<String> = extended
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(actual_extra, expected_extra, "R21: extras kept, none added");
}

// ---------------------------------------------------------------------------
// R21 — per-field masking + self-audit preview
// ---------------------------------------------------------------------------

/// R21: before export, a field-level preview shows exactly which fields will
/// leave — and states what is not included. The provider audits the export
/// themselves before anything leaves their control.
#[test]
fn preview_lists_exactly_what_leaves() {
    let plan = vec![
        ("cn_code".to_string(), FieldMask::Keep),
        ("unit_price_eur".to_string(), FieldMask::Redact),
        ("installation_id".to_string(), FieldMask::Anonymize),
    ];
    let report = preview(&plan);
    assert_eq!(
        report.included,
        vec!["cn_code".to_string()],
        "R21: only Keep-ed fields leave"
    );
    assert_eq!(
        report.excluded,
        vec!["unit_price_eur".to_string(), "installation_id".to_string()],
        "R21: the preview must state what is NOT included (Redact + Anonymize)"
    );
}

/// R21: the buyer sees compliance data, never the trading book — Redact
/// removes the field entirely, Anonymize keeps it as a non-identifying
/// placeholder, Keep and unmatched fields pass through unchanged.
#[test]
fn masks_remove_and_anonymize() {
    let fields = vec![
        df("cn_code", json!("72081000")),
        df("unit_price_eur", json!(650.0)),
        df("installation_id", json!("INSTALL-77")),
        df("import_date", json!("2027-03-01")),
    ];
    let masks = vec![
        ("unit_price_eur".to_string(), FieldMask::Redact),
        ("installation_id".to_string(), FieldMask::Anonymize),
    ];
    let out = apply_masks(&fields, &masks);

    let names: Vec<&str> = out.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["cn_code", "installation_id", "import_date"],
        "R21: Redact omits the field entirely; order preserved"
    );
    let anonymized = out
        .iter()
        .find(|f| f.name == "installation_id")
        .expect("anonymized field retained");
    assert_eq!(
        anonymized.value,
        json!("ANONYMIZED"),
        "R21: Anonymize keeps the field with a single non-identifying placeholder"
    );
    let kept = out
        .iter()
        .find(|f| f.name == "cn_code")
        .expect("kept field retained");
    assert_eq!(kept.value, json!("72081000"), "R21: Keep is unchanged");
    let unmatched = out
        .iter()
        .find(|f| f.name == "import_date")
        .expect("unmasked field retained");
    assert_eq!(
        unmatched.value,
        json!("2027-03-01"),
        "R21: fields with no mask entry pass through unchanged"
    );
}

// ---------------------------------------------------------------------------
// Sealed pack — Merkle commitment (R21)
// ---------------------------------------------------------------------------

/// R21: the pack carries a Merkle root of the production log so a verifier
/// can confirm the data derives from the audited record without seeing it.
/// Convention pinned here: leaf = SHA-256(0x00 || leaf bytes), node =
/// SHA-256(0x01 || left || right) over raw child bytes, odd tail promoted
/// (duplicated with itself), empty input -> SHA-256 of the empty string.
#[test]
fn merkle_properties_pinned() {
    // Empty input: the SHA-256 of the empty string, pinned.
    assert_eq!(
        merkle_root(&[]),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "R21: empty production log has the well-known empty SHA-256 root"
    );

    // Single leaf: the root is that leaf hash, deterministically.
    let single = merkle_root(&["a".to_string()]);
    assert_eq!(single, hex::encode(leaf_hash("a")));
    assert_eq!(single, merkle_root(&["a".to_string()]));

    // Three leaves: root recomputed independently from the documented
    // convention (odd tail promoted at every level).
    let (l0, l1, l2) = (leaf_hash("a"), leaf_hash("b"), leaf_hash("c"));
    let n01 = node_hash(&l0, &l1);
    let n22 = node_hash(&l2, &l2); // odd tail duplicated with itself
    let expected_root = hex::encode(node_hash(&n01, &n22));
    let three = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    assert_eq!(merkle_root(&three), expected_root);
    assert_eq!(merkle_root(&three), merkle_root(&three), "stable");

    // Proof + verify round-trip at every index of the 3-leaf tree.
    for index in 0..3usize {
        let proof = merkle_proof(&three, index).expect("in-range index");
        assert_eq!(proof.leaf_index, index);
        assert_eq!(proof.root, expected_root, "proof commits to the root");
        assert!(
            verify_inclusion(&three[index], &proof),
            "R21: inclusion proof must verify offline for index {index}"
        );
        // A different leaf under the same proof must fail.
        assert!(
            !verify_inclusion("tampered", &proof),
            "tampered leaf must not verify (index {index})"
        );
    }

    // Out-of-range index is refused.
    assert!(
        matches!(merkle_proof(&three, 3), Err(DomainError::Storage(_))),
        "out-of-range proof index must be a Storage error"
    );
    assert!(merkle_proof(&[], 0).is_err());
}

// ---------------------------------------------------------------------------
// Sealed pack — Ed25519, verified entirely offline (R21/R22)
// ---------------------------------------------------------------------------

/// R21 sealed pack (blind pass-through: content carries an anonymized
/// installation ref + a single CN factor only): seal -> offline verify
/// round-trips; a tampered byte of content or a wrong-key signature is
/// refused. R22: no Kaimeter server is in the loop — verification uses
/// only the pack itself.
#[test]
fn sealed_pack_round_trips_offline() {
    let content = sample_content();
    let key = SigningKey::from_bytes(&[42u8; 32]);
    let pack = seal_pack(content.clone(), &key).expect("seal");

    assert_eq!(
        pack.public_key_hex,
        hex::encode(key.verifying_key().as_bytes()),
        "pack carries the signer's public key"
    );
    let verified = verify_sealed_pack(&pack).expect("offline verification");
    assert_eq!(verified, content, "round-trip is lossless");

    // Tamper one field of the content -> signature no longer matches.
    let mut tampered = pack.clone();
    tampered.content.emission_factor_tco2e_per_t += 0.5;
    assert!(
        verify_sealed_pack(&tampered).is_err(),
        "R21: tampered content must be refused"
    );

    // Signature made by a different key than the embedded public key.
    let other_key = SigningKey::from_bytes(&[7u8; 32]);
    let other_pack = seal_pack(content.clone(), &other_key).expect("seal with other key");
    let wrong_key = SealedPack {
        content,
        public_key_hex: pack.public_key_hex,
        signature_hex: other_pack.signature_hex,
    };
    assert!(
        verify_sealed_pack(&wrong_key).is_err(),
        "R21: wrong-key signature must be refused"
    );
}

// ---------------------------------------------------------------------------
// Sealed pack — W3C Verifiable Presentation JSON-LD (R21)
// ---------------------------------------------------------------------------

/// R21: the pack serializes as a W3C Verifiable Presentation (JSON-LD) so
/// verifiers validate integrity programmatically, entirely offline.
#[test]
fn vp_json_ld_round_trips() {
    let pack = sample_pack();
    let content = pack.content.clone();
    let vp = to_vp_json_ld(&pack);

    // Kaimeter pragmatic profile shape.
    assert_eq!(
        vp["@context"][0],
        json!("https://www.w3.org/2018/credentials/v1"),
        "R21: credentials context"
    );
    assert_eq!(
        vp["type"][0],
        json!("VerifiablePresentation"),
        "R21: VP type"
    );
    let vc = &vp["verifiableCredential"];
    assert_eq!(vc["type"][0], json!("VerifiableCredential"));
    assert_eq!(vc["proof"]["type"], json!("DataIntegrityProof"));
    assert_eq!(vc["proof"]["cryptosuite"], json!("eddsa-rdfc-2022"));
    assert_eq!(
        vc["proof"]["verificationMethod"],
        json!(format!("did:key:{}", pack.public_key_hex)),
        "R21: proof anchored to the signer's did:key"
    );
    assert_eq!(vc["proof"]["proofValue"], json!(pack.signature_hex));

    // Offline verification returns the verified content.
    let verified = verify_vp_json_ld(&vp).expect("offline VP verification");
    assert_eq!(verified, content);

    // Tampered credentialSubject -> proof fails.
    let mut tampered = vp.clone();
    tampered["verifiableCredential"]["credentialSubject"]["emission_factor_tco2e_per_t"] =
        json!(999.0);
    assert!(
        verify_vp_json_ld(&tampered).is_err(),
        "R21: tampered VP must be refused"
    );

    // Wrong shape -> schema violation, not a crypto panic.
    let mut shapeless = vp.clone();
    shapeless["verifiableCredential"]
        .as_object_mut()
        .expect("object")
        .remove("proof");
    assert!(matches!(
        verify_vp_json_ld(&shapeless),
        Err(DomainError::SchemaViolation(_))
    ));
}

// ---------------------------------------------------------------------------
// Sealed pack — VC-JWT for low-spec machines (R21/R25 build note)
// ---------------------------------------------------------------------------

/// R25 build note (R21): VC-JWT (RFC 7519) — low-spec machines pick fast JWT
/// verification while JSON-LD stays available. Round-trip is lossless; a
/// tampered payload is refused; a non-EdDSA alg is a schema violation.
#[test]
fn vc_jwt_round_trips() {
    let pack = sample_pack();
    let content = pack.content.clone();
    let jwt = to_vc_jwt(&pack).expect("serialize VC-JWT");

    // Compact JWS shape: three dot-separated base64url parts.
    let parts: Vec<&str> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3, "compact JWS: header.payload.signature");
    let header: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).expect("b64url")).expect("json");
    assert_eq!(header["alg"], json!("EdDSA"));
    assert_eq!(header["typ"], json!("JWT"));
    let payload: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).expect("b64url")).expect("json");
    assert_eq!(payload["iss"], json!("kaimeter-pack"));

    // Round-trip.
    let verified = verify_vc_jwt(&jwt).expect("offline VC-JWT verification");
    assert_eq!(verified, content);

    // Tampered payload (original signature kept) -> crypto refusal.
    let mut forged_payload = payload.clone();
    forged_payload["vc"]["credentialSubject"]["embedded_emissions_tco2e"] = json!(1.0);
    let forged = format!(
        "{}.{}.{}",
        parts[0],
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&forged_payload).expect("json")),
        parts[2]
    );
    assert!(
        matches!(verify_vc_jwt(&forged), Err(DomainError::CryptoError(_))),
        "tampered VC-JWT payload must be refused"
    );

    // Wrong algorithm -> schema violation before any crypto runs.
    let mut rs_header = header.clone();
    rs_header["alg"] = json!("RS256");
    let rs = format!(
        "{}.{}.{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&rs_header).expect("json")),
        parts[1],
        parts[2]
    );
    assert!(
        matches!(verify_vc_jwt(&rs), Err(DomainError::SchemaViolation(_))),
        "alg RS256 must be a schema violation (EdDSA-only profile)"
    );
}

// ---------------------------------------------------------------------------
// R30 — pre-flight registry schema validation
// ---------------------------------------------------------------------------

/// R30: export files are validated offline BEFORE submission, flagging
/// missing required attributes before an upload can fail. Missing fields
/// yield MISSING; present-but-mistyped fields yield TYPE. Clean instances
/// validate to an empty violation list.
#[test]
fn preflight_flags_missing_and_mistyped() {
    let schema = SchemaEntry {
        version: "2027.1".to_string(),
        required: vec!["cn_code".to_string(), "net_mass_kg".to_string()],
        types: [
            ("cn_code".to_string(), "string".to_string()),
            ("net_mass_kg".to_string(), "number".to_string()),
        ]
        .into_iter()
        .collect(),
    };

    // cn_code missing entirely; net_mass_kg present but a JSON string.
    let bad = json!({ "net_mass_kg": "12", "country_of_origin": "CN" });
    let err = preflight_validate(&bad, &schema).expect_err("violations must fail");
    let message = err.to_string();
    assert!(
        matches!(err, DomainError::SchemaViolation(_)),
        "R30: violations surface as SchemaViolation"
    );
    assert!(
        message.contains("2"),
        "R30: the summary must name the violation count, got: {message}"
    );
    assert!(
        message.contains("cn_code") && message.contains("MISSING"),
        "R30: missing field flagged MISSING, got: {message}"
    );
    assert!(
        message.contains("net_mass_kg") && message.contains("TYPE"),
        "R30: mistyped field flagged TYPE, got: {message}"
    );

    // Fixed instance validates clean (empty violation list).
    let fixed = json!({ "cn_code": "72081000", "net_mass_kg": 12.0 });
    let violations = preflight_validate(&fixed, &schema).expect("clean instance");
    assert!(
        violations.is_empty(),
        "R30: clean instance -> Ok with an empty violation list"
    );
}
