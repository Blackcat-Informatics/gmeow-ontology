// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Grade the producer's exact production-profile flagship negative control.

use std::path::Path;

use super::super::flagship_unwired::{ARTIFACT, Observation, PROFILE};

/// Preserve the missing-producer MinCount and owning-shape failure-class lookup,
/// requiring exactly one such violation with the gate-failing severity.
#[test]
fn shared_flagship_shape_bites_on_a_missing_required_link() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(&root, ARTIFACT)
        .expect("explicit producer supplies authenticated flagship observation");
    let observed: Observation = serde_json::from_slice(&bytes).expect("typed flagship observation");
    assert_eq!(observed.profile, PROFILE);
    assert!(!observed.conforms, "unwired scenario must fail validation");
    let mincount: Vec<_> = observed
        .findings
        .iter()
        .filter(|finding| {
            finding.component == "http://www.w3.org/ns/shacl#MinCountConstraintComponent"
        })
        .collect();
    assert_eq!(
        mincount.len(),
        1,
        "exactly the missing producer MinCount: {:?}",
        observed.findings
    );
    let finding = mincount[0];
    assert_eq!(
        finding.path.as_deref(),
        Some("<https://blackcatinformatics.ca/gmeow/demonstratedByProducer>")
    );
    assert_eq!(
        finding.focus,
        "<https://blackcatinformatics.ca/gmeow/examples/unwired/missingProducer>"
    );
    assert_eq!(finding.severity, "http://www.w3.org/ns/shacl#Violation");
    assert!(
        !finding.source_shape.is_empty(),
        "retain exact property-shape owner"
    );
    assert_eq!(
        finding.failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/UnwiredFlagshipScenario"),
        "the exact source shape {} must resolve its failure class",
        finding.source_shape
    );
}
