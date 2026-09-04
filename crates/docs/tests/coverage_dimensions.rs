// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Guard tests for the documentation-coverage → RDF projection.
//!
//! Three invariants the coverage incidence must uphold on the production surface:
//! (1) `to_gmeow_rdf` actually emits the `gmeow:docCoversDimension` /
//! `gmeow:docMissesDimension` incidence, the bounded `gmeow:coverageFraction`, and
//! the FCA-derived `gmeow:docEarnedMaturity` (proven by grepping the emitted
//! N-Quads, since materialization is a later task); (2) the THREE-WAY key
//! agreement — every coverage key ↔ `maturity::Dimension` variant ↔ `gmeow:dim*`
//! individual in `slices/core/documentation/module.ttl` — with no orphan on any
//! side; (3) the coverage/maturity path invokes NO reasoner (the incidence is a
//! pure deterministic function of the model, unaffected by whether a reasoning
//! verdict is attached).

use std::collections::BTreeSet;

use gmeow_docs::coverage::{DIMENSIONS, SLICE_DIMENSIONS};
use gmeow_docs::maturity::Dimension;
use gmeow_docs::model::ReasoningVerdict;
use gmeow_docs::{DocSlice, DocTerm, DocTermCategory, DocsModel, to_gmeow_rdf};
use purrdf::slice::rdf_query::{Dataset, GraphSel, Object, Subject};

mod common;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const DOC_COVERAGE_DIMENSION: &str = "https://blackcatinformatics.ca/gmeow/DocCoverageDimension";

/// A minimal but representative model: one slice owning one bare term. The bare
/// term MISSES most dimensions (so `docMissesDimension` is exercised) while the
/// slice carries the aggregate coverage, fraction, and earned-maturity triples.
fn representative_model() -> DocsModel {
    let slice_iri = format!("{GMEOW}slices/zoo");
    let mut model = DocsModel::default();
    model.slices.push(DocSlice {
        iri: slice_iri.clone(),
        label: Some("Zoo".to_string()),
        title: None,
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
    });
    model.terms.push(DocTerm {
        iri: format!("{GMEOW}Cat"),
        curie: "gmeow:Cat".to_string(),
        label: Some("Cat".to_string()),
        definition: Some("A small domesticated felid.".to_string()),
        category: DocTermCategory::Class,
        owner_slice: slice_iri,
        ..Default::default()
    });
    model
}

#[test]
fn projection_emits_the_coverage_incidence_on_the_production_surface() {
    let nq = to_gmeow_rdf(&representative_model(), &std::collections::BTreeMap::new());

    // Per-term incidence: the bare `Cat` misses (e.g.) the fixture-pair dimension
    // and covers definition/label.
    assert!(
        nq.contains(&format!(
            "<{GMEOW}documentation/term/cat> <{GMEOW}docMissesDimension> <{GMEOW}dimFixturePair>"
        )),
        "per-term docMissesDimension not emitted"
    );
    assert!(
        nq.contains(&format!(
            "<{GMEOW}documentation/term/cat> <{GMEOW}docCoversDimension> <{GMEOW}dimDefinition>"
        )),
        "per-term docCoversDimension not emitted"
    );

    // Per-slice aggregate incidence + bounded fraction + earned maturity.
    assert!(
        nq.contains(&format!(
            "<{GMEOW}documentation/slice/zoo> <{GMEOW}docCoversDimension>"
        )),
        "per-slice docCoversDimension not emitted"
    );
    let fraction_line = nq
        .lines()
        .find(|l| {
            l.starts_with(&format!(
                "<{GMEOW}documentation/slice/zoo> <{GMEOW}coverageFraction>"
            ))
        })
        .expect("per-slice coverageFraction not emitted");
    let fraction_lexical = fraction_line
        .split('"')
        .nth(1)
        .expect("coverageFraction literal carries a quoted lexical form");
    let fraction: f64 = fraction_lexical.parse().unwrap_or_else(|e| {
        panic!("coverageFraction lexical form `{fraction_lexical}` is not a valid float: {e}")
    });
    assert!(
        (0.0..=1.0).contains(&fraction),
        "coverageFraction {fraction} is outside the closed unit range [0.0, 1.0]"
    );
    // The bare-term slice earns at least Minimal (definition + label present on the
    // sole term), so docEarnedMaturity is emitted.
    assert!(
        nq.contains(&format!(
            "<{GMEOW}documentation/slice/zoo> <{GMEOW}docEarnedMaturity> <{GMEOW}docMaturityMinimal>"
        )),
        "per-slice docEarnedMaturity (Minimal) not emitted"
    );
}

