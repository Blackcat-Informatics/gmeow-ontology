// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Ten journal requests retain observed, final and pending shipped verdicts.

use std::collections::BTreeSet;

use gmeow_logic::result::{CompletenessStatus, EvaluationStatus, InformationState};

use super::super::contextual_results::TEMPORAL;
use super::contextual_common::{LOGIC, WORLD, iri, linked, observations, proof};

/// Read the same compact publication as the modal consumer; never load a second
/// corpus or execute temporal evaluation from a test process.
#[test]
fn shipped_temporal_verdicts_distinguish_observed_final_and_pending_positions() {
    let observed = observations();
    for (request, information, scope, position) in [
        ("observedNext", InformationState::Supported, "closed", 0),
        ("finalNext", InformationState::Opposed, "closed", 1),
        ("pendingNext", InformationState::Undetermined, "open", 1),
        ("closedEventuality", InformationState::Opposed, "closed", 0),
        (
            "pendingEventuality",
            InformationState::Undetermined,
            "open",
            0,
        ),
        (
            "closedMaintenance",
            InformationState::Supported,
            "closed",
            0,
        ),
        (
            "pendingMaintenance",
            InformationState::Undetermined,
            "open",
            0,
        ),
        ("closedUntil", InformationState::Opposed, "closed", 0),
        ("pendingUntil", InformationState::Undetermined, "open", 0),
        (
            "immediateUntilWitness",
            InformationState::Supported,
            "closed",
            0,
        ),
    ] {
        let request_node = &observed.nodes[&format!("{TEMPORAL}{request}")];
        let result = linked(observed, request_node, "contextualResult");
        let completeness = if scope == "closed" {
            CompletenessStatus::CompleteForFragment
        } else {
            CompletenessStatus::Incomplete
        };
        iri(result, "resultInformation", &information.iri());
        iri(
            result,
            "resultEvaluation",
            &EvaluationStatus::Completed.iri(),
        );
        iri(result, "resultCompleteness", &completeness.iri());
        iri(
            result,
            "resultAttributedContext",
            &format!("{TEMPORAL}{scope}At{position}"),
        );
        iri(result, "resultWorld", WORLD);
        iri(result, "resultStandpoint", &format!("{TEMPORAL}observer"));
        match information {
            InformationState::Supported => proof(observed, result, "resultProof"),
            InformationState::Opposed => proof(observed, result, "resultCounterproof"),
            InformationState::Undetermined => {
                for property in ["resultProof", "resultCounterproof"] {
                    assert!(
                        !result
                            .properties
                            .contains_key(&format!("{LOGIC}{property}")),
                        "{request}: pending truth cannot publish a completed proof"
                    );
                }
            }
            other => panic!("unexpected demonstrator expectation {other:?}"),
        }
        let prefix = linked(observed, result, "observedTemporalPrefix");
        let boundary = if scope == "closed" {
            "FinalizedJournalBoundary"
        } else {
            "OpenJournalBoundary"
        };
        for (property, expected) in [
            ("prefixJournal", format!("{TEMPORAL}{scope}Journal")),
            ("prefixEnactment", format!("{TEMPORAL}{scope}Run")),
            ("prefixHead", format!("{TEMPORAL}{scope}Entry1")),
            ("journalBoundary", format!("{LOGIC}{boundary}")),
        ] {
            iri(prefix, property, &expected);
        }
        for (property, hash) in [
            (
                "prefixInitialHead",
                "blake3:70959dbd606510b67d2121f9ebf503fdb2896e3b2b4653c33832af23e78be6ff",
            ),
            (
                "prefixHeadHash",
                "blake3:4abbd5423f859a2b38b4acc246831a064aacba664acf9a49ad971404afc561a8",
            ),
        ] {
            assert_eq!(
                prefix.properties.get(&format!("{LOGIC}{property}")),
                Some(&BTreeSet::from([format!(
                    "\"{hash}\"^^<http://www.w3.org/2001/XMLSchema#string>"
                )])),
                "{request}: exact selected prefix digest"
            );
        }
    }
    assert!(
        observed.readiness_graphs[TEMPORAL].is_empty(),
        "temporal evaluation cannot assert its quoted readiness proposition"
    );
}
