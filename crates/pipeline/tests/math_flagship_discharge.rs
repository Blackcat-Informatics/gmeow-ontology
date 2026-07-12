// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `math:` grounding layer's flagship execution-discharge harness.
//!
//! Where the math flagship manifest once discharged its acceptance bar by EXISTENCE only,
//! this harness lifts it to the same EXECUTION rung as `lang:`, through the slice-generic
//! discharge core in [`support::flagship_discharge`]. The generic runner reads the acceptance
//! manifest `slices/grounding/math/examples/flagship-acceptance.ttl`, and for each of the five
//! `gmeow:FlagshipScenario` individuals it:
//!
//! 1. **Runs the guard.** Loads the `gmeow:guardedByCounterExample` fixture, pushes it through
//!    BOTH native validation channels (structural lint + native SHACL), and asserts the UNION
//!    of triggered `math:` failure classes equals EXACTLY the one named by
//!    `gmeow:enforcesFailureClass` — set equality, never mere membership.
//! 2. **Checks the worked example.** Loads the `gmeow:demonstratedByExample` fixture, runs the
//!    SAME two channels, and asserts NO `math:` failure class fires.
//! 3. **Runs the producer.** This file supplies the per-slice producer callback: it dispatches
//!    the `gmeow:demonstratedByProducer` identifier to the matching native
//!    [`gmeow_math::producers`] entrypoint, RUNS it, and asserts its output equals the pinned
//!    falsifiable datum the scenario claims (|W(E8)| = 696729600; Dec(Enc(a)⊕Enc(b)) = a+b; a
//!    grounded verification result; an ingest run with the pinned lifted-observation count; an
//!    exact-rational PCA whose dominant axis and LDLᵀ pivots are the pinned exact values).
//!
//! The five (counter-example, example, failure-class, producer) tuples are READ from the
//! manifest, never hard-coded — a manifest edit that unwires a flagship is caught here.

use gmeow_math::Rational;
use gmeow_math::producers;
use gmeow_validate::store::parse_file_dataset;
use purrdf::RdfTerm;

mod support;
use support::flagship_discharge::{
    CounterExampleExecution, Flagship, FlagshipCtx, SliceSpec, flagship_error, repo_root,
    run_flagship_discharge_with_counterexample,
};

/// The `math:` grounding namespace — used for the SCANNED failure classes (`math:<Class>`),
/// which stay slice-namespaced.
const MATH_NS: &str = "https://blackcatinformatics.ca/math/";

/// The `math:` slice's discharge identity: base IRI, short prefix, on-disk root, and the
/// acceptance-manifest path relative to that root.
fn math_spec() -> SliceSpec {
    SliceSpec {
        slice_ns: MATH_NS,
        slice_prefix: "math",
        slice_root: repo_root().join("slices").join("grounding").join("math"),
        manifest_rel: "examples/flagship-acceptance.ttl",
    }
}

#[test]
fn every_flagship_is_discharged_by_execution() {
    run_flagship_discharge_with_counterexample(&math_spec(), 5, &run_producer, &run_counterexample);
}

