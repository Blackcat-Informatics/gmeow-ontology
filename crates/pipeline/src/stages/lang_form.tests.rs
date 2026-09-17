// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn english_binding() -> CarrierBinding {
    CarrierBinding {
        script_local: "latinScript".to_string(),
        language_iri: iri(LANG_NS, "gmeowEnglish"),
    }
}

#[test]
fn prose_hash_coincides_with_the_obligations_gate() {
    for text in ["A definition prose field.", "café", "", "   "] {
        let prose = build_prose(text, &english_binding()).expect("build prose");
        assert_eq!(prose.source_hash, candidate_source_hash(text));
    }
}

#[test]
fn prose_hash_resolves_for_both_nfc_and_nfd() {
    let nfc = "caf\u{e9}"; // codespell:ignore caf
    let nfd = "cafe\u{301}";
    let p_nfc = build_prose(nfc, &english_binding()).expect("nfc");
    let p_nfd = build_prose(nfd, &english_binding()).expect("nfd");
    assert_ne!(p_nfc.surface_iri, p_nfd.surface_iri);
    assert_eq!(p_nfc.source_hash, candidate_source_hash(nfc));
    assert_eq!(p_nfd.source_hash, candidate_source_hash(nfd));
    assert_eq!(p_nfc.normalization, "NFC");
    assert_eq!(p_nfd.normalization, "NFD");
}

#[test]
fn document_scale_surface_holds_bytes_by_reference() {
    let long = "x".repeat(DOCUMENT_SCALE_BYTES + 1);
    let nt = String::from_utf8(emit_ntriples(&[
        build_prose(&long, &english_binding()).expect("long prose")
    ]))
    .expect("utf8");
    assert!(nt.contains(&surface_blob_digest(&long)));
    assert!(!nt.contains(&iri(LANG_NS, "surfaceText")));
    assert!(!nt.contains(&long));
}

#[test]
fn binding_lookup_hard_fails_unknown_without_discovery() {
    let bindings = BTreeMap::from([("x-gmeow-english".to_string(), english_binding())]);
    assert_eq!(
        binding_for_tag("x-gmeow-english", &bindings)
            .expect("known binding")
            .script_local,
        "latinScript"
    );
    let err = binding_for_tag("qtz", &bindings).expect_err("unknown tag must fail");
    assert!(format!("{err}").contains("no lang:Script binding"));
}
