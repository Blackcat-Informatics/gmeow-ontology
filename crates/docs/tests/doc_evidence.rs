// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The fail-fast GROUNDING INVARIANT for the uniform `gmeow:DocEvidence` RDF
//! projection (issue 1404, transformational #3).
//!
//! Proof-carrying documentation demands that EVERY projected evidence node be a
//! claim WITH its grounds: an ungrounded `gmeow:DocEvidence` node is the
//! doc-layer analogue of a DARK finding. This test builds a representative model
//! that genuinely populates all five evidence kinds (competency, diagnostics,
//! fixture, loss, provenance), projects it through [`to_gmeow_rdf`], re-parses
//! the N-Quads through the independent native codec, and asserts that ZERO
//! `gmeow:DocEvidence` subjects lack a `gmeow:docGroundedBy` object. This is the
//! genuine, on-gate (`make check` → `rust-test`), executable enforcement of the
//! invariant — see the crate report on why the SHACL form is scoped elsewhere.

use gmeow_docs::{
    DiagnosticsDigest, DocCompetency, DocDiagFinding, DocFixture, DocFixtureKind, DocFlowEdge,
    DocPipeline, DocStage, DocTerm, DocTermCategory, DocsModel, TermLossDigest, TermLossRow,
    to_gmeow_rdf,
};
use purrdf::{DatasetView, GraphMatch, TermValue};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// A model whose single documented term genuinely carries every evidence kind,
/// so the projection emits one grounded node per kind.
fn evidence_rich_model() -> DocsModel {
    let cat = format!("{GMEOW}Cat");

    let term = DocTerm {
        iri: cat.clone(),
        curie: "gmeow:Cat".to_string(),
        label: Some("Cat".to_string()),
        definition: Some("A small domesticated felid.".to_string()),
        category: DocTermCategory::Class,
        owner_slice: format!("{GMEOW}slice/zoo"),
        ..Default::default()
    };

    // A fixture that references the term (the fixture Do/Don't join, Task 1).
    let fixture = DocFixture {
        slice: format!("{GMEOW}slice/zoo"),
        logical_path: "tests/conformance-fixtures/cat-ok.ttl".to_string(),
        title: "A conforming cat".to_string(),
        text: "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n".to_string(),
        kind: DocFixtureKind::Wellformed,
        terms_referenced: vec!["gmeow:Cat".to_string()],
        expected_outcome: Some("conforms".to_string()),
        violation_code: None,
        rationale: None,
        catalog_slug: None,
    };

    // A competency question that exercises the term (Task 2), with a query body
    // so the evidence node carries a `blake3:` query digest.
    let competency = DocCompetency {
        iri: format!("{GMEOW}cq/cats-are-animals"),
        rationale: Some("Every cat must classify as an animal.".to_string()),
        query_file: None,
        query_text: Some("SELECT ?c WHERE { ?c a gmeow:Cat }".to_string()),
        exact_rows: None,
        expected_row_count: None,
        expected_rows: Vec::new(),
        exercises: vec![cat.clone()],
        owner_slice: format!("{GMEOW}slice/zoo"),
    };

    // A diagnostics-to-term join row (Task 7). On the real repo `by_term` is
    // empty today; here a synthetic finding exercises the code path.
    let mut diag_by_term = std::collections::BTreeMap::new();
    diag_by_term.insert(
        cat.clone(),
        vec![DocDiagFinding {
            code: "shacl.MinCountConstraintComponent".to_string(),
            severity: "error".to_string(),
            category: "shacl".to_string(),
            message: "cat is missing a required owner".to_string(),
            slice_iri: Some(format!("{GMEOW}slice/zoo")),
            help_uri: None,
        }],
    );
    let diagnostics = DiagnosticsDigest {
        by_term: diag_by_term,
        by_slice: std::collections::BTreeMap::new(),
        total: 1,
    };

    // A dynamic per-term projection-loss row (Task 8).
    let mut loss_by_term = std::collections::BTreeMap::new();
    loss_by_term.insert(
        cat.clone(),
        vec![TermLossRow {
            target: format!("property-path:{GMEOW}hasOwner"),
            preservation_kind: "SoundUnderApproximation".to_string(),
            complexity_class: "PTIME".to_string(),
            lossy_drops: vec!["owl:qualifiedCardinality".to_string()],
        }],
    );
    let term_loss = TermLossDigest {
        by_term: loss_by_term,
        total_property_path_rows: 1,
    };

    // A pipeline so every term gets a provenance evidence node with a real
    // stage-docs-render grounding IRI + backward-walk chain.
    let pipeline = DocPipeline {
        stages: vec![
            DocStage {
                iri: format!("{GMEOW}stage-source-load"),
                consumes: Vec::new(),
                ..Default::default()
            },
            DocStage {
                iri: format!("{GMEOW}stage-docs-render"),
                consumes: vec![format!("{GMEOW}stage-source-load")],
                ..Default::default()
            },
        ],
        edges: vec![DocFlowEdge {
            from: format!("{GMEOW}stage-source-load"),
            to: format!("{GMEOW}stage-docs-render"),
            flow_entities: Vec::new(),
        }],
        goal: None,
        success_mode: None,
    };

    DocsModel {
        title: "Evidence-rich model".to_string(),
        version: "2".to_string(),
        terms: vec![term],
        fixtures: vec![fixture],
        competencies: vec![competency],
        diagnostics: Some(diagnostics),
        term_loss: Some(term_loss),
        pipeline: Some(pipeline),
        available_languages: vec!["english".to_string()],
        ..Default::default()
    }
}

