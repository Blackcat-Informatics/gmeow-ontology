// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ten native producers of the `math:` grounding slice: five bound to the
//! flagship-acceptance manifest ([`e8_weyl_order`], [`additive_he_demo`], [`proof_ingest`],
//! [`r_lift`], [`exact_pca_residual`]), plus the probability layer's live
//! `logic:probabilityModel` seam producer, the signature `lang:` → `logic:` → `math:`
//! p-value tri-slice producer, the exact `Cl(12)` → `Cl(13)` positive-extension producer,
//! and the two remaining EXECUTABLE ingestion lifts ([`onnx_lift`], [`proof_lift`]).
//!
//! Each flagship scenario in `slices/grounding/math/examples/flagship-acceptance.ttl`
//! names a native producer entrypoint (`gmeow:demonstratedByProducer`). This module IS
//! those five entrypoints — deterministic, exact-arithmetic Rust functions that each
//!
//! 1. compute a **falsifiable pinned value** (the E8 Weyl order, the homomorphic-sum
//!    equality, the grounded verification verdict, the R lift's codomain size, and the
//!    exact PCA dominant axis / LDLᵀ pivots), and
//! 2. emit a **deterministic RDF graph fragment** (Turtle) in the exact `math:` / `gmeow:`
//!    / `logic:` vocabulary the slice's SHACL shapes expect, so the emitted graph validates
//!    clean against the math shapes (no `math:` failure) once merged with the ontology.
//!
//! The `rBridge` flagship's producer is [`r_lift`] — the EXECUTABLE R front-end run over a
//! real committed script, not a hand-written imitation of one. There is no second, in-code
//! R-bridge producer: an emitter that parsed nothing was strictly subsumed by the one that
//! runs the shipped parser, so it was removed rather than kept alongside it.
//!
//! [`probability_model_seam`] is folded into the bundle the SAME way
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
//! [`pvalue_tri_slice`] is folded the SAME way and likewise NOT
//! flagship-bound. It carries the charter's signature tri-slice round-trip — the sentence
//! "the p-value was 0.03" grounded as a `lang:SurfaceForm` → a `logic:Formula` → a
//! well-framed `math:PValue` with a framed `math:pValue` measure — as ONE genuinely grounded
//! chain (a `lang:denotesLogicFormula` denotation whose target is the formula, and the
//! formula predicating over the specific p-value) inside `gmeow.gts` itself.
//!
//! [`clifford_twelve_thirteen`] is likewise non-flagship. It calculates
//! both `Cl(12,0)` → `Cl(13,0)` and `Cl(6,6)` → `Cl(7,6)` with the exact sparse Clifford
//! kernel, including generator laws, pseudoscalar squares, algebra dimensions, and the
//! `8192 = 4096 + 4096` last-generator split. It exposes no E8 action or equivalence: such a
//! claim requires a supplied faithful representation/equivariant map, not dimensional
//! coincidence.
//!
//! [`r_lift`], [`onnx_lift`], and [`proof_lift`] are the three EXECUTABLE lifts ([`r_lift`]
//! flagship-bound, the other two not). They differ from every producer above in one decisive
//! way: their graphs are not written here at all. Each calls the SAME
//! `gmeow_math_lift::{r,onnx,proof}::lift` entrypoint the shipped `gmeow` CLI calls, over a
//! REAL committed artifact (`mtcars.R`, `mlp.onnx`, `theorem-subclass.tstp`) embedded at
//! COMPILE TIME with `include_str!` / `include_bytes!`. So what ships in `gmeow.gts` is the
//! output of the actual R recursive-descent parser, the actual ONNX protobuf wire decoder,
//! and the actual TSTP annotated-formula reader — not a hand-typed imitation of what they
//! would produce. The embedding is what keeps the producers pure: the bytes are in the
//! binary, so there is no disk read, no argument, and no machine dependence; the same lift
//! functions serve the CLI (bytes read from a user's path) and the bundle (bytes compiled
//! in). A lift failure is a HARD FAIL (a panic), never a degraded or omitted graph.
//!
//! The graphs are built from constant templates and formatted exact integers/rationals —
//! there is no `HashMap` iteration, no clock, and no randomness — so two calls to the same
//! producer return byte-identical Turtle. All arithmetic that pins a value is exact
//! (`i128` / [`Rational`]); the only engine that touches this module is the shared
//! [`crate::InnerProductSpace`], never `f64`. The three lift producers inherit the same
//! property from the other end: every IRI they mint is a pure function of the embedded
//! source bytes (a content digest, no counter), and their Turtle is serialized by the
//! canonicalizing purrdf codec, so a re-lift of the same bytes is byte-identical too.

