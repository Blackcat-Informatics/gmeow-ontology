// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW's GTS vocabulary inventory: profile and codec floors, and the three
//! declared codec classes. These individuals are owned by the slice's module,
//! so their inventory is checked on the authenticated ontology without imports.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Required population floors for the open value vocabularies (Principle 9).
#[gmeow_test_batch_macros::batch_test]
fn value_vocabulary_cardinality_floors() {
    let g = GraphStore::ontology();

    let profiles = g.subjects_of_type(&gm("GTSProfile"));
    assert!(
        profiles.len() >= 7,
        "expected >= 7 GTSProfile individuals, got {}: {profiles:?}",
        profiles.len()
    );

    let codecs = g.subjects_of_type(&gm("TransformCodec"));
    assert!(
        codecs.len() >= 7,
        "expected >= 7 TransformCodec individuals, got {}: {codecs:?}",
        codecs.len()
    );

    let classes = g.subjects_of_type(&gm("CodecClass"));
    assert_eq!(
        classes.len(),
        3,
        "expected exactly 3 CodecClass individuals, got {}: {classes:?}",
        classes.len()
    );
}
