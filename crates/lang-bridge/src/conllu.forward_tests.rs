// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::is_exact_correspondence;

const SENTENCE: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .

ex:lexCat a lang:Lexeme ; rdfs:label "cat" ; lang:partOfSpeech lang:noun .
ex:lexChase a lang:Lexeme ; rdfs:label "chase" ; lang:partOfSpeech lang:verb .
ex:lexMouse a lang:Lexeme ; rdfs:label "mouse" ; lang:partOfSpeech lang:noun .
ex:featPlur a lang:MorphFeature ; lang:featureKey lang:featNumber ; lang:featureValue lang:valPlur .
ex:featPres a lang:MorphFeature ; lang:featureKey lang:featTense ; lang:featureValue lang:valPres .
ex:wfCats a lang:WordForm ; rdfs:label "cats" ; lang:inflectionOf ex:lexCat ; lang:morphFeature ex:featPlur .
ex:wfChase a lang:WordForm ; rdfs:label "chase" ; lang:inflectionOf ex:lexChase ; lang:morphFeature ex:featPres .
ex:wfMice a lang:WordForm ; rdfs:label "mice" ; lang:inflectionOf ex:lexMouse ; lang:morphFeature ex:featPlur .

ex:analysis a lang:Analysis .
ex:sent a lang:ComposedForm ; rdfs:label "cats chase mice" ; lang:inAnalysis ex:analysis ;
    lang:formHead ex:wfChase ; lang:formSlot ex:s0 , ex:s1 , ex:s2 .
ex:s0 a lang:FormSlot ; lang:inAnalysis ex:analysis ; lang:slotIndex 0 ; lang:slotForm ex:wfCats ;
    lang:slotRole lang:subjectRole ; lang:dependsOn ex:s1 .
ex:s1 a lang:FormSlot ; lang:inAnalysis ex:analysis ; lang:slotIndex 1 ; lang:slotForm ex:wfChase ;
    lang:slotRole lang:predicateRole .
ex:s2 a lang:FormSlot ; lang:inAnalysis ex:analysis ; lang:slotIndex 2 ; lang:slotForm ex:wfMice ;
    lang:slotRole lang:objectRole ; lang:dependsOn ex:s1 .
"#;

fn source() -> NamedSource {
    NamedSource {
        name: "s".to_owned(),
        bytes: SENTENCE.as_bytes().to_vec(),
    }
}

#[test]
fn composed_form_lowers_to_a_ud_tree() {
    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let emissions = ConlluTarget.emit(&input).expect("emit");
    assert_eq!(
        emissions.len(),
        1,
        "one emission per analyzed composed form"
    );
    let e = &emissions[0];
    assert_eq!(e.emitted_reading_count, Some(1));
    assert_eq!(
        e.artifacts.len(),
        1,
        "single reading ⇒ one CoNLL-U artifact"
    );
    let text = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();

    // The UD tree: cats(nsubj→chase) chase(root) mice(obj→chase), features lowered.
    assert!(
        text.contains("1\tcats\tcat\tNOUN\t_\tNumber=Plur\t2\tnsubj\t_\t_"),
        "{text}"
    );
    assert!(
        text.contains("2\tchase\tchase\tVERB\t_\tTense=Pres\t0\troot\t_\t_"),
        "{text}"
    );
    assert!(
        text.contains("3\tmice\tmouse\tNOUN\t_\tNumber=Plur\t2\tobj\t_\t_"),
        "{text}"
    );
    assert!(e.artifacts[0].path_suffix.ends_with(".reading-0.conllu"));

    // Faithful morphosyntax: byte round-trips, so the derived kind is Exact.
    assert!(e.round_trip_holds);
    assert!(is_exact_correspondence(&e.correspondence));
    assert_eq!(e.lossy_kind, PreservationKind::Exact);
}

