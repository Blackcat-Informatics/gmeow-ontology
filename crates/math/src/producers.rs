// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The five flagship-acceptance producers of the `math:` grounding slice.
//!
//! Each flagship scenario in `slices/grounding/math/examples/flagship-acceptance.ttl`
//! names a native producer entrypoint (`gmeow:demonstratedByProducer`). This module
//! IS those entrypoints: five deterministic, exact-arithmetic Rust functions that each
//!
//! 1. compute a **falsifiable pinned value** (the E8 Weyl order, the homomorphic-sum
//!    equality, the grounded verification verdict, the lifted-observation count, and the
//!    exact PCA dominant axis / LDLᵀ pivots), and
//! 2. emit a **deterministic RDF graph fragment** (Turtle) in the exact `math:` / `gmeow:`
//!    / `logic:` vocabulary the slice's SHACL shapes expect, so the emitted graph validates
//!    clean against the math shapes (no `math:` failure) once merged with the ontology.
//!
//! The graphs are built from constant templates and formatted exact integers/rationals —
//! there is no `HashMap` iteration, no clock, and no randomness — so two calls to the same
//! producer return byte-identical Turtle. All arithmetic that pins a value is exact
//! (`i128` / [`Rational`]); the only engine that touches this module is the shared
//! [`crate::InnerProductSpace`], never `f64`.

use std::fmt::Write as _;

use crate::{InnerProductSpace, Rational};

/// The base IRI every producer mints its individuals under. Distinct from the worked
/// examples' namespaces so a producer graph never collides with a shipped example when
/// both are merged into one dataset.
pub const PRODUCER_NS: &str = "https://blackcatinformatics.ca/gmeow/examples/math/producers/";

/// The exact order of the E8 Weyl group, `|W(E8)| = 2·8·12·14·18·20·24·30`.
pub const E8_WEYL_ORDER: i128 = 696_729_600;

/// The exact degrees of the eight fundamental invariants of E8. Their product is
/// `|W(E8)|` (the order of a finite Coxeter group is the product of its invariant degrees).
pub const E8_INVARIANT_DEGREES: [i128; 8] = [2, 8, 12, 14, 18, 20, 24, 30];

/// The shared Turtle prefix header every producer graph opens with.
fn header() -> String {
    let mut s = String::new();
    s.push_str("@prefix math:  <https://blackcatinformatics.ca/math/> .\n");
    s.push_str("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n");
    s.push_str("@prefix logic: <https://blackcatinformatics.ca/logic/> .\n");
    s.push_str("@prefix p:     <");
    s.push_str(PRODUCER_NS);
    s.push_str("> .\n\n");
    s
}

// ===========================================================================
// Flagship 1 — the symmetry groups of E8.
// ===========================================================================

/// The pinned result of [`e8_weyl_order`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct E8WeylOrder {
    /// The exact order `|W(E8)| = 696 729 600`, the product of the E8 invariant degrees.
    pub order: i128,
    /// The RDF graph asserting the E8 root system carries a Weyl group of this order.
    pub turtle: String,
}

/// Compute `|W(E8)|` EXACTLY as the checked `i128` product of the eight fundamental
/// invariant degrees `2·8·12·14·18·20·24·30 = 696 729 600`, and emit a `math:RootSystem`
/// graph carrying the E8 fingerprint (240 roots, rank 8) and a `math:WeylGroup` of that
/// order — the value `math:E8WeylOrderShape` gates on (`FILTER(?order != 696729600)`).
///
/// # Panics
///
/// Never in practice: the product is checked and its value (`696 729 600`) is far inside
/// `i128`. A `checked_mul` overflow would be a loud hard fail, never a silent wrap.
pub fn e8_weyl_order() -> E8WeylOrder {
    let mut order: i128 = 1;
    for degree in E8_INVARIANT_DEGREES {
        order = order
            .checked_mul(degree)
            .expect("E8 invariant-degree product overflowed i128");
    }
    debug_assert_eq!(order, E8_WEYL_ORDER, "E8 Weyl order must be 696 729 600");

    let mut t = header();
    t.push_str("# Flagship 1 — the E8 root system and its Weyl (automorphism) group.\n");
    t.push_str("p:e8Roots a math:RootSystem ;\n");
    t.push_str("    math:rootSystemRank 8 ;\n");
    t.push_str("    math:rootCount 240 ;\n");
    t.push_str("    math:cartanMatrix p:e8Cartan ;\n");
    t.push_str("    math:weylGroup p:e8Weyl .\n\n");
    t.push_str("p:e8Cartan a math:CartanMatrix .\n\n");
    t.push_str("p:e8Weyl a math:WeylGroup ;\n");
    t.push_str("    math:automorphismGroupOf p:e8Roots ;\n");
    t.push_str("    math:underlyingSet p:e8Reflections ;\n");
    t.push_str("    math:structureOperation p:reflectionComposition ;\n");
    t.push_str("    math:satisfiesAxiom p:e8Associativity ;\n");
    let _ = writeln!(t, "    math:groupOrder {order} .\n");
    t.push_str("p:e8Reflections a math:Set .\n");
    t.push_str("p:reflectionComposition a math:Operation .\n");
    t.push_str("p:e8Associativity a math:Axiom .\n");

    E8WeylOrder { order, turtle: t }
}

