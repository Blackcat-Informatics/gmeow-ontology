// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Requirement 9 — the OntoLex-Lemon lexicon lift. A lexicon is somebody's claim about a
//! language, so its sense inventory is HELD FROM the source lexicon as the `gmeow:vantage`,
//! never folded flat. The fixture is ORIGINAL repo content (no external CC-BY import), so it
//! carries its own SPDX header.

use gmeow_lang_bridge::ontolex::{ontolex_correspondence, OntoLexBridge, ONTOLEX_SIGN_SYSTEM};
use gmeow_lang_bridge::{is_exact_correspondence, Bridge, LangFailure};
use gmeow_lang_form::Form;
use gmeow_logic_compile::ir::{MorphismClass, PreservationKind};

/// The original, self-attributed OntoLex-Lemon lexicon fixture.
const FIXTURE: &[u8] = include_bytes!("fixtures/lexicon.ontolex.ttl");

/// The source lexicon identifier held as the vantage of the sense inventory.
const SOURCE_VANTAGE: &str = "http://example.org/lexicon/en";

fn bridge() -> OntoLexBridge {
    OntoLexBridge {
        source_vantage: SOURCE_VANTAGE.to_owned(),
    }
}

#[test]
fn fixture_lifts_to_lexemes_wordforms_and_senses() {
    let lifted = bridge().lift(FIXTURE).expect("fixture lifts");

    // Two lexemes ("cat", "feline") and one inflected word form ("cats").
    let lexemes: Vec<&Form> = lifted
        .forms
        .iter()
        .filter(|f| matches!(f, Form::Lexeme { .. }))
        .collect();
    let word_forms: Vec<&Form> = lifted
        .forms
        .iter()
        .filter(|f| matches!(f, Form::WordForm { .. }))
        .collect();
    assert_eq!(lexemes.len(), 2, "cat + feline lexemes");
    assert_eq!(word_forms.len(), 1, "the plural other form 'cats'");

    // The "cat" lexeme carries its lemma, its lexinfo part of speech, and the OntoLex sign
    // system.
    let cat = lexemes
        .iter()
        .find_map(|f| match f {
            Form::Lexeme {
                lemma,
                part_of_speech,
                sign_system,
            } if lemma == "cat" => Some((part_of_speech.clone(), sign_system.clone())),
            _ => None,
        })
        .expect("the 'cat' lexeme is lifted");
    assert_eq!(
        cat.0.as_deref(),
        Some("http://www.lexinfo.net/ontology/2.0/lexinfo#noun"),
        "the declared part of speech is the lexicon's own lexinfo IRI",
    );
    assert_eq!(cat.1, ONTOLEX_SIGN_SYSTEM);

    // The word form inflects the "cat" lexeme and carries the typed plural feature.
    let Form::WordForm {
        lexeme, features, ..
    } = word_forms[0]
    else {
        unreachable!("filtered to WordForm");
    };
    match lexeme.as_ref() {
        Form::Lexeme { lemma, .. } => assert_eq!(lemma, "cat"),
        other => panic!("word form must inflect the cat lexeme, got {other:?}"),
    }
    assert!(
        features
            .iter()
            .any(|feat| feat.key == "number" && feat.values.iter().any(|v| v == "plural")),
        "the 'cats' other form carries lexinfo:number=plural, got {features:?}",
    );

    // The two glossed senses are emitted in the N-Triples product (senses are not forms).
    let content = &lifted.ledger[0].content;
    let sense_count = content
        .lines()
        .filter(|l| {
            l.contains("#Sense>") || l.ends_with("<https://blackcatinformatics.ca/lang/Sense> .")
        })
        .count();
    assert_eq!(sense_count, 2, "cat + feline senses, in the emitted RDF");
}

