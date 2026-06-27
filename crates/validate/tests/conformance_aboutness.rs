// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_aboutness.py (#867)
//!
//! Each test loads a fixture file from `tests/fixtures/shapes/` and validates
//! it against the whole shapes corpus using the native SHACL engine.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::wellformed_aboutness_fixture_conforms(Case::file("shapes", "aboutness-wellformed"))]
#[case::malformed_aboutness_fixture_is_flagged(
    Case::file("shapes", "aboutness-malformed")
        .fails()
        .violations(&["not a free literal"])
)]
fn aboutness(#[case] case: Case) {
    case.run();
}
