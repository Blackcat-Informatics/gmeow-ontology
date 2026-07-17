// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN-1 codec's fixture corpus — the executed byte witness behind
//! `gmeow:gmnCorrNormalToGmn`'s `logic:mnemomorphic true` claim.
//!
//! Every fixture here is a real [`round_trip_check`] over a [`Gmn0Model`]: write, read,
//! canonically compare via `purrdf::canonicalize`. The corpus is deliberately NOT a
//! trivially small fragment — it exercises every record form and factored slot the
//! charter names, PLUS a real fragment of each grounding slice's authored `module.ttl`,
//! so the "total over grounding" claim is proven against real content, not only
//! hand-built toy triples.

use std::sync::Arc;

use gmeow_lang_bridge::{
    Gmn0Model, Gmn1Document, Gmn1Error, GmnDictionary, gmn0_canonically_equal, gmn1_read,
    gmn1_write, gmn1_write_tabular, round_trip_check,
};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, parse_dataset};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

fn lang_module_dataset() -> Arc<RdfDataset> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../slices/grounding/lang/module.ttl"
    );
    let bytes = std::fs::read(path).expect("lang module.ttl is readable");
    parse_dataset(&bytes, "text/turtle", None).expect("lang module.ttl parses")
}

fn dict() -> GmnDictionary {
    GmnDictionary::from_dataset(&lang_module_dataset()).expect("dict-v3 loads from the carrier")
}

/// Load and parse one grounding slice's authored `module.ttl`.
fn grounding_module_dataset(slice: &str) -> Arc<RdfDataset> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding")
        .join(slice)
        .join("module.ttl");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    parse_dataset(&bytes, "text/turtle", None).unwrap_or_else(|e| panic!("parse {path:?}: {e}"))
}

// ── 1. A single claim record ─────────────────────────────────────────────────────────

#[test]
fn single_claim_record_round_trips() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}gate1"));
    let p = b.intern_iri(&format!("{GMEOW}hasState"));
    let o = b.intern_iri(&format!("{GMEOW}doorGate1"));
    b.push_quad(s, p, o, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    round_trip_check(&model, &dict()).expect("single claim record round-trips");

    let doc = gmn1_write(&model, &dict()).expect("write");
    assert!(
        doc.text
            .starts_with("@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n")
    );
    assert!(
        doc.text.contains("@c{"),
        "must emit one @c record: {}",
        doc.text
    );
}

// ── 2. Tabular batch AND equivalent individual records: two surfaces, one GMN-0 ────

#[test]
fn tabular_and_record_surfaces_canonicalize_to_the_same_gmn0() {
    let mut b = RdfDatasetBuilder::new();
    for (subj, obj) in [("gate1", "yardNorth"), ("gate2", "yardSouth")] {
        let s = b.intern_iri(&format!("{GMEOW}{subj}"));
        let p = b.intern_iri(&format!("{GMEOW}locatedIn"));
        let o = b.intern_iri(&format!("{GMEOW}{obj}"));
        b.push_quad(s, p, o, None);
    }
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    let d = dict();

    let tabular_doc = gmn1_write_tabular(&model, &d).expect("tabular write");
    assert!(
        tabular_doc.text.contains("@claims["),
        "uniform @c records must batch as a tabular surface: {}",
        tabular_doc.text
    );
    let record_doc = gmn1_write(&model, &d).expect("record write");
    assert!(!record_doc.text.contains("@claims["));

    let from_tabular = gmn1_read(&tabular_doc, &d).expect("read tabular");
    let from_record = gmn1_read(&record_doc, &d).expect("read record");
    assert!(gmn0_canonically_equal(&from_tabular, &model));
    assert!(gmn0_canonically_equal(&from_record, &model));
    assert!(
        gmn0_canonically_equal(&from_tabular, &from_record),
        "tabular and record surfaces must decode to the SAME GMN-0 model"
    );
}

// ── 3. o-vs-v slot split: IRI object vs literal-value object ───────────────────────