use std::fmt::Write as _;

use crate::clifford::{CliffordAlgebra, Multivector};
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

// ===========================================================================
// Eighth producer — exact Cl(12) -> Cl(13) positive extensions.
// ===========================================================================

/// The pinned exact result of [`clifford_twelve_thirteen`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliffordTwelveThirteen {
    /// Dimensions of `Cl(12,0)` and `Cl(6,6)`.
    pub base_dimensions: [u128; 2],
    /// Dimensions of `Cl(13,0)` and `Cl(7,6)`.
    pub extension_dimensions: [u128; 2],
    /// Pseudoscalar squares for the two base algebras.
    pub base_pseudoscalar_squares: [i8; 2],
    /// Pseudoscalar squares for the two extensions.
    pub extension_pseudoscalar_squares: [i8; 2],
    /// Number of generator-square laws checked across all four algebras.
    pub generator_laws_verified: usize,
    /// Number of distinct-generator anticommutation pairs checked across all four algebras.
    pub anticommutation_pairs_verified: usize,
    /// Whether both exact `embed(a) + e_(p+1)embed(b)` split/join witnesses round-trip.
    pub split_join_verified: bool,
    /// Deterministic RDF graph describing the calculated algebras and decompositions.
    pub turtle: String,
}

fn clifford_split_witness(base: CliffordAlgebra, extension: CliffordAlgebra) -> bool {
    let a = Multivector::from_terms([
        (
            crate::clifford::BasisBlade::scalar(),
            Rational::from_i128(3).expect("3"),
        ),
        (
            base.generator(0).expect("base e1"),
            Rational::from_i128(2).expect("2"),
        ),
    ])
    .expect("exact base witness");
    let b = Multivector::from_terms([
        (
            crate::clifford::BasisBlade::scalar(),
            Rational::from_i128(-5).expect("-5"),
        ),
        (
            base.generator(1).expect("base e2"),
            Rational::from_i128(7).expect("7"),
        ),
    ])
    .expect("exact tail witness");
    let joined = extension
        .join_positive_extension(&a, &b)
        .expect("Cl13 join witness");
    let (split_a, split_b) = extension
        .split_positive_extension(&joined)
        .expect("Cl13 split witness");
    split_a == a
        && split_b == b
        && extension
            .join_positive_extension(&split_a, &split_b)
            .is_ok_and(|rejoined| rejoined == joined)
}