/// Interpret the malformed fixture into the SAME native semantic predicate asserted by its
/// positive producer. A parser/infrastructure failure is an error; only a decided negative
/// result becomes the manifest's typed math failure.
fn run_counterexample(
    flagship: &Flagship,
    _ctx: &FlagshipCtx<'_>,
) -> gmeow_errors::Result<CounterExampleExecution> {
    let ds = parse_file_dataset(&flagship.counter_example).map_err(|e| {
        flagship_error(format!(
            "counter-example {} parses: {e}",
            flagship.counter_example.display()
        ))
    })?;
    let values = |predicate: &str| {
        ds.owned_quads()
            .filter(|q| q.predicate == predicate)
            .map(|q| q.object)
            .collect::<Vec<_>>()
    };

    let failure = match flagship.producer.as_str() {
        "math::producers::e8_weyl_order" => {
            let orders = values(&format!("{MATH_NS}groupOrder"));
            let asserted = match orders.as_slice() {
                [RdfTerm::Literal(lit)] => lit.lexical_form.parse::<i128>().map_err(|e| {
                    flagship_error(format!("invalid math:groupOrder in E8 guard: {e}"))
                })?,
                other => {
                    return Err(flagship_error(format!(
                        "E8 guard must carry exactly one literal math:groupOrder, got {other:?}"
                    )));
                }
            };
            producers::verify_e8_weyl_order(asserted)
        }
        "math::producers::additive_he_demo" => {
            producers::verify_he_scheme(!values(&format!("{MATH_NS}noiseModel")).is_empty())
        }
        "math::producers::proof_ingest" => {
            let result_pred = "https://blackcatinformatics.ca/gmeow/observationResult";
            let vantage_pred = "https://blackcatinformatics.ca/gmeow/vantage";
            let result_subjects: std::collections::BTreeSet<_> = ds
                .owned_quads()
                .filter(|q| q.predicate == result_pred)
                .map(|q| format!("{:?}", q.subject))
                .collect();
            let vantage_subjects: std::collections::BTreeSet<_> = ds
                .owned_quads()
                .filter(|q| q.predicate == vantage_pred)
                .map(|q| format!("{:?}", q.subject))
                .collect();
            producers::verify_proof_result(!result_subjects.is_disjoint(&vantage_subjects))
        }
        "math::producers::r_bridge_lift" => producers::verify_ingest_lift(
            values("https://blackcatinformatics.ca/gmeow/wasGeneratedBy").len(),
        ),
        "math::producers::exact_pca_residual" => producers::verify_pca_analysis(
            !values(&format!("{MATH_NS}covarianceOperator")).is_empty(),
        ),
        other => {
            return Err(flagship_error(format!(
                "unknown math flagship producer identifier: {other}"
            )));
        }
    }
    .expect_err("the guarding fixture must be rejected by native semantic execution");

    Ok(CounterExampleExecution::ReasonerDriven(
        std::iter::once(failure.0.to_owned()).collect(),
    ))
}

/// Build the expected LDLᵀ pivot vector `[4, 11/4, 18/11]` from exact rationals — the pinned
/// spectrum of the exact-rational PCA Gram matrix `G = [[4,1,0],[1,3,1],[0,1,2]]`.
fn expected_pivots() -> Vec<Rational> {
    vec![
        Rational::from_i128(4).expect("4 is a rational"),
        Rational::new(11, 4).expect("11/4 is a rational"),
        Rational::new(18, 11).expect("18/11 is a rational"),
    ]
}

