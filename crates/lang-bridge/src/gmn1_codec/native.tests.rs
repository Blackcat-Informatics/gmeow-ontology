// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::cbor::{self, Value};

fn tiny() -> (GmnDictionary, CurrentCodebook) {
    let signature = GmnGlyphSignature {
        fixity: Some("infix".into()),
        arity: Some(2),
    };
    let scope = super::super::SIGIL_MATH.to_owned();
    let term = "urn:synthetic:add".to_owned();
    let dictionary = GmnDictionary {
        version: super::super::DICTIONARY_VERSION.into(),
        term_to_alias: BTreeMap::from([("urn:synthetic:subject".into(), "subject".into())]),
        alias_to_term: BTreeMap::from([
            ("subject".into(), "urn:synthetic:subject".into()),
            ("subject_old".into(), "urn:synthetic:subject".into()),
        ]),
        glyphs: GmnGlyphRegistry {
            version: super::super::GLYPH_VERSION.into(),
            term_to_glyph: BTreeMap::from([(
                (scope.clone(), term.clone(), signature.clone()),
                "+".into(),
            )]),
            glyph_to_term: BTreeMap::from([(
                (scope.clone(), "+".into(), signature.clone()),
                term.clone(),
            )]),
            fallback_to_term: BTreeMap::from([
                (
                    (scope.clone(), "add".into(), signature.clone()),
                    term.clone(),
                ),
                ((scope, "plus".into(), signature), term),
            ]),
        },
        acceptance: DialectAcceptance {
            latest_major: 7,
            accept_window: 2,
        },
    };
    let codebook = CurrentCodebook {
        references: BTreeSet::from([
            "urn:synthetic:dictionary".into(),
            "urn:synthetic:script".into(),
        ]),
        dictionary_version: dictionary.version.clone(),
        glyph_version: dictionary.glyphs.version.clone(),
        graphemes: BTreeSet::from(["urn:synthetic:plus".into()]),
        dictionary_entries: BTreeSet::from([
            "urn:synthetic:entry".into(),
            "urn:synthetic:old-entry".into(),
        ]),
    };
    (dictionary, codebook)
}

fn tiny_packet() -> (Vec<u8>, String) {
    let (dictionary, codebook) = tiny();
    let source = b"explicit tiny native source identity";
    let expected = purrdf_hash::hex::encode(purrdf_hash::blake3::hash(source).as_bytes());
    (
        encode(
            &dictionary,
            &codebook,
            &format!("{:x}", Sha256::digest(source)),
            &expected,
        )
        .unwrap(),
        expected,
    )
}

fn reference_packet() -> (Vec<u8>, String) {
    let (mut dictionary, codebook) = tiny();
    dictionary.acceptance.latest_major = 257;
    let scope = super::super::SIGIL_MATH.to_owned();
    let term = "urn:synthetic:multiply".to_owned();
    let signature = GmnGlyphSignature {
        fixity: None,
        arity: None,
    };
    dictionary.glyphs.term_to_glyph.insert(
        (scope.clone(), term.clone(), signature.clone()),
        "×".to_owned(),
    );
    dictionary.glyphs.glyph_to_term.insert(
        (scope.clone(), "×".to_owned(), signature.clone()),
        term.clone(),
    );
    dictionary
        .glyphs
        .fallback_to_term
        .insert((scope, "multiply".to_owned(), signature), term);
    let source = b"explicit native CBOR reference packet";
    let expected = purrdf_hash::hex::encode(purrdf_hash::blake3::hash(source).as_bytes());
    let bytes = encode(
        &dictionary,
        &codebook,
        &format!("{:x}", Sha256::digest(source)),
        &expected,
    )
    .expect("authored complete reference packet");
    (bytes, expected)
}

