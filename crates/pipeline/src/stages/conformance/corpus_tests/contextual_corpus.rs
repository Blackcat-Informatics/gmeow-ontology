// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The canonical modal examples' shipped verdict, attribution and proof contract.

use gmeow_logic::result::{CompletenessStatus, EvaluationStatus, InformationState};

use super::super::contextual_results::CONTEXTUAL;
use super::contextual_common::{WORLD, iri, linked, observations, proof};

/// Preserve all three original request verdicts and their exact result count;
/// also pin the published world, standpoint and supporting/opposing proof.
#[test]
fn canonical_contextual_examples_ship_scoped_native_results_without_asserting_the_query() {
    let observed = observations();
    for (request, information, context, standpoint, proof_property) in [
        (
            "proposalAssessment",
            InformationState::Supported,
            "proposalView",
            "proposer",
            "resultProof",
        ),
        (
            "reviewAssessment",
            InformationState::Opposed,
            "reviewView",
            "reviewer",
            "resultCounterproof",
        ),
        (
            "nestedAssessment",
            InformationState::Supported,
            "proposalView",
            "proposer",
            "resultProof",
        ),
    ] {
        let request = &observed.nodes[&format!("{CONTEXTUAL}{request}")];
        let result = linked(observed, request, "contextualResult");
        iri(result, "resultInformation", &information.iri());
        iri(
            result,
            "resultEvaluation",
            &EvaluationStatus::Completed.iri(),
        );
        iri(
            result,
            "resultCompleteness",
            &CompletenessStatus::CompleteForFragment.iri(),
        );
        iri(
            result,
            "resultAttributedContext",
            &format!("{CONTEXTUAL}{context}"),
        );
        iri(result, "resultWorld", WORLD);
        iri(
            result,
            "resultStandpoint",
            &format!("{CONTEXTUAL}{standpoint}"),
        );
        proof(observed, result, proof_property);
    }
    assert!(
        observed.readiness_graphs[CONTEXTUAL].is_empty(),
        "contextual evaluation cannot assert its quoted readiness proposition"
    );
}
