// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust-owned affect-intensity geometry.
//!
//! Overall affect intensity is the norm `√(xᵀGx)` over a **non-orthogonal,
//! positive-definite** metric Gram matrix `G` — never a raw L² norm — computed
//! **outside** the reasoned core (Principle 12) and fully deterministically.
//!
//! The reusable exact-rational inner-product-space over `G` ([`InnerProductSpace`])
//! and its exact-rational scalar ([`Rational`]) live in the shared `gmeow-math`
//! crate and are re-exported here; this crate owns the affect-specific graph-reading
//! front doors ([`affective_geometry`], [`geometry_from_gts_bytes`],
//! [`distance_and_cosine`]) consumed by the `gmeow affect` CLI and the EmotionML
//! emitter, computing THROUGH that shared engine.
//!
//! # Determinism contract
//!
//! All arithmetic is exact rational ([`Rational`], `i128`-backed, gcd-normalized,
//! hard-fail on overflow). The ONLY approximation is the final square root,
//! emitted as a fixed-precision decimal with [`SQRT_DECIMALS`] (`= 6`) fractional
//! digits, round-half-up at the seventh digit, via an integer floor-sqrt — never
//! `f64::sqrt`. Given the same inputs the output strings are byte-identical.

use std::collections::BTreeMap;

use purrdf::gts::model::Graph;

// The reusable exact-rational geometry engine lives in the shared `gmeow-math`
// crate. The affect-specific graph readers below compute THROUGH it. Re-export
// the previously affect-owned public surface so downstream paths
// (`gmeow_affect::Rational` / `InnerProductSpace` / `normalize_to_unit` /
// `SQRT_DECIMALS`) keep resolving unchanged.
pub use gmeow_math::{InnerProductSpace, Rational, SQRT_DECIMALS, normalize_to_unit};
use gmeow_math::{
    MAX_BASIS_DIM, TripleIndex, all_iris, bounded_index, first_i128, first_iri, first_literal,
    has_type, index_graph, load_gram, sqrt_rational_decimal, subjects,
};

use gmeow_errors::Diag;

pub mod error;
use error::compute_err;

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const MATH: &str = "https://blackcatinformatics.ca/math/";

/// The one recognized affect norm function IRI (the metric-tensor norm).
const NORM_FUNCTION_IRI: &str = "https://blackcatinformatics.ca/gmeow/affectMetricTensorNorm";

/// The known, seeded `gmeow:WeightingPolicy` individuals (an OPEN vocabulary,
/// but an intensity record MUST name one of the grounded policies).
const KNOWN_WEIGHTING_POLICIES: &[&str] = &[
    "https://blackcatinformatics.ca/gmeow/weightingEqualCoreAffect",
    "https://blackcatinformatics.ca/gmeow/weightingValenceDominant",
];

fn gm(local: &str) -> String {
    format!("{GM}{local}")
}

fn math(local: &str) -> String {
    format!("{MATH}{local}")
}

// ---------------------------------------------------------------------------
// Result of reading + computing one derived-intensity observation.
// ---------------------------------------------------------------------------

/// A per-axis unit-clamp-normalized reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedAxis {
    /// The core-affect axis index (valence 0, arousal 1, dominance 2, unpredictability 3).
    pub axis: usize,
    /// The appraisal-dimension IRI.
    pub dimension: String,
    /// The unit-clamp-normalized value on `[0,1]` (trimmed decimal).
    pub value: String,
}

/// The computed affect-intensity geometry of one
/// `gmeow:DerivedAffectIntensityObservation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geometry {
    /// The observation IRI this geometry was computed for.
    pub observation: String,
    /// The exact quadratic form `Q = xᵀGx` as a printable ratio (e.g. `"79/100"`).
    pub quadratic_form: String,
    /// The intensity `√Q` as a fixed-precision decimal string (e.g. `"0.888819"`).
    pub intensity: String,
    /// The metric-aware dominant-axis appraisal-dimension IRI.
    pub dominant_axis: String,
    /// The per-axis unit-clamp-normalized values (ascending by axis).
    pub normalized: Vec<NormalizedAxis>,
    /// The LDLᵀ pivots of `G` (positive-definiteness certificate; all `> 0`),
    /// as printable ratios.
    pub pivots: Vec<String>,
}

// ---------------------------------------------------------------------------
// Reading the affect data model from the graph.
// ---------------------------------------------------------------------------