#[test]
fn native_transport_preserves_frozen_packet_bytes() {
    let (bytes, expected) = reference_packet();
    let reference = purrdf_hash::hex::decode(include_str!("native/fixtures/packet-v1.hex").trim())
        .expect("frozen complete packet hex");
    assert_eq!(
        bytes, reference,
        "complete producer packet bytes remain fixed"
    );
    let hydrated = decode(&bytes, &expected).expect("authored reference packet");
    assert_eq!(hydrated.dictionary.acceptance.latest_major, 257);
    assert!(
        hydrated
            .dictionary
            .glyphs
            .term_to_glyph
            .keys()
            .any(|(_, _, signature)| { signature.fixity.is_none() && signature.arity.is_none() })
    );
}

#[test]
fn native_transport_preserves_all_tables_memberships_signatures_and_acceptance() {
    let (bytes, expected) = tiny_packet();
    let hydrated = decode(&bytes, &expected).unwrap();
    let (dictionary, codebook) = tiny();
    assert_eq!(hydrated.dictionary.term_to_alias, dictionary.term_to_alias);
    assert_eq!(hydrated.dictionary.alias_to_term, dictionary.alias_to_term);
    assert_eq!(
        hydrated.dictionary.glyphs.term_to_glyph,
        dictionary.glyphs.term_to_glyph
    );
    assert_eq!(
        hydrated.dictionary.glyphs.glyph_to_term,
        dictionary.glyphs.glyph_to_term
    );
    assert_eq!(
        hydrated.dictionary.glyphs.fallback_to_term,
        dictionary.glyphs.fallback_to_term
    );
    assert_eq!(hydrated.dictionary.acceptance, dictionary.acceptance);
    assert_eq!(hydrated.codebook.references, codebook.references);
    assert_eq!(hydrated.codebook.graphemes, codebook.graphemes);
    assert_eq!(
        hydrated.codebook.dictionary_entries,
        codebook.dictionary_entries
    );
    assert_eq!(
        hydrated.dictionary.term_for("subject_old"),
        Some("urn:synthetic:subject")
    );
    assert_eq!(
        hydrated.dictionary.glyphs.glyph_for_signature(
            "urn:synthetic:add",
            super::super::SIGIL_MATH,
            Some("infix"),
            Some(2)
        ),
        Some("+")
    );
    assert!(
        hydrated
            .dictionary
            .glyphs
            .glyph_for_signature(
                "urn:synthetic:add",
                super::super::SIGIL_MATH,
                Some("prefix"),
                Some(1)
            )
            .is_none()
    );
    assert!(hydrated.dictionary.acceptance.accepts(5));
    assert!(!hydrated.dictionary.acceptance.accepts(4));
    assert!(!hydrated.dictionary.acceptance.accepts(8));
    assert_eq!(
        encode(
            hydrated.dictionary(),
            hydrated.codebook(),
            hydrated.source_sha256(),
            hydrated.source_blake3()
        )
        .unwrap(),
        bytes
    );
    let mut boundary = tiny().0;
    boundary.acceptance.latest_major = u32::MAX;
    boundary.acceptance.accept_window = u32::MAX;
    let boundary_bytes = encode(&boundary, &codebook, hydrated.source_sha256(), &expected).unwrap();
    let boundary = decode(&boundary_bytes, &expected).unwrap();
    assert_eq!(boundary.dictionary.acceptance.latest_major, u32::MAX);
    assert_eq!(boundary.dictionary.acceptance.accept_window, u32::MAX);

    // Keep accepted well-formed CBOR containers; producer spelling remains definite.
    let original = &bytes[HEADER_BYTES..];
    assert_eq!(original[0], 0xa8);
    let mut long_head = vec![0xb8, 8];
    long_head.extend_from_slice(&original[1..]);
    let mut indefinite = vec![0xbf];
    indefinite.extend_from_slice(&original[1..]);
    indefinite.push(0xff);
    for payload in [long_head, indefinite] {
        let hydrated = decode(&packet_payload(&payload), &expected).unwrap();
        assert_eq!(hydrated.dictionary.term_to_alias, dictionary.term_to_alias);
    }
}

