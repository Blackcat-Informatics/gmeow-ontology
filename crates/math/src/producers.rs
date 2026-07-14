// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The seven native producers of the `math:` grounding slice: five flagship-acceptance
//! producers, the probability layer's live `logic:probabilityModel` seam producer, and the
//! signature `lang:` → `logic:` → `math:` p-value tri-slice producer.
//!
//! Each flagship scenario in `slices/grounding/math/examples/flagship-acceptance.ttl`
//! names a native producer entrypoint (`gmeow:demonstratedByProducer`). This module IS
//! those five entrypoints — deterministic, exact-arithmetic Rust functions that each
//!
//! 1. compute a **falsifiable pinned value** (the E8 Weyl order, the homomorphic-sum
//!    equality, the grounded verification verdict, the lifted-observation count, and the
//!    exact PCA dominant axis / LDLᵀ pivots), and
//! 2. emit a **deterministic RDF graph fragment** (Turtle) in the exact `math:` / `gmeow:`
//!    / `logic:` vocabulary the slice's SHACL shapes expect, so the emitted graph validates
//!    clean against the math shapes (no `math:` failure) once merged with the ontology.
//!
//! [`probability_model_seam`] is a SIXTH producer, folded into the bundle the SAME way
//! (Design A) but NOT bound to a `gmeow:FlagshipScenario` — the flagship manifest's "five,
//! not adjectives" depth-bar contract stays exactly five. It exists so the probability
//! layer's `logic:probabilityModel` reasoning seam has a LIVE A-box crossing triple inside
//! `gmeow.gts` itself, not merely in the illustrative `examples/probability.ttl` fixture
//! (which — like every `examples/*.ttl` worked example across the whole ontology — is
//! validated on disk by `make validate` but never folded into the shipped bundle; only
//! `module.ttl` + `imports/*.ttl` feed the bundle's authored default graph, and Design A's
//! native producers are the one established path for demonstrator A-box content to ride
//! inside `gmeow.gts` as queryable RDF).
//!
//! [`pvalue_tri_slice`] is a SEVENTH producer, folded the SAME way and likewise NOT
//! flagship-bound. It carries the charter's signature tri-slice round-trip — the sentence
//! "the p-value was 0.03" grounded as a `lang:SurfaceForm` → a `logic:Formula` → a
//! well-framed `math:PValue` with a framed `math:pValue` measure — as ONE genuinely grounded
//! chain (a `lang:denotesLogicFormula` denotation whose target is the formula, and the
//! formula predicating over the specific p-value) inside `gmeow.gts` itself.
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
    // The vantage is asserted BOTH a Standpoint and a gmeow:Entity: the canonical
    // gmeow:Observation validation shape checks `sh:class gmeow:Entity` on the raw graph
    // (no subclass inference), so the producer states the type its output must validate under.
    t.push_str("p:proofChecker a gmeow:Standpoint , gmeow:Entity .\n");

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

// ===========================================================================
// Sixth producer — the probability layer's live logic:probabilityModel seam.
// ===========================================================================

/// The rain×sprinkler joint table's four exact outcome masses: `3/10, 2/10, 4/10, 1/10`
/// (mirroring `examples/probability.ttl`'s `ex:rainSprinklerJoint`).
fn sprinkler_joint_masses() -> [Rational; 4] {
    [
        Rational::new(3, 10).expect("3/10 is a rational"),
        Rational::new(2, 10).expect("2/10 is a rational"),
        Rational::new(4, 10).expect("4/10 is a rational"),
        Rational::new(1, 10).expect("1/10 is a rational"),
    ]
}

/// The pinned result of [`probability_model_seam`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbabilityModelSeam {
    /// The exact sum of the four joint-outcome masses — pinned at exactly one, the
    /// completeness condition the table's declared `logic:ExactPreservation` claims.
    pub joint_mass_total: Rational,
    /// The IRI of the `logic:World` carrying the LIVE `logic:probabilityModel` crossing
    /// triple.
    pub world: String,
    /// The IRI of the `math:BayesianNetwork` the world's probability model names.
    pub model: String,
    /// The graph: a rain×sprinkler joint table whose four outcomes sum to exactly one, a
    /// `math:BayesianNetwork` over the same scene, and a `logic:World` naming that network
    /// through the LIVE `logic:probabilityModel` crossing triple.
    pub turtle: String,
}

