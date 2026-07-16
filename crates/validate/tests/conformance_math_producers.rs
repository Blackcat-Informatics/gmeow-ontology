// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Real-SHACL conformance for the eight `math:` producers: five flagship-acceptance
//! producers plus the probability seam, p-value tri-slice, and exact Clifford producers.
//!
//! `gmeow-validate` depends on `gmeow-math`, so this crate can call the native
//! producers directly and validate their emitted RDF graph fragments against the
//! LIVE merged SHACL shape corpus (`whole_shapes()`, merged with the base ontology
//! via [`validate_with_ontology`]). For each producer we assert that no
//! `Violation`-severity result is raised on a focus node minted in the producer
//! namespace — i.e. the emitted graph is example-clean against the math slice's
//! shapes, exactly as the hand-authored worked examples are.
//!
//! This is the reused-across-crates proof for the producers: the same shapes the
//! `make validate` gate runs accept the producer output, so the flagship worked
//! examples now have a native, deterministic, exact-arithmetic origin instead of a
//! hand-authored fixture.

mod conformance_support;
use conformance_support::*;

use gmeow_math::producers::{
    self, PRODUCER_NS, additive_he_demo, clifford_twelve_thirteen, e8_weyl_order,
    exact_pca_residual, probability_model_seam, proof_ingest, pvalue_tri_slice, r_bridge_lift,
};
use purrdf::shapes::report::Severity;
use purrdf::shapes::term::Term;

/// Convert producer Turtle to N-Triples and validate it merged with the base
/// ontology against the whole SHACL corpus, returning the messages of every
/// `Violation` whose focus node lives in the producer namespace.
fn producer_violations(turtle: &str) -> Vec<String> {
    let nt = ttl_str_to_nt(turtle);
    let report = validate_with_ontology(&nt);
    report
        .results
        .iter()
        .filter(|res| res.severity == Severity::Violation)
        .filter(|res| match &res.focus_node {
            Term::NamedNode(iri) => iri.as_str().starts_with(PRODUCER_NS),
            _ => false,
        })
        .map(|res| {
            format!(
                "focus={:?} shape-component={} msg={}",
                res.focus_node,
                res.source_constraint_component.as_str(),
                res.message.clone().unwrap_or_default()
            )
        })
        .collect()
}

// Each producer graph is validated against the LIVE whole-ontology-union SHACL corpus
// (`validate_with_ontology` merges the full base ontology with `whole_shapes()`). That is
// genuinely-irreducible whole-ontology conformance (~20 s each), so — exactly like the
// `conformance_{finance,agentic,ai_claims,music_analysis}` domain conformance binaries —
// this binary is carved out of the per-commit `default-filter` in `.config/nextest.toml`
// and runs on the `maint-heavy` profile. The five producers' pinned falsifiable values
// stay enforced ON the per-commit gate by `crates/pipeline/tests/math_flagship_discharge.rs`,
// which RUNS every producer and asserts its output; this binary is the extra cross-crate
// proof that the producer output also validates clean against the real shape corpus.

#[test]
fn e8_weyl_order_graph_validates_clean() {
    let v = producer_violations(&e8_weyl_order().turtle);
    assert!(
        v.is_empty(),
        "E8 producer graph raised math violations: {v:#?}"
    );
}

#[test]
fn additive_he_demo_graph_validates_clean() {
    let v = producer_violations(&additive_he_demo().turtle);
    assert!(
        v.is_empty(),
        "HE producer graph raised math violations: {v:#?}"
    );
}

#[test]
fn proof_ingest_graph_validates_clean() {
    let v = producer_violations(&proof_ingest().turtle);
    assert!(
        v.is_empty(),
        "proof producer graph raised math violations: {v:#?}"
    );
}

#[test]
fn r_bridge_lift_graph_validates_clean() {
    let v = producer_violations(&r_bridge_lift().turtle);
    assert!(
        v.is_empty(),
        "R-bridge producer graph raised math violations: {v:#?}"
    );
}

#[test]
fn exact_pca_residual_graph_validates_clean() {
    let v = producer_violations(&exact_pca_residual().turtle);
    assert!(
        v.is_empty(),
        "PCA producer graph raised math violations: {v:#?}"
    );
}

/// The sixth (non-flagship) producer — the probability layer's live
/// `logic:probabilityModel` seam — validates clean the SAME way the five flagship
/// producers do: the LIVE A-box crossing triple this fold exists to ship must be
/// example-clean against the real shape corpus, not merely well-typed Turtle.
#[test]
fn probability_model_seam_graph_validates_clean() {
    let v = producer_violations(&probability_model_seam().turtle);
    assert!(
        v.is_empty(),
        "probability-model-seam producer graph raised math violations: {v:#?}"
    );
}

#[test]
fn pvalue_tri_slice_graph_validates_clean() {
    let v = producer_violations(&pvalue_tri_slice().turtle);
    assert!(
        v.is_empty(),
        "p-value tri-slice producer graph raised math violations: {v:#?}"
    );
}

#[test]
fn clifford_twelve_thirteen_graph_validates_clean() {
    let v = producer_violations(&clifford_twelve_thirteen().turtle);
    assert!(
        v.is_empty(),
        "Clifford producer graph raised math violations: {v:#?}"
    );
}

/// The pinned values are the flagship falsifiable invariants — re-assert them here
/// so the conformance surface and the value surface stay wired to one producer call.
#[test]
fn producers_pin_their_falsifiable_values() {
    assert_eq!(e8_weyl_order().order, 696_729_600);
    assert_eq!(producers::E8_WEYL_ORDER, 696_729_600);

    let he = additive_he_demo();
    assert_eq!(he.decrypted_sum, (he.a + he.b).rem_euclid(he.modulus));

    assert!(proof_ingest().grounded);

    assert_eq!(r_bridge_lift().lifted_observations, 5);

    let pca = exact_pca_residual();
    assert_eq!(pca.dominant_axis, 0);
    assert_eq!(pca.ldlt_pivots.len(), 3);

    let seam = probability_model_seam();
    assert_eq!(seam.joint_mass_total, gmeow_math::Rational::one());

    let pvalue = pvalue_tri_slice();
    assert_eq!(
        pvalue.magnitude,
        gmeow_math::Rational::new(3, 100).expect("3/100")
    );

    let clifford = clifford_twelve_thirteen();
    assert_eq!(clifford.base_dimensions, [4096, 4096]);
    assert_eq!(clifford.extension_dimensions, [8192, 8192]);
    assert_eq!(clifford.generator_laws_verified, 50);
    assert_eq!(clifford.anticommutation_pairs_verified, 288);
    assert!(clifford.split_join_verified);
}