#[test]
fn iri_object_and_literal_value_both_round_trip() {
    let mut b = RdfDatasetBuilder::new();
    let s1 = b.intern_iri(&format!("{GMEOW}gate1"));
    let p1 = b.intern_iri(&format!("{GMEOW}hasState"));
    let o1 = b.intern_iri(&format!("{GMEOW}doorGate1"));
    b.push_quad(s1, p1, o1, None);

    let s2 = b.intern_iri(&format!("{GMEOW}gate1"));
    let p2 = b.intern_iri(&format!("{GMEOW}statusLabel"));
    let lit = b.intern_literal(RdfLiteral::typed("open", XSD_STRING));
    b.push_quad(s2, p2, lit, None);

    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    round_trip_check(&model, &dict()).expect("o/v split round-trips");

    let doc = gmn1_write(&model, &dict()).expect("write");
    assert!(
        doc.text.contains("o: "),
        "must use the o slot for an IRI object: {}",
        doc.text
    );
    assert!(
        doc.text.contains("v: "),
        "must use the v slot for a literal value: {}",
        doc.text
    );
}

// ── 4. rdf:langString / language-tagged literals ────────────────────────────────────

#[test]
fn lang_tagged_literal_round_trips_the_language_tag() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}term1"));
    let p = b.intern_iri("http://www.w3.org/2004/02/skos/core#definition");
    let lit = b.intern_literal(RdfLiteral::language_tagged(
        "a prose definition with spaces, punctuation, and \"quotes\"",
        "x-gmeow-english",
    ));
    b.push_quad(s, p, lit, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    round_trip_check(&model, &dict()).expect("langString round-trips");

    // The language tag must NOT be silently dropped: the reconstructed model must
    // canonically differ from a same-text literal carrying a DIFFERENT language tag.
    let mut b2 = RdfDatasetBuilder::new();
    let s2 = b2.intern_iri(&format!("{GMEOW}term1"));
    let p2 = b2.intern_iri("http://www.w3.org/2004/02/skos/core#definition");
    let lit2 = b2.intern_literal(RdfLiteral::language_tagged(
        "a prose definition with spaces, punctuation, and \"quotes\"",
        "fr",
    ));
    b2.push_quad(s2, p2, lit2, None);
    let other_lang_model = Gmn0Model::from_dataset(&b2.freeze().expect("freeze"));
    assert!(
        !gmn0_canonically_equal(&model, &other_lang_model),
        "sanity: two literals differing only in language tag must be distinct models"
    );

    let doc = gmn1_write(&model, &dict()).expect("write");
    let back = gmn1_read(&doc, &dict()).expect("read");
    assert!(
        gmn0_canonically_equal(&back, &model),
        "the language tag must round-trip byte-exactly, never silently dropped"
    );
}

// ── 5. n-ary predication reification — round-tripped, not hard-failed ──────────────
//
// Per LOGIC-IR.md: a fixed-arity n-ary predication `op(a, b, c)` reifies to ordinary
// binary triples over a content-addressed reifier node — `logic:instanceOf(R, Rel)` plus
// `logic:naryArg0(R, a)`, `logic:naryArg1(R, b)`, … This is ALREADY inside this codec's
// covered fragment: it is nothing but a subject (the reifier node R, an ordinary IRI or
// blank node) carrying several ordinary triples, each with >1 primary predicate per
// subject so the safe-fold guard emits them as flat logic (`@ℒ`) records — no special-casing
// needed. This fixture proves the round-trip explicitly rather than asserting it.

#[test]
fn nary_predication_reification_round_trips() {
    let mut b = RdfDatasetBuilder::new();
    let reifier = b.intern_iri(&format!("{GMEOW}naryReifier1"));
    let instance_of = b.intern_iri(&format!("{LOGIC}instanceOf"));
    let rel = b.intern_iri(&format!("{GMEOW}betweenRelation"));
    b.push_quad(reifier, instance_of, rel, None);
    for (i, arg) in ["a1", "a2", "a3"].iter().enumerate() {
        let p = b.intern_iri(&format!("{LOGIC}naryArg{i}"));
        let o = b.intern_iri(&format!("{GMEOW}{arg}"));
        b.push_quad(reifier, p, o, None);
    }
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    round_trip_check(&model, &dict())
        .expect("content-addressed n-ary reification round-trips losslessly");

    let doc = gmn1_write(&model, &dict()).expect("write");
    // Four distinct predicates share the reifier subject, so the safe-fold guard (>1
    // primary triple per subject) emits four flat @ℒ records — none silently dropped.
    let claim_count = doc.text.lines().filter(|l| l.starts_with("@ℒ{")).count();
    assert_eq!(
        claim_count, 4,
        "all four reifier triples must round-trip: {}",
        doc.text
    );
}