/// EVERY `gmeow:DocEvidence` node the projection emits carries at least one
/// `gmeow:docGroundedBy` object — the fail-fast grounding invariant. A future
/// refactor that drops a grounding edge reds this test.
#[test]
fn every_doc_evidence_node_is_grounded() {
    let nq = to_gmeow_rdf(&evidence_rich_model());
    let ds = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
        .expect("to_gmeow_rdf must emit valid, round-trippable N-Quads");

    let type_id = ds
        .term_id_by_value(&TermValue::iri(RDF_TYPE))
        .expect("rdf:type interned");
    let docevidence_id = ds
        .term_id_by_value(&TermValue::iri(format!("{GMEOW}DocEvidence")))
        .expect("gmeow:DocEvidence interned");
    let grounded_id = ds
        .term_id_by_value(&TermValue::iri(format!("{GMEOW}docGroundedBy")))
        .expect("gmeow:docGroundedBy interned");

    // Every subject typed gmeow:DocEvidence.
    let subjects: Vec<_> = ds
        .quads_for_pattern(None, Some(type_id), Some(docevidence_id), GraphMatch::Any)
        .map(|q| q.s)
        .collect();

    // Non-vacuity: the invariant would be trivially satisfiable over an empty
    // set. The rich model MUST project evidence — one node per kind.
    assert_eq!(
        subjects.len(),
        5,
        "expected one gmeow:DocEvidence node per evidence kind (competency, \
         diagnostics, fixture, loss, provenance)"
    );

    let ungrounded = subjects
        .iter()
        .filter(|s| {
            ds.quads_for_pattern(Some(**s), Some(grounded_id), None, GraphMatch::Any)
                .count()
                == 0
        })
        .count();
    assert_eq!(
        ungrounded, 0,
        "every gmeow:DocEvidence node must carry a gmeow:docGroundedBy edge \
         (an ungrounded evidence node is the doc-layer analogue of a DARK finding)"
    );

    // Every kind genuinely fired (proves the per-kind code paths, not just one).
    for kind in [
        "docEvidenceKindCompetency",
        "docEvidenceKindDiagnostics",
        "docEvidenceKindFixture",
        "docEvidenceKindLoss",
        "docEvidenceKindProvenance",
    ] {
        assert!(
            nq.contains(kind),
            "projection missing evidence kind `{kind}`"
        );
    }

    // The provenance chain rides every evidence node, and the loss node carries
    // its preservation judgment.
    assert!(
        nq.contains("docProducedByChain"),
        "provenance chain must ride the evidence nodes"
    );
    assert!(
        nq.contains("docJudgment"),
        "the loss evidence node must carry its preservation judgment"
    );
    assert!(
        nq.contains("docCompetencyQueryDigest"),
        "the competency evidence node must carry its blake3 query digest"
    );
}
