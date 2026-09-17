// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
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

use std::collections::{BTreeMap, BTreeSet};

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
// The native bilinear-form distance authority: Q9 classification computes its exact-ℚ
// squared distances THROUGH this governed moded-builtin family, never a private path
// (the affect classifier). `compare_sqdist` is the overflow-safe ordering the ranking rides.
use gmeow_logic::{BilinearFormError, bilinear_sqdist, compare_sqdist};

use gmeow_errors::Diag;

pub mod error;

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
    let mut seen_axes = BTreeSet::new();
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
        let axis = bounded_index(axis, "core axis")?;
        // One cell per core axis: a second cell on the same axis would silently
        // overwrite the first (an order-dependent, wrong vector) — hard-fail instead.
        if !seen_axes.insert(axis) {
            return Err(Diag::of_kind(error::DuplicateCoordinateAxis {
                detail: format!(
                    "{vector_iri} declares more than one gmeow:vectorComponent cell on core \
                     axis {axis} ({dimension}); exactly one cell per axis is required"
                ),
            }));
        }
        let value = Rational::parse_decimal(
            &first_literal(index, &cell, &gm("appraisalValue")).ok_or_else(|| {
                Diag::of_kind(error::MissingAffectProperty {
                    node: cell.clone(),
                    property: "gmeow:appraisalValue".to_owned(),
                })
            })?,
        )?;
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
    )?;
    let range_max = Rational::parse_decimal(
        &first_literal(index, &profile, &gm("profileRangeMax")).ok_or_else(|| {
            Diag::of_kind(error::MissingAffectProperty {
                node: format!("scale profile {profile}"),
                property: "gmeow:profileRangeMax".to_owned(),
            })
        })?,
    )?;

    let gram_cells = load_gram(index, &gram_iri)?;
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
    // Every contributing index was bounded below `MAX_BASIS_DIM`, so the derived
    // dimension is bounded too; enforce it in ALL build profiles before it sizes
    // the matrix (a debug_assert would compile out of release).
    if dim > MAX_BASIS_DIM {
        return Err(Diag::of_kind(error::CoordinateDimensionMismatch {
            detail: format!(
                "derived affect basis dimension {dim} exceeds the maximum basis order {MAX_BASIS_DIM}"
            ),
        }));
    }

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
        space: InnerProductSpace::new(gram)?,
        vector,
        axis_to_dim,
        cells: vector_cells,
        range_min,
        range_max,
    })
}

fn compute_geometry(index: &TripleIndex, observation: &str) -> gmeow_errors::Result<Geometry> {
    let inputs = load_inputs(index, observation)?;
    let quadratic = inputs.space.quadratic_form(&inputs.vector)?;
    let intensity = sqrt_rational_decimal(quadratic)?;
    let pivots = inputs.space.ldlt_pivots()?; // hard-fails on non-PD G
    let dominant = inputs.space.dominant_axis(&inputs.vector)?;
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
                value: normalize_to_unit(value, &inputs.range_min, &inputs.range_max)?,
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
    geometry_from_index(&index, observation_iri)
}

/// Compute the same exact affect geometry directly from an admitted native dataset.
/// The caller retains its selected graph role; no container serialization or parse occurs.
///
/// # Errors
/// Rejects missing observations and invalid geometry with the same diagnostics as the
/// GTS consumer boundary.
pub fn geometry_from_dataset(
    dataset: &purrdf::RdfDataset,
    observation_iri: Option<&str>,
) -> gmeow_errors::Result<Vec<Geometry>> {
    geometry_from_index(&gmeow_math::index_dataset(dataset), observation_iri)
}