/// Emit the `math:` ↔ `logic:` probability-model reasoning seam.
///
/// Sums the rain×sprinkler joint table's four outcome masses (`3/10, 2/10, 4/10, 1/10`)
/// EXACTLY (as [`Rational`], never `f64`) and asserts they total exactly one — the
/// completeness condition `math:JointProbabilityTable`'s declared `logic:ExactPreservation`
/// claims — then emits a `math:BayesianNetwork` over the same scene, referenced by a
/// `logic:World` through the LIVE `logic:probabilityModel` crossing triple: a worked
/// probability model reference folded into `gmeow.gts` itself (Design A, mirroring the five
/// flagship producers), not merely a fixture behind a test-side gate.
///
/// # Panics
///
/// Panics (a loud hard fail) if the four masses do not sum to exactly one — a demonstrator
/// whose own joint table is not a probability measure is a bug, never a silently-degraded
/// fallback.
pub fn probability_model_seam() -> ProbabilityModelSeam {
    let masses = sprinkler_joint_masses();
    let mut total = Rational::zero();
    for mass in masses {
        total = total
            .checked_add(mass)
            .expect("exact joint-mass sum overflow");
    }
    assert_eq!(
        total,
        Rational::one(),
        "the rain×sprinkler joint table's outcome masses must sum to EXACTLY one"
    );

    let world = format!("{PRODUCER_NS}forecastWorld");
    let model = format!("{PRODUCER_NS}sprinklerNet");

    let mut t = header();
    t.push_str("# The probability layer's live logic:probabilityModel seam producer.\n");
    t.push_str("# The rain×sprinkler joint table's four outcomes sum to EXACTLY one\n");
    t.push_str("# (3/10 + 2/10 + 4/10 + 1/10 = 1), verified exactly (Rational, never f64).\n");
    t.push_str("p:rainSprinklerJoint a math:JointProbabilityTable ;\n");
    t.push_str("    logic:jointOutcome p:joRainOn , p:joRainOff , p:joDryOn , p:joDryOff .\n\n");
    t.push_str("p:joRainOn  logic:jointProbability 0.3 .\n");
    t.push_str("p:joRainOff logic:jointProbability 0.2 .\n");
    t.push_str("p:joDryOn   logic:jointProbability 0.4 .\n");
    t.push_str("p:joDryOff  logic:jointProbability 0.1 .\n\n");
    t.push_str("# A Bayesian network over the same rain×sprinkler scene, naming its\n");
    t.push_str("# dependency graph — the TBox lowering (math:probabilityModelLowering) on\n");
    t.push_str("# math:BayesianNetwork carries the declared logic: crossing.\n");
    t.push_str("p:sprinklerNet a math:BayesianNetwork ;\n");
    t.push_str("    math:dependencyGraph p:sprinklerDag ;\n");
    t.push_str("    logic:preservationKind logic:ExactPreservation .\n\n");
    t.push_str("p:sprinklerDag a math:MathematicalObject .\n\n");
    t.push_str("# The LIVE logic:probabilityModel crossing triple this producer exists to\n");
    t.push_str("# fold into gmeow.gts: a logic:World naming its probability model.\n");
    t.push_str("p:forecastWorld a logic:World ;\n");
    t.push_str("    logic:probabilityModel p:sprinklerNet .\n");

    ProbabilityModelSeam {
        joint_mass_total: total,
        world,
        model,
        turtle: t,
    }
}

// ===========================================================================
// Seventh producer — the signature lang: → logic: → math: p-value tri-slice.
// ===========================================================================

/// The base IRI the tri-slice producer mints every individual under — a content-free,
/// stable example base, distinct from [`PRODUCER_NS`] so its scene never collides with a
/// worked example when both are merged into one dataset. Its `#`-fragment individuals keep
/// the base itself a single document IRI.
pub const PVALUE_TRI_SLICE_NS: &str =
    "https://blackcatinformatics.ca/gmeow/examples/math/statistics/pvalue-tri-slice#";

