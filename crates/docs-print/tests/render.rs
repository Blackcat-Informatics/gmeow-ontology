// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the deterministic Typst renderer and in-memory PDF
//! compiler. All tests run over a SMALL hand-built fixture model (a few terms,
//! one slice) so the whole suite compiles fonts once and stays well under the
//! 25 s per-test budget.

use std::collections::BTreeMap;

use docs_print::{compile_pdf, embedded_font_digest, render_typ};
use gmeow_docs::formats::{DocFormat, format_capabilities};
use gmeow_docs::model::{
    DocCompetency, DocLinkage, DocSlice, DocTerm, DocTermCategory, DocsModel, ReasoningVerdict,
};

/// The per-format capability partitions for every format (the loss appendix
/// reads the PDF row).
fn losses() -> Vec<gmeow_docs::formats::FormatCapabilities> {
    DocFormat::ALL
        .into_iter()
        .map(format_capabilities)
        .collect()
}

/// One demo slice.
fn demo_slice() -> DocSlice {
    DocSlice {
        iri: "https://blackcatinformatics.ca/gmeow/slice/demo".to_string(),
        label: Some("Demo".to_string()),
        title: Some("Demo slice".to_string()),
        tier: None,
        identifier: None,
        creators: Vec::new(),
        consumers: Vec::new(),
        profiles: Vec::new(),
        depends_on: Vec::new(),
        artifacts: Vec::new(),
    }
}

fn term(iri: &str, curie: &str, label: &str, def: &str, cat: DocTermCategory) -> DocTerm {
    DocTerm {
        iri: iri.to_string(),
        curie: curie.to_string(),
        label: Some(label.to_string()),
        definition: Some(def.to_string()),
        category: cat,
        owner_slice: "https://blackcatinformatics.ca/gmeow/slice/demo".to_string(),
        ..Default::default()
    }
}

/// A small, deterministic fixture model: one slice, three terms, one competency
/// question, one gUFO linkage, and a consistency verdict.
fn fixture_model() -> DocsModel {
    let mut cls_term = term(
        "https://blackcatinformatics.ca/gmeow/Foo",
        "gmeow:Foo",
        "Foo",
        "A foundational demonstration class.",
        DocTermCategory::Class,
    );
    cls_term.parents = vec!["gufo:Object".to_string()];
    cls_term.use_when = vec!["When you need a demo class.".to_string()];
    cls_term.examples = vec!["ex:a a gmeow:Foo .".to_string()];

    let mut prop_term = term(
        "https://blackcatinformatics.ca/gmeow/hasValue",
        "gmeow:hasValue",
        "hasValue",
        "Relates a Foo to a value.",
        DocTermCategory::Property,
    );
    prop_term.domain = vec!["gmeow:Foo".to_string()];
    prop_term.range = vec!["http://www.w3.org/2001/XMLSchema#string".to_string()];

    let indiv_term = term(
        "https://blackcatinformatics.ca/gmeow/Baz",
        "gmeow:Baz",
        "Baz",
        "An individual of the demo.",
        DocTermCategory::Individual,
    );

    let competency = DocCompetency {
        iri: "https://blackcatinformatics.ca/gmeow/cq/demo".to_string(),
        rationale: Some("Can a demo Foo be found?".to_string()),
        query_file: Some("demo.rq".to_string()),
        exercises: vec!["https://blackcatinformatics.ca/gmeow/Foo".to_string()],
        owner_slice: "https://blackcatinformatics.ca/gmeow/slice/demo".to_string(),
        ..Default::default()
    };

    let linkage = DocLinkage {
        mapping_set: None,
        subject: "https://blackcatinformatics.ca/gmeow/Foo".to_string(),
        subject_curie: "gmeow:Foo".to_string(),
        predicate: "http://www.w3.org/2004/02/skos/core#closeMatch".to_string(),
        object: "http://purl.org/nemo/gufo#Object".to_string(),
        justification: None,
        confidence: Some(0.9),
        owner_slice: "https://blackcatinformatics.ca/gmeow/slice/demo".to_string(),
    };

    DocsModel {
        title: "GMEOW Demo Documentation".to_string(),
        version: "test-1".to_string(),
        slices: vec![demo_slice()],
        terms: vec![cls_term, prop_term, indiv_term],
        competencies: vec![competency],
        linkages: vec![linkage],
        reasoning: Some(ReasoningVerdict {
            is_consistent: true,
            unsatisfiable: Default::default(),
        }),
        ..Default::default()
    }
}

