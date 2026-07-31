// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The RDF projection of a slice assessment: each per-axis grade a
//! `gmeow:QualityAssessment` observation, the roll-up tier one more, all in the
//! `gmeow:graph/slice-quality` named graph, deterministic to the byte.

use std::path::{Path, PathBuf};

use gmeow_slice_quality::ScoringEnv;
use gmeow_slice_quality::report::{SliceReport, score_slice_with_standard};

const GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/slice-quality";
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const MATH: &str = "https://blackcatinformatics.ca/math/";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Score a slice against the repo rubric's measurement standard, in repo mode — the
/// in-repo replacement for the retired `score_slice(root, dir)`.
fn score(root: &Path, dir: &Path) -> gmeow_errors::Result<SliceReport> {
    let module = root.join("slices/core/slice-quality-rubric/module.ttl");
    let ds = gmeow_slice_quality::dataset_from_paths(&[&module])?;
    let standard = gmeow_slice_quality::rubric::load_rubric(&ds)?.standard;
    score_slice_with_standard(dir, &standard, ScoringEnv::Repo)
}

#[test]
fn rdf_projection_is_well_formed_and_deterministic() {
    let root = repo_root();
    let dir = root.join("slices/core/slice-quality-rubric");
    let report = score(&root, &dir).expect("the rubric slice scores");
    let slice_iri = report.assessment.slice.clone();

    let a = report.to_gmeow_rdf();
    let b = report.to_gmeow_rdf();
    assert_eq!(
        a, b,
        "the RDF projection must be byte-identical across runs"
    );
    assert!(
        a.ends_with('\n'),
        "trailing newline like the sibling emitters"
    );

    // Every line is a 4-term N-Quad in the slice-quality named graph.
    for line in a.lines() {
        assert!(
            line.ends_with(&format!("<{GRAPH}> .")),
            "line not in the slice-quality graph: {line}"
        );
    }

    // Each per-axis grade is a QualityAssessment observation on the slice.
    assert!(
        a.contains(&format!("<{GMEOW}QualityAssessment>")),
        "the projection types QualityAssessment individuals"
    );
    assert!(
        a.contains(&format!("<{GMEOW}assessedEntity> <{slice_iri}>")),
        "every assessment names the slice IRI as its assessedEntity"
    );
    // The grounding axis's emitted dimension appears verbatim (real ontology IRI).
    assert!(
        a.contains(&format!(
            "<{GMEOW}qualityDimension> <{GMEOW}qualityDimensionGrounding>"
        )),
        "grades are emitted under their axis's real gmeow:QualityDimension"
    );
    // The score is a dimensionless math:Quantity, with no pseudo-unit witness.
    assert!(a.contains(&format!("<{MATH}Quantity>")));
    assert!(a.contains(&format!("<{MATH}quantityValue>")));
    assert!(a.contains(&format!("<{MATH}hasDimension> <{MATH}dimensionless>")));
    assert!(
        !a.contains(&format!("<{GMEOW}unit>")),
        "a normalized 0..1 score carries math:dimensionless, not a pseudo-unit"
    );
    // The roll-up assessment carries the meet tier as its result.
    let rollup_iri = &report.assessment.rollup.iri;
    assert!(
        a.contains(&format!("<{GMEOW}observationResult> <{rollup_iri}>")),
        "the roll-up tier is emitted as a top-level assessment result"
    );
    // Assertional-tier provenance is stamped on every generated subject.
    assert!(a.contains(&format!("<{GMEOW}graphBoxRole> <{GMEOW}boxABox>")));
}

