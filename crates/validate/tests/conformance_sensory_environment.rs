// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_sensory_environment.py (the two
//! TBox-membership functions; the SOSA / psychological mapping-load functions
//! remain in Python for a later batch).
//!
//! Both twins run over the merged ontology (`GraphStore::ontology()`): the
//! sensory-environment axes and perceptual frame realm are defined as subjects in
//! slices/core/places/module.ttl (cross-slice), so a scopeModule cell over the
//! sensory-environment module would silently miss them.

mod conformance_support;
use conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// The ten sensory-environment axes are declared as gmeow:Axis individuals.
#[test]
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
#[test]
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