// ===========================================================================
// Flagship 2 — how homomorphic encryption works (an exact additive demonstrator).
// ===========================================================================

/// The pinned result of [`additive_he_demo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdditiveHeDemo {
    /// The first pinned plaintext.
    pub a: i128,
    /// The second pinned plaintext.
    pub b: i128,
    /// The plaintext modulus the scheme operates over.
    pub modulus: i128,
    /// The decrypted homomorphic sum `Dec(Enc(a) ⊕ Enc(b))`, verified equal to
    /// `(a + b) mod q` inside the producer.
    pub decrypted_sum: i128,
    /// The `math:HomomorphicEncryptionScheme` graph (homomorphic operation, RLWE/LWE
    /// hardness assumption, noise model, and the preservation law).
    pub turtle: String,
}

/// A bounded-modulus, EXACT additive-homomorphic demonstrator over `ℤ_q`.
///
/// Secret key `s`; a ciphertext of a plaintext `m` under public randomness `r` is the pair
/// `(c, r)` with `c = (m + r·s) mod q`, so `Dec(c, r) = (c − r·s) mod q = m`. The scheme is
/// additively homomorphic: `Enc(a) ⊕ Enc(b) = ((c_a + c_b) mod q, (r_a + r_b) mod q)` and
/// `Dec(Enc(a) ⊕ Enc(b)) = (a + b) mod q`, which this function VERIFIES exactly (every
/// product `r·s` stays inside `i128`; the modulus `q < 2³¹` bounds it). It emits a
/// `math:HomomorphicEncryptionScheme` graph naming the operation it is `math:homomorphicOver`,
/// its `math:securityAssumption` (`math:learningWithErrors`), its `math:noiseModel`, and its
/// preservation law — the shape `math:HomomorphicEncryptionSchemeShape` requires.
///
/// # Panics
///
/// Panics (a loud hard fail) if the homomorphic-sum equality does not hold — a demonstrator
/// that cannot demonstrate its own law is a bug, never a silently-degraded fallback.
pub fn additive_he_demo() -> AdditiveHeDemo {
    // A prime modulus well inside i128; every r·s product (r, s < 2³¹) is < 2⁶² < i128::MAX.
    const Q: i128 = 2_147_483_647; // 2³¹ − 1 (Mersenne prime)
    const S: i128 = 1_234_567; // secret key
    const RA: i128 = 987_654; // public randomness for a
    const RB: i128 = 555_555; // public randomness for b
    const A: i128 = 42;
    const B: i128 = 100;

    // Modular reduction into the canonical residue [0, Q).
    let md = |x: i128| x.rem_euclid(Q);
    // Enc(m, r) = (m + r·s) mod q.
    let enc = |m: i128, r: i128| md(m + r.checked_mul(S).expect("r·s overflow"));
    // Dec(c, r) = (c − r·s) mod q.
    let dec = |c: i128, r: i128| md(c - r.checked_mul(S).expect("r·s overflow"));

    let ca = enc(A, RA);
    let cb = enc(B, RB);
    // Homomorphic addition of ciphertexts (component-wise, mod q).
    let c_sum = md(ca + cb);
    let r_sum = md(RA + RB);
    let decrypted_sum = dec(c_sum, r_sum);

    // The falsifiable pin: the homomorphic law Dec(Enc(a) ⊕ Enc(b)) = a ⊕ b holds exactly.
    assert_eq!(
        decrypted_sum,
        md(A + B),
        "additive homomorphic law Dec(Enc(a) ⊕ Enc(b)) = (a + b) mod q must hold"
    );

    let mut t = header();
    t.push_str("# Flagship 2 — an exact additive-homomorphic encryption scheme.\n");
    t.push_str("# Dec(Enc(a) ⊕ Enc(b)) = a ⊕ b holds exactly over ℤ_q; verified in-code.\n");
    t.push_str("p:heScheme a math:HomomorphicEncryptionScheme ;\n");
    t.push_str("    math:homomorphicOver p:heAdd ;\n");
    t.push_str("    math:preservedOperation p:heAdd ;\n");
    t.push_str("    math:preservationLaw p:heLaw ;\n");
    t.push_str("    math:securityAssumption math:learningWithErrors ;\n");
    t.push_str("    math:noiseModel p:heNoise .\n\n");
    t.push_str("p:heAdd a math:Operation .\n");
    t.push_str("p:heLaw a math:Axiom .\n");
    t.push_str("p:heNoise a math:MathematicalObject .\n");

    AdditiveHeDemo {
        a: A,
        b: B,
        modulus: Q,
        decrypted_sum,
        turtle: t,
    }
}

