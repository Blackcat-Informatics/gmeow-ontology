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
    score_slice_with_standard(
        dir,
        &standard,
        ScoringEnv::Repo {
            slice_dir: dir.to_path_buf(),
        },
    )
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
