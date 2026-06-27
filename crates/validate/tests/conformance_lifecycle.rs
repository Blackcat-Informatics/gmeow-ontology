// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_lifecycle.py (#867)

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

// ── Tests migrated from tests/test_lifecycle.py ───────────────────────────────

#[rstest]
#[case::wellformed_entity_existence_conforms(Case::file("shapes", "entity-existence-wellformed"))]
#[case::malformed_entity_existence_is_flagged(
    Case::file("shapes", "entity-existence-malformed")
        .fails()
        .violations(&["existenceEntity", "duringInterval"])
)]
fn lifecycle(#[case] case: Case) {
    case.run();
}