/// Calculate exact positive extensions `Cl(12,0)` → `Cl(13,0)` and `Cl(6,6)` →
/// `Cl(7,6)` and emit their structural RDF record.
///
/// The calculation verifies every generator square from the declared signature, distinct
/// generator anticommutation, the pseudoscalar square, the `2^12 = 4096` / `2^13 = 8192`
/// dimensions, and an exact sparse-multivector round trip through
/// `Cl(p+1,q) = embed(Cl(p,q)) ⊕ e_(p+1)embed(Cl(p,q))` as a vector-space/module
/// decomposition. The emitted graph describes these calculations using
/// the general Clifford vocabulary. It deliberately contains no E8-to-Clifford edge: a
/// dimensional coincidence is not a representation.
pub fn clifford_twelve_thirteen() -> CliffordTwelveThirteen {
    let bases = [
        CliffordAlgebra::new(12, 0).expect("Cl(12,0)"),
        CliffordAlgebra::new(6, 6).expect("Cl(6,6)"),
    ];
    let extensions = [
        bases[0].positive_extension().expect("Cl(13,0)"),
        bases[1].positive_extension().expect("Cl(7,6)"),
    ];

    let base_dimensions = [bases[0].dimension(), bases[1].dimension()];
    let extension_dimensions = [extensions[0].dimension(), extensions[1].dimension()];
    assert_eq!(base_dimensions, [4096, 4096]);
    assert_eq!(extension_dimensions, [8192, 8192]);

    // Verify every declared generator square and every distinct anticommuting pair.
    let mut generator_laws_verified = 0_usize;
    let mut anticommutation_pairs_verified = 0_usize;
    for algebra in bases.into_iter().chain(extensions) {
        for index in 0..algebra.signature().generators() {
            let generator = algebra.generator(index).expect("declared generator");
            let square = algebra
                .geometric_product_blades(generator, generator)
                .expect("generator square");
            assert_eq!(square.blade(), crate::clifford::BasisBlade::scalar());
            assert_eq!(
                square.sign(),
                algebra
                    .signature()
                    .generator_square(index)
                    .expect("signature square")
            );
            generator_laws_verified += 1;
        }
        for left_index in 0..algebra.signature().generators() {
            for right_index in (left_index + 1)..algebra.signature().generators() {
                let left = algebra.generator(left_index).expect("left generator");
                let right = algebra.generator(right_index).expect("right generator");
                let forward = algebra
                    .geometric_product_blades(left, right)
                    .expect("forward product");
                let reverse = algebra
                    .geometric_product_blades(right, left)
                    .expect("reverse product");
                assert_eq!(forward.blade(), reverse.blade());
                assert_eq!(forward.sign(), -reverse.sign());
                anticommutation_pairs_verified += 1;
            }
        }
    }
    assert_eq!(generator_laws_verified, 50);
    assert_eq!(anticommutation_pairs_verified, 288);

    let base_pseudoscalar_squares = [
        bases[0].pseudoscalar_square().expect("Cl(12,0) I^2"),
        bases[1].pseudoscalar_square().expect("Cl(6,6) I^2"),
    ];
    let extension_pseudoscalar_squares = [
        extensions[0].pseudoscalar_square().expect("Cl(13,0) I^2"),
        extensions[1].pseudoscalar_square().expect("Cl(7,6) I^2"),
    ];
    assert_eq!(base_pseudoscalar_squares, [1, 1]);
    assert_eq!(extension_pseudoscalar_squares, [1, 1]);

    let split_join_verified = clifford_split_witness(bases[0], extensions[0])
        && clifford_split_witness(bases[1], extensions[1]);
    assert!(
        split_join_verified,
        "both Cl12 -> Cl13 splits must round-trip"
    );

    let mut t = header();
    t.push_str("@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .\n\n");
    t.push_str("# Eighth producer — exact Cl(12) -> Cl(13) positive extensions.\n");
    t.push_str("# No E8 relation is asserted: that requires a faithful representation.\n\n");
    t.push_str("p:cliffordComputation a gmeow:Activity .\n\n");
    for (name, p, q, dimension, pseudoscalar_square) in [
        (
            "cl120",
            12,
            0,
            base_dimensions[0],
            base_pseudoscalar_squares[0],
        ),
        (
            "cl66",
            6,
            6,
            base_dimensions[1],
            base_pseudoscalar_squares[1],
        ),
        (
            "cl130",
            13,
            0,
            extension_dimensions[0],
            extension_pseudoscalar_squares[0],
        ),
        (
            "cl76",
            7,
            6,
            extension_dimensions[1],
            extension_pseudoscalar_squares[1],
        ),
    ] {
        let _ = writeln!(
            t,
            "p:{name} a math:CliffordAlgebra ;\n    math:underlyingSet p:{name}Carrier ;\n    math:scalarField math:realNumbers ;\n    math:structureOperation p:{name}GeometricProduct ;\n    math:satisfiesAxiom p:{name}Associativity , p:{name}Anticommutation ;\n    math:hasBasis p:{name}BladeBasis ;\n    math:hasGrading p:{name}Grading ;\n    math:cliffordInvolution p:{name}Reversion , p:{name}GradeInvolution , p:{name}Conjugation ;\n    math:metricSignature p:{name}Signature ;\n    math:spaceDimension \"{dimension}\"^^xsd:nonNegativeInteger ;\n    math:pseudoscalarSquare {pseudoscalar_square} ;\n    gmeow:wasGeneratedBy p:cliffordComputation .\n\np:{name}Carrier a math:Set .\np:{name}GeometricProduct a math:GeometricProduct .\np:{name}Associativity a math:Axiom .\np:{name}Anticommutation a math:Axiom .\np:{name}BladeBasis a math:Basis .\np:{name}Grading a math:Grading .\np:{name}Reversion a math:CliffordInvolution .\np:{name}GradeInvolution a math:CliffordInvolution .\np:{name}Conjugation a math:CliffordInvolution .\np:{name}Signature a math:MetricSignature ;\n    math:signaturePositive \"{p}\"^^xsd:nonNegativeInteger ;\n    math:signatureNegative \"{q}\"^^xsd:nonNegativeInteger .\n"
        );

        let generator_count = p + q;
        for index in 0..generator_count {
            let square = if index < p { 1 } else { -1 };
            let generator_number = index + 1;
            let _ = writeln!(
                t,
                "p:{name} math:hasBasisBlade p:{name}e{generator_number} .\np:{name}e{generator_number} a math:BasisBlade ;\n    math:bladeGrade \"1\"^^xsd:nonNegativeInteger ;\n    math:generatorIndex \"{generator_number}\"^^xsd:positiveInteger ;\n    math:generatorSquare {square} ;\n    gmeow:wasGeneratedBy p:cliffordComputation ."
            );
        }
        for left_index in 1..=generator_count {
            for right_index in (left_index + 1)..=generator_count {
                let _ = writeln!(
                    t,
                    "p:{name} math:hasAnticommutationWitness p:{name}Anti{left_index}_{right_index} .\np:{name}Anti{left_index}_{right_index} a math:CliffordAnticommutationWitness ;\n    math:leftGenerator p:{name}e{left_index} ;\n    math:rightGenerator p:{name}e{right_index} ;\n    math:anticommutationVerified true ;\n    gmeow:wasGeneratedBy p:cliffordComputation ."
                );
            }
        }
        t.push('\n');
    }

    t.push_str("p:cl130Extension a math:CliffordExtension , math:CliffordModuleDecomposition ;\n");
    t.push_str("    math:baseAlgebra p:cl120 ;\n");
    t.push_str("    math:extendedAlgebra p:cl130 ;\n");
    t.push_str("    math:extensionGenerator p:e13Euclidean ;\n");
    t.push_str("    math:decomposedObject p:cl130 ;\n");
    t.push_str("    math:moduleBaseSummand p:cl120 ;\n");
    t.push_str("    math:moduleExtensionSummand p:e13Cl120 ;\n");
    t.push_str("    math:splitJoinVerified true ;\n");
    t.push_str("    gmeow:wasGeneratedBy p:cliffordComputation .\n");
    t.push_str("p:e13Euclidean a math:BasisBlade ; math:bladeGrade \"1\"^^xsd:nonNegativeInteger ; math:generatorSquare 1 ; gmeow:wasGeneratedBy p:cliffordComputation .\n");
    t.push_str("p:e13Cl120 a math:MathematicalObject ; math:spaceDimension \"4096\"^^xsd:nonNegativeInteger ; gmeow:wasGeneratedBy p:cliffordComputation .\n\n");

    t.push_str("p:cl76Extension a math:CliffordExtension , math:CliffordModuleDecomposition ;\n");
    t.push_str("    math:baseAlgebra p:cl66 ;\n");
    t.push_str("    math:extendedAlgebra p:cl76 ;\n");
    t.push_str("    math:extensionGenerator p:e7Split ;\n");
    t.push_str("    math:decomposedObject p:cl76 ;\n");
    t.push_str("    math:moduleBaseSummand p:cl66 ;\n");
    t.push_str("    math:moduleExtensionSummand p:e7Cl66 ;\n");
    t.push_str("    math:splitJoinVerified true ;\n");
    t.push_str("    gmeow:wasGeneratedBy p:cliffordComputation .\n");
    t.push_str("p:e7Split a math:BasisBlade ; math:bladeGrade \"1\"^^xsd:nonNegativeInteger ; math:generatorSquare 1 ; gmeow:wasGeneratedBy p:cliffordComputation .\n");
    t.push_str("p:e7Cl66 a math:MathematicalObject ; math:spaceDimension \"4096\"^^xsd:nonNegativeInteger ; gmeow:wasGeneratedBy p:cliffordComputation .\n");

    CliffordTwelveThirteen {
        base_dimensions,
        extension_dimensions,
        base_pseudoscalar_squares,
        extension_pseudoscalar_squares,
        generator_laws_verified,
        anticommutation_pairs_verified,
        split_join_verified,
        turtle: t,
    }
}