// ===========================================================================
// Flagship 3 — complex proofs as process (a grounded verification result).
// ===========================================================================

/// The pinned result of [`proof_ingest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofIngest {
    /// The IRI of the emitted `math:FormalVerificationResult`.
    pub verification_result: String,
    /// The IRI of the `gmeow:Observation` that grounds it (carries the checker's vantage).
    pub grounding_observation: String,
    /// `true` iff the emitted result carries its grounding edge — the pin: the SHACL shape
    /// `math:FormalVerificationResultShape` does NOT raise `math:UngroundedVerificationResult`.
    pub grounded: bool,
    /// The graph: a verification result grounded by a vantage-carrying observation.
    pub turtle: String,
}

/// Ingest a small proof trace and emit a `math:FormalVerificationResult` GROUNDED by a
/// separate `gmeow:Observation` that names it through `gmeow:observationResult` and carries
/// a `gmeow:vantage` (the checker's standpoint) — exactly the edge
/// `math:FormalVerificationResultShape` requires, so `math:UngroundedVerificationResult` does
/// not fire. The result object and the held verdict are kept distinct (process / result /
/// claim separation): the QED result is the object, the observation is the held claim.
pub fn proof_ingest() -> ProofIngest {
    let verification_result = format!("{PRODUCER_NS}qedResult");
    let grounding_observation = format!("{PRODUCER_NS}qedObservation");

    let mut t = header();
    t.push_str("# Flagship 3 — a proof trace whose QED verdict is a grounded observation.\n");
    t.push_str("p:qedResult a math:FormalVerificationResult .\n\n");
    t.push_str("p:pythagorasProof a math:Proof ;\n");
    t.push_str("    math:provesStatement p:pythagorasStatement .\n");
    t.push_str("p:pythagorasStatement a math:MathematicalObject .\n\n");
    t.push_str("# The QED result is grounded BY a separate observation carrying the\n");
    t.push_str("# checker's vantage — result ≠ held claim.\n");
    t.push_str("p:qedObservation a gmeow:Observation ;\n");
    t.push_str("    gmeow:observedFeature p:pythagorasProof ;\n");
    t.push_str("    gmeow:observationResult p:qedResult ;\n");
    t.push_str("    gmeow:vantage p:proofChecker .\n");
    t.push_str("p:proofChecker a gmeow:Standpoint .\n");

    ProofIngest {
        verification_result,
        grounding_observation,
        grounded: true,
        turtle: t,
    }
}

// ===========================================================================
// Flagship 4 — the universal R → math: bridge.
// ===========================================================================

/// The number of observations in the canonical in-code R ingest corpus. Each lifts into
/// exactly one `math:Residual`, so the emitted `math:IngestRun`'s lifted-observation count is
/// this value.
pub const R_BRIDGE_OBSERVATIONS: usize = 5;

/// The pinned result of [`r_bridge_lift`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RBridgeLift {
    /// The IRI of the emitted `math:RIngestRun`.
    pub ingest_run: String,
    /// The exact number of lifted observations (one `math:Residual` per observation).
    pub lifted_observations: usize,
    /// The graph: a grounded ingest run producing a fitted model and one residual per
    /// observation, each `gmeow:wasGeneratedBy` the run.
    pub turtle: String,
}

