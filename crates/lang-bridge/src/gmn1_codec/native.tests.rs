// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

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
    let expected = blake3::hash(source).to_hex().to_string();
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
}

#[test]
fn native_transport_rejects_wrong_identity_tamper_and_inconsistent_tables() {
    let (bytes, expected) = tiny_packet();
    assert!(decode(&bytes, &"0".repeat(64)).is_err());
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(decode(&corrupt, &expected).is_err());
    for mutation in 0..8 {
        let mut wire: Wire = ciborium::de::from_reader(&bytes[HEADER_BYTES..]).unwrap();
        match mutation {
            0 => wire
                .dictionary
                .term_to_alias
                .push(wire.dictionary.term_to_alias[0].clone()),
            1 => wire.dictionary.term_to_glyph[0].3 = Some(3),
            2 => wire.dictionary.fallback_to_term[0].1 = "subject".into(),
            3 => wire.profile = "different-native-profile".into(),
            4 => wire.schema_version += 1,
            5 => wire.source_path = "different/source.ttl".into(),
            6 => wire.source_sha256 = "malformed".into(),
            _ => wire.codebook_digest = "different-codebook".into(),
        }
        assert!(decode(&packet(&wire).unwrap(), &expected).is_err());
    }
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
    bytes.resize(MAX_NATIVE_CODEBOOK_BYTES + 1, 0);
    assert!(decode(&bytes, &expected).is_err());
}

#[test]
fn browser_source_identity_pin_matches_original_bytes_without_compilation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes =
        std::fs::read(root.join(SOURCE_PATH)).expect("original source bytes for pure digest check");
    assert_eq!(blake3::hash(&bytes).to_hex().as_str(), SOURCE_BLAKE3);
}