#[test]
fn native_transport_rejects_wrong_identity_tamper_and_inconsistent_tables() {
    let (bytes, expected) = tiny_packet();
    assert!(decode(&bytes, &"0".repeat(64)).is_err());
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(decode(&corrupt, &expected).is_err());
    for mutation in 0..8 {
        let mut value = payload(&bytes);
        match mutation {
            0 => {
                let rows = field(field(&mut value, "dictionary"), "term_to_alias")
                    .as_array_mut()
                    .unwrap();
                rows.push(rows[0].clone());
            }
            1 => {
                field(field(&mut value, "dictionary"), "term_to_glyph")
                    .as_array_mut()
                    .unwrap()[0]
                    .as_array_mut()
                    .unwrap()[3] = Value::from(3_u32)
            }
            2 => {
                field(field(&mut value, "dictionary"), "fallback_to_term")
                    .as_array_mut()
                    .unwrap()[0]
                    .as_array_mut()
                    .unwrap()[1] = Value::from("subject")
            }
            3 => *field(&mut value, "profile") = Value::from("different-native-profile"),
            4 => *field(&mut value, "schema_version") = Value::from(2_u32),
            5 => *field(&mut value, "source_path") = Value::from("different/source.ttl"),
            6 => *field(&mut value, "source_sha256") = Value::from("malformed"),
            _ => *field(&mut value, "codebook_digest") = Value::from("different-codebook"),
        }
        assert!(decode(&packet(&value), &expected).is_err());
    }
    // Every named-record boundary refuses ambiguity, omission and alternate shapes.
    for path in [
        vec![],
        vec!["codebook"],
        vec!["dictionary"],
        vec!["dictionary", "acceptance"],
    ] {
        for mutation in 0..5 {
            let mut value = payload(&bytes);
            let mut record = &mut value;
            for name in &path {
                record = field(record, name);
            }
            if mutation == 0 {
                *record = Value::Array(
                    record
                        .as_map()
                        .unwrap()
                        .iter()
                        .map(|(_, value)| value.clone())
                        .collect(),
                );
            } else {
                let fields = record.as_map_mut().unwrap();
                match mutation {
                    1 => fields.push((Value::from("unknown"), Value::Null)),
                    2 => fields.push(fields[0].clone()),
                    3 => {
                        fields.remove(0);
                    }
                    _ => {
                        fields[0].0 =
                            Value::Bytes(fields[0].0.as_text().unwrap().as_bytes().to_vec())
                    }
                }
            }
            assert!(
                decode(&packet(&value), &expected).is_err(),
                "record {path:?}, mutation {mutation}"
            );
        }
    }
    for table in ["term_to_alias", "term_to_glyph"] {
        for extra in [false, true] {
            let mut value = payload(&bytes);
            let row = field(field(&mut value, "dictionary"), table)
                .as_array_mut()
                .unwrap()[0]
                .as_array_mut()
                .unwrap();
            if extra {
                row.push(Value::Null);
            } else {
                row.pop().unwrap();
            }
            assert!(
                decode(&packet(&value), &expected).is_err(),
                "exact width for {table}"
            );
        }
    }
    for mutation in 0..8 {
        let mut value = payload(&bytes);
        let row = field(field(&mut value, "dictionary"), "term_to_glyph")
            .as_array_mut()
            .unwrap()[0]
            .as_array_mut()
            .unwrap();
        match mutation {
            0 => row[0] = Value::Bool(false),
            1 => row[3] = Value::from(u64::from(u32::MAX) + 1),
            2 => row[3] = Value::from(-1_i32),
            3 => row[3] = Value::Float(2.0),
            4 => row[2] = Value::Tag(0, Box::new(row[2].clone())),
            5 => row[3] = Value::Tag(2, Box::new(Value::Bytes(vec![2]))),
            6 => row[2] = Value::Bytes(b"infix".to_vec()),
            _ => row[3] = Value::Bool(true),
        }
        assert!(
            decode(&packet(&value), &expected).is_err(),
            "typed binding cells, mutation {mutation}"
        );
    }
    // Undefined must not become an absent signature on an otherwise valid bare row.
    let (reference, reference_identity) = reference_packet();
    for index in [2, 3] {
        let mut value = payload(&reference);
        let row = field(field(&mut value, "dictionary"), "term_to_glyph")
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row.as_array().unwrap()[2].is_null())
            .unwrap()
            .as_array_mut()
            .unwrap();
        row[index] = Value::Simple(23);
        assert!(
            decode(&packet(&value), &reference_identity).is_err(),
            "undefined is not null"
        );
    }
    // Empty byte strings must not hydrate legitimate empty table arrays.
    let decoded = decode(&bytes, &expected).unwrap();
    let (mut empty_dictionary, mut empty_codebook) = tiny();
    empty_dictionary.term_to_alias.clear();
    empty_dictionary.alias_to_term.clear();
    empty_codebook.dictionary_entries.clear();
    let empty = encode(
        &empty_dictionary,
        &empty_codebook,
        decoded.source_sha256(),
        &expected,
    )
    .unwrap();
    assert!(decode(&empty, &expected).is_ok());
    for path in [
        vec!["codebook", "dictionary_entries"],
        vec!["dictionary", "term_to_alias"],
    ] {
        let mut value = payload(&empty);
        *field(field(&mut value, path[0]), path[1]) = Value::Bytes(vec![]);
        assert!(
            decode(&packet(&value), &expected).is_err(),
            "only real arrays for {path:?}"
        );
    }
    for invalid in [
        Value::from(-1_i32),
        Value::from(u64::from(u32::MAX) + 1),
        Value::Float(7.0),
    ] {
        let mut value = payload(&bytes);
        *field(
            field(field(&mut value, "dictionary"), "acceptance"),
            "latest_major",
        ) = invalid;
        assert!(
            decode(&packet(&value), &expected).is_err(),
            "u32 acceptance domain"
        );
    }
    let tagged = Value::Tag(0, Box::new(payload(&bytes)));
    assert!(
        decode(&packet(&tagged), &expected).is_err(),
        "packet must be an untagged named map"
    );
}