// ── 6. Absence-preservation: every optional slot absent stays absent ───────────────

#[test]
fn absent_optional_slots_round_trip_to_absence_not_a_default() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}gate1"));
    let p = b.intern_iri(&format!("{GMEOW}hasState"));
    let o = b.intern_iri(&format!("{GMEOW}doorGate1"));
    b.push_quad(s, p, o, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);

    let doc = gmn1_write(&model, &dict()).expect("write");
    // Skip the `@gmn{v: 1, ...}` header line: its schema-version field is also spelled
    // `v`, which is unrelated to the record-level value slot this assertion targets.
    let record_lines: String = doc
        .text
        .lines()
        .filter(|l| !l.starts_with("@gmn{"))
        .collect::<Vec<_>>()
        .join("\n");
    for absent in ["v:", "q:", "st:", "ev:", "m:", "ek:", "bd:", "it:"] {
        assert!(
            !record_lines.contains(absent),
            "no synthesized default for an absent slot: found {absent} in {record_lines}"
        );
    }
    let back = gmn1_read(&doc, &dict()).expect("read");
    assert!(gmn0_canonically_equal(&back, &model));
    // Exactly one quad — no confidence/standpoint/evidence quad was ever synthesized.
    assert_eq!(back.quads.len(), 1);
}

// ── 7. An evidence + standpoint record (st + ev populated together) ────────────────

#[test]
fn evidence_and_standpoint_record_round_trips() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}gate1"));
    let p = b.intern_iri(&format!("{GMEOW}hasState"));
    let o = b.intern_iri(&format!("{GMEOW}doorGate1"));
    b.push_quad(s, p, o, None);

    let according_to = b.intern_iri(&format!("{GMEOW}accordingTo"));
    let standpoint = b.intern_iri(&format!("{GMEOW}sensorCrew"));
    b.push_quad(s, according_to, standpoint, None);

    let evidence_pred = b.intern_iri(&format!("{GMEOW}hasAvailableEvidence"));
    let evidence = b.intern_iri(&format!("{GMEOW}e12"));
    b.push_quad(s, evidence_pred, evidence, None);

    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    round_trip_check(&model, &dict()).expect("evidence+standpoint record round-trips");

    let doc = gmn1_write(&model, &dict()).expect("write");
    assert!(
        doc.text.contains("st: "),
        "must fold accordingTo into st: {}",
        doc.text
    );
    assert!(
        doc.text.contains("ev: "),
        "must fold evidence into ev: {}",
        doc.text
    );
    // Folded into ONE compact record, not three flat triples.
    let claim_count = doc.text.lines().filter(|l| l.starts_with("@c{")).count();
    assert_eq!(
        claim_count, 1,
        "st/ev must fold into the host record: {}",
        doc.text
    );
}

// ── 8. A @p process record exercising the factored aspect slots (bd, it) ───────────

#[test]
fn process_record_with_boundary_and_iteration_round_trips() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}gate1"));
    let p = b.intern_iri(&format!("{GMEOW}cycling"));
    let o = b.intern_iri(&format!("{GMEOW}doorGate1"));
    b.push_quad(s, p, o, None);

    let boundary_pred = b.intern_iri(&format!("{LOGIC}occurrentBoundary"));
    let open = b.intern_iri(&format!("{LOGIC}Open"));
    b.push_quad(s, boundary_pred, open, None);

    let series_pred = b.intern_iri(&format!("{GMEOW}occurrenceOfSeries"));
    let series = b.intern_iri(&format!("{GMEOW}cycleSeries1"));
    b.push_quad(s, series_pred, series, None);

    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    round_trip_check(&model, &dict()).expect("@p record with bd/it round-trips");

    let doc = gmn1_write(&model, &dict()).expect("write");
    assert!(
        doc.text.contains("@p{"),
        "must use the @p sigil: {}",
        doc.text
    );
    assert!(
        doc.text.contains("bd: open"),
        "bd must use the dict-v3 alias: {}",
        doc.text
    );
    assert!(
        doc.text.contains("it: "),
        "must carry the it slot: {}",
        doc.text
    );
}

