// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN math signature-coherence verify queries return ZERO rows against the
//! CURRENT shipped graph.
//!
//! `slices/grounding/lang/queries/verify/*.rq` are harvested by `crates/logic/build.rs`
//! and run over the reasoned graph at `make reason-verify` (a returned row = a
//! violation). Both `gmn-operator-arity-coherence.rq` and
//! `gmn-form-signature-completeness.rq` are pure ABox/schema joins over asserted
//! triples (no derived edges), so this test proves — cheaply, without a full reason
//! chase — that neither gate false-positives on the merged lang + math + logic
//! grounding graph. It is the executable form of the "must return ZERO rows against
//! the current graph" authoring obligation.

use std::sync::Arc;

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    DatasetMut, MutableDataset, RdfDataset, SparqlEngine, SparqlRequest, SparqlResult, parse_dataset,
};

const OPERATOR_ARITY_Q: &str =
    include_str!("../../../slices/grounding/lang/queries/verify/gmn-operator-arity-coherence.rq");
const FORM_SIGNATURE_Q: &str = include_str!(
    "../../../slices/grounding/lang/queries/verify/gmn-form-signature-completeness.rq"
);

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
fn merged_grounding_graph() -> Arc<RdfDataset> {
    let mut store = MutableDataset::new(Arc::new(RdfDataset::union(&[])));
    for slice in ["lang", "math", "logic"] {
        let ds = grounding_module(slice);
        for quad in ds.flat_default_graph_quads() {
            store.insert(quad);
        }
    }
    store.freeze().expect("freeze merged grounding graph")
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
fn gmn_form_signature_completeness_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, FORM_SIGNATURE_Q);
    assert_eq!(
        rows, 0,
        "gmn-form-signature-completeness.rq must return zero rows against the shipped graph: \
         every fixity-bearing GMN form declares both gmnPrecedence and gmnArity"
    );
}
