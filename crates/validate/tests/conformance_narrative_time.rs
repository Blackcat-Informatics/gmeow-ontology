// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_narrative_time.py (#867)
//!
//! Two SHACL-conformance tests from the narrative-time test module are ported
//! here. The remaining tests (TBox membership, dynamic sweep, SPARQL competency
//! query, fixture graph-walk) are retained in Python because they either call
//! `load_merged_graph`, iterate subjects dynamically, or run SPARQL queries.
//!
//! Retained in Python (not migrated):
//!   - `test_narrative_time_frame_is_a_reference_frame`: TBox `(triple) in g` membership.
//!   - `test_axis_vocab_spans_exactly_fabula_and_syuzhet`: dynamic sweep of `g.subjects(...)`.
//!   - `test_frame_properties_are_functional_with_correct_anchors`: TBox membership loop.
//!   - `test_position_is_an_object_with_frame_ordinal_label`: TBox membership checks.
//!   - `test_at_narrative_position_is_domain_free_and_not_functional`: TBox membership.
//!   - `test_flashback_fixture_carries_coexisting_orders`: fixture graph-walk (`g.objects()`).
//!   - `test_competency_narrative_time_axes_query`: SPARQL competency query.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::wellformed_narrative_time_fixture_conforms(Case::file(
    "shapes",
    "narrative-time-wellformed"
))]
#[case::malformed_narrative_time_fixture_is_flagged(
    Case::file("shapes", "narrative-time-malformed")
        .fails()
        .violations(&[
            "exactly one gmeow:narrativeTimeAxis",
            "never the other anchor",
            "exactly one reference frame (gmeow:positionFrame)",
        ])
)]
fn narrative_time(#[case] case: Case) {
    case.run();
}
