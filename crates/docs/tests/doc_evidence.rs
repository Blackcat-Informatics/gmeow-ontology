// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The fail-fast GROUNDING INVARIANT for the uniform `gmeow:DocEvidence` RDF
//! projection from the shared documentation evidence graph.
//!
//! Proof-carrying documentation demands that EVERY projected evidence node be a
//! claim WITH its grounds: an ungrounded `gmeow:DocEvidence` node is the
//! doc-layer analogue of a DARK finding. Two tests enforce this:
//!
//! * [`every_doc_evidence_node_is_grounded`] builds a representative SYNTHETIC
//!   model that genuinely populates all five evidence kinds (competency,
//!   diagnostics, fixture, loss, provenance) so every per-kind code path fires,
//!   and asserts each projects exactly one grounded node.
//! * [`live_doc_evidence_projection_is_fully_grounded`] runs the SAME invariant
//!   over the REAL production `DocsModel` (the live slice catalog), so the gate
//!   bites on the shipped documentation graph — not only a hand-built fixture.
//!
//! Both project through [`to_gmeow_rdf`], re-parse the N-Quads through the
//! independent native codec, and assert that ZERO `gmeow:DocEvidence` subjects
//! lack a `gmeow:docGroundedBy` object.
//!
//! This is the genuine, on-gate (`make check` → `rust-test`), executable
//! enforcement of the invariant. The SHACL form is NOT authored: the
//! `gmeow:DocEvidence` graph is produced by `stage-docs-render`, which runs
//! strictly AFTER `stage-validate` (`stage-validate` consumes only
//! `stage-source-load`; `stage-docs-render` consumes `stage-validate`). `make
//! validate` / `make check` run SHACL over the authored source graph only (never
//! the downstream `graph/documentation` projection), so a shape over
//! `gmeow:DocEvidence` would have nothing to bite on at the gate — a DARK
//! producer-without-consumer. This Rust invariant, over both a synthetic and the
//! live model, is the architecturally-correct enforcement point.

use gmeow_docs::{
    DiagnosticsDigest, DocCompetency, DocDiagFinding, DocFixture, DocFixtureKind, DocFlowEdge,
    DocPipeline, DocStage, DocTerm, DocTermCategory, DocsModel, TermLossDigest, TermLossRow,
    to_gmeow_rdf,
};
use purrdf::{DatasetView, GraphMatch, TermValue};

mod common;

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

    // A fixture that references the term (the fixture Do/Don't join).
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

    // A competency question that exercises the term, with a query body
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

    // A diagnostics-to-term join row. On the real repo `by_term` is
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

    // A dynamic per-term projection-loss row.
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
    let nq = to_gmeow_rdf(&evidence_rich_model(), &std::collections::BTreeMap::new());
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

/// The grounding invariant over an arbitrary projection: parse the N-Quads, find
/// every `gmeow:DocEvidence` subject, and return `(total_evidence_nodes,
/// ungrounded_count)`. Shared by the synthetic and the live-model tests so both
/// enforce the invariant through one code path.
fn grounding_tally(nq: &str) -> (usize, usize) {
    let ds = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
        .expect("to_gmeow_rdf must emit valid, round-trippable N-Quads");

    // On a projection with zero evidence nodes the type/predicate IRIs may never
    // be interned; treat an absent id as "no such nodes" rather than panicking.
    let type_id = match ds.term_id_by_value(&TermValue::iri(RDF_TYPE)) {
        Some(id) => id,
        None => return (0, 0),
    };
    let docevidence_id = match ds.term_id_by_value(&TermValue::iri(format!("{GMEOW}DocEvidence"))) {
        Some(id) => id,
        None => return (0, 0),
    };
    let grounded_id = match ds.term_id_by_value(&TermValue::iri(format!("{GMEOW}docGroundedBy"))) {
        Some(id) => id,
        None => {
            // The predicate is never interned ⇒ no node is grounded. Count the
            // evidence subjects so the caller sees them all as ungrounded.
            let n = ds
                .quads_for_pattern(None, Some(type_id), Some(docevidence_id), GraphMatch::Any)
                .count();
            return (n, n);
        }
    };

    let subjects: Vec<_> = ds
        .quads_for_pattern(None, Some(type_id), Some(docevidence_id), GraphMatch::Any)
        .map(|q| q.s)
        .collect();
    let ungrounded = subjects
        .iter()
        .filter(|s| {
            ds.quads_for_pattern(Some(**s), Some(grounded_id), None, GraphMatch::Any)
                .count()
                == 0
        })
        .count();
    (subjects.len(), ungrounded)
}

/// The grounding invariant over the LIVE production documentation model: EVERY
/// `gmeow:DocEvidence` node the real slice catalog projects carries at least one
/// `gmeow:docGroundedBy` object. This is the on-real-data enforcement point —
/// SHACL over `gmeow:DocEvidence` cannot bite at `make validate`/`make check`
/// (the `graph/documentation` projection is produced by `stage-docs-render`,
/// strictly downstream of the source-graph-only `stage-validate`), so this Rust
/// invariant, run over the shipped model, is where a dropped grounding edge reds
/// the gate.
#[test]
fn live_doc_evidence_projection_is_fully_grounded() {
    let model = common::cached_model();
    let nq = to_gmeow_rdf(&model, &std::collections::BTreeMap::new());
    let (total, ungrounded) = grounding_tally(&nq);

    // Non-vacuity: the live catalog MUST project real evidence, else the
    // invariant would be trivially satisfiable over an empty set. The production
    // model carries a pipeline (so every documented term gets a `provenance`
    // node) and thousands of enriched terms.
    assert!(
        total > 0,
        "the live documentation model must project gmeow:DocEvidence nodes \
         (zero would make the grounding invariant vacuous)"
    );
    assert_eq!(
        ungrounded, 0,
        "every gmeow:DocEvidence node in the LIVE documentation graph must carry \
         a gmeow:docGroundedBy edge — {ungrounded} of {total} are ungrounded \
         (an ungrounded evidence node is the doc-layer analogue of a DARK finding)"
    );
}