// ── 9a. A by-reference confidence (higher precision than the 2-digit assertion rule) ─

#[test]
fn high_precision_confidence_rides_by_reference() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}gate1"));
    let p = b.intern_iri(&format!("{GMEOW}hasState"));
    let o = b.intern_iri(&format!("{GMEOW}doorGate1"));
    b.push_quad(s, p, o, None);

    let confidence_pred = b.intern_iri(&format!("{GMEOW}confidence"));
    // A DERIVED confidence (product t-norm of two 2-digit values): 4 fractional digits,
    // outside the grammar's exactly-2-digit assertion rule — must ride by reference.
    let confidence = b.intern_literal(RdfLiteral::typed(
        "0.9025",
        "http://www.w3.org/2001/XMLSchema#decimal",
    ));
    b.push_quad(s, confidence_pred, confidence, None);

    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    round_trip_check(&model, &dict()).expect("high-precision confidence round-trips by reference");

    let doc = gmn1_write(&model, &dict()).expect("write");
    assert!(
        doc.text.contains("q: r_"),
        "a >2-digit confidence must ride by reference: {}",
        doc.text
    );
    assert!(
        !doc.text.contains("0.9025"),
        "the raw high-precision digits must never be inlined in the record text: {}",
        doc.text
    );
}

// ── 9b. A by-reference annotation (prose guidance text, not identifier-shaped) ─────

#[test]
fn prose_annotation_rides_by_reference() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}HowToUseExample"));
    let p = b.intern_iri(&format!("{GMEOW}howToUse"));
    let lit = b.intern_literal(RdfLiteral::typed(
        "Mint one entry per aliased term; changelog and provenance notes ride by reference.",
        XSD_STRING,
    ));
    b.push_quad(s, p, lit, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    round_trip_check(&model, &dict()).expect("prose annotation round-trips by reference");

    let doc = gmn1_write(&model, &dict()).expect("write");
    assert!(
        doc.text.contains("v: r_"),
        "prose must ride by reference, never inlined: {}",
        doc.text
    );
    assert!(
        !doc.text.contains("Mint one entry"),
        "raw prose must never appear in record text"
    );
}

// ── 10. Each new Task-5 qualifier slot at least once (m, ek, bd, it) ───────────────

#[test]
fn every_task5_qualifier_slot_round_trips() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}gate1"));
    let p = b.intern_iri(&format!("{GMEOW}hasState"));
    let o = b.intern_iri(&format!("{GMEOW}doorGate1"));
    b.push_quad(s, p, o, None);
    let modal_pred = b.intern_iri(&format!("{GMEOW}claimModalForce"));
    let modal_val = b.intern_iri(&format!("{GMEOW}modalForcePossible"));
    b.push_quad(s, modal_pred, modal_val, None);
    let ek_pred = b.intern_iri(&format!("{GMEOW}observationMethod"));
    let ek_val = b.intern_iri(&format!("{GMEOW}methodInstrumentalReading"));
    b.push_quad(s, ek_pred, ek_val, None);

    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    round_trip_check(&model, &dict()).expect("m/ek slots round-trip");
    let doc = gmn1_write(&model, &dict()).expect("write");
    assert!(
        doc.text.contains("m: poss"),
        "m must use the dict-v3 alias: {}",
        doc.text
    );
    assert!(
        doc.text.contains("ek: inst"),
        "ek must use the dict-v3 alias: {}",
        doc.text
    );

    // bd/it are exercised by `process_record_with_boundary_and_iteration_round_trips`.
}

// ── 11. Real content from the grounding slices: logic, lang, math ──────────────────

#[test]
fn real_lang_module_round_trips() {
    let ds = lang_module_dataset();
    let model = Gmn0Model::from_dataset(&ds);
    assert!(
        model.quads.len() > 1000,
        "sanity: real content, not a trivial fragment"
    );
    match round_trip_check(&model, &dict()) {
        Ok(()) => {}
        Err(e) => panic!("real lang: module.ttl must round-trip losslessly: {e}"),
    }
}