/// **The round-trip proof.** Score fresh, project to RDF, read the projection back,
/// and require the recovered grade vector to be EXACTLY the fresh one — every axis,
/// every score bit-for-bit, every tier.
///
/// This is the precondition for any consumer replacing a live scoring sweep with a
/// read of the recorded corpus. If any field were lossy the substitution would change
/// what a gate sees: the per-axis floor check compares at `f64::EPSILON` tolerance, so
/// even the sixth-decimal rounding the human-facing `fmt_score` applies would be
/// enough to lift a score that sits just below a committed floor up through it and
/// flip a FAIL into a PASS. `assert_eq!` on the raw `f64` — never an epsilon compare —
/// is the point of this test.
#[test]
fn recorded_grades_round_trip_exactly() {
    let root = repo_root();
    let dir = root.join("slices/core/slice-quality-rubric");
    let module = root.join("slices/core/slice-quality-rubric/module.ttl");
    let ds = gmeow_slice_quality::dataset_from_paths(&[&module]).expect("rubric parses");
    let standard = gmeow_slice_quality::rubric::load_rubric(&ds)
        .expect("rubric loads")
        .standard;
    let fresh = score_slice_with_standard(&dir, &standard, ScoringEnv::Repo)
        .expect("the rubric slice scores")
        .assessment;

    // The corpus as the pipeline projects it: the freshness witness plus this slice's
    // block. The fingerprint value is irrelevant here (freshness is proven separately);
    // what matters is that the reader consumes the identical bytes the emitter writes.
    let projected = format!(
        "{}{}",
        gmeow_slice_quality::report::corpus_fingerprint_nquads("blake3:0"),
        {
            let report = score_slice_with_standard(&dir, &standard, ScoringEnv::Repo)
                .expect("the rubric slice scores");
            report.to_gmeow_rdf()
        }
    );

    let corpus =
        gmeow_slice_quality::read::read_recorded_corpus_bytes(projected.as_bytes(), &standard)
            .expect("the projection reads back");
    let recovered = corpus
        .assessment(&fresh.slice)
        .expect("the scored slice is in the record");

    assert_eq!(
        recovered.slice, fresh.slice,
        "the assessed slice IRI must round-trip"
    );
    assert_eq!(
        recovered.grades.len(),
        fresh.grades.len(),
        "every graded axis must round-trip — a dropped axis silently un-floors it"
    );
    assert!(
        !fresh.grades.is_empty(),
        "the rubric declares axes; a vacuous round-trip proves nothing"
    );
    for (got, want) in recovered.grades.iter().zip(&fresh.grades) {
        assert_eq!(got.axis_iri, want.axis_iri, "axis identity must round-trip");
        // Bit-for-bit: NOT an epsilon compare. A rounded score can cross a floor.
        assert_eq!(
            got.score.to_bits(),
            want.score.to_bits(),
            "axis {} score must round-trip bit-for-bit: recorded {} vs fresh {}",
            want.axis_iri,
            got.score,
            want.score
        );
        assert_eq!(
            got.tier, want.tier,
            "axis {} tier must round-trip",
            want.axis_iri
        );
    }
    assert_eq!(
        recovered.rollup, fresh.rollup,
        "the roll-up meet tier must round-trip"
    );
    assert_eq!(
        corpus.fingerprint, "blake3:0",
        "the witness must round-trip"
    );
}

/// The projection carries the axis as FIRST-CLASS data, and the reader keys off that
/// rather than off the minted subject IRI or the lossy dimension.
#[test]
fn every_per_axis_grade_names_its_axis() {
    let root = repo_root();
    let dir = root.join("slices/core/slice-quality-rubric");
    let module = root.join("slices/core/slice-quality-rubric/module.ttl");
    let ds = gmeow_slice_quality::dataset_from_paths(&[&module]).expect("rubric parses");
    let standard = gmeow_slice_quality::rubric::load_rubric(&ds)
        .expect("rubric loads")
        .standard;
    let report =
        score_slice_with_standard(&dir, &standard, ScoringEnv::Repo).expect("the slice scores");
    let projected = report.to_gmeow_rdf();

    for grade in &report.assessment.grades {
        assert!(
            projected.contains(&format!("<{GMEOW}assessmentAxis> <{}>", grade.axis_iri)),
            "axis {} must be recorded as a first-class gmeow:assessmentAxis",
            grade.axis_iri
        );
    }
    // The axis→dimension map is many-to-one, so the dimension cannot substitute: the
    // rubric grades strictly more axes than there are distinct emitted dimensions.
    let dimensions: std::collections::BTreeSet<&str> = standard
        .axes
        .iter()
        .map(|a| a.dimension_iri.as_str())
        .collect();
    assert!(
        dimensions.len() < standard.axes.len(),
        "the axis→dimension map must remain many-to-one for this argument to hold: \
         {} axes onto {} dimensions",
        standard.axes.len(),
        dimensions.len()
    );
    // The roll-up spans every axis and therefore names none.
    let rollup_subject = projected
        .lines()
        .find(|l| l.contains("/rollup> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>"))
        .expect("the roll-up assessment is projected");
    let rollup_iri = rollup_subject
        .split_whitespace()
        .next()
        .expect("subject term");
    assert!(
        !projected.contains(&format!("{rollup_iri} <{GMEOW}assessmentAxis>")),
        "the roll-up is the meet across every axis and must name no single axis"
    );
}
