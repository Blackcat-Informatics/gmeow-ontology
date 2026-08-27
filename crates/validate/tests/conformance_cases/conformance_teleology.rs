// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_teleology.py
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

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

#[batch_cases]
#[case::wellformed_teleology_fixture_conforms(Case::file("shapes", "teleology-wellformed"))]
#[case::malformed_teleology_fixture_is_flagged(
    Case::file("shapes", "teleology-malformed")
        .fails()
        .violations(&[
            "exactly one gmeow:intentBearer",
            "gmeow:commitmentBeneficiary and gmeow:committedAgent must be distinct",
            "its own gmeow:counterGoal",
            "exactly one gmeow:tenureAgent",
        ])
)]
fn teleology(#[case] case: Case) {
    case.run();
}

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// Principle 9: no preferredGoal / primaryIntention-style selector exists — a
/// dynamic whole-graph subject sweep over the merged ontology for any gmeow term
/// whose local name begins with a banned goal/intention prefix.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_goal_terms() {
    let g = GraphStore::ontology();
    let (_, rows) = g.select(&[], "SELECT DISTINCT ?s WHERE { ?s ?p ?o }");
    let banned = [
        "primarygoal",
        "preferredgoal",
        "primaryintention",
        "preferredintention",
    ];
    let mut offenders: Vec<String> = Vec::new();
    for row in rows {
        let Some(Some(purrdf::TermValue::Iri(iri))) = row.into_iter().next() else {
            continue;
        };
        let Some(local) = iri.strip_prefix(GMEOW) else {
            continue;
        };
        if local.contains('/') {
            continue;
        }
        let lower = local.to_lowercase();
        if banned.iter().any(|b| lower.starts_with(b)) {
            offenders.push(iri);
        }
    }
    assert!(
        offenders.is_empty(),
        "preferred/primary goal selectors must not exist: {offenders:?}"
    );
}

/// The Commitment at-least-one-beneficiary obligation of the retired
/// hand-authored `gmeow:CommitmentShape` now rides the projected declarative
/// surface (`generated/shapes/validation-shapes.ttl`, `Commitment-shape`
/// `sh:minCount 1` on `gmeow:commitmentBeneficiary`), which the fixture corpus
/// deliberately excludes — witness it by path on the LIVE production shape
/// union (projected shapes carry no `sh:message`).
#[gmeow_test_batch_macros::batch_test]
fn commitment_without_beneficiary_fails_on_union() {
    Case::inline(
        "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
ex:c a gmeow:Commitment ;
    gmeow:committedAgent ex:agent ;
    gmeow:intentionGoal ex:goal .
"
        .to_owned(),
    )
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/commitmentBeneficiary",
        "MinCountConstraintComponent",
    )
    .run();
}
