// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance tests for rubrics slice SHACL shapes.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

#[batch_cases]
#[case::wellformed_rubrics_fixture_conforms(Case::file("shapes", "rubrics-wellformed"))]
#[case::malformed_rubrics_fixture_is_flagged(
    Case::file("shapes", "rubrics-malformed")
        .fails()
        .violations(&[
            "gmeow:penaltyPole and gmeow:rewardPole must be distinct",
            "minimum must be strictly below its maximum",
            "at least one gmeow:anchorMeaning",
            "range minimum must not exceed",
            "must name exactly one gmeow:rewardPole",
            "binds at most one gmeow:usesScale",
            "must pin exactly one decimal gmeow:anchorRangeMin",
            "must lie within the scale",
            "may not redirect to the criterion that anchors it",
            "at least one of gmeow:viaSelector",
            "exactly one gmeow:exemplarPolarity",
            "a gmeow:assessmentCriterion, a gmeow:assessmentRubric, or both",
        ])
)]
fn rubrics(#[case] case: Case) {
    case.run();
}

// ── GraphStore twins migrated from tests/test_rubrics.py ──────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_SHAPES: &str = "https://example.org/shapes/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_no_preferred_assessment_machinery` (Principle 9): no gmeow: term
/// whose local name (containing no `/`) case-insensitively starts with any
/// preferred/canonical/primary assessment selector. Two judges disagreeing are two
/// coexisting cells, never a ranked winner.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_assessment_machinery() {
    let g = GraphStore::ontology();
    let banned = [
        "preferredscore",
        "canonicalassessment",
        "primaryassessment",
        "preferredassessment",
    ];
    let (_vars, rows) = g.select(&[], "SELECT DISTINCT ?s WHERE { ?s ?p ?o }");
    let mut offenders = Vec::new();
    for row in &rows {
        let Some(Some(term)) = row.first() else {
            continue;
        };
        let Some(iri) = term.as_iri() else {
            continue;
        };
        if let Some(local) = iri.strip_prefix(GMEOW) {
            let lower = local.to_lowercase();
            if !local.contains('/') && banned.iter().any(|b| lower.starts_with(b)) {
                offenders.push(iri.to_owned());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "preferred assessment machinery leaked: {offenders:?}"
    );
}

/// Twin of `test_two_judges_disagree_without_contradiction`: the LLM-judge doctrine
/// in fixture form — one chunk, two vantages, two scores; both cells stand
/// (judgeA = 0.9, judgeB = 0.4).
#[gmeow_test_batch_macros::batch_test]
fn two_judges_disagree_without_contradiction() {
    let g = GraphStore::parse_ttl_file(
        &repo_root().join("tests/fixtures/shapes/rubrics-wellformed.ttl"),
    );
    let judge_a = format!("{EX_SHAPES}judgeA");
    let judge_b = format!("{EX_SHAPES}judgeB");
    let mut score_a: Option<f64> = None;
    let mut score_b: Option<f64> = None;
    for assessment in g.subjects_of_type(&gm("Assessment")) {
        let vantages = g.objects(&assessment, &gm("vantage"));
        let scores = g.objects_lex(&assessment, &gm("assessmentScoreValue"));
        let score: f64 = scores
            .iter()
            .next()
            .expect("assessment must carry a score value")
            .parse()
            .expect("score value must be a number");
        if vantages.contains(&judge_a) {
            score_a = Some(score);
        } else if vantages.contains(&judge_b) {
            score_b = Some(score);
        }
    }
    assert_eq!(score_a, Some(0.9), "judgeA score must be 0.9");
    assert_eq!(score_b, Some(0.4), "judgeB score must be 0.4");
}