#[test]
fn real_logic_module_round_trips() {
    let ds = grounding_module_dataset("logic");
    let model = Gmn0Model::from_dataset(&ds);
    assert!(
        model.quads.len() > 1000,
        "sanity: real content, not a trivial fragment"
    );
    match round_trip_check(&model, &dict()) {
        Ok(()) => {}
        Err(e) => panic!("real logic: module.ttl must round-trip losslessly: {e}"),
    }
}

/// Round-trip EVERY authored `examples/*.ttl` fixture of a grounding slice — the
/// `axisGmn1Coverage` axis's own definition ("every construct the slice's module/
/// examples emit") scopes coverage to module.ttl PLUS examples, not module.ttl alone.
fn round_trip_every_example(slice: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding")
        .join(slice)
        .join("examples");
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}")) {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("ttl") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let ds = parse_dataset(&bytes, "text/turtle", None)
            .unwrap_or_else(|e| panic!("parse {path:?}: {e}"));
        let model = Gmn0Model::from_dataset(&ds);
        if let Err(e) = round_trip_check(&model, &dict()) {
            panic!("{slice} example {path:?} must round-trip losslessly: {e}");
        }
        checked += 1;
    }
    assert!(
        checked > 0,
        "expected at least one examples/*.ttl fixture for {slice}"
    );
}

#[test]
fn real_logic_examples_round_trip() {
    round_trip_every_example("logic");
}

#[test]
fn real_lang_examples_round_trip() {
    round_trip_every_example("lang");
}

#[test]
fn real_math_examples_round_trip() {
    round_trip_every_example("math");
}

#[test]
fn real_math_module_round_trips() {
    let ds = grounding_module_dataset("math");
    let model = Gmn0Model::from_dataset(&ds);
    assert!(
        model.quads.len() > 500,
        "sanity: real content, not a trivial fragment"
    );
    match round_trip_check(&model, &dict()) {
        Ok(()) => {}
        Err(e) => panic!("real math: module.ttl must round-trip losslessly: {e}"),
    }
}

// ── RDF 1.2 triple terms + reifiers round-trip losslessly (RDF-star native) ─────────

