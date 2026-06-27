// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_teleology.py (#867)
//!
//! Each test loads a fixture file from `tests/fixtures/shapes/` and validates
//! it against the whole shapes corpus using `validate()`.
//!
//! Retained in Python (not migrated):
//!   - `test_intrinsic_modes_are_grounded`: `(triple) in g` membership test
//!     on `_graph()` (cross-slice subject — gmeow:MentalMoment defined in
//!     the mentation slice, not the teleology module).
//!   - `test_no_preferred_or_primary_goal_terms`: dynamic whole-graph sweep
//!     over `g.subjects()`; scoping to the teleology module would narrow the
//!     live-set intent.
//!   - `test_competency_teleology_modes_query`: reads an external `.rq` file
//!     and asserts SPARQL SELECT result sets — not portable to SHACL engine.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::wellformed_teleology_fixture_conforms(Case::file("shapes", "teleology-wellformed"))]
#[case::malformed_teleology_fixture_is_flagged(
    Case::file("shapes", "teleology-malformed")
        .fails()
        .violations(&[
            "exactly one gmeow:intentBearer",
            "distinct from its committed agent",
            "never its own counter-goal",
            "exactly one gmeow:tenureAgent",
        ])
)]
fn teleology(#[case] case: Case) {
    case.run();
}
