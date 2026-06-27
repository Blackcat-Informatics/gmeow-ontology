// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_narration.py (#867)
//!
//! Migrated tests (SHACL fixture-based):
//!   - `test_wellformed_narration_fixture_conforms`  → `wellformed_narration_fixture_conforms`
//!   - `test_malformed_narration_fixture_is_flagged` → `malformed_narration_fixture_is_flagged`
//!
//! Retained in Python (not migrated):
//!   - `test_seam_links_specialize_one_ancestor`: pure `_graph()` TBox membership checks.
//!   - `test_orientations_are_not_inverse_axioms`: pure `_graph()` TBox membership checks.
//!   - `test_narration_usage_is_a_reified_relator_with_open_subject`: `_graph()` TBox checks.
//!   - `test_narration_mode_vocab_seeds`: `_graph()` subject iteration.
//!   - `test_no_truth_bridge_from_unreliable_mode`: `_graph()` object iteration.
//!   - `test_fixture_obeys_the_efficiency_budget`: iterates fixture quads, no `run_shacl`.
//!   - `test_competency_cooccurrence_query_over_fixture`: SPARQL SELECT over fixture.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::wellformed_narration_fixture_conforms(Case::file("shapes", "narration-wellformed"))]
#[case::malformed_narration_fixture_is_flagged(
    Case::file("shapes", "narration-malformed")
        .fails()
        .violations(&[
            "at least one gmeow:narrationMode",
            "exactly one gmeow:narrationSubject",
            "exactly one gmeow:narrationSegment",
        ])
)]
fn narration(#[case] case: Case) {
    case.run();
}
