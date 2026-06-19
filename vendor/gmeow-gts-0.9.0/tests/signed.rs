// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0
//! File-level signing conformance (§9.2): the Rust engine reproduces the frozen
//! signed GTS (`vectors/signed/basic.json`) when signing, and verifies it.

use std::path::Path;

use ed25519_dalek::{SigningKey, VerifyingKey};
use gmeow_gts::cose::verify_signatures;
use gmeow_gts::model::{Term, TermKind};
use gmeow_gts::openpgp::parse_transport_key;
use gmeow_gts::reader::read;
use gmeow_gts::writer::Writer;

const CAT: &str = "https://example.org/Cat";
const LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn iri(v: &str) -> Term {
    Term {
        kind: TermKind::Iri,
        value: Some(v.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn lit_lang(v: &str, lang: &str) -> Term {
    Term {
        kind: TermKind::Literal,
        value: Some(v.to_string()),
        datatype: None,
        lang: Some(lang.to_string()),
        reifier: None,
    }
}

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../python/tests/fixtures")
}

#[test]
fn signed_file_vector() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors/signed/basic.json");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let seed: [u8; 32] = unhex(json["seed"].as_str().unwrap()).try_into().unwrap();
    let pub32: [u8; 32] = unhex(json["pub"].as_str().unwrap()).try_into().unwrap();
    let kid = json["kid"].as_str().unwrap();
    let expected = unhex(json["gts"].as_str().unwrap());

    // Writer signing reproduces the frozen signed file byte-for-byte.
    let mut w = Writer::new("dist");
    w.sign_with(SigningKey::from_bytes(&seed), kid);
    w.add_terms(&[iri(CAT), iri(LABEL), lit_lang("Cat", "en")]);
    w.add_quads(&[(0, 1, 2, None)]);
    assert_eq!(w.to_bytes(), expected, "writer signing mismatch");

    let vk = VerifyingKey::from_bytes(&pub32).unwrap();

    // Right key -> every signature valid.
    let mut g = read(&expected, false, None);
    assert_eq!(g.signatures.len(), 2);
    verify_signatures(&mut g.signatures, |k| (k == kid).then_some(vk));
    assert!(g.signatures.iter().all(|s| s.status == "valid"));
    assert!(g.signatures.iter().all(|s| s.kid.as_deref() == Some(kid)));

    // No key resolved -> unverified.
    let mut g = read(&expected, false, None);
    verify_signatures(&mut g.signatures, |_| None);
    assert!(g.signatures.iter().all(|s| s.status == "unverified"));

    // Wrong key -> invalid.
    let wrong = SigningKey::from_bytes(&[7u8; 32]).verifying_key();
    let mut g = read(&expected, false, None);
    verify_signatures(&mut g.signatures, |_| Some(wrong));
    assert!(g.signatures.iter().all(|s| s.status == "invalid"));
}

#[test]
fn writer_signs_with_openpgp_secret_key_default_fingerprint_kid() {
    let public_armor = std::fs::read_to_string(fixtures_dir().join("test_key.pub.asc")).unwrap();
    let secret_armor = std::fs::read_to_string(fixtures_dir().join("test_key.sec.asc")).unwrap();
    let fingerprint = std::fs::read_to_string(fixtures_dir().join("test_key.fingerprint")).unwrap();
    let fingerprint = fingerprint.trim();
    let public = parse_transport_key(&public_armor).unwrap();

    let mut w = Writer::new("dist");
    w.sign_with_openpgp_secret_key(&secret_armor, None).unwrap();
    w.add_terms(&[iri(CAT), iri(LABEL), lit_lang("Cat", "en")]);
    w.add_quads(&[(0, 1, 2, None)]);

    let verifier = VerifyingKey::from_bytes(&public.raw_public).unwrap();
    let mut g = read(&w.to_bytes(), false, None);
    assert_eq!(g.signatures.len(), 2);
    verify_signatures(&mut g.signatures, |kid| {
        (kid == fingerprint).then_some(verifier)
    });
    assert!(g.signatures.iter().all(|s| s.status == "valid"));
    assert!(g
        .signatures
        .iter()
        .all(|s| s.kid.as_deref() == Some(fingerprint)));
}

#[test]
fn writer_signs_with_openpgp_secret_key_override_kid() {
    let public_armor = std::fs::read_to_string(fixtures_dir().join("test_key.pub.asc")).unwrap();
    let secret_armor = std::fs::read_to_string(fixtures_dir().join("test_key.sec.asc")).unwrap();
    let public = parse_transport_key(&public_armor).unwrap();
    let override_kid = "did:example:openpgp-test";

    let mut w = Writer::new("dist");
    w.sign_with_openpgp_secret_key(&secret_armor, Some(override_kid))
        .unwrap();
    w.add_terms(&[iri(CAT), iri(LABEL), lit_lang("Cat", "en")]);

    let verifier = VerifyingKey::from_bytes(&public.raw_public).unwrap();
    let mut g = read(&w.to_bytes(), false, None);
    assert_eq!(g.signatures.len(), 1);
    verify_signatures(&mut g.signatures, |kid| {
        (kid == override_kid).then_some(verifier)
    });
    assert!(g.signatures.iter().all(|s| s.status == "valid"));
    assert!(g
        .signatures
        .iter()
        .all(|s| s.kid.as_deref() == Some(override_kid)));
}