#[test]
fn every_emitted_sense_is_held_from_the_source_vantage() {
    let lifted = bridge().lift(FIXTURE).expect("fixture lifts");
    let content = &lifted.ledger[0].content;

    // Each sense IRI that is typed lang:Sense must carry a gmeow:vantage naming the source
    // lexicon — the sense inventory is the lexicon's perspectival claim, never a flat fact.
    let sense_iris: Vec<&str> = content
        .lines()
        .filter_map(|l| {
            l.strip_suffix(" <https://blackcatinformatics.ca/lang/Sense> .")
                .and_then(|s| s.strip_prefix('<'))
                .and_then(|s| s.split_once("> <"))
                .map(|(iri, _)| iri)
        })
        .collect();
    assert_eq!(sense_iris.len(), 2, "two typed lang:Sense nodes");

    for sense in &sense_iris {
        let vantage_line = format!(
            "<{sense}> <https://blackcatinformatics.ca/gmeow/vantage> <{SOURCE_VANTAGE}> ."
        );
        assert!(
            content.lines().any(|l| l == vantage_line),
            "sense {sense} must be held from the source vantage; missing:\n{vantage_line}\nin:\n{content}",
        );
    }
}

#[test]
fn carried_correspondence_is_an_honest_lossy_lens() {
    let lifted = bridge().lift(FIXTURE).expect("fixture lifts");

    // The lift is a lossy lens over the richer OntoLex source, never an exact isomorphism.
    assert_eq!(
        lifted.correspondence.morphism_class,
        MorphismClass::LossyLens
    );
    assert!(
        !lifted.correspondence.mnemomorphic,
        "the lift sheds the gloss complement, so it does not retain the whole source",
    );
    assert!(
        !is_exact_correspondence(&lifted.correspondence),
        "a lossy lens with an undischarged law is not exact",
    );

    // The preservation is SoundUnder (not Exact) and the dropped glosses are recorded.
    assert_eq!(lifted.ledger.len(), 1);
    assert_eq!(lifted.ledger[0].preservation, PreservationKind::SoundUnder);
    assert_eq!(
        lifted.ledger[0].actual_drops.len(),
        2,
        "both sense glosses are recorded as residue, never silently dropped: {:?}",
        lifted.ledger[0].actual_drops,
    );
    assert!(
        lifted.ledger[0]
            .actual_drops
            .iter()
            .all(|d| d.contains("skos:definition")),
        "each drop names the gloss it shed",
    );
}

#[test]
fn emission_is_byte_deterministic() {
    let a = bridge().lift_to_ntriples(FIXTURE).expect("lifts once");
    let b = bridge().lift_to_ntriples(FIXTURE).expect("lifts twice");
    assert_eq!(a, b, "the same lexicon serializes byte-identically");

    // The Bridge emit path re-renders the same product off the lifted ledger.
    let lifted = bridge().lift(FIXTURE).expect("fixture lifts");
    assert_eq!(
        bridge().emit(&lifted),
        a,
        "emit re-renders the same deterministic N-Triples",
    );
}

#[test]
fn correspondence_iri_is_content_addressed_and_stable() {
    let one = ontolex_correspondence("some-key");
    let two = ontolex_correspondence("some-key");
    assert_eq!(one.iri, two.iri, "the same key addresses the same IRI");
    let other = ontolex_correspondence("other-key");
    assert_ne!(one.iri, other.iri, "distinct keys address distinct IRIs");
}

/// An OntoLex `LexicalEntry` with no `ontolex:canonicalForm` cannot be lifted to a lexeme (the
/// form AST's lemma is mandatory). The lift HARD FAILS naming the exact construct — never a
/// silent drop.
#[test]
fn entry_without_canonical_form_hard_fails_naming_the_construct() {
    let malformed = br#"
@prefix ontolex: <http://www.w3.org/ns/lemon/ontolex#> .
@prefix lexinfo: <http://www.lexinfo.net/ontology/2.0/lexinfo#> .
@prefix ex:      <http://example.org/lexicon/en/> .

ex:dog a ontolex:LexicalEntry ;
    lexinfo:partOfSpeech lexinfo:noun .
"#;
    let err = bridge()
        .lift(malformed)
        .expect_err("an entry with no canonical form must hard-fail");
    assert_eq!(err.failure_class, LangFailure::SilentIngestDrop);
    assert!(
        err.construct.contains("ontolex:canonicalForm")
            && err.construct.contains("http://example.org/lexicon/en/dog"),
        "the diagnostic names the offending entry and construct: {}",
        err.construct,
    );
}