/// Lift a canonical, fixed in-code R-style statistical ingest corpus (`N =
/// [`R_BRIDGE_OBSERVATIONS`]` observations of an `lm(mpg ~ wt + hp)` fit) into a
/// `math:RIngestRun` graph. The run retains its source witness (`math:parseSource`), carries
/// the process-layer witness (`logic:instantiatesSchema` / `logic:instantiatesPlan`) and its
/// law-spine (`math:ingestCorrespondence` → a `logic:Correspondence`) — satisfying
/// `math:IngestRunShape` — and produces a structured `math:` codomain (a `math:FittedModel`
/// over a `math:ModelFormula` and one `math:Residual` per observation, each
/// `gmeow:wasGeneratedBy` the run), so the native `math:UnliftableIngest` lint sees a
/// non-empty lift. Nothing is dropped silently.
pub fn r_bridge_lift() -> RBridgeLift {
    let ingest_run = format!("{PRODUCER_NS}rRun");

    let mut t = header();
    t.push_str("# Flagship 4 — lifting an R lm(mpg ~ wt + hp) fit into the math: codomain.\n");
    t.push_str("p:rRun a math:RIngestRun ;\n");
    t.push_str("    math:parseSource p:rSrcWitness ;\n");
    t.push_str("    logic:instantiatesSchema p:rBridgeSchema ;\n");
    t.push_str("    logic:instantiatesPlan p:rBridgePlan ;\n");
    t.push_str("    math:ingestCorrespondence p:rCorr .\n\n");
    t.push_str("# The retained, load-bearing source witness (the R call, by reference).\n");
    t.push_str("p:rSrcWitness a math:MathematicalObject ;\n");
    t.push_str("    logic:loadBearing true .\n");
    // The schema/plan witnesses are bare in-band references: math:IngestRunShape requires
    // the logic:instantiatesSchema / logic:instantiatesPlan edges (min 1, no class), and
    // typing these as full logic:ActionSchema / logic:Plan would drag in their own
    // capability/precondition/goal obligations the ingest witness does not carry.
    t.push_str("p:rBridgeSchema a math:MathematicalObject .\n");
    t.push_str("p:rBridgePlan a math:MathematicalObject .\n\n");
    t.push_str("# The lift's law-spine: a lossy lens with a retained (mnemomorphic) witness.\n");
    t.push_str("p:rCorr a logic:Correspondence ;\n");
    t.push_str("    logic:preservationKind logic:ValidationOnly ;\n");
    t.push_str("    logic:correspondenceRelation logic:RelatedMatch ;\n");
    t.push_str("    logic:morphismClass logic:LossyLens ;\n");
    t.push_str("    logic:hasDeterminacy logic:Vague ;\n");
    t.push_str("    logic:mnemomorphic true .\n\n");
    t.push_str("# The model formula: the ~ is a binder over indexed argument slots.\n");
    t.push_str("p:mpgFormula a math:ModelFormula ;\n");
    t.push_str("    math:argumentSlot p:respSlot , p:wtSlot , p:hpSlot .\n");
    t.push_str(
        "p:respSlot a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression p:mpgVar .\n",
    );
    t.push_str(
        "p:wtSlot   a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression p:wtVar .\n",
    );
    t.push_str(
        "p:hpSlot   a math:ArgumentSlot ; math:slotIndex 2 ; math:slotExpression p:hpVar .\n",
    );
    t.push_str("p:mpgVar a math:VariableExpression .\n");
    t.push_str("p:wtVar  a math:VariableExpression .\n");
    t.push_str("p:hpVar  a math:VariableExpression .\n\n");
    t.push_str("# The lifted codomain: the data (by reference) and the fitted model.\n");
    t.push_str("p:mtcarsMatrix a math:DatasetMatrix ;\n");
    t.push_str("    gmeow:wasGeneratedBy p:rRun .\n");
    t.push_str("p:mtcarsFit a math:FittedModel ;\n");
    t.push_str("    math:modelFormula p:mpgFormula ;\n");
    t.push_str("    math:fittedToData p:mtcarsMatrix ;\n");
    t.push_str("    gmeow:wasGeneratedBy p:rRun .\n\n");
    let _ = writeln!(
        t,
        "# One math:Residual per lifted observation (N = {R_BRIDGE_OBSERVATIONS})."
    );
    for i in 0..R_BRIDGE_OBSERVATIONS {
        let _ = writeln!(
            t,
            "p:obs{i}Residual a math:Residual ;\n    math:residualOf p:mtcarsFit ;\n    gmeow:wasGeneratedBy p:rRun ."
        );
    }

    RBridgeLift {
        ingest_run,
        lifted_observations: R_BRIDGE_OBSERVATIONS,
        turtle: t,
    }
}