#[test]
fn coverage_keys_are_unique_and_partition_all_dimensions() {
    // The union of per-term and slice-scoped coverage dimensions is EXACTLY the
    // eighteen maturity dimensions — no orphan, no duplicate, on either side.
    let mut keys = BTreeSet::new();
    let mut dims = BTreeSet::new();
    for cd in DIMENSIONS.iter().chain(SLICE_DIMENSIONS.iter()) {
        assert!(keys.insert(cd.key), "duplicate coverage key `{}`", cd.key);
        assert!(
            dims.insert(cd.dimension),
            "duplicate coverage dimension {:?}",
            cd.dimension
        );
    }
    let all: BTreeSet<Dimension> = Dimension::ALL.into_iter().collect();
    assert_eq!(
        dims, all,
        "coverage dimensions must cover every maturity variant"
    );
    assert_eq!(keys.len(), Dimension::ALL.len(), "one key per dimension");
}

#[test]
fn every_coverage_key_maps_to_a_declared_doc_coverage_dimension_individual() {
    // The third leg of the three-way join: each maturity::Dimension local name is a
    // gmeow:dim* individual typed `a gmeow:DocCoverageDimension` in the
    // documentation slice module. No coverage key names an undeclared dimension.
    let root = common::repo_root();
    let path = root.join("slices/core/documentation/module.ttl");
    let bytes = std::fs::read(&path).expect("read documentation module.ttl");
    let ds = Dataset::parse_turtle(&bytes, None, &path.display().to_string())
        .expect("documentation module.ttl parses");

    let mut declared: BTreeSet<String> = BTreeSet::new();
    ds.graph(GraphSel::Any).for_each_quad(|s, p, o, _g| {
        if p != RDF_TYPE {
            return;
        }
        if let (Subject::Named(iri), Object::Named(ty)) = (&s, &o)
            && ty == DOC_COVERAGE_DIMENSION
            && let Some(local) = iri.strip_prefix(GMEOW)
        {
            declared.insert(local.to_string());
        }
    });

    // Non-vacuity: the eighteen dimensions must genuinely be declared.
    assert_eq!(
        declared.len(),
        Dimension::ALL.len(),
        "expected {} gmeow:DocCoverageDimension individuals, found {}: {declared:?}",
        Dimension::ALL.len(),
        declared.len(),
    );
    // Every coverage key's dimension local name is declared.
    for cd in DIMENSIONS.iter().chain(SLICE_DIMENSIONS.iter()) {
        let local = cd.dimension.local_name();
        assert!(
            declared.contains(local),
            "coverage key `{}` → dimension `{local}` has no gmeow:DocCoverageDimension declaration",
            cd.key
        );
    }
}

#[test]
fn coverage_projection_is_reasoner_independent_and_deterministic() {
    // The docs projection invokes NO reasoner: the coverage incidence and the
    // FCA-derived earned maturity are a pure function of the model, unaffected by
    // whether a reasoning verdict is attached. Emitting with reasoning None vs a
    // verdict changes ONLY the reasoning-status triple, never the coverage incidence.
    let mut with_verdict = representative_model();
    with_verdict.reasoning = Some(ReasoningVerdict::default());
    let without = representative_model();

    let a = to_gmeow_rdf(&without, &std::collections::BTreeMap::new());
    let b = to_gmeow_rdf(&with_verdict, &std::collections::BTreeMap::new());

    // Determinism.
    assert_eq!(
        a,
        to_gmeow_rdf(&without, &std::collections::BTreeMap::new()),
        "projection must be deterministic"
    );

    // The WHOLE projection is byte-identical whether or not a reasoner verdict is
    // present, apart from the `docReasoningStatus` triple the verdict adds — not
    // just the coverage-related lines, so an unrelated projection regression
    // (anywhere else in the emitted N-Quads) fails this test too.
    let strip_reasoning_status = |nq: &str| -> String {
        nq.lines()
            .filter(|l| !l.contains("docReasoningStatus"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        strip_reasoning_status(&a),
        strip_reasoning_status(&b),
        "the projection must be identical apart from the docReasoningStatus triple the verdict adds"
    );
    // The only difference is the reasoning-status triple the verdict adds.
    assert!(
        b.contains("docReasoningStatus") && !a.contains("docReasoningStatus"),
        "the reasoning verdict should add ONLY the reasoning-status triple"
    );
}
