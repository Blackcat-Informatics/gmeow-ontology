// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Cross-engine COSE_Encrypt0 conformance (§9.3): the Rust engine reproduces the
//! frozen `vectors/encrypt0/basic.json` (a fixed-IV AES-256-GCM seal) byte-for-byte
//! and opens it, plus a random-IV round-trip.

use std::path::Path;

use gmeow_gts::cose::{decrypt0, encrypt0, encrypt0_with_iv, recipient_kid, Encrypt0Error};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

#[test]
fn encrypt0_vector_seals_and_opens() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors/encrypt0");
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("basic.json")).unwrap()).unwrap();
    let key: [u8; 32] = unhex(json["key"].as_str().unwrap()).try_into().unwrap();
    let iv: [u8; 12] = unhex(json["iv"].as_str().unwrap()).try_into().unwrap();
    let kid = json["kid"].as_str().unwrap();
    let plaintext = unhex(json["plaintext"].as_str().unwrap());
    let expected = unhex(json["cose"].as_str().unwrap());

    // Fixed IV -> the sealed bytes reproduce the frozen vector exactly.
    assert_eq!(encrypt0_with_iv(&plaintext, kid, &key, &iv), expected);

    // The recipient kid round-trips out of the cleartext header.
    assert_eq!(recipient_kid(&expected).as_deref(), Some(kid));

    // The frozen COSE opens back to the plaintext under the content key.
    assert_eq!(
        decrypt0(&expected, |k| (k == kid).then_some(key)),
        Ok(plaintext)
    );

    // No key resolved -> MissingKey; wrong key -> AuthFailed.
    assert_eq!(
        decrypt0(&expected, |_| None),
        Err(Encrypt0Error::MissingKey)
    );
    assert_eq!(
        decrypt0(&expected, |_| Some([0u8; 32])),
        Err(Encrypt0Error::AuthFailed)
    );
}

#[test]
fn encrypt0_random_iv_round_trip() {
    let key = [7u8; 32];
    let sealed = encrypt0(b"verified id record", "did:court", &key);
    assert_eq!(recipient_kid(&sealed).as_deref(), Some("did:court"));
    assert_eq!(
        decrypt0(&sealed, |k| (k == "did:court").then_some(key)),
        Ok(b"verified id record".to_vec())
    );
}