#[test]
fn native_transport_rejects_trailing_and_oversized_packets() {
    let (mut bytes, expected) = tiny_packet();
    assert!(decode(&bytes[..HEADER_BYTES - 1], &expected).is_err());
    // Retain a valid integrity checksum: the decoder must reject the extra CBOR
    // value itself, independently of the outer corruption check.
    bytes.push(0);
    let digest = Sha256::digest(&bytes[HEADER_BYTES..]);
    bytes[MAGIC.len()..HEADER_BYTES].copy_from_slice(&digest);
    assert!(decode(&bytes, &expected).is_err());
    let truncated_item = packet_payload(&[0x81]);
    let error = decode(&truncated_item, &expected).unwrap_err();
    assert!(
        error.is::<cbor::DecodeError>(),
        "retain the typed native decoder cause"
    );
    bytes.resize(MAX_NATIVE_CODEBOOK_BYTES + 1, 0);
    assert!(decode(&bytes, &expected).is_err());
}

#[test]
fn browser_source_identity_pin_matches_original_bytes_without_compilation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes =
        std::fs::read(root.join(SOURCE_PATH)).expect("original source bytes for pure digest check");
    assert_eq!(
        purrdf_hash::hex::encode(purrdf_hash::blake3::hash(&bytes).as_bytes()).as_str(),
        SOURCE_BLAKE3
    );
}

fn payload(bytes: &[u8]) -> Value {
    cbor::decode(&bytes[HEADER_BYTES..], cbor::Limits { max_depth: 32 }).unwrap()
}

fn field<'a>(value: &'a mut Value, name: &str) -> &'a mut Value {
    &mut value
        .as_map_mut()
        .unwrap()
        .iter_mut()
        .find(|(key, _)| key.as_text() == Some(name))
        .unwrap()
        .1
}

fn packet(value: &Value) -> Vec<u8> {
    packet_payload(&cbor::encode(value))
}

fn packet_payload(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; HEADER_BYTES];
    bytes.extend_from_slice(payload);
    let digest = Sha256::digest(payload);
    bytes[..MAGIC.len()].copy_from_slice(MAGIC);
    bytes[MAGIC.len()..HEADER_BYTES].copy_from_slice(&digest);
    bytes
}