// ===========================================================================
// Ninth, tenth, and eleventh producers — the EXECUTABLE R / ONNX / proof lifts.
// ===========================================================================

/// The R script [`r_lift`] lifts: `crates/math-lift/fixtures/mtcars.R`, the same committed
/// artifact the R front-end's own tests read, embedded at COMPILE TIME so the producer
/// performs no disk read.
const R_LIFT_SOURCE: &str = include_str!("../../math-lift/fixtures/mtcars.R");

/// The ONNX model [`onnx_lift`] lifts: `crates/math-lift/fixtures/mlp.onnx`, a real
/// protobuf `ModelProto`, embedded at COMPILE TIME. `include_bytes!` (not `include_str!`)
/// because an ONNX graph is binary and is decoded off the wire format, never off text.
const ONNX_LIFT_SOURCE: &[u8] = include_bytes!("../../math-lift/fixtures/mlp.onnx");

/// The TSTP derivation [`proof_lift`] lifts:
/// `crates/math-lift/fixtures/theorem-subclass.tstp`, embedded at COMPILE TIME.
const PROOF_LIFT_SOURCE: &str = include_str!("../../math-lift/fixtures/theorem-subclass.tstp");

/// The codomain sizes the three executable lifts produce.
///
/// ONE source for a number that four surfaces assert: the two producer test binaries, the
/// three dogfood fixture headers, and the drift gate. It was hard-coded in each, and a
/// parser change updated some and not others TWICE — the gate that reds is not always the
/// one you re-blessed, so the miss surfaced an hour later in a different crate.
///
/// The fixture headers stay independently written (they are prose a reader sees) and
/// `lift_fixture_drift` proves them equal to the live producer, so the two mechanisms
/// cross-check rather than duplicate.
pub mod codomain {
    /// [`super::r_lift`]'s codomain node count.
    ///
    /// 141 -> 142 when every lifted free-variable declaration gained the mandatory
    /// `math:variableDomain` edge to the run's `math:Set`: the set node is one new codomain
    /// member. A declaration without a domain is `math:UntypedFreeVariable`, so the lift was
    /// emitting a class of violation the math slice forbids.
    pub const R_LIFT: usize = 142;
    /// [`super::onnx_lift`]'s codomain node count.
    pub const ONNX_LIFT: usize = 55;
    /// [`super::proof_lift`]'s codomain node count.
    pub const PROOF_LIFT: usize = 33;
}

