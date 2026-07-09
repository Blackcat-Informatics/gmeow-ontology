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

mod support;
use support::flagship_discharge::{
    Flagship, FlagshipCtx, SliceSpec, repo_root, run_flagship_discharge,
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
    run_flagship_discharge(&math_spec(), 5, &run_producer);
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
        }
        // proofAsProcess: the emitted verification result carries its grounding edge.
        "math::producers::proof_ingest" => {
            let out = producers::proof_ingest();
            assert!(
                out.grounded,
                "proof-ingest must emit a verification result grounded in its process"
            );
        }
        // rBridge: the ingest run lifts exactly the pinned observation count (non-empty lift).
        "math::producers::r_bridge_lift" => {
            let out = producers::r_bridge_lift();
            assert_eq!(
                out.lifted_observations, 5,
                "the R-bridge lift must produce the pinned number of ingest observations"
            );
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
        }
        other => panic!("unknown math flagship producer identifier: {other}"),
    }
}