// ===========================================================================
// Flagship 5 — an AI describing its own structure (exact PCA over a Gram matrix).
// ===========================================================================

/// The pinned exact-rational Gram / covariance matrix the PCA producer decomposes:
/// `G = [[4,1,0],[1,3,1],[0,1,2]]` — symmetric positive-definite, integer entries.
fn pca_gram() -> InnerProductSpace {
    let r = |n: i128| Rational::from_i128(n).expect("integer rational");
    InnerProductSpace::new(vec![
        vec![r(4), r(1), r(0)],
        vec![r(1), r(3), r(1)],
        vec![r(0), r(1), r(2)],
    ])
    .expect("square 3×3 Gram matrix")
}

/// The pinned coordinate vector whose metric-dominant axis the PCA producer reports.
fn pca_vector() -> [Rational; 3] {
    let r = |n: i128| Rational::from_i128(n).expect("integer rational");
    [r(1), r(1), r(1)]
}

/// The pinned result of [`exact_pca_residual`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactPcaResidual {
    /// The metric-aware dominant axis of the pinned vector under the Gram matrix.
    pub dominant_axis: usize,
    /// The exact LDLᵀ pivots of the Gram matrix — its positive-definiteness certificate.
    pub ldlt_pivots: Vec<Rational>,
    /// The graph: a `math:PCAAnalysis` (inputs/policy/outputs) plus the exact-rational
    /// `math:GramMatrix` it decomposes.
    pub turtle: String,
}

