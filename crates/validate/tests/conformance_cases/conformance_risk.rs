// SPDX-License-Identifier: AGPL-3.0-only
// Conformance twins migrated from tests/test_risk.py

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

// ── Tests migrated from tests/test_risk.py ───────────────────────────────────

#[batch_cases]
#[case::wellformed_risk_fixture_conforms(Case::file("shapes", "risk-wellformed"))]
#[case::malformed_risk_fixture_is_flagged(
    Case::file("shapes", "risk-malformed")
        .fails()
        .violations(&[
            "exactly one gmeow:hazardBearer",
            "at least one feared gmeow:manifestedAsType",
            "gmeow:linkAntecedent and gmeow:linkConsequent must be distinct",
            "exactly one gmeow:causalModality",
            "reach itself through gmeow:linkNext",
            "an ungraded cascade is just a story",
            "at least one gmeow:mitigationMeasure",
            "CausalLink (barrier on the chain) or a Hazard",
        ])
)]
fn risk(#[case] case: Case) {
    case.run();
}

// ── SPARQL / GraphStore twins migrated from tests/test_risk.py ─────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_no_occurrence_gate`: loading the risk shapes fixture together
/// with the trust-collapse worked example entails ZERO `gmeow:Event` instances —
/// a cascade is expressible without anything having HAPPENED — while the feared
/// kinds ARE present as `gmeow:EventType`s (>= 3). The Python multi-file ABox
/// parse (`Graph.parse` twice into one graph) becomes a single store over the
/// concatenated Turtle of both files, then `subjects_of_type` membership.
#[gmeow_test_batch_macros::batch_test]
fn no_occurrence_gate() {
    let root = repo_root();
    let wellformed = read_ttl(&root.join("tests/fixtures/shapes/risk-wellformed.ttl"));
    let trust_collapse = read_ttl(&root.join("slices/extensions/risk/examples/trust-collapse.ttl"));
    // Turtle permits redeclaring the same @prefix, so concatenating both files
    // into one document is a faithful native twin of two `Graph.parse` calls into
    // a shared rdflib graph.
    let combined = format!("{wellformed}\n{trust_collapse}");
    let g = GraphStore::parse_ttl(&combined);

    assert!(
        g.subjects_of_type(&gm("Event")).is_empty(),
        "no gmeow:Event instance may be entailed — a cascade needs nothing to have happened"
    );
    assert!(
        g.subjects_of_type(&gm("EventType")).len() >= 3,
        "the feared kinds must be present as gmeow:EventType (>= 3)"
    );
}

/// Twin of `test_competency_severity_order_query`: `risk-severity-order.rq`
/// (a `moreSevereThan+` transitive property path + `FILTER NOT EXISTS` for the
/// maximal element) over the merged ontology returns exactly the single top of
/// the severity chain, `gmeow:severityCatastrophic`. The Python `set == {…}`
/// becomes an order-insensitive `select_row_set` of the one expected row.
#[gmeow_test_batch_macros::batch_test]
fn competency_severity_order_query() {
    QueryCase::new("risk/severity-order", &[Feature::FilterNotExists])
        .over_ontology()
        .query_file("risk-severity-order.rq")
        .select_row_set(vec![vec![iri(&gm("severityCatastrophic"))]])
        .run();
}
