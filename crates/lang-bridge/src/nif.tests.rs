// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::is_exact_correspondence;

/// A surface anchor over "café" — é is one NFC codepoint but two UTF-8 bytes, so the same
/// [0,4) span addresses different characters under codepoint vs byte offsets.
const CAFE: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex:   <http://example.org/lang/> .

ex:surf a lang:SurfaceForm ;
    lang:surfaceText "café bar" ;
    lang:unicodeNormalization "NFC" ;
    lang:surfaceAnchor ex:anc .
ex:anc a lang:SurfaceAnchor ;
    lang:anchorSource ex:doc ;
    lang:anchorStart 0 ;
    lang:anchorEnd 4 ;
    lang:offsetSpace lang:codepointOffset .
"#;

fn source() -> NamedSource {
    NamedSource {
        name: "cafe".to_owned(),
        bytes: CAFE.as_bytes().to_vec(),
    }
}

#[test]
fn anchor_emits_nif_and_web_annotation() {
    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let emissions = NifBridge.emit(&input).expect("emit");
    assert_eq!(emissions.len(), 1);
    let e = &emissions[0];
    assert_eq!(
        e.artifacts.len(),
        2,
        "NIF string + Web-Annotation companion"
    );

    let nif = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();
    assert!(e.artifacts[0].is_rdf, "NIF is RDF");
    assert!(nif.contains("beginIndex> \"0\""), "{nif}");
    assert!(nif.contains("endIndex> \"4\""), "{nif}");
    assert!(
        nif.contains("referenceContext> <http://example.org/lang/doc>"),
        "{nif}"
    );
    // The DECLARED binding: which offset space + normalization the offsets assume.
    assert!(
        nif.contains("offsetSpace> <https://blackcatinformatics.ca/lang/codepointOffset>"),
        "{nif}"
    );
    assert!(nif.contains("unicodeNormalization> \"NFC\""), "{nif}");

    let anno = String::from_utf8(e.artifacts[1].bytes.clone()).unwrap();
    assert!(!e.artifacts[1].is_rdf);
    assert!(anno.contains("\"TextPositionSelector\""), "{anno}");
    assert!(anno.contains("\"start\": 0"), "{anno}");
    assert!(anno.contains("\"end\": 4"), "{anno}");

    // Honest preservation: never exact; SoundUnder.
    assert!(!is_exact_correspondence(&e.correspondence));
    assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
}

#[test]
fn anchor_of_is_the_selected_substring_not_the_whole_text() {
    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let e = &NifBridge.emit(&input).expect("emit")[0];
    let nif = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();
    // "café bar" sliced to [0,4) codepoints is "café" — anchorOf is the SELECTED substring,
    // never the whole surface text.
    assert!(nif.contains("anchorOf> \"café\" ."), "{nif}");
    assert!(
        !nif.contains("café bar"),
        "anchorOf must be the anchored substring, not the whole surface text: {nif}"
    );
    let anno = String::from_utf8(e.artifacts[1].bytes.clone()).unwrap();
    assert!(anno.contains("\"exact\": \"café\""), "{anno}");
    assert!(!anno.contains("café bar"), "{anno}");
}

#[test]
fn inverted_span_hard_fails() {
    let bad = CAFE
        .replace("lang:anchorStart 0", "lang:anchorStart 3")
        .replace("lang:anchorEnd 4", "lang:anchorEnd 2");
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "bad".to_owned(),
            bytes: bad.into_bytes(),
        }],
        ..Default::default()
    };
    let err = NifBridge
        .emit(&input)
        .expect_err("an inverted span must hard-fail");
    assert!(err.construct.contains("inverted"), "{err:?}");
}

#[test]
fn out_of_range_span_hard_fails() {
    let bad = CAFE.replace("lang:anchorEnd 4", "lang:anchorEnd 99");
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "oob".to_owned(),
            bytes: bad.into_bytes(),
        }],
        ..Default::default()
    };
    let err = NifBridge
        .emit(&input)
        .expect_err("an out-of-range span must hard-fail");
    assert!(err.construct.contains("exceeds"), "{err:?}");
}

#[test]
fn emitter_is_byte_reproducible() {
    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let a = NifBridge.emit(&input).expect("a");
    let b = NifBridge.emit(&input).expect("b");
    for i in 0..a[0].artifacts.len() {
        assert_eq!(a[0].artifacts[i].bytes, b[0].artifacts[i].bytes);
    }
}

/// The offset-fragility DISCLOSING test: emit the anchor, then re-encode the surface (switch
/// the offset space from codepoints to bytes) and show the SAME [0,4) span no longer
/// addresses "café" — and ASSERT the ledger row discloses exactly this re-encoding
/// invalidation.
#[test]
fn offsets_are_fragile_under_reencoding_and_the_ledger_discloses_it() {
    let text = "café bar";
    // Under the anchor's declared codepoint offset space, [0,4) addresses "café".
    let by_codepoint: String = text.chars().take(4).collect();
    assert_eq!(by_codepoint, "café");
    // Re-encode to a BYTE offset space (a different encoding of the same text): [0,4) now
    // addresses only the first 4 bytes — the ASCII `c`, `a`, `f` plus the first byte of é's
    // 2-byte sequence, which is not even a whole character. The offsets no longer address the
    // same span.
    let by_byte = String::from_utf8_lossy(&text.as_bytes()[0..4]);
    assert_ne!(
        by_byte, by_codepoint,
        "re-encoding codepoint→byte must shift the span the offsets address"
    );

    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let e = &NifBridge.emit(&input).expect("emit")[0];

    // The ledger row DISCLOSES the fragility (residue mentions re-encoding invalidation),
    // read back from the emission's loss store by the row target.
    let residue = e.loss.projection_drops_for(&e.ledger[0].target).join("\n");
    assert!(residue.contains("offset fragility"), "{residue}");
    assert!(
        residue.contains("re-encoding") || residue.contains("re-normalizing"),
        "the ledger must disclose that re-encoding invalidates the offsets: {residue}"
    );
    assert!(residue.contains("codepoint↔byte"), "{residue}");
    // And it names WHICH surface + normalization the offsets assume.
    assert!(residue.contains("unicodeNormalization=NFC"), "{residue}");
}
