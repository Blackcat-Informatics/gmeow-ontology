// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_aboutness.py
//!
//! Each test loads a fixture file from `tests/fixtures/shapes/` and validates
//! it against the whole shapes corpus using the native SHACL engine.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

#[batch_cases]
#[case::wellformed_aboutness_fixture_conforms(Case::file("shapes", "aboutness-wellformed"))]
#[case::malformed_aboutness_fixture_is_flagged(
    Case::file("shapes", "aboutness-malformed")
        .fails()
        .violations(&["not a free literal"])
)]
fn aboutness(#[case] case: Case) {
    case.run();
}

// ── GraphStore twins migrated from tests/test_aboutness.py ────────────────────

use std::collections::BTreeSet;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_aboutness_orthogonal_to_other_axes` (Principle 9): hasAboutness ⟂
/// every other kernel axis — no subPropertyOf / equivalentProperty bridge among the
/// six axes, in either direction. Checked over the merged graph because the axes are
/// declared across many slice modules.
#[gmeow_test_batch_macros::batch_test]
fn orthogonal_to_other_axes() {
    let g = GraphStore::ontology();
    let axes = [
        gm("hasAboutness"),
        gm("hasGranularity"),
        gm("hasDeterminacy"),
        gm("hasSensitivity"),
        gm("hasDisclosurePolicy"),
        gm("confidence"),
    ];
    for i in 0..axes.len() {
        for j in (i + 1)..axes.len() {
            let a = &axes[i];
            let b = &axes[j];
            assert!(!g.has(Some(a), Some(RDFS_SUB_PROPERTY_OF), Some(b)));
            assert!(!g.has(Some(b), Some(RDFS_SUB_PROPERTY_OF), Some(a)));
            assert!(!g.has(Some(a), Some(OWL_EQUIVALENT_PROPERTY), Some(b)));
            assert!(!g.has(Some(b), Some(OWL_EQUIVALENT_PROPERTY), Some(a)));
        }
    }
}

/// Twin of `test_no_aboutness_truth_bridge`: aboutnessDescribes / aboutnessEnacts
/// are plain vocabulary individuals — each has exactly the single class membership
/// gmeow:AboutnessMode (no veridicality / standpoint-modality bridge).
#[gmeow_test_batch_macros::batch_test]
fn no_aboutness_truth_bridge() {
    let g = GraphStore::ontology();
    let expected: BTreeSet<String> = [gm("AboutnessMode")].into_iter().collect();
    for seed in ["aboutnessDescribes", "aboutnessEnacts"] {
        let types = g.objects(&gm(seed), RDF_TYPE);
        assert_eq!(
            types, expected,
            "gmeow:{seed} must have exactly one class membership: gmeow:AboutnessMode"
        );
    }
}
