// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::is_exact_correspondence;

const LEXICON: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .

ex:lexCat a lang:Lexeme ; rdfs:label "cat" ; lang:partOfSpeech lang:noun .
ex:senseCat a lang:Sense ; rdfs:label "the animal sense of 'cat'" ; lang:senseOf ex:lexCat .
ex:wfCats a lang:WordForm ; rdfs:label "cats" ; lang:inflectionOf ex:lexCat ;
    lang:morphFeature ex:featPlur .
ex:featPlur a lang:MorphFeature ; lang:featureKey lang:featNumber ; lang:featureValue lang:valPlur .
"#;

fn source() -> NamedSource {
    NamedSource {
        name: "lex".to_owned(),
        bytes: LEXICON.as_bytes().to_vec(),
    }
}

#[test]
fn lexeme_inventory_projects_forward_to_ontolex() {
    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let emissions = OntoLexTarget.emit(&input).expect("emit");
    assert_eq!(
        emissions.len(),
        1,
        "one lexicon emission for a lexeme-bearing surface"
    );
    let e = &emissions[0];
    let ttl = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();

    // Faithful form/sense/reference structure: entry, canonical form + writtenRep, sense.
    assert!(
        ttl.contains(&format!("<{ONTOLEX_NS}LexicalEntry>")),
        "{ttl}"
    );
    assert!(
        ttl.contains(&format!("<{ONTOLEX_NS}canonicalForm>")),
        "{ttl}"
    );
    assert!(
        ttl.contains(&format!("<{ONTOLEX_NS}writtenRep> \"cat\"")),
        "{ttl}"
    );
    assert!(ttl.contains(&format!("<{ONTOLEX_NS}otherForm>")), "{ttl}");
    assert!(
        ttl.contains(&format!("<{ONTOLEX_NS}writtenRep> \"cats\"")),
        "{ttl}"
    );
    assert!(
        ttl.contains(&format!("<{ONTOLEX_NS}LexicalSense>")),
        "{ttl}"
    );
    // The gloss the ingest lift shed is carried FORWARD as skos:definition.
    assert!(
        ttl.contains(&format!(
            "<{SKOS_NS}definition> \"the animal sense of 'cat'\""
        )),
        "{ttl}"
    );
    // UD-aligned features lowered to lexinfo: POS + number/plural.
    assert!(
        ttl.contains(&format!("<{LEXINFO_NS}partOfSpeech> <{LEXINFO_NS}noun>")),
        "{ttl}"
    );
    assert!(
        ttl.contains(&format!("<{LEXINFO_NS}number> <{LEXINFO_NS}plural>")),
        "{ttl}"
    );
    assert!(e.artifacts[0].is_rdf);
    assert!(e.artifacts[0].path_suffix.starts_with("ontolex-lemon/"));

    // Honest preservation: never exact; SoundUnder with the flattened epistemic strata named.
    assert!(!is_exact_correspondence(&e.correspondence));
    assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
    let joined = e.unsupported.join("\n");
    assert!(joined.contains("vantage"), "{joined}");
    assert!(joined.contains("lang:InterpretationAct"), "{joined}");
}

#[test]
fn unmapped_pos_is_carried_verbatim_and_flagged() {
    let src = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/lang/> .
ex:lexOnom a lang:Lexeme ; rdfs:label "meow" ; lang:partOfSpeech lang:onomatopoeia .
"#;
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "x".to_owned(),
            bytes: src.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    let e = &OntoLexTarget.emit(&input).expect("emit")[0];
    let ttl = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();
    // No lexinfo POS invented; the lang: POS is carried verbatim and flagged as residue.
    assert!(
        ttl.contains(&format!("<{LANG_NS}partOfSpeech> <{LANG_NS}onomatopoeia>")),
        "{ttl}"
    );
    assert!(
        e.unsupported
            .iter()
            .any(|r| r.contains("no lexinfo class mapping")),
        "{:?}",
        e.unsupported
    );
}

#[test]
fn no_lexeme_surface_does_not_emit() {
    let src = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex:   <http://example.org/lang/> .
ex:sys a lang:SignSystem .
"#;
    let input = LangProjectionInput {
        lang_models: vec![NamedSource {
            name: "nolex".to_owned(),
            bytes: src.as_bytes().to_vec(),
        }],
        ..Default::default()
    };
    assert!(OntoLexTarget.emit(&input).expect("emit").is_empty());
}

#[test]
fn emitter_is_byte_reproducible() {
    let input = LangProjectionInput {
        lang_models: vec![source()],
        ..Default::default()
    };
    let a = OntoLexTarget.emit(&input).expect("a");
    let b = OntoLexTarget.emit(&input).expect("b");
    assert_eq!(a[0].artifacts[0].bytes, b[0].artifacts[0].bytes);
}
