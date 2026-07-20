// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN math signature-coherence verify queries return ZERO rows against the
//! CURRENT shipped graph, and each FIRES on an injected violation.
//!
//! `slices/grounding/lang/queries/verify/*.rq` are harvested by `crates/logic/build.rs`
//! and run over the reasoned graph at `make reason-verify` (a returned row = a
//! violation). The four GMN signature gates —
//! `gmn-operator-arity-coherence.rq`, `gmn-form-signature-completeness.rq`,
//! `gmn-infix-precedence-consistency.rq`, and `gmn-ascii-fallback-uniqueness.rq` — are
//! pure ABox/schema joins over asserted triples (no derived edges), so these tests prove
//! — cheaply, without a full reason chase — BOTH halves of a real gate: it stays silent
//! on the shipped merged lang + math + logic graph, AND it returns exactly the offending
//! row when a counter-example is injected. Without the injected-negative half, a gate that
//! can never fire would pass the "zero rows" obligation vacuously.

use std::sync::{Arc, OnceLock};

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    DatasetMut, MutableDataset, RdfDataset, SparqlEngine, SparqlRequest, SparqlResult,
    parse_dataset,
};

const OPERATOR_ARITY_Q: &str =
    include_str!("../../../slices/grounding/lang/queries/verify/gmn-operator-arity-coherence.rq");
const FORM_SIGNATURE_Q: &str = include_str!(
    "../../../slices/grounding/lang/queries/verify/gmn-form-signature-completeness.rq"
);
const PRECEDENCE_CONSISTENCY_Q: &str = include_str!(
    "../../../slices/grounding/lang/queries/verify/gmn-infix-precedence-consistency.rq"
);
const FALLBACK_UNIQUENESS_Q: &str =
    include_str!("../../../slices/grounding/lang/queries/verify/gmn-ascii-fallback-uniqueness.rq");

/// Parse one grounding slice's `module.ttl` into a dataset.
fn grounding_module(slice: &str) -> Arc<RdfDataset> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding")
        .join(slice)
        .join("module.ttl");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    parse_dataset(&bytes, "text/turtle", None).unwrap_or_else(|e| panic!("parse {path:?}: {e}"))
}

/// The frozen union of the lang + math + logic module graphs, flattened to the default
/// graph so a no-GRAPH verify SELECT matches it (mirrors the native verify substrate).
/// Cached once across the suite — the three modules are static fixtures, so re-parsing
/// them per test is wasted I/O and CPU.
fn merged_grounding_graph() -> Arc<RdfDataset> {
    static MERGED: OnceLock<Arc<RdfDataset>> = OnceLock::new();
    MERGED
        .get_or_init(|| {
            let mut store = MutableDataset::new(Arc::new(RdfDataset::union(&[])));
            for slice in ["lang", "math", "logic"] {
                let ds = grounding_module(slice);
                for quad in ds.flat_default_graph_quads() {
                    store.insert(quad);
                }
            }
            store.freeze().expect("freeze merged grounding graph")
        })
        .clone()
}

/// The merged grounding graph with an extra Turtle fragment overlaid — the seam for a
/// falsifiable negative: inject a counter-example and assert the gate returns exactly it.
/// Built with the same insert-then-freeze path as `merged_grounding_graph` so the base is
/// genuinely present alongside the injected quads.
fn merged_plus(extra_ttl: &str) -> Arc<RdfDataset> {
    let mut store = MutableDataset::new(Arc::new(RdfDataset::union(&[])));
    let base = merged_grounding_graph();
    for quad in base.flat_default_graph_quads() {
        store.insert(quad);
    }
    let extra = parse_dataset(extra_ttl.as_bytes(), "text/turtle", None)
        .expect("parse injected counter-example turtle");
    for quad in extra.flat_default_graph_quads() {
        store.insert(quad);
    }
    store
        .freeze()
        .expect("freeze merged grounding graph plus injection")
}

/// Row count of a SELECT verify query over the frozen graph.
fn violation_rows(graph: &Arc<RdfDataset>, sparql: &str) -> usize {
    let engine = NativeSparqlEngine::new();
    let result = engine
        .query(
            graph,
            SparqlRequest {
                query: sparql,
                base_iri: None,
                substitutions: &[],
            },
        )
        .expect("verify query parses and evaluates");
    match result {
        SparqlResult::Solutions { rows, .. } => rows.len(),
        other => panic!("a verify query must be a SELECT, got {other:?}"),
    }
}

