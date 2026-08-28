// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-SHACL conformance for the ten shipped `math:` producer graphs.
//!
//! The production DAG has already run the five flagship producers, probability seam,
//! p-value tri-slice, exact Clifford producer, and three executable lifts and folded their
//! outputs into the authenticated bundle. These contracts project those exact named graphs
//! read-only and validate them against the authenticated whole-ontology SHACL corpus. No
//! producer entry point is reachable from this test group, and a missing graph fails closed.

use crate::conformance_support::*;

use purrdf::shapes::report::Severity;
use purrdf::shapes::term::Term;

const PRODUCER_NS: &str = "https://blackcatinformatics.ca/gmeow/examples/math/producers/";
const E8_WEYL: &str = "https://blackcatinformatics.ca/gmeow/graph/math-producers/e8-weyl";
const ADDITIVE_HE: &str = "https://blackcatinformatics.ca/gmeow/graph/math-producers/additive-he";
const PROOF_INGEST: &str = "https://blackcatinformatics.ca/gmeow/graph/math-producers/proof-ingest";
const PCA_RESIDUAL: &str = "https://blackcatinformatics.ca/gmeow/graph/math-producers/pca-residual";
const PROBABILITY_MODEL: &str =
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/probability-model";
const PVALUE_TRI_SLICE: &str =
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/pvalue-tri-slice";
const CLIFFORD_12_13: &str =
    "https://blackcatinformatics.ca/gmeow/graph/math-producers/clifford-12-13";
const R_LIFT: &str = "https://blackcatinformatics.ca/gmeow/graph/math-producers/r-lift";
const ONNX_LIFT: &str = "https://blackcatinformatics.ca/gmeow/graph/math-producers/onnx-lift";
const PROOF_LIFT: &str = "https://blackcatinformatics.ca/gmeow/graph/math-producers/proof-lift";

/// Validate one already-produced named graph merged with the authenticated authored
/// ontology, returning every violation on a producer-minted focus node.
fn bundled_producer_violations(graph_iri: &str) -> Vec<String> {
    let graph_nt = authenticated_named_graph_nt(graph_iri);
    let report = validate_with_ontology(&graph_nt);
    report
        .results
        .iter()
        .filter(|result| result.severity == Severity::Violation)
        .filter(|result| match &result.focus_node {
            Term::NamedNode(iri) => iri.as_str().starts_with(PRODUCER_NS),
            _ => false,
        })
        .map(|result| {
            format!(
                "focus={:?} shape-component={} msg={}",
                result.focus_node,
                result.source_constraint_component.as_str(),
                result.message.clone().unwrap_or_default()
            )
        })
        .collect()
}

fn assert_bundled_producer_graph_clean(graph_iri: &str, label: &str) {
    let violations = bundled_producer_violations(graph_iri);
    assert!(
        violations.is_empty(),
        "authenticated {label} graph raised math violations: {violations:#?}"
    );
}

// These whole-ontology validations remain in the maint-heavy contract group because their
// cost is the exhaustive SHACL scan, not corpus setup. The group shares one authenticated
// bundle and parsed shape model through the consolidated runner.

#[gmeow_test_batch_macros::batch_test]
fn e8_weyl_order_graph_validates_clean() {
    assert_bundled_producer_graph_clean(E8_WEYL, "E8 producer");
}

#[gmeow_test_batch_macros::batch_test]
fn additive_he_demo_graph_validates_clean() {
    assert_bundled_producer_graph_clean(ADDITIVE_HE, "additive-HE producer");
}

#[gmeow_test_batch_macros::batch_test]
fn proof_ingest_graph_validates_clean() {
    assert_bundled_producer_graph_clean(PROOF_INGEST, "proof-ingest producer");
}

#[gmeow_test_batch_macros::batch_test]
fn exact_pca_residual_graph_validates_clean() {
    assert_bundled_producer_graph_clean(PCA_RESIDUAL, "PCA producer");
}

#[gmeow_test_batch_macros::batch_test]
fn probability_model_seam_graph_validates_clean() {
    assert_bundled_producer_graph_clean(PROBABILITY_MODEL, "probability-model producer");
}

#[gmeow_test_batch_macros::batch_test]
fn pvalue_tri_slice_graph_validates_clean() {
    assert_bundled_producer_graph_clean(PVALUE_TRI_SLICE, "p-value tri-slice producer");
}

#[gmeow_test_batch_macros::batch_test]
fn clifford_twelve_thirteen_graph_validates_clean() {
    assert_bundled_producer_graph_clean(CLIFFORD_12_13, "Clifford producer");
}

#[gmeow_test_batch_macros::batch_test]
fn r_lift_graph_validates_clean() {
    assert_bundled_producer_graph_clean(R_LIFT, "executable R lift");
}

#[gmeow_test_batch_macros::batch_test]
fn onnx_lift_graph_validates_clean() {
    assert_bundled_producer_graph_clean(ONNX_LIFT, "executable ONNX lift");
}

#[gmeow_test_batch_macros::batch_test]
fn proof_lift_graph_validates_clean() {
    assert_bundled_producer_graph_clean(PROOF_LIFT, "executable proof lift");
}
