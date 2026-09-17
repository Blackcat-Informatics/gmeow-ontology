// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_lang_form::{Form, Slot, SurfaceForm, dedup_by_content_key};

/// A leaf lexeme form for a slot.
fn lexeme(lemma: &str) -> Form {
    Form::Lexeme {
        sign_system: "english".to_owned(),
        lemma: lemma.to_owned(),
        part_of_speech: None,
    }
}

/// One slot at `index` filling in `form`.
fn slot(index: u32, form: Form) -> Slot {
    Slot {
        index,
        role: None,
        dep_relation: None,
        depends_on: None,
        form,
    }
}

/// The two co-resident readings of "saw her duck": reading A analyses `duck` as
/// the bird (a nominal head), reading B as the crouch action (a verbal head).
/// Same surface, DISTINCT analysis strings → distinct `Composed` forms.
fn saw_her_duck_readings() -> (Reading, Reading) {
    let composed = |analysis: &str, head_lemma: &str| Form::Composed {
        sign_system: "english".to_owned(),
        level: "clause".to_owned(),
        analysis: Some(analysis.to_owned()),
        head: Some(2),
        slots: vec![
            slot(0, lexeme("see")),
            slot(1, lexeme("her")),
            slot(2, lexeme(head_lemma)),
        ],
    };
    let reading_a = Reading {
        analysis: "duck-as-bird".to_owned(),
        form: composed("duck-as-bird", "duck.n"),
        vantage: "http://example.org/lang/engineVantage".to_owned(),
        confidence: 0.55,
        denotation_context: "http://example.org/lang/ctx".to_owned(),
    };
    let reading_b = Reading {
        analysis: "duck-as-crouch".to_owned(),
        form: composed("duck-as-crouch", "duck.v"),
        vantage: "http://example.org/lang/engineVantage".to_owned(),
        confidence: 0.45,
        denotation_context: "http://example.org/lang/ctx".to_owned(),
    };
    (reading_a, reading_b)
}

/// A surface engine that never provides provenance — the counter-capability the
/// emitter must refuse rather than fabricate attribution for.
struct UnattributingEngine;
impl NlpEngine for UnattributingEngine {
    fn name(&self) -> &str {
        "unattributing"
    }
    fn version(&self) -> &str {
        "0"
    }
    fn provides_provenance(&self) -> bool {
        false
    }
    fn interpret(&self, _surface: &SurfaceForm) -> Result<Vec<Reading>, EngineError> {
        Err(EngineError::InterpretFailed {
            engine: "unattributing".to_owned(),
            reason: "no provenance".to_owned(),
        })
    }
}

fn saw_her_duck_surface() -> SurfaceForm {
    SurfaceForm {
        text: "saw her duck".to_owned(),
        script: "Latn".to_owned(),
        encoding: "UTF-8".to_owned(),
        normalization: "NFC".to_owned(),
        collation: "en".to_owned(),
    }
}

/// The registry hard-fails naming an unregistered engine — never a silent skip.
#[test]
fn registry_hard_fails_on_absent_engine() {
    let registry = EngineRegistry::new();
    let err = match registry.get("absent") {
        Ok(_) => panic!("an unregistered engine name must be a hard fail, not Ok"),
        Err(e) => e,
    };
    assert_eq!(err, EngineError::UnregisteredEngine("absent".to_owned()));
    assert!(
        err.to_string()
            .contains("no NLP engine registered under name 'absent'")
    );
}

/// A registered engine resolves through the seam and reports its single-site name.
#[test]
fn registry_resolves_a_registered_engine() {
    let (a, b) = saw_her_duck_readings();
    let mut registry = EngineRegistry::new();
    registry.register(
        "fixture-ud",
        Box::new(FixtureEngine::new("fixture-ud", "1.0", vec![a, b])),
    );
    let engine = registry
        .get("fixture-ud")
        .expect("registered engine resolves");
    assert_eq!(engine.name(), "fixture-ud");
    assert!(engine.provides_provenance());
    assert_eq!(registry.names(), vec!["fixture-ud"]);
}

/// R6 / Gate 5 — the ambiguity-survival invariant at the identity layer: the two
/// co-resident readings build DISTINCT `Composed` forms (distinct `content_key`),
/// `dedup_by_content_key` keeps BOTH, and both vantages and denotation contexts
/// are retained — no stage collapses the co-resident readings.
#[test]
fn co_resident_readings_survive_dedup_by_content_key() {
    let (a, b) = saw_her_duck_readings();

    // Distinct analyses → distinct content keys (the interner never merges them).
    assert_ne!(
        a.form.content_key(),
        b.form.content_key(),
        "co-resident readings with distinct analyses must key distinctly"
    );

    let mut forms = vec![a.form.clone(), b.form.clone()];
    dedup_by_content_key(&mut forms);
    assert_eq!(
        forms.len(),
        2,
        "dedup_by_content_key must keep BOTH co-resident readings, not collapse them"
    );

    // Both readings are held from a vantage and anchored in a denotation context.
    for reading in [&a, &b] {
        assert!(!reading.vantage.is_empty(), "every reading is vantage-held");
        assert!(
            !reading.denotation_context.is_empty(),
            "every reading names its denotation context"
        );
    }
}