/// The pinned result of [`r_lift`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RLift {
    /// The IRI of the emitted `math:RIngestRun` — content-addressed on the embedded source
    /// bytes, so it names exactly this artifact.
    pub ingest_run: String,
    /// How many structured `math:` codomain nodes the run generated. Non-zero by
    /// construction: [`gmeow_math_lift::Lifted::seal`] refuses an empty codomain rather
    /// than serializing a run the native `math:UnliftableIngest` lint would reject.
    pub codomain_nodes: usize,
    /// The lift's canonical Turtle, exactly as the R front-end emitted it.
    pub turtle: String,
}

/// Run the REAL R front-end (`gmeow_math_lift::r::lift` — the same entrypoint the shipped
/// `gmeow` CLI calls) over the embedded `mtcars.R` fixture and ship its `math:RIngestRun`
/// graph in the bundle.
///
/// This producer writes no RDF at all: the graph is whatever the recursive-descent R parser
/// and its lift tier actually derive from a real script. That is the point — the bundle
/// carries evidence about the executable bridge, not about a template. It is the `rBridge`
/// flagship's producer: the acceptance manifest's `gmeow:demonstratedByExample` names the
/// committed `tests/fixtures/lifted-r.ttl` this emits byte for byte.
///
/// # Panics
///
/// Panics (a loud hard fail) if the embedded fixture does not lift. The bytes are compiled
/// in, so a failure is a defect in this workspace — a parser regression or a fixture edit —
/// never an environment condition, and never grounds for shipping a degraded or absent
/// graph.
#[must_use]
pub fn r_lift() -> RLift {
    let lifted = gmeow_math_lift::r::lift(R_LIFT_SOURCE.as_bytes(), PRODUCER_NS)
        .expect("the embedded mtcars.R fixture must lift through the R front-end");
    RLift {
        ingest_run: lifted.run_iri,
        codomain_nodes: lifted.codomain_nodes,
        turtle: lifted.turtle,
    }
}

