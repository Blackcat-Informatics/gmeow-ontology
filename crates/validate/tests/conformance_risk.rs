// SPDX-License-Identifier: AGPL-3.0-only
// Conformance twins migrated from tests/test_risk.py (#867)

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

// ── Tests migrated from tests/test_risk.py ───────────────────────────────────

#[rstest]
#[case::wellformed_risk_fixture_conforms(Case::file("shapes", "risk-wellformed"))]
#[case::malformed_risk_fixture_is_flagged(
    Case::file("shapes", "risk-malformed")
        .fails()
        .violations(&[
            "exactly one gmeow:hazardBearer",
            "at least one feared gmeow:manifestedAsType",
            "antecedent and consequent must be distinct",
            "exactly one gmeow:causalModality",
            "no causal link may reach itself",
            "an ungraded cascade is just a story",
            "at least one gmeow:mitigationMeasure",
            "CausalLink (barrier on the chain) or a Hazard",
        ])
)]
fn risk(#[case] case: Case) {
    case.run();
}
