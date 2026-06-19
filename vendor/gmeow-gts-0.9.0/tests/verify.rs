// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! High-level embedded-key verification API parity.

use ciborium::value::Value;
use ed25519_dalek::SigningKey;
use gmeow_gts::model::{Term, TermKind};
use gmeow_gts::openpgp::parse_transport_key;
use gmeow_gts::reader::read;
use gmeow_gts::verify::{
    extract_transport_key, format_fingerprint, verify_file, verify_file_with_options, VerifyOptions,
};
use gmeow_gts::writer::Writer;

fn b64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() >= 2 {
            TABLE[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() == 3 {
            TABLE[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn public_key_armor(raw_public: &[u8; 32]) -> String {
    let mut body = Vec::new();
    body.push(4); // v4 public-key packet
    body.extend_from_slice(&0u32.to_be_bytes()); // synthetic creation time
    body.push(22); // EdDSA / Ed25519
    body.push(9); // OID length
    body.extend_from_slice(&[0x2b, 0x06, 0x01, 0x04, 0x01, 0xda, 0x47, 0x0f, 0x01]);
    body.extend_from_slice(&263u16.to_be_bytes()); // 0x40 marker + 32-byte key
    body.push(0x40);
    body.extend_from_slice(raw_public);

    let mut packet = vec![0xc6, body.len() as u8]; // new-format public-key packet
    packet.extend_from_slice(&body);
    format!(
        "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n{}\n-----END PGP PUBLIC KEY BLOCK-----\n",
        b64(&packet)
    )
}

fn iri(value: &str) -> Term {
    Term {
        kind: TermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn lit(value: &str) -> Term {
    Term {
        kind: TermKind::Literal,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn signed_file(seed: [u8; 32], kid: &str, armor: &str) -> Vec<u8> {
    let mut writer = Writer::new("dist");
    writer.sign_with(SigningKey::from_bytes(&seed), kid);
    writer.add_meta(Value::Map(vec![(
        "gts:transportKey".into(),
        Value::Map(vec![
            ("kid".into(), kid.into()),
            ("gpg".into(), armor.into()),
        ]),
    )]));
    writer.add_terms(&[
        iri("https://example.org/s"),
        iri("https://example.org/p"),
        lit("object"),
    ]);
    writer.add_quads(&[(0, 1, 2, None)]);
    writer.to_bytes()
}

#[test]
fn format_fingerprint_groups_openpgp_hex() {
    assert_eq!(
        format_fingerprint("93F32F9F1439F0FBA266331B6F4732092D747581"),
        "93F3 2F9F 1439 F0FB A266 331B 6F47 3209 2D74 7581"
    );
    assert_eq!(format_fingerprint("not-a-fingerprint"), "not-a-fingerprint");
}

#[test]
fn extract_transport_key_round_trips_from_graph_meta() {
    let seed = [9u8; 32];
    let raw_public = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    let armor = public_key_armor(&raw_public);
    let data = signed_file(seed, "did:example:transport", &armor);
    let graph = read(&data, true, None);

    let key = extract_transport_key(&graph).expect("transport key");
    assert_eq!(key.kid, "did:example:transport");
    assert!(key.gpg.contains("BEGIN PGP PUBLIC KEY BLOCK"));
}

#[test]
fn verify_signed_file_with_embedded_key() {
    let seed = [3u8; 32];
    let raw_public = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    let armor = public_key_armor(&raw_public);
    let data = signed_file(seed, "did:example:transport", &armor);

    let result = verify_file(&data);
    assert!(result.ok, "{:?}", result.errors);
    assert_eq!(result.kid.as_deref(), Some("did:example:transport"));
    assert_eq!(result.signed, 3);
    assert_eq!(result.valid, 3);
    assert_eq!(result.invalid, 0);
    assert_eq!(result.unverified, 0);
    assert!(result.fingerprint.is_some());
    assert!(result.emojihash.is_some());
    assert!(result.emojihash_labels.is_some());
    assert!(result.randomart.is_some());
}

#[test]
fn verify_with_trusted_key_uses_openpgp_fingerprint_as_kid() {
    let seed = [4u8; 32];
    let raw_public = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    let armor = public_key_armor(&raw_public);
    let fingerprint = parse_transport_key(&armor).unwrap().fingerprint;
    let data = signed_file(seed, &fingerprint, &armor);

    let result = verify_file_with_options(
        &data,
        &VerifyOptions::strict().with_armored_key(armor.clone()),
    );
    assert!(result.ok, "{:?}", result.errors);
    assert_eq!(result.kid.as_deref(), Some(fingerprint.as_str()));
    assert_eq!(result.fingerprint.as_deref(), Some(fingerprint.as_str()));
}

#[test]
fn trusted_policy_counts_trusted_valid_signatures() {
    let seed = [5u8; 32];
    let raw_public = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    let armor = public_key_armor(&raw_public);
    let data = signed_file(seed, "did:example:trusted", &armor);
    let policy = gmeow_gts::policy::TrustPolicy::new(["did:example:trusted"], false);

    let result = verify_file_with_options(&data, &VerifyOptions::strict().trust_policy(policy));
    assert!(result.ok, "{:?}", result.errors);
    assert_eq!(result.trusted, result.signed);
}

#[test]
fn verify_unsigned_file_is_ok_when_signatures_are_not_required() {
    let data = Writer::new("dist").to_bytes();
    let result =
        verify_file_with_options(&data, &VerifyOptions::strict().require_signatures(false));
    assert!(result.ok, "{:?}", result.errors);
    assert_eq!(result.signed, 0);
}

#[test]
fn verify_unsigned_file_fails_when_required() {
    let data = Writer::new("dist").to_bytes();
    let result = verify_file(&data);
    assert!(!result.ok);
    assert!(result.errors[0].contains("no gts:transportKey found"));
}

#[test]
fn verify_tampered_file_fails() {
    let seed = [6u8; 32];
    let raw_public = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    let armor = public_key_armor(&raw_public);
    let mut data = signed_file(seed, "did:example:transport", &armor);
    let idx = data.len() / 2;
    data[idx] ^= 0xff;

    let result = verify_file(&data);
    assert!(!result.ok);
}

#[test]
fn verify_with_wrong_out_of_band_key_reports_unverified() {
    let signer_seed = [7u8; 32];
    let signer_public = SigningKey::from_bytes(&signer_seed)
        .verifying_key()
        .to_bytes();
    let signer_armor = public_key_armor(&signer_public);
    let fingerprint = parse_transport_key(&signer_armor).unwrap().fingerprint;
    let data = signed_file(signer_seed, &fingerprint, &signer_armor);

    let wrong_public = SigningKey::from_bytes(&[8u8; 32])
        .verifying_key()
        .to_bytes();
    let wrong_armor = public_key_armor(&wrong_public);
    let result = verify_file_with_options(
        &data,
        &VerifyOptions::strict().with_armored_key(wrong_armor),
    );

    assert!(!result.ok);
    assert_eq!(result.unverified, result.signed);
    assert!(result.errors[0].contains("unverified"));
}