#[test]
fn quoted_triple_term_object_round_trips_losslessly() {
    let mut b = RdfDatasetBuilder::new();
    // A quoted RDF 1.2 triple term as OBJECT — first-class model content the codec encodes
    // losslessly as the compact `( s p o )` surface (RDF 1.2 triple terms occur only as an
    // object; subjects are rejected by the dataset builder itself).
    let s = b.intern_iri(&format!("{GMEOW}someAgent"));
    let p = b.intern_iri(&format!("{GMEOW}asserts"));
    let ta = b.intern_iri(&format!("{GMEOW}a"));
    let tb = b.intern_iri(&format!("{GMEOW}b"));
    let tc = b.intern_iri(&format!("{GMEOW}c"));
    let o = b.intern_triple(ta, tb, tc);
    b.push_quad(s, p, o, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    let dictionary = dict();

    // Primary assertion: the round-trip is canonically exact.
    round_trip_check(&model, &dictionary).expect("a triple-term object must round-trip losslessly");

    // The surface actually carries the `( s p o )` spelling over three nested terms.
    let document = gmn1_write(&model, &dictionary).expect("triple-term object writes");
    assert!(
        document.text.contains("o: ( gmeow__a gmeow__b gmeow__c )"),
        "the triple term must render as the compact `( s p o )` object surface:\n{}",
        document.text
    );

    // Reconstruct and compare canonically, then assert idempotence (write → read → write
    // reproduces the SAME bytes).
    let reconstructed = gmn1_read(&document, &dictionary).expect("triple-term object reads back");
    assert!(
        gmn0_canonically_equal(&model, &reconstructed),
        "reconstructed model must be canonically equal to the original"
    );
    let rewritten = gmn1_write(&reconstructed, &dictionary).expect("rewrite");
    assert_eq!(
        document.text, rewritten.text,
        "gmn1_read then gmn1_write must reproduce the same bytes (idempotence)"
    );
}

#[test]
fn reifier_with_folded_annotations_round_trips_losslessly() {
    let mut b = RdfDatasetBuilder::new();
    // The canonical RDF 1.2 reifier: `x rdf:reifies <<( s p o )>>`, with folded `st`/`ev`
    // annotations on the same reifier subject.
    let reifier = b.intern_iri(&format!("{GMEOW}reifier1"));
    let reifies = b.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies");
    let ts = b.intern_iri(&format!("{GMEOW}doorGate1"));
    let tp = b.intern_iri(&format!("{GMEOW}hasState"));
    let to = b.intern_iri(&format!("{LOGIC}Open"));
    let triple = b.intern_triple(ts, tp, to);
    b.push_quad(reifier, reifies, triple, None);

    // Folded annotations: `accordingTo` → st slot, `hasAvailableEvidence` → ev slot.
    let according_to = b.intern_iri(&format!("{GMEOW}accordingTo"));
    let standpoint = b.intern_iri(&format!("{GMEOW}sensorStandpoint"));
    b.push_quad(reifier, according_to, standpoint, None);
    let evidence = b.intern_iri(&format!("{GMEOW}hasAvailableEvidence"));
    let evidence_span = b.intern_iri(&format!("{GMEOW}span42"));
    b.push_quad(reifier, evidence, evidence_span, None);

    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    let dictionary = dict();

    round_trip_check(&model, &dictionary)
        .expect("a reifier + triple term + folded annotations must round-trip losslessly");

    let document = gmn1_write(&model, &dictionary).expect("reifier writes");
    // The reifier subject, its `rdf:reifies` triple-term object, and BOTH folded annotations
    // fold into ONE record (the `st` and `ev` slots ride beside the primary `o` triple term).
    assert!(
        document.text.contains("rdf__reifies")
            && document
                .text
                .contains("o: ( gmeow__doorGate1 gmeow__hasState ")
            && document.text.contains("st: ")
            && document.text.contains("ev: "),
        "the reifier must fold its triple-term object and annotations into one record:\n{}",
        document.text
    );

    let reconstructed = gmn1_read(&document, &dictionary).expect("reifier reads back");
    assert!(
        gmn0_canonically_equal(&model, &reconstructed),
        "the reconstructed reifier model must be canonically equal to the original"
    );
    let rewritten = gmn1_write(&reconstructed, &dictionary).expect("rewrite");
    assert_eq!(
        document.text, rewritten.text,
        "reifier round-trip must be byte-idempotent"
    );
}

#[test]
fn nested_triple_term_object_round_trips_losslessly() {
    let mut b = RdfDatasetBuilder::new();
    // A nested triple term as object: `s p <<( a q <<( x y z )>> )>>`.
    let inner_x = b.intern_iri(&format!("{GMEOW}x"));
    let inner_y = b.intern_iri(&format!("{GMEOW}y"));
    let inner_z = b.intern_iri(&format!("{GMEOW}z"));
    let inner = b.intern_triple(inner_x, inner_y, inner_z);
    let outer_a = b.intern_iri(&format!("{GMEOW}a"));
    let outer_q = b.intern_iri(&format!("{GMEOW}q"));
    let outer = b.intern_triple(outer_a, outer_q, inner);
    let s = b.intern_iri(&format!("{GMEOW}someAgent"));
    let p = b.intern_iri(&format!("{GMEOW}asserts"));
    b.push_quad(s, p, outer, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    let dictionary = dict();

    round_trip_check(&model, &dictionary)
        .expect("a nested triple-term object must round-trip losslessly");
    let document = gmn1_write(&model, &dictionary).expect("nested triple-term writes");
    assert!(
        document
            .text
            .contains("( gmeow__a gmeow__q ( gmeow__x gmeow__y gmeow__z ) )"),
        "nested triple terms must render recursively:\n{}",
        document.text
    );
}

// ── Named-graph domain boundary: a named-graph quad is out-of-domain, not "uncovered" ──

#[test]
fn named_graph_quad_raises_out_of_domain_not_uncovered() {
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}doorGate1"));
    let p = b.intern_iri(&format!("{GMEOW}hasState"));
    let o = b.intern_iri(&format!("{LOGIC}Open"));
    let g = b.intern_iri(&format!("{GMEOW}graphA"));
    b.push_quad(s, p, o, Some(g));
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);

    let err = gmn1_write(&model, &dict()).expect_err("a named-graph quad is out of domain");
    assert!(
        matches!(err, Gmn1Error::NamedGraphOutOfDomain { .. }),
        "a named-graph quad must raise the honest domain-boundary class, not Uncovered: {err:?}"
    );
    assert_eq!(
        err.failure_class(),
        Gmn1Error::CLASS_GRAPH_OUT_OF_DOMAIN,
        "the failure must classify as lang:GmnGraphOutOfDomain"
    );
}