/// The engine interprets the surface into two co-resident readings, and the
/// projection emits BOTH — with both vantages intact — as a `lang:InterpretationAct`.
#[test]
fn interpret_and_emit_keeps_both_readings_attributed() {
    let (a, b) = saw_her_duck_readings();
    let engine = FixtureEngine::new("fixture-ud", "1.0", vec![a, b]);
    let readings = engine
        .interpret(&saw_her_duck_surface())
        .expect("fixture engine interprets");
    assert_eq!(readings.len(), 2, "both co-resident readings are returned");

    let act = "http://example.org/lang/act/saw-her-duck";
    let surface = "http://example.org/lang/surface/saw-her-duck";
    let bytes = interpretation_act_to_ntriples(act, &engine, surface, &readings)
        .expect("attributing engine emits");
    let ntriples = String::from_utf8(bytes).expect("N-Triples is UTF-8");

    // The act is a lang:InterpretationAct and a gmeow:Activity, attributed to the
    // engine + version, interpreting the surface.
    assert!(ntriples.contains(&format!(
        "<{act}> <{RDF_TYPE}> <{LANG_NS}InterpretationAct> ."
    )));
    assert!(ntriples.contains(&format!("<{act}> <{RDF_TYPE}> <{GMEOW_NS}Activity> .")));
    assert!(ntriples.contains("fixture-ud 1.0"));

    // BOTH readings survive to the projection, each producing exactly one
    // producedReading edge, and NO resolvedReading is emitted (no silent winner).
    assert_eq!(
        ntriples
            .matches(&format!("<{LANG_NS}producedReading>"))
            .count(),
        2,
        "both co-resident readings must survive to the projection"
    );
    assert!(
        !ntriples.contains(&format!("<{LANG_NS}resolvedReading>")),
        "the engine seam must never pick a silent winner"
    );
}

/// R8 — the emitter's output is vantage-held corpus data: every produced reading
/// carries a `gmeow:vantage` (never an unattributed assertion) and no reading is
/// resolved to a canonical winner. The two facts together are what let the emission
/// live safely in a non-EDB corpus graph rather than a reasoned input.
#[test]
fn r8_engine_output_is_vantage_held_corpus_data() {
    let (a, b) = saw_her_duck_readings();
    let engine = FixtureEngine::new("fixture-ud", "1.0", vec![a.clone(), b.clone()]);
    let act = "http://example.org/lang/act/saw-her-duck";
    let surface = "http://example.org/lang/surface/saw-her-duck";
    let ntriples = String::from_utf8(
        interpretation_act_to_ntriples(act, &engine, surface, &[a, b]).expect("emits"),
    )
    .expect("UTF-8");

    // One gmeow:vantage per produced reading: no reading is an unattributed fact.
    let produced = ntriples
        .matches(&format!("<{LANG_NS}producedReading>"))
        .count();
    let vantages = ntriples.matches(&format!("<{GMEOW_NS}vantage>")).count();
    assert_eq!(
        produced, vantages,
        "every produced reading must carry a gmeow:vantage (no unattributed structure)"
    );
    assert!(
        !ntriples.contains(&format!("<{LANG_NS}resolvedReading>")),
        "corpus emission never asserts a resolved winner"
    );
}

/// The emitter HARD-FAILS on an engine that cannot attribute its output rather than
/// fabricating attribution — provenance is a queried capability.
#[test]
fn emit_hard_fails_on_unattributing_engine() {
    let (a, b) = saw_her_duck_readings();
    let err = interpretation_act_to_ntriples(
        "http://example.org/lang/act/x",
        &UnattributingEngine,
        "http://example.org/lang/surface/x",
        &[a, b],
    )
    .expect_err("an engine with no provenance must not be emitted as vantage-held readings");
    assert_eq!(
        err,
        EngineError::UnattributableEngine("unattributing".to_owned())
    );
}

/// The projection is deterministic: two runs over the same readings are
/// byte-identical (the project-wide determinism bar).
#[test]
fn emission_is_byte_deterministic() {
    let (a, b) = saw_her_duck_readings();
    let engine = FixtureEngine::new("fixture-ud", "1.0", vec![]);
    let act = "http://example.org/lang/act/saw-her-duck";
    let surface = "http://example.org/lang/surface/saw-her-duck";
    let first = interpretation_act_to_ntriples(act, &engine, surface, &[a.clone(), b.clone()])
        .expect("emits");
    let second = interpretation_act_to_ntriples(act, &engine, surface, &[a, b]).expect("emits");
    assert_eq!(first, second, "two runs must be byte-identical");
}