fn geometry_from_index(
    index: &TripleIndex,
    observation_iri: Option<&str>,
) -> gmeow_errors::Result<Vec<Geometry>> {
    let observations = match observation_iri {
        Some(iri) => vec![iri.to_string()],
        None => derived_observations(index),
    };
    if observations.is_empty() {
        return Err(Diag::of_kind(error::NoAffectObservations {}));
    }
    observations
        .iter()
        .map(|iri| compute_geometry(index, iri))
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
    let cells = load_gram(&index, gram_iri)?;
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
    // Enforced in ALL build profiles (a debug_assert would compile out of release);
    // every Gram index is bounded below `MAX_BASIS_DIM` upstream, so this cannot fire.
    if dim > MAX_BASIS_DIM {
        return Err(Diag::of_kind(error::CoordinateDimensionMismatch {
            detail: format!(
                "Gram matrix {gram_iri} order {dim} exceeds the maximum basis order {MAX_BASIS_DIM}"
            ),
        }));
    }
    let mut gram = vec![vec![Rational::zero(); dim]; dim];
    for (row, col, value) in cells {
        gram[row][col] = value;
        gram[col][row] = value; // declared symmetric fill
    }

    let space = InnerProductSpace::new(gram)?;
    let pivots = space.ldlt_pivots();
    let computed_pd = pivots.is_ok();

    if authored_pd != computed_pd {
        return Err(Diag::of_kind(error::DefinitenessCrosscheckFailed {
            detail: format!(
                "definiteness cross-check failed for {gram_iri}: authored {authored} but LDLᵀ says {}",
                if computed_pd {
                    "positive-definite".to_string()
                } else {
                    pivots.unwrap_err().message().to_string()
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
    let distance = a.space.distance(&a.vector, &b.vector)?;
    let cosine = a.space.cosine(&a.vector, &b.vector)?;
    Ok((distance, cosine))
}

// ---------------------------------------------------------------------------
// Nearest-prototype classification (competency Q9) — a vantage-relative ranked
// judgment under an EXPLICIT vantage Gram, dispatched through the native builtin family.
// ---------------------------------------------------------------------------

/// The metric lens a classification ranks prototypes under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricLens {
    /// Exact G-distance: rank by ASCENDING squared metric distance
    /// `(x − p)ᵀG(x − p)`. Answers "which prototype, INCLUDING intensity".
    GDistance,
    /// G-cosine alignment: rank by DESCENDING `⟨x,p⟩_G / (‖x‖‖p‖)`. Answers "which
    /// QUALITY / direction" — deliberately collapses gradation (annoyance→anger→rage
    /// is one direction at growing magnitude; AFFECT-DESIGN §"Magnitudes, intensity,
    /// and gradation").
    Cosine,
}

impl MetricLens {
    /// The greppable output tag naming the lens (`distance` / `cosine`).
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            MetricLens::GDistance => "distance",
            MetricLens::Cosine => "cosine",
        }
    }
}

/// One ranked prototype in a classification profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedPrototype {
    /// The prototype observation IRI.
    pub prototype: String,
    /// The EXACT squared G-distance `(x − p)ᵀG(x − p)` as a printable ratio — the value
    /// the G-distance ranking decides on, computed through the native bilinear builtin.
    pub squared_distance: String,
    /// `√(squared_distance)` as a fixed-precision decimal — the display seam, never
    /// used for selection.
    pub distance: String,
    /// The signed G-cosine as a fixed-precision decimal — `Some` only under the cosine
    /// lens (where it is the selection basis), `None` under the G-distance lens.
    pub cosine: Option<String>,
}

/// A vantage-relative nearest-prototype classification: a total-order ranked prototype
/// profile under a chosen [`MetricLens`] and an EXPLICIT vantage Gram, plus the exact
/// rank-1/rank-2 margin. Swapping the vantage Gram or the lens can flip the winner —
/// classification is a computed, vantage-relative claim, never ground truth (Q9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    /// The lens the ranking was selected under.
    pub metric: MetricLens,
    /// The vantage `gmeow:AffectScaleProfile` whose `gmeow:metricGram` was imposed on
    /// every coordinate vector.
    pub vantage_profile: String,
    /// The ranked prototypes, best-first (ascending G-distance, or descending cosine),
    /// truncated to the requested `top_k`.
    pub ranked: Vec<RankedPrototype>,
    /// The EXACT perpendicular Voronoi margin² between the top-two SELECTED prototypes,
    /// `Δ² / (4‖p₁−p₂‖²_G)` (with `Δ` their squared-distance gap to the state), as a
    /// printable ratio — the honest "how contested" number. `"0"` when fewer than two
    /// prototypes or an exact first-place tie (maximally contested).
    pub margin_squared: String,
    /// `√(margin_squared)` as a fixed-precision decimal — the display seam.
    pub margin: String,
}

/// The dense zero vector of length `dim` (the metric origin).
fn origin(dim: usize) -> Vec<Rational> {
    vec![Rational::zero(); dim]
}

/// The exact sign of a rational: `-1`, `0`, or `+1`.
fn rsign(r: Rational) -> i32 {
    if r.is_zero() {
        0
    } else if r.is_non_positive() {
        -1
    } else {
        1
    }
}

/// Map a native [`BilinearFormError`] to a typed affect diagnostic.
fn map_bilinear(e: BilinearFormError) -> Diag {
    match e {
        BilinearFormError::DimensionMismatch => Diag::of_kind(error::CoordinateDimensionMismatch {
            detail: "coordinate vectors differ in dimension under the vantage metric form"
                .to_owned(),
        }),
        BilinearFormError::Overflow => Diag::of_kind(error::BilinearDistanceFailed {
            detail: "exact-rational overflow computing the bilinear-form squared distance"
                .to_owned(),
        }),
        BilinearFormError::MetricForm => Diag::of_kind(error::BilinearDistanceFailed {
            detail: "malformed vantage metric form (Gram/vector) in the bilinear-form builtin"
                .to_owned(),
        }),
    }
}

/// Load one affect vector observation's dense exact-ℚ coordinate vector over the
/// core-affect basis, sized to the vantage form order `dim`, validating every cell
/// magnitude against the vantage profile's declared `[range_min, range_max]`.
///
/// A cell whose axis index reaches or exceeds `dim` is a HARD fail (the vantage form
/// would silently truncate it); an out-of-range magnitude is a HARD fail (the scale
/// does not define it). Both are numeric invariants, enforced OUTSIDE the logic here
/// (Principle 12).
fn load_coordinate_vector(
    index: &TripleIndex,
    obs_iri: &str,
    dim: usize,
    range_min: Rational,
    range_max: Rational,
) -> gmeow_errors::Result<Vec<Rational>> {
    let cells = load_cells(index, obs_iri)?;
    let mut vector = origin(dim);
    for (axis, dimension, value) in &cells {
        if *axis >= dim {
            return Err(Diag::of_kind(error::CoordinateDimensionMismatch {
                detail: format!(
                    "{obs_iri} cell on axis {axis} ({dimension}) exceeds the vantage form order {dim}"
                ),
            }));
        }
        if *value < range_min || *value > range_max {
            return Err(Diag::of_kind(error::ValueOutOfRange {
                detail: format!(
                    "{obs_iri} cell on axis {axis} ({dimension}) magnitude {} is outside the \
                     vantage profile range [{}, {}]",
                    value.ratio_string(),
                    range_min.ratio_string(),
                    range_max.ratio_string()
                ),
            }));
        }
        vector[*axis] = *value;
    }
    Ok(vector)
}

/// A loaded prototype candidate: its IRI, coordinate vector, exact squared G-distance
/// to the state, and (cosine lens only) the exact `(⟨x,p⟩_G, ‖p‖²_G)` selection key.
struct Candidate {
    iri: String,
    vector: Vec<Rational>,
    squared_distance: Rational,
    /// `(inner, proto_norm_sq)` under the vantage G — `Some` only in cosine mode.
    cosine_key: Option<(Rational, Rational)>,
}

/// True when `a` ranks strictly before `b` under `metric` (best-first), with a
/// deterministic lexicographically-least-IRI tie-break. Ordering rides the the native builtin
/// family's overflow-safe exact compare, never `Rational` ordering (which panics on
/// overflow); the only bare comparisons are on axis signs (`i32`) and IRIs.
fn ranks_before(a: &Candidate, b: &Candidate, metric: MetricLens) -> gmeow_errors::Result<bool> {
    let order = match metric {
        MetricLens::GDistance => {
            // Ascending exact squared distance.
            compare_sqdist(&a.squared_distance, &b.squared_distance).map_err(map_bilinear)?
        }
        MetricLens::Cosine => {
            // Descending exact cosine, sign-first then squared-magnitude (any positive
            // cosine beats any negative — squaring alone is wrong).
            let (ia, qa) = a.cosine_key.expect("cosine key present in cosine mode");
            let (ib, qb) = b.cosine_key.expect("cosine key present in cosine mode");
            let (sa, sb) = (rsign(ia), rsign(ib));
            if sa != sb {
                // Higher cosine sign sorts first (best-first).
                if sa > sb {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            } else {
                // Same sign: compare inner² · other_norm² (exact, overflow-safe). For
                // both-positive the LARGER product is the higher cosine (sorts first);
                // for both-negative the SMALLER product is the higher (less negative).
                let lhs = ia.checked_mul(ia)?.checked_mul(qb)?;
                let rhs = ib.checked_mul(ib)?.checked_mul(qa)?;
                let mag = compare_sqdist(&lhs, &rhs).map_err(map_bilinear)?;
                if sa >= 0 { mag.reverse() } else { mag }
            }
        }
    };
    Ok(match order {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => a.iri.as_str() < b.iri.as_str(),
    })
}

/// Classify a state affect vector to its nearest named-emotion prototype(s) under an
/// EXPLICIT vantage metric, dispatching every exact-ℚ squared distance THROUGH the
/// native bilinear-form builtin family ([`gmeow_logic::bilinear_sqdist`]) — never a
/// private numeric path (the maintainer routing mandate). Answers competency Q9
/// ("is this vector a schadenfreude?").
///
/// # Explicit vantage-Gram (the decoupled metric)
///
/// The `vantage_profile_iri`'s `gmeow:metricGram` `G` is imposed on the state AND every
/// prototype coordinate vector, regardless of each observation's own declared profile.
/// Swapping the vantage profile re-measures everyone under a different `G` and can flip
/// the winner — vantage-relativity is a function, not a caveat.
///
/// # Exactness & routing
///
/// The squared distance `(x − p)ᵀG(x − p)` and (cosine lens) the inner product via the
/// polarization identity `⟨x,p⟩ = (‖x‖² + ‖p‖² − ‖x−p‖²)/2` are ALL computed through the
/// native builtin; ordering rides [`gmeow_logic::compare_sqdist`] (overflow-safe). The
/// `√` decimals and the cosine display string cross the solver seam for DISPLAY only,
/// never selection. Ties break to the lexicographically-least prototype IRI.
///
/// # Failure (no-optionality)
///
/// HARD fails on: an EMPTY prototype set; a missing vantage `metricGram`/range; a
/// NON-positive-definite vantage Gram (the builtin trusts PD, so an indefinite form
/// would make distances negative and the argmin garbage); a coordinate axis wider than
/// the form (`DimensionMismatch`); an out-of-range magnitude; COINCIDENT prototype
/// signatures under `G`; and a zero-G-norm state or prototype under the cosine lens.
pub fn classify(
    graph: &Graph,
    state_iri: &str,
    prototype_iris: &[String],
    vantage_profile_iri: &str,
    metric: MetricLens,
    top_k: Option<usize>,
) -> gmeow_errors::Result<Classification> {
    if prototype_iris.is_empty() {
        return Err(Diag::of_kind(error::EmptyPrototypeSet {}));
    }
    let index = index_graph(graph);

    // ── The explicit vantage Gram + its declared coordinate range ───────────────
    let gram_iri = first_iri(&index, vantage_profile_iri, &gm("metricGram")).ok_or_else(|| {
        Diag::of_kind(error::MissingAffectProperty {
            node: format!("vantage profile {vantage_profile_iri}"),
            property: "gmeow:metricGram".to_owned(),
        })
    })?;
    let gram_cells = load_gram(&index, &gram_iri)?;
    let range_min = Rational::parse_decimal(
        &first_literal(&index, vantage_profile_iri, &gm("profileRangeMin")).ok_or_else(|| {
            Diag::of_kind(error::MissingAffectProperty {
                node: format!("vantage profile {vantage_profile_iri}"),
                property: "gmeow:profileRangeMin".to_owned(),
            })
        })?,
    )?;
    let range_max = Rational::parse_decimal(
        &first_literal(&index, vantage_profile_iri, &gm("profileRangeMax")).ok_or_else(|| {
            Diag::of_kind(error::MissingAffectProperty {
                node: format!("vantage profile {vantage_profile_iri}"),
                property: "gmeow:profileRangeMax".to_owned(),
            })
        })?,
    )?;

    let dim = gram_cells
        .iter()
        .flat_map(|(r, c, _)| [*r, *c])
        .max()
        .map(|m| m + 1)
        .ok_or_else(|| {
            Diag::of_kind(error::GramHasNoEntries {
                gram_iri: gram_iri.clone(),
            })
        })?;
    // Enforced in ALL build profiles (a debug_assert would compile out of release);
    // every vantage-Gram index is bounded below `MAX_BASIS_DIM` upstream.
    if dim > MAX_BASIS_DIM {
        return Err(Diag::of_kind(error::CoordinateDimensionMismatch {
            detail: format!(
                "vantage Gram {gram_iri} order {dim} exceeds the maximum basis order {MAX_BASIS_DIM}"
            ),
        }));
    }

    // ── PD-certify the vantage Gram: the builtin TRUSTS positive-definiteness, so a
    //    non-PD vantage would make the "distances" negative and the argmin garbage. ─
    let mut matrix = vec![vec![Rational::zero(); dim]; dim];
    for (row, col, value) in &gram_cells {
        matrix[*row][*col] = *value;
        matrix[*col][*row] = *value; // declared symmetric fill
    }
    let space = InnerProductSpace::new(matrix)?;
    space.ldlt_pivots().map_err(|e| {
        Diag::of_kind(error::NonPositiveDefiniteVantage {
            detail: format!(
                "vantage Gram {gram_iri} is not positive-definite ({}); classification needs a \
                 metric, not an indefinite form",
                e.message()
            ),
        })
    })?;

    // ── The state coordinate vector, and (cosine mode) its exact G-norm² ────────
    let state = load_coordinate_vector(&index, state_iri, dim, range_min, range_max)?;
    let state_norm_sq = match metric {
        MetricLens::Cosine => {
            let q = bilinear_sqdist(&gram_cells, &state, &origin(dim)).map_err(map_bilinear)?;
            if q.is_zero() {
                return Err(Diag::of_kind(error::ZeroNormCosine {
                    detail: format!("state {state_iri} has zero G-norm; its cosine is undefined"),
                }));
            }
            Some(q)
        }
        MetricLens::GDistance => None,
    };

    // ── Load + measure every prototype THROUGH the native bilinear builtin ───────
    let two = Rational::from_i128(2)?;
    let mut candidates: Vec<Candidate> = Vec::with_capacity(prototype_iris.len());
    for proto_iri in prototype_iris {
        let vector = load_coordinate_vector(&index, proto_iri, dim, range_min, range_max)?;
        let squared_distance =
            bilinear_sqdist(&gram_cells, &state, &vector).map_err(map_bilinear)?;
        let cosine_key = match (metric, state_norm_sq) {
            (MetricLens::Cosine, Some(x_norm_sq)) => {
                let p_norm_sq =
                    bilinear_sqdist(&gram_cells, &vector, &origin(dim)).map_err(map_bilinear)?;
                if p_norm_sq.is_zero() {
                    return Err(Diag::of_kind(error::ZeroNormCosine {
                        detail: format!(
                            "prototype {proto_iri} has zero G-norm; its cosine is undefined"
                        ),
                    }));
                }
                // ⟨x,p⟩_G = (‖x‖² + ‖p‖² − ‖x−p‖²) / 2 — polarization, all via the native builtin.
                let inner = x_norm_sq
                    .checked_add(p_norm_sq)?
                    .checked_sub(squared_distance)?
                    .checked_div(two)?;
                Some((inner, p_norm_sq))
            }
            _ => None,
        };
        candidates.push(Candidate {
            iri: proto_iri.clone(),
            vector,
            squared_distance,
            cosine_key,
        });
    }

    // ── Pairwise-distinctness: coincident signatures are an authoring error that
    //    makes the margin bisector undefined (numeric invariant, enforced per P12). ─
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            let sep = bilinear_sqdist(&gram_cells, &candidates[i].vector, &candidates[j].vector)
                .map_err(map_bilinear)?;
            if sep.is_zero() {
                return Err(Diag::of_kind(error::CoincidentPrototypes {
                    detail: format!(
                        "prototypes {} and {} are coincident under the vantage metric \
                         (zero G-distance apart)",
                        candidates[i].iri, candidates[j].iri
                    ),
                }));
            }
        }
    }

    // ── Total-order ranking, best-first, via selection over the governed compare ─
    let mut ranked_candidates: Vec<Candidate> = Vec::with_capacity(candidates.len());
    while !candidates.is_empty() {
        let mut best = 0usize;
        for i in 1..candidates.len() {
            if ranks_before(&candidates[i], &candidates[best], metric)? {
                best = i;
            }
        }
        ranked_candidates.push(candidates.swap_remove(best));
    }

    // ── The exact perpendicular Voronoi margin between the top-two ──────────────
    let (margin_squared, margin) = if ranked_candidates.len() >= 2 {
        let p1 = &ranked_candidates[0];
        let p2 = &ranked_candidates[1];
        let separation =
            bilinear_sqdist(&gram_cells, &p1.vector, &p2.vector).map_err(map_bilinear)?;
        if separation.is_zero() {
            return Err(Diag::of_kind(error::CoincidentPrototypes {
                detail: format!(
                    "top-two prototypes {} and {} are coincident under the vantage metric",
                    p1.iri, p2.iri
                ),
            }));
        }
        let delta = p2.squared_distance.checked_sub(p1.squared_distance)?;
        let margin_sq = delta
            .checked_mul(delta)?
            .checked_div(Rational::from_i128(4)?.checked_mul(separation)?)?;
        (margin_sq.ratio_string(), sqrt_rational_decimal(margin_sq)?)
    } else {
        (
            Rational::zero().ratio_string(),
            sqrt_rational_decimal(Rational::zero())?,
        )
    };

    // ── Project to the report: cosine display via the shared space, top-k clamp ──
    let keep = top_k.map_or(ranked_candidates.len(), |k| k.min(ranked_candidates.len()));
    let mut ranked = Vec::with_capacity(keep);
    for cand in ranked_candidates.iter().take(keep) {
        let cosine = match metric {
            MetricLens::Cosine => Some(space.cosine(&state, &cand.vector)?),
            MetricLens::GDistance => None,
        };
        ranked.push(RankedPrototype {
            prototype: cand.iri.clone(),
            squared_distance: cand.squared_distance.ratio_string(),
            distance: sqrt_rational_decimal(cand.squared_distance)?,
            cosine,
        });
    }

    Ok(Classification {
        metric,
        vantage_profile: vantage_profile_iri.to_owned(),
        ranked,
        margin_squared,
        margin,
    })
}

/// Enumerate every `gmeow:AffectPrototype` individual in `graph`, in deterministic
/// ascending-IRI order — the default canonical prototype set the `gmeow affect classify`
/// CLI ranks against when no explicit `--prototype` is given.
pub fn affect_prototypes(graph: &Graph) -> Vec<String> {
    let index = index_graph(graph);
    let class = gm("AffectPrototype");
    let mut prototypes = subjects(&index)
        .filter(|subject| has_type(&index, subject, &class))
        .cloned()
        .collect::<Vec<_>>();
    prototypes.sort();
    prototypes
}

#[path = "lib.tests.rs"]
#[cfg(test)]
mod tests;
