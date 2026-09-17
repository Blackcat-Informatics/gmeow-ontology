// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::is_exact_correspondence;

const DOC: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .

ex:sent a lang:ComposedForm ;
    rdfs:label "cats chase mice" ;
    lang:formHead ex:wChase ;
    lang:formSlot ex:s0 , ex:s1 , ex:s2 .
ex:s0 a lang:FormSlot ; lang:slotIndex 0 ; lang:slotForm ex:wCats .
ex:s1 a lang:FormSlot ; lang:slotIndex 1 ; lang:slotForm ex:wChase .
ex:s2 a lang:FormSlot ; lang:slotIndex 2 ; lang:slotForm ex:wMice .
ex:wCats  rdfs:label "cats" .
ex:wChase rdfs:label "chase" .
ex:wMice  rdfs:label "mice" .
"#;

fn source() -> NamedSource {
    NamedSource {
        name: "doc".to_owned(),
        bytes: DOC.as_bytes().to_vec(),
    }
}

#[test]
fn composed_form_emits_faithful_tei_fragment() {
    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let emissions = TeiBridge.emit(&input).expect("emit");
    assert_eq!(emissions.len(), 1, "one TEI document per composed form");
    let e = &emissions[0];
    let xml = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();

    // The faithful fragment: the tokens in slot order, the analyzed head marked.
    assert!(xml.contains("<w n=\"0\">cats</w>"), "{xml}");
    assert!(
        xml.contains("<w n=\"1\" function=\"head\">chase</w>"),
        "{xml}"
    );
    assert!(xml.contains("<w n=\"2\">mice</w>"), "{xml}");
    assert!(xml.contains("<title>cats chase mice</title>"));
    assert!(e.artifacts[0].path_suffix.ends_with(".tei.xml"));
    assert!(!e.artifacts[0].is_rdf);

    // Honest preservation: the carried correspondence is NEVER exact; SoundUnder.
    assert!(!is_exact_correspondence(&e.correspondence));
    assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
    assert_eq!(e.ledger[0].preservation, PreservationKind::SoundUnder);

    // Every dropped stratum is enumerated (denotation, readings, preservation, vantage).
    let joined = e.unsupported.join("\n");
    assert!(joined.contains("lang:Denotation"), "{joined}");
    assert!(joined.contains("lang:Reading"), "{joined}");
    assert!(joined.contains("preservation"), "{joined}");
    assert!(joined.contains("vantage"), "{joined}");
}

#[test]
fn emitter_is_byte_reproducible() {
    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let a = TeiBridge.emit(&input).expect("a");
    let b = TeiBridge.emit(&input).expect("b");
    assert_eq!(a[0].artifacts[0].bytes, b[0].artifacts[0].bytes);
}

#[test]
fn non_integer_slot_index_hard_fails() {
    let bad = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:sent a lang:ComposedForm ; rdfs:label "x" ; lang:formSlot ex:s0 .
ex:s0 a lang:FormSlot ; lang:slotIndex "oops" ; lang:slotForm ex:w0 .
ex:w0 rdfs:label "x" .
"#;
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "bad".to_owned(),
            bytes: bad.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let err = TeiBridge
        .emit(&input)
        .expect_err("non-integer slot index must hard-fail");
    assert!(err.construct.contains("not an integer"), "{err:?}");
}
