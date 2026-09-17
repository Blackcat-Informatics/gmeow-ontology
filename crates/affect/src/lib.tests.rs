// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    // gmeow-test-input: synthetic-only
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
        &purrdf::gts_compose::MediumPlan::dist_default(None),
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

    let bad_policy = observation_turtle("gmeow:affectMetricTensorNorm", "gmeow:weightingMadeUp");
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

// Two vectorComponent cells on the SAME core axis is a hard fail, not a
// silent last-writer-wins overwrite (no-optionality).
#[test]
fn duplicate_core_axis_cell_hard_fails() {
    let turtle = observation_turtle(
        "gmeow:affectMetricTensorNorm",
        "gmeow:weightingValenceDominant",
    )
    .replace(
        "gmeow:vectorComponent ex:valenceCell , ex:arousalCell .",
        "gmeow:vectorComponent ex:valenceCell , ex:arousalCell , ex:valenceCellDup .\n\
             ex:valenceCellDup a gmeow:Appraisal ;\n    \
             gmeow:appraisalDimension gmeow:dimensionValence ;\n    \
             gmeow:appraisalValue \"0.2\"^^xsd:decimal .",
    );
    let err = geometry_from_gts_bytes(&turtle_to_gts(&turtle), None).unwrap_err();
    assert!(
        err.message().contains("more than one") && err.message().contains("core axis 0"),
        "{err}"
    );
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
    let native = purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None).unwrap();
    assert_eq!(
        geometry_from_dataset(&native, None).unwrap(),
        all,
        "native pipeline and GTS consumer must compute the same exact geometry"
    );
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
    let again = distance_and_cosine(&graph, &obs_iri("A"), &obs_iri("B")).expect("matching basis");
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

// ── nearest-prototype classification (Q9 production surface) ─────────────
//
// The classifier reads AffectVectorObservation coordinate vectors and imposes an
// EXPLICIT vantage Gram (the chosen profile's metricGram) on all of them, routing
// every squared distance through the native bilinear builtin.

const CLS_NS: &str = "https://blackcatinformatics.ca/gmeow/examples/affect/classify/";
const VANT_PROFILE: &str =
    "https://blackcatinformatics.ca/gmeow/examples/affect/classify/vantMetric";

/// An `AffectVectorObservation` named `suffix` with a valence + arousal cell
/// (decimal strings, signed OK). It declares NO metric profile — classification
/// imposes the explicit vantage Gram.
fn cls_vec(suffix: &str, valence: &str, arousal: &str) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        r#"ex:vec{suffix} a gmeow:AffectVectorObservation ;
    gmeow:vectorComponent ex:val{suffix} , ex:aro{suffix} .
ex:val{suffix} a gmeow:Appraisal ; gmeow:appraisalDimension gmeow:dimensionValence ; gmeow:appraisalValue "{valence}"^^xsd:decimal .
ex:aro{suffix} a gmeow:Appraisal ; gmeow:appraisalDimension gmeow:dimensionArousal ; gmeow:appraisalValue "{arousal}"^^xsd:decimal .
"#
    );
    out
}

fn cls_iri(suffix: &str) -> String {
    format!("{CLS_NS}vec{suffix}")
}

/// The diag(2, 1) valence-dominant vantage Gram entries (no `math:definiteness` —
/// classification computes positive-definiteness itself).
fn diag21_entries() -> &'static str {
    r#"ex:vantGram a math:GramMatrix ; math:hasEntry ex:vg00 , ex:vg11 .
ex:vg00 a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "0"^^xsd:integer ; math:entryValue ex:ratTwo .
ex:vg11 a math:MatrixEntry ; math:atRow "1"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOne .
ex:ratTwo a math:RationalValue ; math:numerator "2"^^xsd:integer ; math:denominator "1"^^xsd:integer .
ex:ratOne a math:RationalValue ; math:numerator "1"^^xsd:integer ; math:denominator "1"^^xsd:integer .
"#
}

/// A NON-positive-definite vantage Gram `[[1, 2], [2, 1]]` (det = −3 < 0).
fn non_pd_entries() -> &'static str {
    r#"ex:vantGram a math:GramMatrix ; math:hasEntry ex:vg00 , ex:vg01 , ex:vg11 .
ex:vg00 a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "0"^^xsd:integer ; math:entryValue ex:ratOne .
ex:vg01 a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratTwo .
ex:vg11 a math:MatrixEntry ; math:atRow "1"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOne .
ex:ratTwo a math:RationalValue ; math:numerator "2"^^xsd:integer ; math:denominator "1"^^xsd:integer .
ex:ratOne a math:RationalValue ; math:numerator "1"^^xsd:integer ; math:denominator "1"^^xsd:integer .
"#
}