#[test]
fn two_analyses_emit_two_readings_never_one() {
    // One surface form scoped to TWO co-resident analyses that assign different heads —
    // each analysis emits its own CoNLL-U file; no reading is silently dropped.
    let doc = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:wSaw a lang:WordForm ; rdfs:label "saw" .
ex:wDuck a lang:WordForm ; rdfs:label "duck" .
ex:sent a lang:ComposedForm ; rdfs:label "saw duck" ;
    lang:inAnalysis ex:aBird , ex:aCrouch ;
    lang:formSlot ex:b0 , ex:b1 , ex:c0 , ex:c1 .
ex:aBird a lang:Analysis .
ex:aCrouch a lang:Analysis .
ex:b0 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .
ex:b1 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:objectRole ; lang:dependsOn ex:b0 .
ex:c0 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .
ex:c1 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:complementRole ; lang:dependsOn ex:c0 .
"#;
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "amb".to_owned(),
            bytes: doc.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let e = &ConlluTarget.emit(&input).expect("emit")[0];
    assert_eq!(e.emitted_reading_count, Some(2));
    assert_eq!(
        e.artifacts.len(),
        2,
        "two co-resident analyses ⇒ two CoNLL-U artifacts"
    );
}

#[test]
fn slot_scoped_analyses_are_not_collapsed_without_a_form_level_in_analysis() {
    // The form declares NO lang:inAnalysis; its slots carry the two analysis scopes. The
    // readings must be recovered from the slots' scopes (union), never merged into one.
    let doc = "\
@prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
@prefix ex:   <http://example.org/lang/> .\n\
ex:wSaw a lang:WordForm ; rdfs:label \"saw\" .\n\
ex:wDuck a lang:WordForm ; rdfs:label \"duck\" .\n\
ex:sent a lang:ComposedForm ; rdfs:label \"saw duck\" ; lang:formSlot ex:b0 , ex:b1 , ex:c0 , ex:c1 .\n\
ex:aBird a lang:Analysis .\n\
ex:aCrouch a lang:Analysis .\n\
ex:b0 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .\n\
ex:b1 a lang:FormSlot ; lang:inAnalysis ex:aBird ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:objectRole ; lang:dependsOn ex:b0 .\n\
ex:c0 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 0 ; lang:slotForm ex:wSaw ; lang:slotRole lang:predicateRole .\n\
ex:c1 a lang:FormSlot ; lang:inAnalysis ex:aCrouch ; lang:slotIndex 1 ; lang:slotForm ex:wDuck ; lang:slotRole lang:complementRole ; lang:dependsOn ex:c0 .\n";
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "slot-scoped".to_owned(),
            bytes: doc.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let e = &ConlluTarget.emit(&input).expect("emit")[0];
    assert_eq!(
        e.emitted_reading_count,
        Some(2),
        "the two slot-scoped analyses must be recovered as two readings"
    );
    assert_eq!(
        e.artifacts.len(),
        2,
        "never collapsed into one merged reading"
    );
}

#[test]
fn missing_slot_index_hard_fails() {
    let bad = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:w a lang:WordForm ; rdfs:label "x" .
ex:sent a lang:ComposedForm ; lang:formSlot ex:s0 .
ex:s0 a lang:FormSlot ; lang:slotForm ex:w .
"#;
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "bad".to_owned(),
            bytes: bad.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let err = ConlluTarget
        .emit(&input)
        .expect_err("missing slot index must hard-fail");
    assert!(err.construct.contains("no lang:slotIndex"), "{err:?}");
}

#[test]
fn duplicate_slot_index_hard_fails() {
    // Two slots claim index 0 — ambiguous word order is a hard fail, never a silent pick.
    let dup = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:wA a lang:WordForm ; rdfs:label "a" .
ex:wB a lang:WordForm ; rdfs:label "b" .
ex:sent a lang:ComposedForm ; lang:formSlot ex:s0 , ex:s1 .
ex:s0 a lang:FormSlot ; lang:slotIndex 0 ; lang:slotForm ex:wA .
ex:s1 a lang:FormSlot ; lang:slotIndex 0 ; lang:slotForm ex:wB .
"#;
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "dup".to_owned(),
            bytes: dup.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let err = ConlluTarget
        .emit(&input)
        .expect_err("duplicate slot index must hard-fail");
    assert!(
        err.construct.contains("duplicate lang:slotIndex"),
        "{err:?}"
    );
}

#[test]
fn emitter_is_byte_reproducible() {
    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let a = ConlluTarget.emit(&input).expect("a");
    let b = ConlluTarget.emit(&input).expect("b");
    assert_eq!(a[0].artifacts[0].bytes, b[0].artifacts[0].bytes);
}