// ── In-band repair records: @err / @patch / @retract (claims-about-claims) ─────────────
//
// Each repair record is a reified NEW claim over a stable TARGET record id, never an
// in-place mutation. The charter surface (`LANG-GMN.md` § "In-band repair") shows a
// blank-node repair subject rendered WITHOUT an `s` slot: the reified claim's own fresh
// identity, minted fresh on read, canonically equal under RDFC-1.0's blank-label
// canonicalization. These fixtures are real `round_trip_check`s over that GMN-0 shape.

/// Parse an inline Turtle fragment (blank-node repair subjects) to a [`Gmn0Model`].
fn model_from_turtle(body: &str) -> Gmn0Model {
    let ttl = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n{body}\n"
    );
    let ds = parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("repair Turtle parses");
    Gmn0Model::from_dataset(&ds)
}

/// The record line (skipping the `@gmn{…}` header) of a single-record document.
fn sole_record_line(doc: &Gmn1Document) -> String {
    doc.text
        .lines()
        .find(|l| l.starts_with('@') && !l.starts_with("@gmn{"))
        .expect("one repair record line")
        .to_owned()
}

#[test]
fn err_repair_record_round_trips_and_renders_id_and_class() {
    let model = model_from_turtle(
        "[] a gmeow:GmnErr ;\n\
         gmeow:gmnRepairId \"c42\" ;\n\
         gmeow:gmnRepairClass lang:GmnMalformedNumber .",
    );
    let d = dict();
    round_trip_check(&model, &d).expect("@err record round-trips losslessly");

    let doc = gmn1_write(&model, &d).expect("write @err");
    let line = sole_record_line(&doc);
    assert!(line.starts_with("@err{"), "must emit an @err sigil: {line}");
    assert!(
        line.contains("id: c42"),
        "must render the target id: {line}"
    );
    assert!(
        line.contains("class: ") && line.contains("GmnMalformedNumber"),
        "must render the failure-class name in the class slot: {line}"
    );
    assert!(
        !line.contains("{s:"),
        "a blank-node repair subject rides no `s` slot: {line}"
    );

    // Reconstruction is canonically the same GMN-0 model.
    let back = gmn1_read(&doc, &d).expect("read @err");
    assert!(gmn0_canonically_equal(&back, &model));
    // The three reconstructed quads: the type, the target id, the failure class.
    assert_eq!(
        back.quads.len(),
        3,
        "exactly type + id + class: {:?}",
        back.quads
    );

    // Read → write idempotence: the surface is a fixed point.
    let doc2 = gmn1_write(&back, &d).expect("re-write @err");
    assert_eq!(
        doc.text, doc2.text,
        "the @err surface is a read→write fixed point"
    );
}

#[test]
fn patch_repair_record_round_trips_and_restates_the_q_slot() {
    let model = model_from_turtle(
        "[] a gmeow:GmnPatch ;\n\
         gmeow:gmnRepairId \"c42\" ;\n\
         gmeow:confidence \"0.95\"^^xsd:decimal .",
    );
    let d = dict();
    round_trip_check(&model, &d).expect("@patch record round-trips losslessly");

    let doc = gmn1_write(&model, &d).expect("write @patch");
    let line = sole_record_line(&doc);
    assert!(
        line.starts_with("@patch{"),
        "must emit a @patch sigil: {line}"
    );
    assert!(
        line.contains("id: c42"),
        "must render the target id: {line}"
    );
    assert!(
        line.contains("q: 0.95"),
        "must restate the confidence in the q slot: {line}"
    );
    // `id` precedes the restated `q`, per the canonical repair key order.
    assert!(
        line.find("id:").unwrap() < line.find("q:").unwrap(),
        "id must precede a restated q: {line}"
    );

    let back = gmn1_read(&doc, &d).expect("read @patch");
    assert!(gmn0_canonically_equal(&back, &model));
    assert_eq!(
        back.quads.len(),
        3,
        "exactly type + id + restated q: {:?}",
        back.quads
    );

    let doc2 = gmn1_write(&back, &d).expect("re-write @patch");
    assert_eq!(
        doc.text, doc2.text,
        "the @patch surface is a read→write fixed point"
    );
}