/// Dispatch and RUN a `math:` flagship's `gmeow:demonstratedByProducer`, asserting the executed
/// output equals the pinned falsifiable datum. Math producers are self-contained native
/// functions, so (unlike `lang:`) the callback needs no pipeline catalog.
fn run_producer(flagship: &Flagship, _ctx: &FlagshipCtx<'_>) {
    match flagship.producer.as_str() {
        // e8Symmetry: |W(E8)| is the exact i128 product of the E8 invariant degrees.
        "math::producers::e8_weyl_order" => {
            let out = producers::e8_weyl_order();
            assert_eq!(
                out.order, 696_729_600,
                "E8 Weyl-group order must be the exact |W(E8)| = 696729600"
            );
            assert!(
                out.turtle.contains("696729600"),
                "the folded E8 corpus must carry the computed Weyl order"
            );
            // No-drift tie: the worked-example fixture the manifest names IS the producer output.
            assert_producer_matches_example(flagship, &out.turtle);
        }
        // homomorphicEncryption: the additive-homomorphic law Dec(Enc(a)⊕Enc(b)) = a+b holds.
        "math::producers::additive_he_demo" => {
            let out = producers::additive_he_demo();
            assert_eq!(
                out.decrypted_sum,
                (out.a + out.b).rem_euclid(out.modulus),
                "additive-HE: Dec(Enc(a)⊕Enc(b)) must equal (a+b) mod q"
            );
            assert_eq!(
                out.decrypted_sum,
                out.a + out.b,
                "the pinned operands stay within the modulus, so the plaintext sum is exact"
            );
            // No-drift tie: the worked-example fixture the manifest names IS the producer output.
            assert_producer_matches_example(flagship, &out.turtle);
        }
        // proofAsProcess: the emitted verification result carries its grounding edge.
        "math::producers::proof_ingest" => {
            let out = producers::proof_ingest();
            assert!(
                out.grounded,
                "proof-ingest must emit a verification result grounded in its process"
            );
            // No-drift tie: the worked-example fixture the manifest names IS the producer output.
            assert_producer_matches_example(flagship, &out.turtle);
        }
        // rBridge: the ingest run lifts exactly the pinned observation count (non-empty lift).
        "math::producers::r_bridge_lift" => {
            let out = producers::r_bridge_lift();
            assert_eq!(
                out.lifted_observations, 5,
                "the R-bridge lift must produce the pinned number of ingest observations"
            );
            // No-drift tie: the worked-example fixture the manifest names IS the producer output.
            assert_producer_matches_example(flagship, &out.turtle);
        }
        // aiSelfStructure: the exact-rational PCA's dominant axis and LDLᵀ pivots are exact.
        "math::producers::exact_pca_residual" => {
            let out = producers::exact_pca_residual();
            assert_eq!(
                out.dominant_axis, 0,
                "the exact PCA dominant axis over the pinned Gram matrix is index 0"
            );
            assert_eq!(
                out.ldlt_pivots,
                expected_pivots(),
                "the exact LDLᵀ pivots must be [4, 11/4, 18/11] over the pinned Gram matrix"
            );
            // No-drift tie: the worked-example fixture the manifest names IS the producer output
            // (the F5 fix — the example now types as math:PCAAnalysis/math:GramMatrix, matching
            // exact_pca_residual, not the mismatched math:TensorComputationGraph it once named).
            assert_producer_matches_example(flagship, &out.turtle);
        }
        other => panic!("unknown math flagship producer identifier: {other}"),
    }
}

/// Assert the worked-example fixture the manifest binds to `flagship`
/// (`gmeow:demonstratedByExample`) is GRAPH-ISOMORPHIC to the producer's emitted Turtle — the
/// dogfooding no-drift tie: the example the manifest names IS the object the producer emits, so
/// a producer change that is not mirrored in the committed fixture (or vice versa) HARD-fails
/// here. Isomorphism (not byte-equality) because the fixture carries an SPDX header and a
/// `mprod:` prefix label where the producer emits `p:` — the SAME expanded IRIs and literals.
fn assert_producer_matches_example(flagship: &Flagship, producer_turtle: &str) {
    let fixture = std::fs::read(&flagship.example).unwrap_or_else(|e| {
        panic!(
            "read worked-example fixture {}: {e}",
            flagship.example.display()
        )
    });
    let fixture_quads = canonical_quad_set(&fixture).unwrap_or_else(|| {
        panic!(
            "parse worked-example fixture {}",
            flagship.example.display()
        )
    });
    let producer_quads = canonical_quad_set(producer_turtle.as_bytes())
        .expect("parse producer Turtle for the isomorphism tie");
    assert_eq!(
        producer_quads,
        fixture_quads,
        "producer {} drifted from its worked example {}: the manifest's \
         gmeow:demonstratedByExample MUST be graph-isomorphic to the producer output",
        flagship.producer,
        flagship.example.display()
    );
}

/// The RDFC-1.0-canonical quad set of a Turtle document, as sorted N-Quads lines. `None` on a
/// parse error. Mirrors the pipeline's own isomorphism check: flatten to the plain-quad stream,
/// re-freeze, canonicalize.
fn canonical_quad_set(bytes: &[u8]) -> Option<std::collections::BTreeSet<String>> {
    let ir = purrdf::parse_dataset(bytes, "text/turtle", None).ok()?;
    let quads = purrdf::flat_rdf_quads_from_dataset(&ir);
    let flat = purrdf::flat_dataset_from_quads(&quads).ok()?;
    Some(
        purrdf::canonicalize(&flat)
            .nquads
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}