/// The pinned result of [`onnx_lift`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnnxLift {
    /// The IRI of the emitted `math:ONNXIngestRun`, content-addressed on the embedded
    /// model bytes.
    pub ingest_run: String,
    /// How many structured `math:` codomain nodes the run generated (non-zero by
    /// construction — an empty codomain never seals).
    pub codomain_nodes: usize,
    /// The lift's canonical Turtle, exactly as the ONNX front-end emitted it.
    pub turtle: String,
}

/// Run the REAL ONNX front-end (`gmeow_math_lift::onnx::lift`) over the embedded
/// `mlp.onnx` model and ship its `math:ONNXIngestRun` graph in the bundle.
///
/// The model is decoded by the hand-written protobuf wire reader — operator types, tensor
/// shapes, and the declared opset are read off the actual bytes — so the emitted
/// `math:TensorComputationGraph` is a report on a real artifact. Weight PAYLOADS are held
/// by reference and never inlined (the blob-by-reference doctrine), which is exactly why
/// the ONNX rung is a lossy lens over a CRISP source.
///
/// # Panics
///
/// Panics (a loud hard fail) if the embedded model does not decode or does not lift; see
/// [`r_lift`] for why that is a workspace defect rather than a fallback condition.
#[must_use]
pub fn onnx_lift() -> OnnxLift {
    let lifted = gmeow_math_lift::onnx::lift(ONNX_LIFT_SOURCE, PRODUCER_NS)
        .expect("the embedded mlp.onnx fixture must lift through the ONNX front-end");
    OnnxLift {
        ingest_run: lifted.run_iri,
        codomain_nodes: lifted.codomain_nodes,
        turtle: lifted.turtle,
    }
}

/// The pinned result of [`proof_lift`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofLift {
    /// The IRI of the emitted `math:ProofIngestRun`, content-addressed on the embedded
    /// derivation bytes.
    pub ingest_run: String,
    /// How many structured `math:` codomain nodes the run generated (non-zero by
    /// construction — an empty codomain never seals).
    pub codomain_nodes: usize,
    /// The lift's canonical Turtle, exactly as the proof front-end emitted it.
    pub turtle: String,
}