#[test]
fn retract_repair_record_round_trips_carrying_just_the_id() {
    let model = model_from_turtle("[] a gmeow:GmnRetract ; gmeow:gmnRepairId \"c17\" .");
    let d = dict();
    round_trip_check(&model, &d).expect("@retract record round-trips losslessly");

    let doc = gmn1_write(&model, &d).expect("write @retract");
    let line = sole_record_line(&doc);
    assert_eq!(
        line, "@retract{id: c17}",
        "a @retract renders exactly its target id: {line}"
    );

    let back = gmn1_read(&doc, &d).expect("read @retract");
    assert!(gmn0_canonically_equal(&back, &model));
    assert_eq!(back.quads.len(), 2, "exactly type + id: {:?}", back.quads);

    let doc2 = gmn1_write(&back, &d).expect("re-write @retract");
    assert_eq!(
        doc.text, doc2.text,
        "the @retract surface is a read→write fixed point"
    );
}

#[test]
fn iri_subject_repair_record_preserves_its_identity_via_the_s_slot() {
    // A repair record whose subject is an IRI (as the module.ttl `ex:err` example is)
    // must preserve that identity — it rides the `s` slot, and round-trips exactly.
    let mut b = RdfDatasetBuilder::new();
    let s = b.intern_iri(&format!("{GMEOW}errRecord1"));
    let ty = b.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let err = b.intern_iri(&format!("{GMEOW}GmnErr"));
    b.push_quad(s, ty, err, None);
    let repair_id = b.intern_iri(&format!("{GMEOW}gmnRepairId"));
    let target = b.intern_literal(RdfLiteral::typed("c42", XSD_STRING));
    b.push_quad(s, repair_id, target, None);
    let ds = b.freeze().expect("freeze");
    let model = Gmn0Model::from_dataset(&ds);
    let d = dict();
    round_trip_check(&model, &d).expect("IRI-subject @err round-trips losslessly");

    let doc = gmn1_write(&model, &d).expect("write IRI-subject @err");
    let line = sole_record_line(&doc);
    assert!(line.starts_with("@err{"), "must emit an @err sigil: {line}");
    assert!(
        line.contains("s: gmeow__errRecord1"),
        "an IRI repair subject rides the s slot: {line}"
    );
    assert!(
        line.contains("id: c42"),
        "must render the target id: {line}"
    );
}

#[test]
fn repair_record_missing_its_id_hard_fails_typed() {
    // A hand-authored @err with no `id` — the repair record does not name its target,
    // a HARD FAIL, never a silently subject-less reconstruction.
    let doc = Gmn1Document::from_text(
        "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@err{class: GmnMalformedNumber}\n",
    );
    let err = gmn1_read(&doc, &dict()).expect_err("an @err without id must hard-fail");
    assert!(
        matches!(err, Gmn1Error::NonDecodableGrammar { .. }),
        "a missing repair id is non-decodable grammar: {err:?}"
    );
    assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR);
}

#[test]
fn repair_record_with_a_disallowed_key_hard_fails_typed() {
    // A `class` slot is legal ONLY on an @err; a @patch carrying one would silently drop
    // a quad on reconstruction — a HARD FAIL instead.
    let doc = Gmn1Document::from_text(
        "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@patch{id: c42, class: GmnMalformedNumber}\n",
    );
    let err = gmn1_read(&doc, &dict()).expect_err("a @patch with a class key must hard-fail");
    assert!(
        matches!(err, Gmn1Error::NonDecodableGrammar { .. }),
        "a disallowed repair key is non-decodable grammar: {err:?}"
    );
    assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR);
}