/// The vector + metric inputs pulled from one observation, ready for geometry.
struct Inputs {
    space: InnerProductSpace,
    /// The zero-completed coordinate vector over the basis.
    vector: Vec<Rational>,
    /// Axis index → appraisal-dimension IRI.
    axis_to_dim: BTreeMap<usize, String>,
    /// Present cells: (axis, dimension IRI, raw appraisal value).
    cells: Vec<(usize, String, Rational)>,
    range_min: Rational,
    range_max: Rational,
}

fn load_cells(
    index: &TripleIndex,
    vector_iri: &str,
) -> gmeow_errors::Result<Vec<(usize, String, Rational)>> {
    let component_iris = all_iris(index, vector_iri, &gm("vectorComponent"));
    if component_iris.is_empty() {
        return Err(Diag::of_kind(error::MissingAffectProperty {
            node: vector_iri.to_owned(),
            property: "gmeow:vectorComponent cells".to_owned(),
        }));
    }
    let mut cells = Vec::new();
    for cell in component_iris {
        let dimension = first_iri(index, &cell, &gm("appraisalDimension")).ok_or_else(|| {
            Diag::of_kind(error::MissingAffectProperty {
                node: cell.clone(),
                property: "gmeow:appraisalDimension".to_owned(),
            })
        })?;
        let axis = first_i128(index, &dimension, &gm("coreAxisIndex")).ok_or_else(|| {
            Diag::of_kind(error::MissingAffectProperty {
                node: dimension.clone(),
                property: "gmeow:coreAxisIndex".to_owned(),
            })
        })?;
        let axis = bounded_index(axis, "core axis").map_err(compute_err)?;
        let value = Rational::parse_decimal(
            &first_literal(index, &cell, &gm("appraisalValue")).ok_or_else(|| {
                Diag::of_kind(error::MissingAffectProperty {
                    node: cell.clone(),
                    property: "gmeow:appraisalValue".to_owned(),
                })
            })?,
        )
        .map_err(compute_err)?;
        cells.push((axis, dimension, value));
    }
    cells.sort_by_key(|(axis, _, _)| *axis);
    Ok(cells)
}

fn load_inputs(index: &TripleIndex, observation: &str) -> gmeow_errors::Result<Inputs> {
    let norm_fn = first_iri(index, observation, &gm("normFunction")).ok_or_else(|| {
        Diag::of_kind(error::MissingAffectProperty {
            node: format!("observation {observation}"),
            property: "gmeow:normFunction".to_owned(),
        })
    })?;
    if norm_fn != NORM_FUNCTION_IRI {
        return Err(Diag::of_kind(error::UnrecognizedAffectHandle {
            detail: format!(
                "unrecognized gmeow:normFunction {norm_fn}: expected {NORM_FUNCTION_IRI}"
            ),
        }));
    }
    let policy = first_iri(index, observation, &gm("weightingPolicy")).ok_or_else(|| {
        Diag::of_kind(error::MissingAffectProperty {
            node: format!("observation {observation}"),
            property: "gmeow:weightingPolicy".to_owned(),
        })
    })?;
    if !KNOWN_WEIGHTING_POLICIES.contains(&policy.as_str()) {
        return Err(Diag::of_kind(error::UnrecognizedAffectHandle {
            detail: format!("unrecognized gmeow:weightingPolicy {policy}"),
        }));
    }
    let basis = first_iri(index, observation, &gm("intensityBasis")).ok_or_else(|| {
        Diag::of_kind(error::MissingAffectProperty {
            node: format!("observation {observation}"),
            property: "gmeow:intensityBasis".to_owned(),
        })
    })?;
    let profile = first_iri(index, observation, &gm("metricProfile")).ok_or_else(|| {
        Diag::of_kind(error::MissingAffectProperty {
            node: format!("observation {observation}"),
            property: "gmeow:metricProfile".to_owned(),
        })
    })?;
    let gram_iri = first_iri(index, &profile, &gm("metricGram")).ok_or_else(|| {
        Diag::of_kind(error::MissingAffectProperty {
            node: format!("scale profile {profile}"),
            property: "gmeow:metricGram".to_owned(),
        })
    })?;
    let range_min = Rational::parse_decimal(
        &first_literal(index, &profile, &gm("profileRangeMin")).ok_or_else(|| {
            Diag::of_kind(error::MissingAffectProperty {
                node: format!("scale profile {profile}"),
                property: "gmeow:profileRangeMin".to_owned(),
            })
        })?,
    )
    .map_err(compute_err)?;
    let range_max = Rational::parse_decimal(
        &first_literal(index, &profile, &gm("profileRangeMax")).ok_or_else(|| {
            Diag::of_kind(error::MissingAffectProperty {
                node: format!("scale profile {profile}"),
                property: "gmeow:profileRangeMax".to_owned(),
            })
        })?,
    )
    .map_err(compute_err)?;

    let gram_cells = load_gram(index, &gram_iri).map_err(compute_err)?;
    let vector_cells = load_cells(index, &basis)?;

    let dim = gram_cells
        .iter()
        .flat_map(|(r, c, _)| [*r, *c])
        .chain(vector_cells.iter().map(|(axis, _, _)| *axis))
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    if dim == 0 {
        return Err(Diag::of_kind(error::EmptyAffectBasis {}));
    }
    // Every contributing index was bounded below `MAX_BASIS_DIM`, so the
    // derived dimension is bounded too; assert it before it sizes the matrix.
    debug_assert!(dim <= MAX_BASIS_DIM, "derived dimension {dim} exceeds cap");

    let mut gram = vec![vec![Rational::zero(); dim]; dim];
    for (row, col, value) in gram_cells {
        gram[row][col] = value;
        gram[col][row] = value; // declared symmetric fill
    }

    let mut vector = vec![Rational::zero(); dim];
    let mut axis_to_dim = BTreeMap::new();
    for (axis, dimension, value) in &vector_cells {
        vector[*axis] = *value;
        axis_to_dim.insert(*axis, dimension.clone());
    }

    Ok(Inputs {
        space: InnerProductSpace::new(gram).map_err(compute_err)?,
        vector,
        axis_to_dim,
        cells: vector_cells,
        range_min,
        range_max,
    })
}