/// The exact rational magnitude the tri-slice p-value carries — `3/100 = 0.03`, the value in
/// the charter's signature sentence "the p-value was 0.03".
fn pvalue_tri_slice_magnitude() -> Rational {
    Rational::new(3, 100).expect("3/100 is a rational")
}

/// The pinned result of [`pvalue_tri_slice`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PValueTriSlice {
    /// The exact rational p-value magnitude, pinned at `3/100` and verified in `[0, 1]`.
    pub magnitude: Rational,
    /// The IRI of the `lang:SurfaceForm` carrying the sentence "the p-value was 0.03".
    pub surface: String,
    /// The IRI of the `logic:Formula` the sentence's `lang:Denotation` targets.
    pub formula: String,
    /// The IRI of the `math:PValue` the formula predicates over — the terminus of the
    /// grounded lang → logic → math chain.
    pub pvalue: String,
    /// The graph: the sentence "the p-value was 0.03" grounded as a `lang:SurfaceForm` that
    /// `lang:realizes` a `lang:ComposedForm`, a `lang:Denotation`
    /// (`lang:denotesLogicFormula`) whose `lang:denotationTarget` is a `logic:Formula`, and
    /// that formula predicating over a well-framed `math:PValue` (a framed `math:pValue`
    /// magnitude, a parameterized `math:nullDistribution`, a `math:testStatistic`, a
    /// `math:alternativeSidedness`) produced by a `math:InferenceRun` and held by a
    /// `gmeow:Observation`.
    pub turtle: String,
}