/// A form with two `ontolex:writtenRep` values cannot collapse to a single lemma — a HARD FAIL
/// naming the ambiguous construct rather than silently picking one.
#[test]
fn ambiguous_written_rep_hard_fails() {
    let malformed = br#"
@prefix ontolex: <http://www.w3.org/ns/lemon/ontolex#> .
@prefix ex:      <http://example.org/lexicon/en/> .

ex:ox a ontolex:LexicalEntry ;
    ontolex:canonicalForm ex:ox_form .

ex:ox_form a ontolex:Form ;
    ontolex:writtenRep "ox" ;
    ontolex:writtenRep "oxen" .
"#;
    let err = bridge()
        .lift(malformed)
        .expect_err("two written reps on one form must hard-fail");
    assert_eq!(err.failure_class, LangFailure::SilentIngestDrop);
    assert!(
        err.construct.contains("ontolex:writtenRep"),
        "the diagnostic names the ambiguous written representation: {}",
        err.construct,
    );
}

/// A source with no `ontolex:LexicalEntry` at all is not a lexicon — a HARD FAIL, never an
/// empty success.
#[test]
fn empty_lexicon_hard_fails() {
    let empty = br#"
@prefix ontolex: <http://www.w3.org/ns/lemon/ontolex#> .
@prefix ex:      <http://example.org/lexicon/en/> .

ex:note a ontolex:LexicalConcept .
"#;
    let err = bridge()
        .lift(empty)
        .expect_err("a source with no lexical entries must hard-fail");
    assert!(
        err.construct.contains("ontolex:LexicalEntry"),
        "the diagnostic names the missing construct: {}",
        err.construct,
    );
}

/// Non-UTF-8 input is a typed `lang:NonUtf8Surface` hard fail, never a silent lossy repair.
#[test]
fn non_utf8_input_hard_fails() {
    let bytes = [0xff, 0xfe, 0x00];
    let err = bridge()
        .lift(&bytes)
        .expect_err("non-UTF-8 bytes cannot be a lexicon");
    assert_eq!(err.failure_class, LangFailure::NonUtf8Surface);
}

/// R9 — the off-gate bulk lexicon sweep. Marked `#[ignore]` so it is NOT part of the default
/// `cargo nextest` gate. GIVEN a real OntoLex lexicon via `GMEOW_ONTOLEX_LEXICON`, it lifts the
/// whole file. No bulk lexicon data ships with the repo (that would import third-party lexical
/// content), so an unset env var is a HARD FAIL that tells the maintainer to point it at a
/// checkout — an honest off-gate failure, never a silent skip.
#[test]
#[ignore = "off-gate maint sweep: set GMEOW_ONTOLEX_LEXICON to a real OntoLex .ttl file"]
fn maint_lexicon_extract_sweep() {
    let path = std::env::var("GMEOW_ONTOLEX_LEXICON").unwrap_or_else(|_| {
        panic!(
            "maint_lexicon_extract_sweep requires GMEOW_ONTOLEX_LEXICON set to an OntoLex-Lemon \
             Turtle file path; no bulk lexicon data ships with the repo (it would import \
             third-party lexical content). Point it at a local checkout, e.g. \
             GMEOW_ONTOLEX_LEXICON=/path/to/lexicon.ttl"
        )
    });
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("cannot read GMEOW_ONTOLEX_LEXICON '{path}': {e}"));
    let lifted = bridge()
        .lift(&bytes)
        .unwrap_or_else(|d| panic!("lexicon '{path}' failed to lift: {d:?}"));
    assert!(
        !lifted.forms.is_empty(),
        "a real lexicon must lift at least one form",
    );
}