// The Turtle prefix preamble every injected counter-example shares.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
";

#[test]
fn gmn_operator_arity_coherence_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, OPERATOR_ARITY_Q);
    assert_eq!(
        rows, 0,
        "gmn-operator-arity-coherence.rq must return zero rows against the shipped graph: \
         every math: owl:ObjectProperty operator (∈ ⊆ ∘) declares gmnArity 2"
    );
}

#[test]
fn gmn_operator_arity_coherence_fires_on_wrong_arity() {
    // A math: ObjectProperty operator whose denoting form declares gmnArity 3 (not 2).
    let injection = format!(
        "{PREFIXES}
        math:badBinaryRel a owl:ObjectProperty .
        gmeow:gmnFormMathBadArity a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix ;
            gmeow:gmnArity 3 .
        gmeow:gmnDenMathBadArity a lang:Denotation ;
            lang:denotationTarget math:badBinaryRel ;
            lang:denotedForm gmeow:gmnFormMathBadArity .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, OPERATOR_ARITY_Q);
    assert_eq!(
        rows, 1,
        "the arity gate must return exactly the one injected ObjectProperty operator whose \
         form declares gmnArity != 2"
    );
}

#[test]
fn gmn_form_signature_completeness_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, FORM_SIGNATURE_Q);
    assert_eq!(
        rows, 0,
        "gmn-form-signature-completeness.rq must return zero rows against the shipped graph: \
         every fixity-bearing GMN form declares both gmnPrecedence and gmnArity"
    );
}

#[test]
fn gmn_form_signature_completeness_fires_on_missing_precedence() {
    // A fixity-bearing form that declares arity but omits precedence — a half-specified
    // operator signature.
    let injection = format!(
        "{PREFIXES}
        gmeow:gmnFormMathIncomplete a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix ;
            gmeow:gmnArity 2 .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, FORM_SIGNATURE_Q);
    assert_eq!(
        rows, 1,
        "the completeness gate must return exactly the one injected fixity-bearing form \
         missing its gmnPrecedence"
    );
}

#[test]
fn gmn_infix_precedence_consistency_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, PRECEDENCE_CONSISTENCY_Q);
    assert_eq!(
        rows, 0,
        "gmn-infix-precedence-consistency.rq must return zero rows against the shipped graph: \
         each math-plane operator binds a single consistent precedence (band-sharing across \
         distinct operators is by design and not flagged)"
    );
}

#[test]
fn gmn_infix_precedence_consistency_fires_on_double_precedence() {
    // A SECOND math-plane infix form for math:Addition at a clashing precedence (99) — the
    // shipped gmnFormMathAddition already binds it at 60, so the operator now carries two
    // binding strengths.
    let injection = format!(
        "{PREFIXES}
        gmeow:gmnFormMathAdditionClash a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix ;
            gmeow:gmnArity 2 ;
            gmeow:gmnPrecedence 99 .
        gmeow:gmnDenMathAdditionClash a lang:Denotation ;
            lang:denotationContext gmeow:gmnMathGlyphContext ;
            lang:denotationTarget math:Addition ;
            lang:denotedForm gmeow:gmnFormMathAdditionClash .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, PRECEDENCE_CONSISTENCY_Q);
    assert_eq!(
        rows, 1,
        "the precedence-consistency gate must return exactly the one injected clash: \
         math:Addition bound at two different precedences"
    );
}

#[test]
fn gmn_ascii_fallback_uniqueness_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, FALLBACK_UNIQUENESS_Q);
    assert_eq!(
        rows, 0,
        "gmn-ascii-fallback-uniqueness.rq must return zero rows against the shipped graph: \
         each math-plane ASCII fallback key is unique across distinct targets"
    );
}

#[test]
fn gmn_ascii_fallback_uniqueness_fires_on_collision() {
    // A candidate reusing the `in` fallback (shipped for math:hasElement / ∈) on a
    // different target math:subsetOf — an ambiguous ASCII key stream.
    let injection = format!(
        "{PREFIXES}
        gmeow:gmnCandidateMathFallbackClash a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget math:subsetOf ;
            gmeow:gmnCandidateGlyph \"⊆\" ;
            gmeow:gmnAsciiFallback \"in\" .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, FALLBACK_UNIQUENESS_Q);
    assert_eq!(
        rows, 1,
        "the fallback-uniqueness gate must return exactly the one injected collision: two \
         distinct math: targets sharing the `in` fallback key"
    );
}
