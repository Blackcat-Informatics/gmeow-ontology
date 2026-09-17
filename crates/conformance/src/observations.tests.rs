// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn native_ontouml_observation_preserves_discipline_and_gap_error_boundaries() {
    use crate::external::ontouml::OntoumlError;
    let source = b"@prefix onto: <https://w3id.org/ontouml#> . <https://example.org/Role> a onto:Class ; onto:stereotype onto:role .";
    // gmeow-test-input: synthetic-only; a controlled one-class model.
    let observed = ontouml(source, "https://example.org/world").unwrap();
    assert!(observed.as_ref().unwrap().contains("FreeRole"));
    let bytes = serde_json::to_vec(&observed).unwrap();
    assert_eq!(
        serde_json::from_slice::<OntoumlOutcome>(&bytes).unwrap(),
        observed
    );
    // gmeow-test-input: synthetic-only; invalid native world cannot become a gap.
    assert!(matches!(
        ontouml(source, "invalid world").unwrap(),
        Err(OntoumlError::Syntax(_))
    ));
    let unsupported = b"@prefix onto: <https://w3id.org/ontouml#> . <https://example.org/Water> a onto:Class ; onto:stereotype onto:quantity .";
    // gmeow-test-input: synthetic-only; a real unsupported stereotype remains a gap.
    assert!(matches!(
        ontouml(unsupported, "https://example.org/world").unwrap(),
        Err(OntoumlError::Unsupported(_))
    ));
}

#[test]
fn one_source_retains_both_native_selections_and_the_complete_proof_export() {
    let source = b"fof(rule, axiom, ![X] : (person(X) => known(X))).\nfof(fact, axiom, person(alice)).\nfof(goal, conjecture, known(alice)).\n";
    // gmeow-test-input: synthetic-only; this source is constructed above.
    let observed = tptp(source, "urn:synthetic-world").unwrap().unwrap();
    assert_eq!(observed.formulas.len(), 3);
    assert_eq!(observed.decision, Ok(ExternalOutcome::Inconsistent));
    let proof = observed.proof.as_ref().unwrap();
    assert_eq!(proof.status, "ok");
    assert_eq!(proof.answers.len(), 1);
    let answer = &proof.answers[0];
    assert_eq!(answer.steps.len(), 2);
    assert!(!answer.steps[0].asserted);
    assert_eq!(answer.steps[0].premises, [1]);
    assert!(answer.steps[1].asserted);
    assert_eq!(answer.parsed.len(), answer.steps.len());
    assert!(!answer.tstp.is_empty());
    let bytes = serde_json::to_vec(&observed).unwrap();
    assert_eq!(
        serde_json::from_slice::<TptpObservation>(&bytes).unwrap(),
        observed
    );
}

#[test]
fn malformed_inputs_are_execution_failures_outside_semantic_results() {
    // gmeow-test-input: synthetic-only; invalid bytes and fields, no corpus.
    assert!(tptp(&[0xff], "urn:world").is_err());
    // gmeow-test-input: synthetic-only; malformed program in an in-memory input.
    assert!(numeric_oracle(br#"{"program":"?", "facts":[]}"#).is_err());
    // gmeow-test-input: synthetic-only; a declared facts input cannot degrade to empty.
    assert!(numeric_oracle(br#"{"program":"", "facts":null}"#).is_err());
}
