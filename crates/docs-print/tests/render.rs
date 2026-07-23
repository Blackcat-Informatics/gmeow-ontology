// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the deterministic Typst renderer and in-memory PDF
//! compiler. All tests run over a SMALL hand-built fixture model (a few terms,
//! one slice) so the suite isolates renderer behavior without rebuilding the
//! whole documentation catalog.

use std::collections::BTreeMap;

use docs_print::{compile_pdf, embedded_font_digest, pdf_text_layer, render_typ};
use gmeow_docs::formats::{DocFormat, format_capabilities};
use gmeow_docs::model::{
    DocCompetency, DocFlowEdge, DocLinkage, DocPipeline, DocSlice, DocStage, DocTerm,
    DocTermCategory, DocsModel, ReasoningVerdict,
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
        documents: Vec::new(),
        has_thesis_sentence: false,
        realized_state_complete: false,
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

/// A two-stage build pipeline (`stage-source-load` → `stage-docs-render`), the
/// minimal fixture that exercises the methodology section's provenance-chain
/// walk (issue 1404, gap G5). Without a `model.pipeline`, `section_methodology`
/// emits no provenance paragraph at all (honest absence) — this is the fixture
/// that turns that paragraph ON so the PDF gate has something real to grep.
fn demo_pipeline() -> DocPipeline {
    const SOURCE: &str = "https://blackcatinformatics.ca/gmeow/stage-source-load";
    const RENDER: &str = "https://blackcatinformatics.ca/gmeow/stage-docs-render";
    DocPipeline {
        stages: vec![
            DocStage {
                iri: SOURCE.to_string(),
                consumes: Vec::new(),
                ..Default::default()
            },
            DocStage {
                iri: RENDER.to_string(),
                consumes: vec![SOURCE.to_string()],
                ..Default::default()
            },
        ],
        edges: vec![DocFlowEdge {
            from: SOURCE.to_string(),
            to: RENDER.to_string(),
            flow_entities: Vec::new(),
        }],
        goal: None,
        success_mode: None,
    }
}

/// [`fixture_model`] plus a [`demo_pipeline`] — kept separate from
/// `fixture_model()` so the existing `render_typ_is_golden` snapshot (built over
/// a bare, pipeline-free model) stays untouched.
fn fixture_model_with_pipeline() -> DocsModel {
    DocsModel {
        pipeline: Some(demo_pipeline()),
        ..fixture_model()
    }
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

/// GAP G5 (issue 1404): the PDF must carry the same provenance chain the
/// HTML/mdbook site footer does — not just in the Typst SOURCE (which trivially
/// contains anything the source author wrote), but in the compiled PDF's TEXT
/// LAYER, i.e. what a PDF text-extraction tool or copy-paste actually recovers.
///
/// This is the falsifiable gate: it greps [`pdf_text_layer`], which walks the
/// SAME frame tree `compile_pdf` hands to `typst-pdf` for serialization. If
/// `section_methodology`'s provenance paragraph were ever dropped (or only
/// written to the Typst source without actually laying out), this test reds.
#[test]
fn pdf_text_layer_carries_provenance_chain() {
    let model = fixture_model_with_pipeline();
    let typ = render_typ(&model, &fixture_axioms(), &fixture_bib(), &losses());

    // Sanity: the Typst SOURCE carries the chain (both stage local names, and
    // the coarse-grain-provenance framing sentence).
    assert!(
        typ.contains("stage-docs-render") && typ.contains("stage-source-load"),
        "Typst source missing the provenance chain stage names"
    );
    assert!(
        typ.contains("provenance chain"),
        "Typst source missing the provenance-chain framing sentence"
    );

    // The gate: the same two stage identifiers must survive all the way to the
    // compiled PDF's extractable text layer, not just the source.
    let text = pdf_text_layer(&typ, &fixture_bib()).expect("pdf text layer extraction must work");
    assert!(
        text.contains("stage-docs-render"),
        "PDF text layer missing 'stage-docs-render'; provenance chain did not survive \
         compilation:\n{text}"
    );
    assert!(
        text.contains("stage-source-load"),
        "PDF text layer missing 'stage-source-load'; provenance chain did not survive \
         compilation:\n{text}"
    );
}

/// A bare model without a `pipeline` (e.g. [`fixture_model`]) is an HONEST
/// absence, not a bug: `section_methodology` emits no provenance paragraph, and
/// neither the Typst source nor the compiled PDF text layer carries the stage
/// chain. Pinned here so a future change that starts fabricating a chain out of
/// nothing (rather than reading the real `model.pipeline`) is caught.
#[test]
fn pdf_text_layer_omits_provenance_chain_without_a_pipeline() {
    let model = fixture_model();
    assert!(
        model.pipeline.is_none(),
        "fixture_model must stay pipeline-free"
    );
    let typ = render_typ(&model, &fixture_axioms(), &fixture_bib(), &losses());
    let text = pdf_text_layer(&typ, &fixture_bib()).expect("pdf text layer extraction must work");
    assert!(
        !text.contains("provenance chain"),
        "a pipeline-free model must not fabricate a provenance-chain paragraph"
    );
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