/// Run the REAL proof front-end (`gmeow_math_lift::proof::lift`) over the embedded
/// `theorem-subclass.tstp` derivation and ship its `math:ProofIngestRun` graph in the
/// bundle.
///
/// This is the one bridge whose law-spine rung is a SECTION/RETRACTION: the lift carries
/// every step name, inference rule, parent edge, and rendered conclusion, so the derivation
/// genuinely reconstructs from the lift plus its witness. It is therefore also the sharpest
/// of the three as bundle evidence — the strongest preservation claim `math:` makes about
/// an ingest is here backed by a graph a parser actually produced.
///
/// Distinct from [`proof_ingest`], which emits a hand-written
/// `math:FormalVerificationResult` scene for flagship 3 and parses nothing.
///
/// # Panics
///
/// Panics (a loud hard fail) if the embedded derivation does not parse or does not lift;
/// see [`r_lift`] for why that is a workspace defect rather than a fallback condition.
#[must_use]
pub fn proof_lift() -> ProofLift {
    let lifted = gmeow_math_lift::proof::lift(PROOF_LIFT_SOURCE.as_bytes(), PRODUCER_NS)
        .expect("the embedded theorem-subclass.tstp fixture must lift through the proof front-end");
    ProofLift {
        ingest_run: lifted.run_iri,
        codomain_nodes: lifted.codomain_nodes,
        turtle: lifted.turtle,
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

    // ---- Eighth producer — exact Cl12 -> Cl13 extensions ------------------

    #[test]
    fn clifford_twelve_thirteen_pins_dimensions_and_split() {
        let result = clifford_twelve_thirteen();
        assert_eq!(result.base_dimensions, [4096, 4096]);
        assert_eq!(result.extension_dimensions, [8192, 8192]);
        assert_eq!(result.base_pseudoscalar_squares, [1, 1]);
        assert_eq!(result.extension_pseudoscalar_squares, [1, 1]);
        assert_eq!(result.generator_laws_verified, 50);
        assert_eq!(result.anticommutation_pairs_verified, 288);
        assert!(result.split_join_verified);
    }

    #[test]
    fn clifford_twelve_thirteen_graph_carries_both_extensions_without_e8_claim() {
        let result = clifford_twelve_thirteen();
        let idx = index_turtle(result.turtle.as_bytes()).expect("parse Clifford graph");
        for algebra in ["cl120", "cl66", "cl130", "cl76"] {
            assert!(has_type(&idx, &prod(algebra), &math_iri("CliffordAlgebra")));
            assert!(first_iri(&idx, &prod(algebra), &math_iri("metricSignature")).is_some());
            assert!(first_i128(&idx, &prod(algebra), &math_iri("spaceDimension")).is_some());
        }
        for extension in ["cl130Extension", "cl76Extension"] {
            assert!(has_type(
                &idx,
                &prod(extension),
                &math_iri("CliffordExtension")
            ));
            assert!(has_type(
                &idx,
                &prod(extension),
                &math_iri("CliffordModuleDecomposition")
            ));
        }
        assert_eq!(result.turtle.matches(" a math:BasisBlade ;").count(), 52);
        assert_eq!(
            result
                .turtle
                .matches(" a math:CliffordAnticommutationWitness ;")
                .count(),
            288
        );
        assert_eq!(
            result.turtle.matches("math:splitJoinVerified true").count(),
            2
        );
        assert!(!result.turtle.contains("e8Roots"));
        assert!(!result.turtle.contains("representationOf p:e8"));
        assert!(!result.turtle.contains("math:actsOn"));
        assert!(!result.turtle.contains("math:equivariantMap"));
    }

    #[test]
    fn clifford_twelve_thirteen_is_deterministic() {
        assert_eq!(
            clifford_twelve_thirteen().turtle,
            clifford_twelve_thirteen().turtle
        );
    }

    // ---- Ninth/tenth/eleventh producers — the executable lifts ------------

    /// A lift producer's run node must be minted under [`PRODUCER_NS`], carry the bridge's
    /// own `math:*IngestRun` class, and generate a non-empty structured codomain (the
    /// condition the native `math:UnliftableIngest` lint checks).
    fn assert_lift_producer(run_iri: &str, turtle: &str, codomain_nodes: usize, run_class: &str) {
        assert!(
            run_iri.starts_with(PRODUCER_NS),
            "the lift run <{run_iri}> must be minted under {PRODUCER_NS}"
        );
        assert!(
            codomain_nodes > 0,
            "a lift producer must generate a non-empty math: codomain"
        );
        let idx = index_turtle(turtle.as_bytes()).expect("parse the lift graph");
        assert!(
            has_type(&idx, run_iri, &math_iri(run_class)),
            "<{run_iri}> must be a math:{run_class}"
        );
        // The four ingest-frame obligations math:IngestRunShape requires.
        assert!(first_iri(&idx, run_iri, &math_iri("parseSource")).is_some());
        assert!(first_iri(&idx, run_iri, &logic_iri("instantiatesSchema")).is_some());
        assert!(first_iri(&idx, run_iri, &logic_iri("instantiatesPlan")).is_some());
        assert!(first_iri(&idx, run_iri, &math_iri("ingestCorrespondence")).is_some());
    }

    #[test]
    fn r_lift_runs_the_real_r_front_end_over_the_embedded_fixture() {
        let l = r_lift();
        assert_lift_producer(&l.ingest_run, &l.turtle, l.codomain_nodes, "RIngestRun");
        // The embedded artifact IS the lifter's committed fixture — the producer parses a
        // real script, it does not template one.
        assert!(R_LIFT_SOURCE.contains("lm("));
    }

    #[test]
    fn r_lift_is_deterministic() {
        assert_eq!(r_lift().turtle, r_lift().turtle);
        assert_eq!(r_lift().ingest_run, r_lift().ingest_run);
    }

    #[test]
    fn onnx_lift_runs_the_real_onnx_front_end_over_the_embedded_fixture() {
        let l = onnx_lift();
        assert_lift_producer(&l.ingest_run, &l.turtle, l.codomain_nodes, "ONNXIngestRun");
        // A real protobuf ModelProto, not text: the wire decoder is what read it. The
        // field-1 varint tag (`\x08`, ir_version) opens every ModelProto.
        assert_eq!(ONNX_LIFT_SOURCE.first(), Some(&0x08));
    }

    #[test]
    fn onnx_lift_is_deterministic() {
        assert_eq!(onnx_lift().turtle, onnx_lift().turtle);
        assert_eq!(onnx_lift().ingest_run, onnx_lift().ingest_run);
    }

    #[test]
    fn proof_lift_runs_the_real_proof_front_end_over_the_embedded_fixture() {
        let l = proof_lift();
        assert_lift_producer(&l.ingest_run, &l.turtle, l.codomain_nodes, "ProofIngestRun");
        // The proof bridge is the one section/retraction rung: the law spine says so on the
        // Correspondence, and nowhere else (math: never shadows the law).
        let idx = index_turtle(l.turtle.as_bytes()).expect("parse the proof lift graph");
        let corr = first_iri(&idx, &l.ingest_run, &math_iri("ingestCorrespondence"))
            .expect("the run names its law-spine Correspondence");
        assert_eq!(
            first_iri(&idx, &corr, &logic_iri("morphismClass")).as_deref(),
            Some(logic_iri("SectionRetraction").as_str())
        );
        assert!(PROOF_LIFT_SOURCE.contains("cnf(") || PROOF_LIFT_SOURCE.contains("fof("));
    }

    #[test]
    fn proof_lift_is_deterministic() {
        assert_eq!(proof_lift().turtle, proof_lift().turtle);
        assert_eq!(proof_lift().ingest_run, proof_lift().ingest_run);
    }

    /// The three executable lifts are three DISTINCT runs over three distinct artifacts —
    /// a shared IRI would mean two bridges' codomains merged into one run's product.
    #[test]
    fn the_three_lift_producers_mint_distinct_runs() {
        let runs = [
            r_lift().ingest_run,
            onnx_lift().ingest_run,
            proof_lift().ingest_run,
        ];
        let distinct: std::collections::BTreeSet<&String> = runs.iter().collect();
        assert_eq!(distinct.len(), runs.len(), "lift run IRIs must be distinct");
    }
}