/// Build a classify graph: the core-affect dimensions, a vantage profile
/// `ex:vantMetric` whose Gram is `gram_entries`, and the observation blocks.
fn cls_graph(gram_entries: &str, add_dominance_dim: bool, obs: &[String]) -> Graph {
    let mut turtle = String::new();
    let _ = writeln!(
        turtle,
        "@prefix gmeow: <{GM}> .\n@prefix math: <{MATH}> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n@prefix ex: <{CLS_NS}> ."
    );
    turtle.push_str(
            "gmeow:dimensionValence a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex \"0\"^^xsd:nonNegativeInteger .\n",
        );
    turtle.push_str(
            "gmeow:dimensionArousal a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex \"1\"^^xsd:nonNegativeInteger .\n",
        );
    if add_dominance_dim {
        turtle.push_str(
                "gmeow:dimensionDominance a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex \"2\"^^xsd:nonNegativeInteger .\n",
            );
    }
    turtle.push_str(
            "ex:vantMetric a gmeow:AffectScaleProfile ; gmeow:profileRangeMin \"-1.0\"^^xsd:decimal ; gmeow:profileRangeMax \"1.0\"^^xsd:decimal ; gmeow:metricGram ex:vantGram .\n",
        );
    turtle.push_str(gram_entries);
    for block in obs {
        turtle.push_str(block);
        turtle.push('\n');
    }
    let bytes = turtle_to_gts(&turtle);
    purrdf::gts::reader::read(&bytes, false, None)
}