fn compute_geometry(index: &TripleIndex, observation: &str) -> gmeow_errors::Result<Geometry> {
    let inputs = load_inputs(index, observation)?;
    let quadratic = inputs
        .space
        .quadratic_form(&inputs.vector)
        .map_err(compute_err)?;
    let intensity = sqrt_rational_decimal(quadratic).map_err(compute_err)?;
    let pivots = inputs.space.ldlt_pivots().map_err(compute_err)?; // hard-fails on non-PD G
    let dominant = inputs
        .space
        .dominant_axis(&inputs.vector)
        .map_err(compute_err)?;
    let dominant_axis = inputs.axis_to_dim.get(&dominant).cloned().ok_or_else(|| {
        Diag::of_kind(error::MissingAffectProperty {
            node: format!("dominant axis {dominant}"),
            property: "declared dimension".to_owned(),
        })
    })?;
    let normalized = inputs
        .cells
        .iter()
        .map(|(axis, dimension, value)| {
            Ok(NormalizedAxis {
                axis: *axis,
                dimension: dimension.clone(),
                value: normalize_to_unit(value, &inputs.range_min, &inputs.range_max)
                    .map_err(compute_err)?,
            })
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    Ok(Geometry {
        observation: observation.to_string(),
        quadratic_form: quadratic.ratio_string(),
        intensity,
        dominant_axis,
        normalized,
        pivots: pivots.into_iter().map(Rational::ratio_string).collect(),
    })
}

// ---------------------------------------------------------------------------
// Public graph front doors.
// ---------------------------------------------------------------------------

/// Compute the affect-intensity geometry of one
/// `gmeow:DerivedAffectIntensityObservation` in `graph`.
pub fn affective_geometry(graph: &Graph, observation_iri: &str) -> gmeow_errors::Result<Geometry> {
    compute_geometry(&index_graph(graph), observation_iri)
}

fn derived_observations(index: &TripleIndex) -> Vec<String> {
    let class = gm("DerivedAffectIntensityObservation");
    let mut observations = subjects(index)
        .filter(|subject| has_type(index, subject, &class))
        .cloned()
        .collect::<Vec<_>>();
    observations.sort();
    observations
}

/// Compute the geometry of every `gmeow:DerivedAffectIntensityObservation` in a
/// GTS bundle (or the single named one when `observation_iri` is `Some`), in
/// deterministic ascending-IRI order.
pub fn geometry_from_gts_bytes(
    bytes: &[u8],
    observation_iri: Option<&str>,
) -> gmeow_errors::Result<Vec<Geometry>> {
    let graph = purrdf::gts::reader::read(bytes, false, None);
    let index = index_graph(&graph);
    let observations = match observation_iri {
        Some(iri) => vec![iri.to_string()],
        None => derived_observations(&index),
    };
    if observations.is_empty() {
        return Err(Diag::of_kind(error::NoAffectObservations {}));
    }
    observations
        .iter()
        .map(|iri| compute_geometry(&index, iri))
        .collect()
}

/// Certify the AUTHORED `math:definiteness` of a Gram matrix against the
/// COMPUTED exact-rational LDLᵀ positive-definiteness witness.
///
/// SHACL/Datalog cannot compute an LDLᵀ factorization, so this cross-check is
/// Rust-only: it reads `gram_iri`'s cells from `bytes`, fills the declared
/// symmetric Gram, runs [`InnerProductSpace::ldlt_pivots`], and compares the
/// computed positive-definiteness (all pivots `> 0`) against the authored
/// `math:definiteness` IRI.
///
/// Hard-fails when the authored declaration is ABSENT (SHACL does not require
/// it, so a silent gap is a loud error here), and when the authored and
/// computed verdicts DISAGREE. On agreement it returns the LDLᵀ pivots as
/// printable ratios (the same `Rational::ratio_string` form `Geometry.pivots`
/// carries), so the authored form is certified by the derived witness.
pub fn crosscheck_authored_definiteness(
    bytes: &[u8],
    gram_iri: &str,
) -> gmeow_errors::Result<Vec<String>> {
    let graph = purrdf::gts::reader::read(bytes, false, None);
    let index = index_graph(&graph);

    let authored = first_iri(&index, gram_iri, &math("definiteness")).ok_or_else(|| {
        Diag::of_kind(error::AuthoredDefinitenessAbsent {
            gram_iri: gram_iri.to_owned(),
        })
    })?;
    let authored_pd = authored == math("positiveDefinite");

    // Fill the declared SYMMETRIC Gram from the graph cells; the max bounded
    // index sizes the matrix (every index is bounded below `MAX_BASIS_DIM`).
    let cells = load_gram(&index, gram_iri).map_err(compute_err)?;
    let dim = cells
        .iter()
        .flat_map(|(r, c, _)| [*r, *c])
        .max()
        .map(|m| m + 1)
        .ok_or_else(|| {
            Diag::of_kind(error::GramHasNoEntries {
                gram_iri: gram_iri.to_owned(),
            })
        })?;
    debug_assert!(dim <= MAX_BASIS_DIM, "derived dimension {dim} exceeds cap");
    let mut gram = vec![vec![Rational::zero(); dim]; dim];
    for (row, col, value) in cells {
        gram[row][col] = value;
        gram[col][row] = value; // declared symmetric fill
    }

    let space = InnerProductSpace::new(gram).map_err(compute_err)?;
    let pivots = space.ldlt_pivots();
    let computed_pd = pivots.is_ok();

    if authored_pd != computed_pd {
        return Err(Diag::of_kind(error::DefinitenessCrosscheckFailed {
            detail: format!(
                "definiteness cross-check failed for {gram_iri}: authored {authored} but LDLᵀ says {}",
                if computed_pd {
                    "positive-definite".to_string()
                } else {
                    pivots.unwrap_err()
                }
            ),
        }));
    }

    // Verdicts agree. When both say positive-definite the pivots are the
    // certificate; when both say NOT positive-definite there are no positive
    // pivots to report (an agreed non-PD Gram carries an empty certificate).
    match pivots {
        Ok(pivots) => Ok(pivots.into_iter().map(Rational::ratio_string).collect()),
        Err(_) => Ok(Vec::new()),
    }
}

/// The metric distance `‖x_a − x_b‖_G` and cosine `⟨x_a,x_b⟩/(‖x_a‖‖x_b‖)`
/// between the basis vectors of two intensity observations, sharing the metric
/// of `obs_a`. Returned as `(distance, cosine)` fixed-precision decimals.
pub fn distance_and_cosine(
    graph: &Graph,
    obs_a_iri: &str,
    obs_b_iri: &str,
) -> gmeow_errors::Result<(String, String)> {
    let index = index_graph(graph);
    let a = load_inputs(&index, obs_a_iri)?;
    let b = load_inputs(&index, obs_b_iri)?;
    // The metric and the axis→dimension basis of `obs_b` are discarded below —
    // both vectors are measured with `obs_a`'s `InnerProductSpace`. That is only
    // meaningful when the two observations share the same metric basis; a
    // mismatch would otherwise be silently zero-padded/truncated into a
    // well-formed but meaningless number. Hard-fail instead.
    if a.space != b.space || a.axis_to_dim != b.axis_to_dim {
        return Err(Diag::of_kind(error::MetricBasisMismatch {
            detail: format!(
                "distance requires both observations to share the same metric basis; \
                 obs_a {obs_a_iri} and obs_b {obs_b_iri} differ in Gram matrix / axis map"
            ),
        }));
    }
    let distance = a
        .space
        .distance(&a.vector, &b.vector)
        .map_err(compute_err)?;
    let cosine = a.space.cosine(&a.vector, &b.vector).map_err(compute_err)?;
    Ok((distance, cosine))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};
    use purrdf::{NativeRdfFormat, parse_dataset};

    fn r(num: i128, den: i128) -> Rational {
        Rational::new(num, den).expect("rational")
    }

    /// The canonical AC2 correlated metric G = [[1, 1/4], [1/4, 1]].
    fn correlated_gram() -> InnerProductSpace {
        InnerProductSpace::new(vec![vec![r(1, 1), r(1, 4)], vec![r(1, 4), r(1, 1)]]).expect("space")
    }

    fn turtle_to_gts(turtle: &str) -> Vec<u8> {
        let dataset = parse_dataset(
            turtle.as_bytes(),
            NativeRdfFormat::Turtle.media_type(),
            None,
        )
        .expect("parse turtle");
        let mut builder = SnapshotBuilder::default();
        builder.add_dataset(&dataset).expect("add dataset");
        emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit gts")
    }

    // Hard fails on unrecognized declared handles, via the graph path.
    #[test]
    fn unrecognized_norm_and_policy_hard_fail() {
        let bad_norm = observation_turtle(
            "gmeow:affectMetricTensorNorm2",
            "gmeow:weightingValenceDominant",
        );
        let err = geometry_from_gts_bytes(&turtle_to_gts(&bad_norm), None).unwrap_err();
        assert!(err.message().contains("normFunction"), "{err}");

        let bad_policy =
            observation_turtle("gmeow:affectMetricTensorNorm", "gmeow:weightingMadeUp");
        let err = geometry_from_gts_bytes(&turtle_to_gts(&bad_policy), None).unwrap_err();
        assert!(err.message().contains("weightingPolicy"), "{err}");
    }

    // A vector cell whose dimension lacks coreAxisIndex is a hard fail.
    #[test]
    fn missing_core_axis_index_hard_fails() {
        let mut turtle = observation_turtle(
            "gmeow:affectMetricTensorNorm",
            "gmeow:weightingValenceDominant",
        );
        // Drop the coreAxisIndex declarations.
        turtle = turtle
            .lines()
            .filter(|line| !line.contains("gmeow:coreAxisIndex"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = geometry_from_gts_bytes(&turtle_to_gts(&turtle), None).unwrap_err();
        assert!(err.message().contains("coreAxisIndex"), "{err}");
    }

    // An oversized Gram-matrix index is a hard fail, NOT a lossy `usize` cast or
    // an OOM-scale allocation: a huge `math:atRow` must return `Err` before any
    // matrix is sized.
    #[test]
    fn oversized_matrix_index_hard_fails() {
        let turtle = observation_turtle(
            "gmeow:affectMetricTensorNorm",
            "gmeow:weightingValenceDominant",
        )
        .replace(
            "math:atRow \"1\"^^xsd:integer ; math:atColumn \"1\"^^xsd:integer",
            "math:atRow \"100000000000\"^^xsd:integer ; math:atColumn \"1\"^^xsd:integer",
        );
        let err = geometry_from_gts_bytes(&turtle_to_gts(&turtle), None).unwrap_err();
        assert!(
            err.message().contains("matrix row") && err.message().contains("100000000000"),
            "{err}"
        );
    }

    // An oversized core-axis index is a hard fail before it can size the vector.
    #[test]
    fn oversized_core_axis_index_hard_fails() {
        let turtle = observation_turtle(
            "gmeow:affectMetricTensorNorm",
            "gmeow:weightingValenceDominant",
        )
        .replace(
            "gmeow:coreAxisIndex \"1\"^^xsd:nonNegativeInteger",
            "gmeow:coreAxisIndex \"9999999999\"^^xsd:nonNegativeInteger",
        );
        let err = geometry_from_gts_bytes(&turtle_to_gts(&turtle), None).unwrap_err();
        assert!(err.message().contains("core axis"), "{err}");
    }

    // Graph-parse path is load-bearing: intensity + dominant axis from turtle.
    #[test]
    fn graph_parse_path_computes_intensity_and_dominant_axis() {
        let turtle = observation_turtle(
            "gmeow:affectMetricTensorNorm",
            "gmeow:weightingValenceDominant",
        );
        let bytes = turtle_to_gts(&turtle);
        let all = geometry_from_gts_bytes(&bytes, None).unwrap();
        assert_eq!(all.len(), 1);
        let geom = &all[0];
        assert_eq!(geom.intensity, "0.888819");
        assert_eq!(geom.quadratic_form, "79/100");
        assert_eq!(geom.dominant_axis, gm("dimensionValence"));
        assert_eq!(geom.pivots, vec!["1".to_string(), "15/16".to_string()]);
        // Unit-clamp normalization on PAD [-1, 1]: valence 0.7 → 0.85, arousal 0.4 → 0.7.
        assert_eq!(
            geom.normalized,
            vec![
                NormalizedAxis {
                    axis: 0,
                    dimension: gm("dimensionValence"),
                    value: "0.85".to_string(),
                },
                NormalizedAxis {
                    axis: 1,
                    dimension: gm("dimensionArousal"),
                    value: "0.7".to_string(),
                },
            ]
        );

        // Same call twice → byte-identical structure (determinism).
        let again = geometry_from_gts_bytes(&bytes, None).unwrap();
        assert_eq!(all, again);

        // Single-observation selection agrees with the sweep.
        let one = affective_geometry(
            &purrdf::gts::reader::read(&bytes, false, None),
            &geom.observation,
        )
        .unwrap();
        assert_eq!(&one, geom);
    }

    /// A complete `gmeow:DerivedAffectIntensityObservation` over the correlated
    /// metric G = [[1, 1/4], [1/4, 1]], vector valence 0.7 / arousal 0.4.
    fn observation_turtle(norm_fn: &str, policy: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "@prefix gmeow: <{GM}> .\n@prefix math: <{MATH}> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/affect/> ."
        );
        out.push_str(
            r#"
gmeow:dimensionValence a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex "0"^^xsd:nonNegativeInteger .
gmeow:dimensionArousal a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex "1"^^xsd:nonNegativeInteger .

ex:padUnitScale a gmeow:AffectScaleProfile ;
    gmeow:profileRangeMin "-1.0"^^xsd:decimal ;
    gmeow:profileRangeMax "1.0"^^xsd:decimal ;
    gmeow:metricGram ex:correlatedGram .

ex:correlatedGram a math:GramMatrix ;
    math:definiteness math:positiveDefinite ;
    math:hasEntry ex:g00 , ex:g01 , ex:g11 .

ex:g00 a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "0"^^xsd:integer ; math:entryValue ex:ratOne .
ex:g01 a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratQuarter .
ex:g11 a math:MatrixEntry ; math:atRow "1"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOne .

ex:ratOne a math:RationalValue ; math:numerator "1"^^xsd:integer ; math:denominator "1"^^xsd:integer .
ex:ratQuarter a math:RationalValue ; math:numerator "1"^^xsd:integer ; math:denominator "4"^^xsd:integer .

ex:vec a gmeow:AffectVectorObservation ;
    gmeow:vectorComponent ex:valenceCell , ex:arousalCell .

ex:valenceCell a gmeow:Appraisal ;
    gmeow:appraisalDimension gmeow:dimensionValence ;
    gmeow:appraisalValue "0.7"^^xsd:decimal .

ex:arousalCell a gmeow:Appraisal ;
    gmeow:appraisalDimension gmeow:dimensionArousal ;
    gmeow:appraisalValue "0.4"^^xsd:decimal .
"#,
        );
        let _ = writeln!(
            out,
            "ex:intensity a gmeow:DerivedAffectIntensityObservation ;\n    gmeow:intensityBasis ex:vec ;\n    gmeow:metricProfile ex:padUnitScale ;\n    gmeow:weightingPolicy {policy} ;\n    gmeow:normFunction {norm_fn} ;\n    gmeow:derivedByFunction gmeow:fnAffectiveIntensity ."
        );
        out
    }

    /// A `gmeow:DerivedAffectIntensityObservation` named `suffix`, over a 2×2
    /// metric with off-diagonal `off = off_num/off_den` and vector
    /// `(v0, v1)` (each written as `n/10`). All resource IRIs are suffixed so two
    /// such blocks compose into one graph with fully independent bases.
    fn distinct_observation_turtle(
        suffix: &str,
        off_num: i128,
        off_den: i128,
        v0_tenths: i128,
        v1_tenths: i128,
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            r#"ex:padUnitScale{suffix} a gmeow:AffectScaleProfile ;
    gmeow:profileRangeMin "-1.0"^^xsd:decimal ;
    gmeow:profileRangeMax "1.0"^^xsd:decimal ;
    gmeow:metricGram ex:gram{suffix} .

ex:gram{suffix} a math:GramMatrix ;
    math:definiteness math:positiveDefinite ;
    math:hasEntry ex:g00{suffix} , ex:g01{suffix} , ex:g11{suffix} .

ex:g00{suffix} a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "0"^^xsd:integer ; math:entryValue ex:ratOne{suffix} .
ex:g01{suffix} a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOff{suffix} .
ex:g11{suffix} a math:MatrixEntry ; math:atRow "1"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOne{suffix} .

ex:ratOne{suffix} a math:RationalValue ; math:numerator "1"^^xsd:integer ; math:denominator "1"^^xsd:integer .
ex:ratOff{suffix} a math:RationalValue ; math:numerator "{off_num}"^^xsd:integer ; math:denominator "{off_den}"^^xsd:integer .

ex:vec{suffix} a gmeow:AffectVectorObservation ;
    gmeow:vectorComponent ex:valenceCell{suffix} , ex:arousalCell{suffix} .

ex:valenceCell{suffix} a gmeow:Appraisal ;
    gmeow:appraisalDimension gmeow:dimensionValence ;
    gmeow:appraisalValue "0.{v0_tenths}"^^xsd:decimal .

ex:arousalCell{suffix} a gmeow:Appraisal ;
    gmeow:appraisalDimension gmeow:dimensionArousal ;
    gmeow:appraisalValue "0.{v1_tenths}"^^xsd:decimal .

ex:intensity{suffix} a gmeow:DerivedAffectIntensityObservation ;
    gmeow:intensityBasis ex:vec{suffix} ;
    gmeow:metricProfile ex:padUnitScale{suffix} ;
    gmeow:weightingPolicy gmeow:weightingValenceDominant ;
    gmeow:normFunction gmeow:affectMetricTensorNorm ;
    gmeow:derivedByFunction gmeow:fnAffectiveIntensity ."#
        );
        out
    }

    /// A standalone `math:GramMatrix` (no observation), authored with
    /// `definiteness` and a 2×2 body whose off-diagonal is `off = off_num/off_den`.
    /// When `with_definiteness` is false the `math:definiteness` triple is omitted.
    fn gram_only_turtle(off_num: i128, off_den: i128, with_definiteness: bool) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "@prefix gmeow: <{GM}> .\n@prefix math: <{MATH}> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/affect/> ."
        );
        let definiteness = if with_definiteness {
            "\n    math:definiteness math:positiveDefinite ;"
        } else {
            ""
        };
        let _ = write!(
            out,
            r#"
ex:testGram a math:GramMatrix ;{definiteness}
    math:hasEntry ex:g00 , ex:g01 , ex:g11 .

ex:g00 a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "0"^^xsd:integer ; math:entryValue ex:ratOne .
ex:g01 a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOff .
ex:g11 a math:MatrixEntry ; math:atRow "1"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOne .

ex:ratOne a math:RationalValue ; math:numerator "1"^^xsd:integer ; math:denominator "1"^^xsd:integer .
ex:ratOff a math:RationalValue ; math:numerator "{off_num}"^^xsd:integer ; math:denominator "{off_den}"^^xsd:integer .
"#
        );
        out
    }

    const TEST_GRAM_IRI: &str = "https://blackcatinformatics.ca/gmeow/examples/affect/testGram";

    // The shipped/authored correlated Gram (off-diagonal 1/4) is authored PD and
    // the LDLᵀ witness AGREES: pivots [1, 15/16] certify it.
    #[test]
    fn crosscheck_agreeing_pd_returns_pivots() {
        let bytes = turtle_to_gts(&gram_only_turtle(1, 4, true));
        let pivots = crosscheck_authored_definiteness(&bytes, TEST_GRAM_IRI).unwrap();
        assert_eq!(pivots, vec!["1".to_string(), "15/16".to_string()]);
    }

    // Authored `math:positiveDefinite` but numerically INDEFINITE (off-diagonal
    // 2 > 1 drives pivot 1 = 1 − 4 = −3 ≤ 0) → the cross-check hard-fails.
    #[test]
    fn crosscheck_authored_pd_but_indefinite_hard_fails() {
        let bytes = turtle_to_gts(&gram_only_turtle(2, 1, true));
        let err = crosscheck_authored_definiteness(&bytes, TEST_GRAM_IRI).unwrap_err();
        assert!(err.message().contains("cross-check failed"), "{err}");
        assert!(err.message().contains("positiveDefinite"), "{err}");
    }

    // A Gram with NO `math:definiteness` is a loud error — SHACL does not require
    // it, so its absence must not silently pass the gate.
    #[test]
    fn crosscheck_missing_definiteness_hard_fails() {
        let bytes = turtle_to_gts(&gram_only_turtle(1, 4, false));
        let err = crosscheck_authored_definiteness(&bytes, TEST_GRAM_IRI).unwrap_err();
        assert!(err.message().contains("authored PD absent"), "{err}");
    }

    fn two_observation_graph(a: &str, b: &str) -> Graph {
        let mut turtle = String::new();
        let _ = writeln!(
            turtle,
            "@prefix gmeow: <{GM}> .\n@prefix math: <{MATH}> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/affect/> ."
        );
        turtle.push_str(
            "gmeow:dimensionValence a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex \"0\"^^xsd:nonNegativeInteger .\n",
        );
        turtle.push_str(
            "gmeow:dimensionArousal a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex \"1\"^^xsd:nonNegativeInteger .\n",
        );
        turtle.push_str(a);
        turtle.push('\n');
        turtle.push_str(b);
        let bytes = turtle_to_gts(&turtle);
        purrdf::gts::reader::read(&bytes, false, None)
    }

    fn obs_iri(suffix: &str) -> String {
        format!("https://blackcatinformatics.ca/gmeow/examples/affect/intensity{suffix}")
    }

    // Matching metric basis (identical Gram + axis map) computes a real value —
    // agreeing bit-for-bit with the direct InnerProductSpace geometry.
    #[test]
    fn distance_and_cosine_matching_basis_ok() {
        let a = distinct_observation_turtle("A", 1, 4, 7, 4); // G off-diag 1/4, (0.7, 0.4)
        let b = distinct_observation_turtle("B", 1, 4, 4, 7); // same G, (0.4, 0.7)
        let graph = two_observation_graph(&a, &b);
        let (distance, cosine) =
            distance_and_cosine(&graph, &obs_iri("A"), &obs_iri("B")).expect("matching basis");
        // Pin to the direct-space computation over the shared correlated metric.
        let space = correlated_gram();
        let x = [r(7, 10), r(2, 5)];
        let y = [r(2, 5), r(7, 10)];
        assert_eq!(distance, space.distance(&x, &y).unwrap());
        assert_eq!(cosine, space.cosine(&x, &y).unwrap());
        // Deterministic: same call twice → identical strings.
        let again =
            distance_and_cosine(&graph, &obs_iri("A"), &obs_iri("B")).expect("matching basis");
        assert_eq!((distance, cosine), again);
    }

    // Different Gram matrices between the two observations is a hard fail — never
    // a silently zero-padded/truncated meaningless number.
    #[test]
    fn distance_and_cosine_mismatched_gram_hard_fails() {
        let a = distinct_observation_turtle("A", 1, 4, 7, 4); // G off-diag 1/4
        let b = distinct_observation_turtle("B", 0, 1, 4, 7); // G off-diag 0 → different metric
        let graph = two_observation_graph(&a, &b);
        let err = distance_and_cosine(&graph, &obs_iri("A"), &obs_iri("B"))
            .expect_err("mismatched Gram must hard fail");
        assert!(err.message().contains("metric basis"), "{err}");
        assert!(err.message().contains("Gram matrix / axis map"), "{err}");
    }
}