/// A small axiom map for the axiom-listing chapter.
fn fixture_axioms() -> BTreeMap<String, Vec<u8>> {
    let mut m = BTreeMap::new();
    m.insert(
        "core".to_string(),
        b"gmeow:Foo rdfs:subClassOf gufo:Object .".to_vec(),
    );
    m
}

/// A minimal valid BibTeX database.
fn fixture_bib() -> Vec<u8> {
    b"@article{gmeow2026,\n  title = {The GMEOW Ontology},\n  author = {Audley, Patrick},\n  year = {2026},\n  journal = {Journal of Ontology},\n}\n".to_vec()
}

#[test]
fn render_typ_is_golden() {
    let model = fixture_model();
    let typ = render_typ(&model, &fixture_axioms(), &fixture_bib(), &losses());
    insta::assert_snapshot!(typ);
}

#[test]
fn every_section_marker_present_and_metrics_carry_real_term_count() {
    let model = fixture_model();
    let typ = render_typ(&model, &fixture_axioms(), &fixture_bib(), &losses());
    for marker in [
        "<<section:metrics>>",
        "<<section:methodology>>",
        "<<section:fair>>",
        "<<section:loss-appendix>>",
        "<<section:comparison-gufo>>",
        "<<section:comparison-bfo>>",
        "<<section:comparison-dolce>>",
        "<<section:pipeline-dag>>",
    ] {
        assert!(typ.contains(marker), "missing marker {marker}");
    }
    // The FAIR gate name is cited literally.
    assert!(typ.contains("meta:gate-fair-metadata"));
    // The metrics section carries the fixture's real term count (3 terms). The
    // metric row emits it as a display string.
    assert!(typ.contains("\"Terms\""), "metrics table missing Terms row");
    assert!(
        typ.contains(&format!("\"{}\"", model.terms.len())),
        "metrics term count {} not present",
        model.terms.len()
    );
}

#[test]
fn compile_pdf_is_reproducible_and_timestamp_free() {
    let model = fixture_model();
    let typ = render_typ(&model, &fixture_axioms(), &fixture_bib(), &losses());
    let bib = fixture_bib();
    let a = compile_pdf(&typ, &bib).expect("compile ok");
    let b = compile_pdf(&typ, &bib).expect("compile ok");

    assert!(a.starts_with(b"%PDF"), "not a PDF");
    assert!(
        !contains(&a, b"/CreationDate"),
        "PDF must carry no /CreationDate"
    );
    assert!(!contains(&a, b"/ModDate"), "PDF must carry no /ModDate");
    assert_eq!(a, b, "compile_pdf must be byte-reproducible");
}

#[test]
fn adversarial_metacharacters_compile_cleanly() {
    // A term whose label and definition are pure Typst metacharacters. If the
    // escape authority missed any, the Typst compile would fail here.
    let mut nasty = term(
        "https://blackcatinformatics.ca/gmeow/Nasty",
        "gmeow:Nasty",
        "# $ @ _ * < ` [x] = tricky",
        "Definition with # $ @ _ * < > ` backticks, [brackets], = signs, and \\ backslashes.",
        DocTermCategory::Class,
    );
    nasty.examples = vec!["#let x = $a_b^c$ // comment".to_string()];
    nasty.use_when = vec!["Use *when* `code` and $math$ appear.".to_string()];

    let model = DocsModel {
        title: "Adversarial # $ @ Doc".to_string(),
        version: "0.0.0-*_<`".to_string(),
        slices: vec![demo_slice()],
        terms: vec![nasty],
        ..Default::default()
    };
    let mut axioms = BTreeMap::new();
    axioms.insert(
        "# nasty @ name".to_string(),
        b"contains ``` triple backticks and $dollars$".to_vec(),
    );

    let typ = render_typ(&model, &axioms, &fixture_bib(), &losses());
    let pdf = compile_pdf(&typ, &fixture_bib())
        .expect("adversarial term text must compile without Typst errors");
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn empty_bib_still_produces_a_valid_pdf() {
    let model = fixture_model();
    let typ = render_typ(&model, &fixture_axioms(), &[], &losses());
    // The bibliography section is omitted entirely for an empty database.
    assert!(!typ.contains("#bibliography("));
    let pdf = compile_pdf(&typ, &[]).expect("empty-bib compile ok");
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn embedded_font_digest_is_pinned() {
    // The exact BLAKE3 over the sorted embedded typst-assets font bytes. Bless
    // once; a change here means the embedded font set changed.
    assert_eq!(
        embedded_font_digest(),
        "d3a5dca598dc43a03b81913cfe790d7efe69fc9883a40c4941d65c008d343e0e",
    );
}

/// Naive substring search over bytes (no external dep).
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