// The metric-nearest prototype under diag(2, 1) is ELATION, though the raw-L²
// nearest is CONTENTMENT — the exact squared-distance order flips vs bare L². The
// ranked profile carries the exact perpendicular Voronoi margin.
#[test]
fn classify_selects_metric_nearest_with_ranked_margin() {
    let graph = cls_graph(
        diag21_entries(),
        false,
        &[
            cls_vec("State", "0.5", "0.0"),
            cls_vec("Cont", "0.2", "0.5"),
            cls_vec("Elat", "0.6", "0.6"),
        ],
    );
    let protos = vec![cls_iri("Cont"), cls_iri("Elat")];
    let c = classify(
        &graph,
        &cls_iri("State"),
        &protos,
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .expect("classify");

    assert_eq!(c.metric, MetricLens::GDistance);
    assert_eq!(c.vantage_profile, VANT_PROFILE);
    assert_eq!(c.ranked.len(), 2);
    // Elation nearest (19/50 < 43/100), contentment second.
    assert_eq!(c.ranked[0].prototype, cls_iri("Elat"));
    assert_eq!(c.ranked[0].squared_distance, "19/50");
    assert_eq!(
        c.ranked[0].distance,
        sqrt_rational_decimal(r(19, 50)).unwrap()
    );
    assert!(c.ranked[0].cosine.is_none()); // no cosine under the distance lens
    assert_eq!(c.ranked[1].prototype, cls_iri("Cont"));
    assert_eq!(c.ranked[1].squared_distance, "43/100");
    // Exact perpendicular Voronoi margin² = Δ²/(4‖p₁−p₂‖²) = (1/20)²/(4·33/100) = 1/528.
    assert_eq!(c.margin_squared, "1/528");
    assert_eq!(c.margin, sqrt_rational_decimal(r(1, 528)).unwrap());

    // Deterministic: same call twice → identical result.
    let again = classify(
        &graph,
        &cls_iri("State"),
        &protos,
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .unwrap();
    assert_eq!(again, c);
}

// The cosine lens (direction) and the G-distance lens (distance incl. intensity)
// pick DIFFERENT winners — vantage-relativity is a function of the lens.
#[test]
fn classify_cosine_and_distance_pick_different_winners() {
    // state (0.5, 0); A (0.5, 0.3) is distance-nearest; B (0.9, 0) is direction-nearest.
    let graph = cls_graph(
        diag21_entries(),
        false,
        &[
            cls_vec("State", "0.5", "0.0"),
            cls_vec("A", "0.5", "0.3"),
            cls_vec("B", "0.9", "0.0"),
        ],
    );
    let protos = vec![cls_iri("A"), cls_iri("B")];
    let dist = classify(
        &graph,
        &cls_iri("State"),
        &protos,
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .unwrap();
    assert_eq!(dist.ranked[0].prototype, cls_iri("A")); // 9/100 < 8/25

    let cos = classify(
        &graph,
        &cls_iri("State"),
        &protos,
        VANT_PROFILE,
        MetricLens::Cosine,
        None,
    )
    .unwrap();
    assert_eq!(cos.ranked[0].prototype, cls_iri("B")); // better-aligned direction
    assert!(cos.ranked[0].cosine.is_some());
}

// Cosine selection is SIGN-FIRST: any positive cosine beats any negative, even a
// tiny-positive against a strong-negative (squaring magnitude alone would be wrong).
#[test]
fn classify_cosine_sign_first_small_positive_beats_large_negative() {
    // state (1, 0); Pos (0.1, 0) → +cosine; Neg (−0.9, 0) → −cosine.
    let graph = cls_graph(
        diag21_entries(),
        false,
        &[
            cls_vec("State", "1.0", "0.0"),
            cls_vec("Pos", "0.1", "0.0"),
            cls_vec("Neg", "-0.9", "0.0"),
        ],
    );
    let cos = classify(
        &graph,
        &cls_iri("State"),
        &[cls_iri("Pos"), cls_iri("Neg")],
        VANT_PROFILE,
        MetricLens::Cosine,
        None,
    )
    .unwrap();
    assert_eq!(
        cos.ranked[0].prototype,
        cls_iri("Pos"),
        "any positive cosine outranks any negative cosine"
    );
}

// The honest asymmetry: a flat (zero-G-norm) state classifies fine under G-distance
// but hard-fails under cosine (its direction is undefined).
#[test]
fn classify_zero_norm_state_cosine_fails_but_distance_ok() {
    let graph = cls_graph(
        diag21_entries(),
        false,
        &[
            cls_vec("State", "0.0", "0.0"),
            cls_vec("Elat", "0.6", "0.6"),
        ],
    );
    let state = cls_iri("State");
    let protos = vec![cls_iri("Elat")];
    let dist = classify(
        &graph,
        &state,
        &protos,
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .unwrap();
    assert_eq!(dist.ranked[0].prototype, cls_iri("Elat"));

    let err = classify(
        &graph,
        &state,
        &protos,
        VANT_PROFILE,
        MetricLens::Cosine,
        None,
    )
    .expect_err("zero-norm state has undefined cosine");
    assert!(err.message().contains("zero G-norm"), "{err}");
}

// A zero-G-norm PROTOTYPE under the cosine lens is a hard fail (undefined direction).
#[test]
fn classify_zero_norm_prototype_cosine_fails() {
    let graph = cls_graph(
        diag21_entries(),
        false,
        &[
            cls_vec("State", "0.5", "0.5"),
            cls_vec("Zero", "0.0", "0.0"),
            cls_vec("Elat", "0.6", "0.6"),
        ],
    );
    let err = classify(
        &graph,
        &cls_iri("State"),
        &[cls_iri("Zero"), cls_iri("Elat")],
        VANT_PROFILE,
        MetricLens::Cosine,
        None,
    )
    .expect_err("zero-norm prototype has undefined cosine");
    assert!(err.message().contains("zero G-norm"), "{err}");
}

// A non-positive-definite vantage Gram is a hard fail — the builtin trusts PD, so an
// indefinite form would make "distances" negative and the argmin garbage.
#[test]
fn classify_non_pd_vantage_hard_fails() {
    let graph = cls_graph(
        non_pd_entries(),
        false,
        &[
            cls_vec("State", "0.5", "0.0"),
            cls_vec("Elat", "0.6", "0.6"),
        ],
    );
    let err = classify(
        &graph,
        &cls_iri("State"),
        &[cls_iri("Elat")],
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .expect_err("indefinite vantage Gram must hard fail");
    assert!(err.message().contains("positive-definite"), "{err}");
}

// Two coincident prototype signatures (identical under G) are an authoring error.
#[test]
fn classify_coincident_prototypes_hard_fails() {
    let graph = cls_graph(
        diag21_entries(),
        false,
        &[
            cls_vec("State", "0.5", "0.0"),
            cls_vec("A", "0.2", "0.5"),
            cls_vec("B", "0.2", "0.5"), // identical to A
        ],
    );
    let err = classify(
        &graph,
        &cls_iri("State"),
        &[cls_iri("A"), cls_iri("B")],
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .expect_err("coincident prototypes must hard fail");
    assert!(err.message().contains("coincident"), "{err}");
}

// A coordinate axis wider than the vantage form is a hard fail, not a silent
// truncation: the diag(2,1) form is 2-D but a prototype declares a dominance (axis 2)
// cell.
#[test]
fn classify_dimension_mismatch_hard_fails() {
    let proto_with_dominance = r#"ex:vecD a gmeow:AffectVectorObservation ;
    gmeow:vectorComponent ex:valD , ex:aroD , ex:domD .
ex:valD a gmeow:Appraisal ; gmeow:appraisalDimension gmeow:dimensionValence ; gmeow:appraisalValue "0.5"^^xsd:decimal .
ex:aroD a gmeow:Appraisal ; gmeow:appraisalDimension gmeow:dimensionArousal ; gmeow:appraisalValue "0.5"^^xsd:decimal .
ex:domD a gmeow:Appraisal ; gmeow:appraisalDimension gmeow:dimensionDominance ; gmeow:appraisalValue "0.5"^^xsd:decimal .
"#
        .to_owned();
    let graph = cls_graph(
        diag21_entries(),
        true,
        &[cls_vec("State", "0.5", "0.0"), proto_with_dominance],
    );
    let err = classify(
        &graph,
        &cls_iri("State"),
        &[cls_iri("D")],
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .expect_err("axis wider than the form must hard fail");
    assert!(
        err.message().contains("exceeds the vantage form order"),
        "{err}"
    );
}

// An empty prototype set is a hard fail — there is nothing to rank over.
#[test]
fn classify_empty_set_hard_fails() {
    let graph = cls_graph(diag21_entries(), false, &[cls_vec("State", "0.5", "0.0")]);
    let err = classify(
        &graph,
        &cls_iri("State"),
        &[],
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .expect_err("empty set must hard fail");
    assert!(err.message().contains("at least one prototype"), "{err}");
}

// An out-of-range coordinate magnitude (1.5 outside [−1, 1]) is a hard fail.
#[test]
fn classify_out_of_range_value_hard_fails() {
    let graph = cls_graph(
        diag21_entries(),
        false,
        &[cls_vec("State", "0.5", "0.0"), cls_vec("Big", "1.5", "0.0")],
    );
    let err = classify(
        &graph,
        &cls_iri("State"),
        &[cls_iri("Big")],
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .expect_err("out-of-range magnitude must hard fail");
    assert!(
        err.message().contains("outside the vantage profile range"),
        "{err}"
    );
}

// `top_k` truncates the reported ranking, but the margin still uses the TRUE top-two.
#[test]
fn classify_top_k_truncates_but_margin_uses_true_top_two() {
    let graph = cls_graph(
        diag21_entries(),
        false,
        &[
            cls_vec("State", "0.5", "0.0"),
            cls_vec("Cont", "0.2", "0.5"),
            cls_vec("Elat", "0.6", "0.6"),
        ],
    );
    let protos = vec![cls_iri("Cont"), cls_iri("Elat")];
    let one = classify(
        &graph,
        &cls_iri("State"),
        &protos,
        VANT_PROFILE,
        MetricLens::GDistance,
        Some(1),
    )
    .unwrap();
    assert_eq!(one.ranked.len(), 1);
    assert_eq!(one.ranked[0].prototype, cls_iri("Elat"));
    assert_eq!(one.margin_squared, "1/528"); // margin over the true top-two

    // top_k > count clamps (not an error).
    let all = classify(
        &graph,
        &cls_iri("State"),
        &protos,
        VANT_PROFILE,
        MetricLens::GDistance,
        Some(99),
    )
    .unwrap();
    assert_eq!(all.ranked.len(), 2);
}

// Negative coordinate magnitudes round-trip through exact ℚ and classify correctly.
#[test]
fn classify_negative_valued_prototype_round_trips() {
    let graph = cls_graph(
        diag21_entries(),
        false,
        &[
            cls_vec("State", "-0.5", "0.0"),
            cls_vec("Neg", "-0.6", "0.2"),
            cls_vec("Pos", "0.6", "0.2"),
        ],
    );
    let c = classify(
        &graph,
        &cls_iri("State"),
        &[cls_iri("Neg"), cls_iri("Pos")],
        VANT_PROFILE,
        MetricLens::GDistance,
        None,
    )
    .unwrap();
    // state (−0.5, 0) → Neg (−0.6, 0.2): 2·(0.1)² + (0.2)² = 3/50; the nearer one.
    assert_eq!(c.ranked[0].prototype, cls_iri("Neg"));
    assert_eq!(c.ranked[0].squared_distance, "3/50");
}

// Enumeration returns every `gmeow:AffectPrototype` individual, ascending, and only
// those (not plain vector observations).
#[test]
fn affect_prototypes_enumerates_sorted() {
    let mut turtle = String::new();
    let _ = writeln!(
        turtle,
        "@prefix gmeow: <{GM}> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n@prefix ex: <{CLS_NS}> ."
    );
    turtle.push_str("ex:zProto a gmeow:AffectPrototype .\n");
    turtle.push_str("ex:aProto a gmeow:AffectPrototype .\n");
    turtle.push_str("ex:notProto a gmeow:AffectVectorObservation .\n");
    let bytes = turtle_to_gts(&turtle);
    let graph = purrdf::gts::reader::read(&bytes, false, None);
    assert_eq!(
        affect_prototypes(&graph),
        vec![format!("{CLS_NS}aProto"), format!("{CLS_NS}zProto")]
    );
}
