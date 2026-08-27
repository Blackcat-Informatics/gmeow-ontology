// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_sensory_environment.py (the two
//! TBox-membership functions plus the SOSA / psychological SSSOM mapping scans).
//!
//! Both twins run over the merged ontology (`GraphStore::ontology()`): the
//! sensory-environment axes and perceptual frame realm are defined as subjects in
//! slices/core/places/module.ttl (cross-slice), so a scopeModule cell over the
//! sensory-environment module would silently miss them.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// The ten sensory-environment axes are declared as gmeow:Axis individuals.
#[gmeow_test_batch_macros::batch_test]
fn new_axes_exist() {
    let g = GraphStore::ontology();
    for axis in [
        "axisTristimulusX",
        "axisTristimulusY",
        "axisTristimulusZ",
        "axisLightness",
        "axisAstar",
        "axisBstar",
        "axisFrequency",
        "axisMagnitude",
        "axisPredictedMeanVote",
        "axisPredictedPercentageDissatisfied",
    ] {
        assert!(
            g.has(Some(&gm(axis)), Some(RDF_TYPE), Some(&gm("Axis"))),
            "gmeow:{axis} must be a gmeow:Axis"
        );
    }
}

/// The perceptual frame realm individual exists for mental reference frames.
#[gmeow_test_batch_macros::batch_test]
fn perceptual_frame_realm_exists() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(&gm("frameRealmPerceptual")),
            Some(RDF_TYPE),
            Some(&gm("FrameRealm"))
        ),
        "gmeow:frameRealmPerceptual must be a gmeow:FrameRealm"
    );
}

// ── SSSOM-scan twins migrated from tests/test_sensory_environment.py ──────────

/// True iff `generated/mappings/gmeow-sensory-environment.sssom.tsv` has a row with
/// the given `(subject_id, predicate_id, object_id)`, skipping `#`-prefixed metadata
/// lines and the TSV header. Mirrors the `load_mappings()` filter narrowed to the
/// sensory-environment source file.
fn sensory_sssom_row(subject_id: &str, predicate_id: &str, object_id: &str) -> bool {
    let text = generated_mapping("gmeow-sensory-environment.sssom.tsv");
    text.lines().any(|line| {
        if line.starts_with('#') || line.starts_with("subject_id") {
            return false;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        cols.len() >= 3 && cols[0] == subject_id && cols[1] == predicate_id && cols[2] == object_id
    })
}

/// Twin of `test_sosa_alignments_loaded`: the sensory-environment mapping set
/// contains the SOSA closeMatch alignments.
#[gmeow_test_batch_macros::batch_test]
fn sosa_alignments_loaded() {
    assert!(
        sensory_sssom_row(
            "gmeow:SensoryEnvironment",
            "skos:closeMatch",
            "sosa:FeatureOfInterest"
        ),
        "SensoryEnvironment must map to sosa:FeatureOfInterest"
    );
    assert!(
        sensory_sssom_row("gmeow:CoordinateMatrix", "skos:closeMatch", "sosa:Result"),
        "CoordinateMatrix must map to sosa:Result"
    );
}

/// Twin of `test_psychological_mappings_loaded`: the sensory-environment
/// mapping set contains the MF and MFOEM relatedMatch alignments.
#[gmeow_test_batch_macros::batch_test]
fn psychological_mappings_loaded() {
    assert!(
        sensory_sssom_row(
            "gmeow:MentalReferenceFrame",
            "skos:relatedMatch",
            "bfo:MF_0000020"
        ),
        "MentalReferenceFrame must map to bfo:MF_0000020 (mental process)"
    );
    assert!(
        sensory_sssom_row(
            "gmeow:referenceFrameAffectiveCircumplex",
            "skos:relatedMatch",
            "bfo:MFOEM_000195"
        ),
        "referenceFrameAffectiveCircumplex must map to bfo:MFOEM_000195 (affective process)"
    );
}