/// Emit the charter's signature tri-slice round-trip: the sentence "the p-value was 0.03"
/// grounded as a `lang:` surface form → a `logic:` formula → a well-framed `math:PValue`.
///
/// The p-value magnitude `3/100` is carried EXACTLY (as [`Rational`], never `f64`) and
/// verified to lie in the probability range `[0, 1]` — the framing condition
/// `math:ProbabilityValueRangeShape` gates on. The single scene links the three slices with
/// genuine crossing triples (it is one grounded chain, not three disconnected islands):
///
/// * lang → logic — a `lang:Denotation` with `lang:denotationKind lang:denotesLogicFormula`
///   whose `lang:denotationTarget` is a specific `logic:Formula`;
/// * logic → math — that `logic:Formula` is an atomic predication whose first argument's
///   `logic:termIri` is the specific `math:PValue`.
///
/// The p-value carries its whole frame (`math:pValue` → a framed `math:ProbabilityValue`,
/// `math:nullDistribution` → a parameterized `math:SamplingDistribution`,
/// `math:testStatistic`, `math:nullHypothesis`, `math:alternativeSidedness` →
/// `math:twoSidedAlternative`) so it PASSES `math:IllFramedPValue`, and it honours the
/// process / result / claim spine (a `math:InferenceRun` produced it through
/// `gmeow:wasGeneratedBy`, a `gmeow:Observation` holds the claim from a `gmeow:vantage`),
/// folded into `gmeow.gts` itself (Design A, like the sixth probability-model seam
/// producer), not merely in an on-disk worked example.
///
/// # Panics
///
/// Panics (a loud hard fail) if the magnitude does not lie in `[0, 1]` — a p-value whose
/// magnitude is not a probability is a bug, never a silently-degraded fallback.
pub fn pvalue_tri_slice() -> PValueTriSlice {
    let magnitude = pvalue_tri_slice_magnitude();
    assert!(
        magnitude >= Rational::zero() && magnitude <= Rational::one(),
        "the tri-slice p-value magnitude 3/100 must lie in the probability range [0, 1]"
    );

    let surface = format!("{PVALUE_TRI_SLICE_NS}pvalueSurface");
    let formula = format!("{PVALUE_TRI_SLICE_NS}pvalueFormula");
    let pvalue = format!("{PVALUE_TRI_SLICE_NS}tTestPValue");

    let mut t = String::new();
    t.push_str("@prefix math:  <https://blackcatinformatics.ca/math/> .\n");
    t.push_str("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n");
    t.push_str("@prefix logic: <https://blackcatinformatics.ca/logic/> .\n");
    t.push_str("@prefix lang:  <https://blackcatinformatics.ca/lang/> .\n");
    t.push_str("@prefix p:     <");
    t.push_str(PVALUE_TRI_SLICE_NS);
    t.push_str("> .\n\n");

    t.push_str("# The signature lang: -> logic: -> math: round-trip: the sentence\n");
    t.push_str("# \"the p-value was 0.03\" grounded as a surface form realizing an analyzed\n");
    t.push_str("# form, denoting a logic:Formula, that predicates over a well-framed\n");
    t.push_str("# math:PValue whose magnitude is exactly 3/100 (0.03), in [0, 1].\n\n");

    // --- lang: the surface, the analyzed composed form, the denotation --------
    t.push_str("# lang: the English sign system and Latin script the surface is written in.\n");
    t.push_str("p:english a lang:SignSystem ;\n");
    t.push_str("    lang:signSystemKind lang:naturalLanguageKind ;\n");
    t.push_str("    lang:modality lang:writtenModality .\n");
    t.push_str("p:latinScript a lang:Script .\n\n");

    t.push_str("# The four word forms of \"the p-value was 0.03\".\n");
    t.push_str("p:wfThe     a lang:WordForm ; lang:inSignSystem p:english .\n");
    t.push_str("p:wfPValue  a lang:WordForm ; lang:inSignSystem p:english .\n");
    t.push_str("p:wfWas     a lang:WordForm ; lang:inSignSystem p:english .\n");
    t.push_str("p:wfMagnitude a lang:WordForm ; lang:inSignSystem p:english .\n\n");

    t.push_str("# The analyzed composed form: one analysis over four zero-based slots.\n");
    t.push_str("p:pvalueAnalysis a lang:Analysis .\n");
    t.push_str("p:pvalueSentence a lang:ComposedForm ;\n");
    t.push_str("    lang:inSignSystem p:english ;\n");
    t.push_str("    lang:compositionLevel lang:sentenceLevel ;\n");
    t.push_str("    lang:inAnalysis p:pvalueAnalysis ;\n");
    t.push_str("    lang:formHead p:wfWas ;\n");
    t.push_str("    lang:formSlot p:slot0 , p:slot1 , p:slot2 , p:slot3 .\n\n");
    t.push_str("p:slot0 a lang:FormSlot ; lang:inAnalysis p:pvalueAnalysis ; lang:slotIndex 0 ;\n");
    t.push_str(
        "    lang:slotForm p:wfThe ; lang:slotRole lang:subjectRole ; lang:dependsOn p:slot1 .\n",
    );
    t.push_str("p:slot1 a lang:FormSlot ; lang:inAnalysis p:pvalueAnalysis ; lang:slotIndex 1 ;\n");
    t.push_str("    lang:slotForm p:wfPValue ; lang:slotRole lang:subjectRole ; lang:dependsOn p:slot2 .\n");
    t.push_str("p:slot2 a lang:FormSlot ; lang:inAnalysis p:pvalueAnalysis ; lang:slotIndex 2 ;\n");
    t.push_str("    lang:slotForm p:wfWas ; lang:slotRole lang:predicateRole .\n");
    t.push_str("p:slot3 a lang:FormSlot ; lang:inAnalysis p:pvalueAnalysis ; lang:slotIndex 3 ;\n");
    t.push_str("    lang:slotForm p:wfMagnitude ; lang:slotRole lang:objectRole ; lang:dependsOn p:slot2 .\n\n");

    t.push_str("# The surface: the concrete text, parsed, realizing the composed form.\n");
    t.push_str("p:pvalueSurface a lang:SurfaceForm ;\n");
    t.push_str("    lang:inSignSystem p:english ;\n");
    t.push_str("    lang:surfaceText \"the p-value was 0.03\" ;\n");
    t.push_str("    lang:inScript p:latinScript ;\n");
    t.push_str("    lang:encoding \"UTF-8\" ;\n");
    t.push_str("    lang:unicodeNormalization \"NFC\" ;\n");
    t.push_str("    lang:collationLocale \"en\" ;\n");
    t.push_str("    lang:analysisLevel lang:parsedLevel ;\n");
    t.push_str("    lang:realizes p:pvalueSentence .\n\n");

    t.push_str(
        "# The assertion act: the sentence is asserted, so it lowers to an asserted formula.\n",
    );
    t.push_str("p:pvalueAssertion a lang:CommunicativeAct ;\n");
    t.push_str("    lang:performedOn p:pvalueSentence ;\n");
    t.push_str("    lang:communicativeForce lang:assertForce .\n\n");

    t.push_str("# lang -> logic: the reified denotation whose target IS the logic:Formula.\n");
    t.push_str("p:pvalueDenotation a lang:Denotation ;\n");
    t.push_str("    lang:denotedForm p:pvalueSentence ;\n");
    t.push_str("    lang:denotationKind lang:denotesLogicFormula ;\n");
    t.push_str("    lang:denotationTarget p:pvalueFormula ;\n");
    t.push_str("    lang:denotationContext p:samplingReportContext ;\n");
    t.push_str("    lang:isIndexical false ;\n");
    t.push_str("    logic:preservationKind logic:ExactPreservation .\n\n");

    // --- logic: the atomic-predication formula whose argument IS the p-value ---
    t.push_str("# logic -> math: an atomic-predication formula pValueMagnitudeOf(pvalue, 3/100)\n");
    t.push_str("# whose first argument's logic:termIri is the specific math:PValue.\n");
    t.push_str("p:pvalueFormula a logic:Formula ;\n");
    t.push_str("    logic:relation p:pValueMagnitudeOfRelation ;\n");
    t.push_str("    logic:argument p:argPValue , p:argMagnitude .\n");
    t.push_str("p:pValueMagnitudeOfRelation a logic:Type .\n");
    t.push_str("p:argPValue logic:termIndex 0 ;\n");
    t.push_str("    logic:termIri p:tTestPValue .\n");
    t.push_str("p:argMagnitude logic:termIndex 1 ;\n");
    t.push_str("    logic:termIri p:pValueMagnitude .\n\n");

    // --- math: the well-framed p-value and its process / claim spine ----------
    t.push_str("# math: the well-framed p-value the formula predicates over -- it carries its\n");
    t.push_str("# whole frame, so it PASSES math:IllFramedPValue.\n");
    t.push_str("p:tTestPValue a math:PValue ;\n");
    t.push_str("    math:testStatistic p:tStatistic ;\n");
    t.push_str("    math:nullHypothesis p:noEffectNull ;\n");
    t.push_str("    math:nullDistribution p:tNullDistribution ;\n");
    t.push_str("    math:alternativeSidedness math:twoSidedAlternative ;\n");
    t.push_str("    math:pValue p:pValueMagnitude ;\n");
    t.push_str("    gmeow:wasGeneratedBy p:inferenceRun .\n");
    t.push_str("p:tStatistic a math:Statistic .\n");
    t.push_str("p:noEffectNull a math:NullHypothesis .\n\n");

    t.push_str("# The null distribution: a parameterized sampling distribution (normal family,\n");
    t.push_str("# mean/stddev parameterization, one parameter per required role).\n");
    t.push_str("p:tNullDistribution a math:SamplingDistribution ;\n");
    t.push_str("    math:distributionFamily p:normalFamily ;\n");
    t.push_str("    math:distributionParameterization p:meanStddevParameterization ;\n");
    t.push_str("    math:hasDistributionParameter p:meanParam , p:stddevParam .\n");
    t.push_str("p:normalFamily a math:DistributionFamily .\n");
    t.push_str("p:meanStddevParameterization a math:DistributionParameterization ;\n");
    t.push_str("    math:requiresParameterRole p:meanRole , p:stddevRole .\n");
    t.push_str("p:meanRole a math:DistributionParameterRole .\n");
    t.push_str("p:stddevRole a math:DistributionParameterRole .\n");
    t.push_str("p:meanParam a math:DistributionParameter ; math:parameterRole p:meanRole .\n");
    t.push_str(
        "p:stddevParam a math:DistributionParameter ; math:parameterRole p:stddevRole .\n\n",
    );

    t.push_str(
        "# The p-value magnitude: a framed exact-rational probability value, 3/100 = 0.03.\n",
    );
    t.push_str("p:pValueMagnitude a math:ProbabilityValue , math:RationalValue ;\n");
    t.push_str("    math:numerator 3 ;\n");
    t.push_str("    math:denominator 100 ;\n");
    t.push_str("    gmeow:hasReferenceFrame p:samplingFrame .\n");
    t.push_str("p:samplingFrame a gmeow:ReferenceFrame .\n\n");

    t.push_str("# The process / result / claim spine: the run produced the p-value, the\n");
    t.push_str("# observation holds the claim about it from a vantage.\n");
    t.push_str("p:inferenceRun a math:InferenceRun , gmeow:Activity ;\n");
    t.push_str("    math:fittedToData p:sampleData .\n");
    t.push_str("p:sampleData a math:DatasetMatrix .\n");
    t.push_str("p:pvalueObservation a gmeow:Observation ;\n");
    t.push_str("    gmeow:vantage p:analystStandpoint ;\n");
    t.push_str("    gmeow:observedFeature p:tTestPValue ;\n");
    t.push_str("    gmeow:observationResult p:tTestPValue ;\n");
    t.push_str("    gmeow:wasGeneratedBy p:inferenceRun .\n");
    t.push_str("p:analystStandpoint a gmeow:Standpoint .\n");

    PValueTriSlice {
        magnitude,
        surface,
        formula,
        pvalue,
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

    // ---- Sixth producer — the probability-model seam ----------------------

    #[test]
    fn probability_model_seam_pins_the_exact_joint_mass_total() {
        let s = probability_model_seam();
        assert_eq!(s.joint_mass_total, r(1, 1));
        // Independent recomputation of the exact sum.
        let recomputed = sprinkler_joint_masses()
            .into_iter()
            .fold(Rational::zero(), |acc, m| acc.checked_add(m).expect("sum"));
        assert_eq!(recomputed, r(1, 1));
        assert_eq!(s.world, prod("forecastWorld"));
        assert_eq!(s.model, prod("sprinklerNet"));
    }

    #[test]
    fn probability_model_seam_graph_carries_the_live_crossing_triple() {
        let s = probability_model_seam();
        let idx = index_turtle(s.turtle.as_bytes()).expect("parse probability-seam graph");
        const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
        let logic_iri = |local: &str| format!("{LOGIC}{local}");

        assert!(has_type(
            &idx,
            &prod("sprinklerNet"),
            &math_iri("BayesianNetwork")
        ));
        assert!(has_type(&idx, &prod("forecastWorld"), &logic_iri("World")));
        // The LIVE logic:probabilityModel A-box crossing triple — the whole point.
        assert_eq!(
            first_iri(&idx, &prod("forecastWorld"), &logic_iri("probabilityModel")).as_deref(),
            Some(prod("sprinklerNet").as_str())
        );
        assert!(first_iri(&idx, &prod("sprinklerNet"), &math_iri("dependencyGraph")).is_some());
        assert_eq!(
            first_iri(&idx, &prod("sprinklerNet"), &logic_iri("preservationKind")).as_deref(),
            Some(logic_iri("ExactPreservation").as_str())
        );
        assert!(has_type(
            &idx,
            &prod("rainSprinklerJoint"),
            &math_iri("JointProbabilityTable")
        ));
    }

    #[test]
    fn probability_model_seam_is_deterministic() {
        assert_eq!(
            probability_model_seam().turtle,
            probability_model_seam().turtle
        );
    }

    // ---- Seventh producer — the lang -> logic -> math p-value tri-slice ----

    const LANG: &str = "https://blackcatinformatics.ca/lang/";
    const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

    fn tri(local: &str) -> String {
        format!("{PVALUE_TRI_SLICE_NS}{local}")
    }
    fn lang_iri(local: &str) -> String {
        format!("{LANG}{local}")
    }
    fn logic_iri(local: &str) -> String {
        format!("{LOGIC}{local}")
    }

    #[test]
    fn pvalue_tri_slice_pins_the_exact_magnitude() {
        let s = pvalue_tri_slice();
        // The signature value: exactly 3/100 = 0.03, and a valid probability in [0, 1].
        assert_eq!(s.magnitude, r(3, 100));
        assert!(s.magnitude >= r(0, 1) && s.magnitude <= r(1, 1));
        assert_eq!(s.surface, tri("pvalueSurface"));
        assert_eq!(s.formula, tri("pvalueFormula"));
        assert_eq!(s.pvalue, tri("tTestPValue"));
    }

    #[test]
    fn pvalue_tri_slice_links_the_three_slices() {
        let s = pvalue_tri_slice();
        let idx = index_turtle(s.turtle.as_bytes()).expect("parse tri-slice graph");

        // lang: a surface form realizing the analyzed composed form.
        assert!(has_type(
            &idx,
            &tri("pvalueSurface"),
            &lang_iri("SurfaceForm")
        ));
        assert_eq!(
            first_iri(&idx, &tri("pvalueSurface"), &lang_iri("realizes")).as_deref(),
            Some(tri("pvalueSentence").as_str())
        );
        // lang -> logic: the denotation's kind is denotesLogicFormula and its target IS the
        // specific logic:Formula (the load-bearing lang->logic crossing triple).
        assert!(has_type(
            &idx,
            &tri("pvalueDenotation"),
            &lang_iri("Denotation")
        ));
        assert_eq!(
            first_iri(&idx, &tri("pvalueDenotation"), &lang_iri("denotationKind")).as_deref(),
            Some(lang_iri("denotesLogicFormula").as_str())
        );
        assert_eq!(
            first_iri(
                &idx,
                &tri("pvalueDenotation"),
                &lang_iri("denotationTarget")
            )
            .as_deref(),
            Some(tri("pvalueFormula").as_str())
        );
        // logic -> math: the formula is an atomic predication whose first argument's
        // logic:termIri is the specific math:PValue (the load-bearing logic->math crossing).
        assert!(has_type(&idx, &tri("pvalueFormula"), &logic_iri("Formula")));
        assert_eq!(
            first_iri(&idx, &tri("argPValue"), &logic_iri("termIri")).as_deref(),
            Some(tri("tTestPValue").as_str())
        );
        // math: the well-framed p-value, its framed magnitude, and the process/claim spine.
        assert!(has_type(&idx, &tri("tTestPValue"), &math_iri("PValue")));
        assert_eq!(
            first_iri(&idx, &tri("tTestPValue"), &math_iri("alternativeSidedness")).as_deref(),
            Some(math_iri("twoSidedAlternative").as_str())
        );
        assert_eq!(
            first_iri(&idx, &tri("tTestPValue"), &math_iri("pValue")).as_deref(),
            Some(tri("pValueMagnitude").as_str())
        );
        assert_eq!(
            first_i128(&idx, &tri("pValueMagnitude"), &math_iri("numerator")),
            Some(3)
        );
        assert_eq!(
            first_i128(&idx, &tri("pValueMagnitude"), &math_iri("denominator")),
            Some(100)
        );
        assert!(
            first_iri(
                &idx,
                &tri("pValueMagnitude"),
                &gmeow_iri("hasReferenceFrame")
            )
            .is_some()
        );
        assert!(has_type(
            &idx,
            &tri("inferenceRun"),
            &math_iri("InferenceRun")
        ));
        assert_eq!(
            first_iri(&idx, &tri("tTestPValue"), &gmeow_iri("wasGeneratedBy")).as_deref(),
            Some(tri("inferenceRun").as_str())
        );
        assert!(has_type(
            &idx,
            &tri("pvalueObservation"),
            &gmeow_iri("Observation")
        ));
        assert_eq!(
            first_iri(
                &idx,
                &tri("pvalueObservation"),
                &gmeow_iri("observationResult")
            )
            .as_deref(),
            Some(tri("tTestPValue").as_str())
        );
    }

    #[test]
    fn pvalue_tri_slice_is_deterministic() {
        assert_eq!(pvalue_tri_slice().turtle, pvalue_tri_slice().turtle);
    }
}