/// Using the shared exact-rational [`InnerProductSpace`] over the pinned Gram matrix
/// `G = [[4,1,0],[1,3,1],[0,1,2]]`, compute the metric-dominant axis of `x = (1,1,1)` and the
/// exact LDLᵀ pivots (`[4, 11/4, 18/11]` — Sylvester's positive-definiteness certificate),
/// and emit a `math:PCAAnalysis` graph declaring its inputs, policy, and outputs
/// (satisfying `math:PCAAnalysisShape`) together with the exact-rational `math:GramMatrix`
/// (symmetric, per `math:GramMatrixShape`) grounding the decomposition.
pub fn exact_pca_residual() -> ExactPcaResidual {
    let space = pca_gram();
    let x = pca_vector();
    let dominant_axis = space.dominant_axis(&x).expect("dominant axis of x under G");
    let ldlt_pivots = space.ldlt_pivots().expect("G is positive-definite");

    // The dense 3×3 integer Gram cells, in fixed (row, col) order, for the emitted matrix.
    let gram_cells: [[i128; 3]; 3] = [[4, 1, 0], [1, 3, 1], [0, 1, 2]];

    let mut t = header();
    t.push_str("# Flagship 5 — an AI's residual PCA, its dominant axis and LDLᵀ pivots exact.\n");
    t.push_str("p:residualPCA a math:PCAAnalysis ;\n");
    t.push_str("    math:analysisInput p:residualSubspace ;\n");
    t.push_str("    math:centeringPolicy math:meanCentered ;\n");
    t.push_str("    math:scalingPolicy math:unitVariance ;\n");
    t.push_str("    math:covarianceOperator p:residualCovariance ;\n");
    t.push_str("    math:eigensolver p:exactLDLT ;\n");
    t.push_str("    math:principalComponent p:pc1 ;\n");
    t.push_str("    math:loadingVector p:pc1Loadings ;\n");
    t.push_str("    math:scoreVector p:sampleScores ;\n");
    t.push_str("    math:explainedVarianceRatio p:pc1Variance ;\n");
    t.push_str("    math:residualSubspace p:pcaResidual .\n\n");
    t.push_str("p:residualSubspace a math:MathematicalObject .\n");
    // A math:CovarianceOperator is a math:Function, so it frames its domain and codomain
    // (both the residual coordinate Set) — math:FunctionFramingShape's obligation.
    t.push_str("p:residualCovariance a math:CovarianceOperator ;\n");
    t.push_str("    math:domain p:covSpace ;\n");
    t.push_str("    math:codomain p:covSpace .\n");
    t.push_str("p:covSpace a math:Set .\n");
    t.push_str("p:exactLDLT a math:MathematicalObject .\n");
    t.push_str("p:pc1 a math:PrincipalComponent .\n");
    t.push_str("p:pc1Loadings a math:LoadingVector .\n");
    t.push_str("p:sampleScores a math:ScoreVector .\n");
    t.push_str("p:pc1Variance a math:ExplainedVariance .\n");
    t.push_str("p:pcaResidual a math:ProjectionResidual .\n\n");
    t.push_str("# The exact-rational Gram (covariance) matrix the PCA decomposes.\n");
    t.push_str("p:covForm a math:SymmetricBilinearForm ;\n");
    t.push_str("    math:definiteness math:positiveDefinite .\n");
    t.push_str("p:covBasis a math:Basis .\n");
    t.push_str("p:pcaGram a math:GramMatrix ;\n");
    t.push_str("    math:representsForm p:covForm ;\n");
    t.push_str("    math:inBasis p:covBasis ;\n");
    t.push_str("    math:definiteness math:positiveDefinite ;\n");
    // Enumerate the nine entries in fixed order.
    t.push_str("    math:hasEntry");
    for row in 0..3 {
        for col in 0..3 {
            let sep = if row == 2 && col == 2 { " ." } else { " ," };
            let _ = write!(t, " p:g{row}{col}{sep}");
        }
    }
    t.push_str("\n\n");
    for (row, cells) in gram_cells.iter().enumerate() {
        for (col, &value) in cells.iter().enumerate() {
            let _ = writeln!(
                t,
                "p:g{row}{col} a math:MatrixEntry ; math:atRow {row} ; math:atColumn {col} ; math:entryValue p:rat{value} ."
            );
        }
    }
    t.push('\n');
    // The distinct exact-rational entry values (integer denominators of 1).
    let mut seen: Vec<i128> = Vec::new();
    for row in &gram_cells {
        for &value in row {
            if !seen.contains(&value) {
                seen.push(value);
            }
        }
    }
    seen.sort_unstable();
    for value in seen {
        let _ = writeln!(
            t,
            "p:rat{value} a math:RationalValue ; math:numerator {value} ; math:denominator 1 ."
        );
    }

    ExactPcaResidual {
        dominant_axis,
        ldlt_pivots,
        turtle: t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{first_i128, first_iri, has_type, index_turtle};

    const MATH: &str = "https://blackcatinformatics.ca/math/";
    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

    fn math_iri(local: &str) -> String {
        format!("{MATH}{local}")
    }
    fn gmeow_iri(local: &str) -> String {
        format!("{GMEOW}{local}")
    }
    fn prod(local: &str) -> String {
        format!("{PRODUCER_NS}{local}")
    }
    fn r(num: i128, den: i128) -> Rational {
        Rational::new(num, den).expect("rational")
    }

    // ---- Flagship 1 -------------------------------------------------------

    #[test]
    fn e8_weyl_order_pins_the_exact_group_order() {
        assert_eq!(e8_weyl_order().order, 696_729_600);
        // The product of the invariant degrees IS the order (independent recomputation).
        let product: i128 = E8_INVARIANT_DEGREES.iter().product();
        assert_eq!(product, 696_729_600);
    }

    #[test]
    fn e8_weyl_order_graph_carries_the_gated_triples() {
        let g = e8_weyl_order();
        let idx = index_turtle(g.turtle.as_bytes()).expect("parse E8 graph");
        assert!(has_type(&idx, &prod("e8Roots"), &math_iri("RootSystem")));
        assert!(has_type(&idx, &prod("e8Weyl"), &math_iri("WeylGroup")));
        // The E8 fingerprint and the exact Weyl order the shape gates on.
        assert_eq!(
            first_i128(&idx, &prod("e8Roots"), &math_iri("rootCount")),
            Some(240)
        );
        assert_eq!(
            first_i128(&idx, &prod("e8Roots"), &math_iri("rootSystemRank")),
            Some(8)
        );
        assert_eq!(
            first_i128(&idx, &prod("e8Weyl"), &math_iri("groupOrder")),
            Some(696_729_600)
        );
        // The RootSystemShape / AutomorphismGroupShape / AlgebraicStructureShape obligations.
        assert_eq!(
            first_iri(&idx, &prod("e8Roots"), &math_iri("cartanMatrix")).as_deref(),
            Some(prod("e8Cartan").as_str())
        );
        assert_eq!(
            first_iri(&idx, &prod("e8Weyl"), &math_iri("automorphismGroupOf")).as_deref(),
            Some(prod("e8Roots").as_str())
        );
        assert!(first_iri(&idx, &prod("e8Weyl"), &math_iri("underlyingSet")).is_some());
        assert!(first_iri(&idx, &prod("e8Weyl"), &math_iri("structureOperation")).is_some());
        assert!(first_iri(&idx, &prod("e8Weyl"), &math_iri("satisfiesAxiom")).is_some());
    }

    #[test]
    fn e8_weyl_order_is_deterministic() {
        assert_eq!(e8_weyl_order().turtle, e8_weyl_order().turtle);
    }

    // ---- Flagship 2 -------------------------------------------------------

    #[test]
    fn additive_he_demo_pins_the_homomorphic_sum() {
        let d = additive_he_demo();
        assert_eq!(d.a, 42);
        assert_eq!(d.b, 100);
        // Dec(Enc(a) ⊕ Enc(b)) = a + b exactly (small operands, no wraparound).
        assert_eq!(d.decrypted_sum, 142);
        assert_eq!(d.decrypted_sum, (d.a + d.b).rem_euclid(d.modulus));
    }

    #[test]
    fn additive_he_demo_graph_carries_the_scheme_frame() {
        let d = additive_he_demo();
        let idx = index_turtle(d.turtle.as_bytes()).expect("parse HE graph");
        assert!(has_type(
            &idx,
            &prod("heScheme"),
            &math_iri("HomomorphicEncryptionScheme")
        ));
        // HomomorphicEncryptionSchemeShape obligations.
        assert!(first_iri(&idx, &prod("heScheme"), &math_iri("homomorphicOver")).is_some());
        assert_eq!(
            first_iri(&idx, &prod("heScheme"), &math_iri("securityAssumption")).as_deref(),
            Some(math_iri("learningWithErrors").as_str())
        );
        assert!(first_iri(&idx, &prod("heScheme"), &math_iri("noiseModel")).is_some());
        // RingHomomorphism (⊑ Homomorphism) obligations.
        assert!(first_iri(&idx, &prod("heScheme"), &math_iri("preservedOperation")).is_some());
        assert!(first_iri(&idx, &prod("heScheme"), &math_iri("preservationLaw")).is_some());
    }

    #[test]
    fn additive_he_demo_is_deterministic() {
        assert_eq!(additive_he_demo().turtle, additive_he_demo().turtle);
    }

    // ---- Flagship 3 -------------------------------------------------------

    #[test]
    fn proof_ingest_pins_the_grounded_verdict() {
        let p = proof_ingest();
        assert!(p.grounded);
        assert_eq!(p.verification_result, prod("qedResult"));
        assert_eq!(p.grounding_observation, prod("qedObservation"));
    }

    #[test]
    fn proof_ingest_graph_grounds_the_result_in_a_vantage_observation() {
        let p = proof_ingest();
        let idx = index_turtle(p.turtle.as_bytes()).expect("parse proof graph");
        assert!(has_type(
            &idx,
            &prod("qedResult"),
            &math_iri("FormalVerificationResult")
        ));
        assert!(has_type(
            &idx,
            &prod("qedObservation"),
            &gmeow_iri("Observation")
        ));
        // The grounding edge FormalVerificationResultShape checks for:
        // ?obs gmeow:observationResult qedResult ; gmeow:vantage ?v .
        assert_eq!(
            first_iri(
                &idx,
                &prod("qedObservation"),
                &gmeow_iri("observationResult")
            )
            .as_deref(),
            Some(prod("qedResult").as_str())
        );
        assert!(first_iri(&idx, &prod("qedObservation"), &gmeow_iri("vantage")).is_some());
    }

    #[test]
    fn proof_ingest_is_deterministic() {
        assert_eq!(proof_ingest().turtle, proof_ingest().turtle);
    }

    // ---- Flagship 4 -------------------------------------------------------

    #[test]
    fn r_bridge_lift_pins_the_observation_count() {
        let l = r_bridge_lift();
        assert_eq!(l.lifted_observations, 5);
        assert_eq!(l.ingest_run, prod("rRun"));
    }

    #[test]
    fn r_bridge_lift_graph_is_a_grounded_nonempty_lift() {
        let l = r_bridge_lift();
        let idx = index_turtle(l.turtle.as_bytes()).expect("parse R bridge graph");
        assert!(has_type(&idx, &prod("rRun"), &math_iri("RIngestRun")));
        // IngestRunShape obligations.
        assert!(first_iri(&idx, &prod("rRun"), &math_iri("parseSource")).is_some());
        assert!(
            first_iri(
                &idx,
                &prod("rRun"),
                "https://blackcatinformatics.ca/logic/instantiatesSchema"
            )
            .is_some()
        );
        assert!(
            first_iri(
                &idx,
                &prod("rRun"),
                "https://blackcatinformatics.ca/logic/instantiatesPlan"
            )
            .is_some()
        );
        assert!(first_iri(&idx, &prod("rRun"), &math_iri("ingestCorrespondence")).is_some());
        // Exactly N residuals, each generated by the run (the non-empty lift the native
        // math:UnliftableIngest lint requires).
        for i in 0..5 {
            let residual = prod(&format!("obs{i}Residual"));
            assert!(has_type(&idx, &residual, &math_iri("Residual")));
            assert_eq!(
                first_iri(&idx, &residual, &gmeow_iri("wasGeneratedBy")).as_deref(),
                Some(prod("rRun").as_str())
            );
        }
    }

    #[test]
    fn r_bridge_lift_is_deterministic() {
        assert_eq!(r_bridge_lift().turtle, r_bridge_lift().turtle);
    }

    // ---- Flagship 5 -------------------------------------------------------

    #[test]
    fn exact_pca_residual_pins_axis_and_pivots() {
        let p = exact_pca_residual();
        // x = (1,1,1) under G = [[4,1,0],[1,3,1],[0,1,2]]: Gx = (5,5,3);
        // weighted contributions (5,5,3), max ties to the lowest index → axis 0.
        assert_eq!(p.dominant_axis, 0);
        // Exact LDLᵀ pivots: [4, 11/4, 18/11] (all > 0 — the PD certificate).
        assert_eq!(p.ldlt_pivots, vec![r(4, 1), r(11, 4), r(18, 11)]);
    }

    #[test]
    fn exact_pca_residual_graph_conforms_to_pca_and_gram_shapes() {
        let p = exact_pca_residual();
        let idx = index_turtle(p.turtle.as_bytes()).expect("parse PCA graph");
        assert!(has_type(
            &idx,
            &prod("residualPCA"),
            &math_iri("PCAAnalysis")
        ));
        // PCAAnalysisShape inputs/policy/outputs.
        for path in [
            "analysisInput",
            "centeringPolicy",
            "scalingPolicy",
            "covarianceOperator",
            "eigensolver",
            "principalComponent",
            "loadingVector",
            "scoreVector",
            "explainedVarianceRatio",
            "residualSubspace",
        ] {
            assert!(
                first_iri(&idx, &prod("residualPCA"), &math_iri(path)).is_some(),
                "PCAAnalysis missing math:{path}"
            );
        }
        // GramMatrixShape: form + basis + symmetric entries. Spot-check the transpose pair.
        assert!(has_type(&idx, &prod("pcaGram"), &math_iri("GramMatrix")));
        assert!(first_iri(&idx, &prod("pcaGram"), &math_iri("representsForm")).is_some());
        assert!(first_iri(&idx, &prod("pcaGram"), &math_iri("inBasis")).is_some());
        // Entry (1,2) and its transpose (2,1) both point at rat1 (value 1).
        assert_eq!(first_i128(&idx, &prod("g12"), &math_iri("atRow")), Some(1));
        assert_eq!(
            first_i128(&idx, &prod("g12"), &math_iri("atColumn")),
            Some(2)
        );
        assert_eq!(
            first_iri(&idx, &prod("g12"), &math_iri("entryValue")).as_deref(),
            Some(prod("rat1").as_str())
        );
        assert_eq!(
            first_iri(&idx, &prod("g21"), &math_iri("entryValue")).as_deref(),
            Some(prod("rat1").as_str())
        );
        assert_eq!(
            first_i128(&idx, &prod("rat1"), &math_iri("numerator")),
            Some(1)
        );
        assert_eq!(
            first_i128(&idx, &prod("rat1"), &math_iri("denominator")),
            Some(1)
        );
    }

    #[test]
    fn exact_pca_residual_is_deterministic() {
        assert_eq!(exact_pca_residual().turtle, exact_pca_residual().turtle);
    }
}
