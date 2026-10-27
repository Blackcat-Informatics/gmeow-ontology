// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const WIRE: &str = r#"{"diagnostics":[{"severity":"Warning","code":"authored","message":"retained 中","subject":"urn:root"}],"declared_roots":["urn:a","urn:z"],"reconstructed":{"urn:a":{"Ok":{"Not":{"Atom":{"relation":{"Iri":"urn:p"},"args":[{"Var":"x"},{"Iri":"urn:o"}]}}}}},"asserted":[{"Atom":{"relation":{"Iri":"urn:q"},"args":[]}},{"Not":{"Atom":{"relation":{"Iri":"urn:p"},"args":[{"Var":"x"},{"Iri":"urn:o"}]}}}],"axiom_predicates":["urn:p","urn:q"]}"#;

#[test]
fn native_abduction_wire_retains_formulas_sets_and_causal_reconstruction() {
    let mut observation: Observation = from_slice(WIRE.as_bytes()).unwrap();
    assert_eq!(to_vec(&observation), WIRE.as_bytes());
    assert_eq!(
        observation.asserted[1],
        observation.reconstructed["urn:a"].as_ref().unwrap().clone()
    );
    let unordered = WIRE.replace("[\"urn:a\",\"urn:z\"]", "[\"urn:z\",\"urn:a\",\"urn:z\"]");
    assert_eq!(
        to_vec(&from_slice::<Observation>(unordered.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let unknown = WIRE.replacen('{', "{\"future\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<Observation>(unknown.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let mut ledger = gmeow_errors::DiagLedger::new();
    let cause = ledger.attach(
        gmeow_errors::Diag::from(std::io::Error::other("missing premise"))
            .with_focus("urn:abduction:premise"),
        gmeow_errors::StageId::new("abduction-source"),
    );
    let refusal = ledger.record(
        gmeow_errors::Diag::from(std::io::Error::other("reconstruction refused"))
            .with_focus("urn:abduction:reconstruction")
            .with_antecedents([cause]),
        gmeow_errors::StageId::new("abduction-observation"),
    );
    observation
        .reconstructed
        .insert("urn:z".into(), Err(refusal.clone()));
    let restored: Observation = from_slice(&to_vec(&observation)).unwrap();
    assert_eq!(restored.diagnostics, observation.diagnostics);
    assert_eq!(restored.asserted, observation.asserted);
    assert_eq!(restored.reconstructed, observation.reconstructed);
    assert_eq!(
        restored.reconstructed["urn:z"].as_ref().unwrap_err(),
        &refusal
    );
    assert_eq!(refusal.causes.len(), 1);
}

#[test]
fn native_abduction_admission_refuses_incomplete_and_shadowed_invalid_evidence() {
    for invalid in [
        WIRE.replacen("\"diagnostics\":", "\"diagnostics\":[],\"diagnostics\":", 1),
        WIRE.replace("\"declared_roots\":[\"urn:a\",\"urn:z\"],", ""),
        WIRE.replace("\"asserted\":[", "\"asserted\":null,\"asserted\":["),
        WIRE.replace(
            "\"reconstructed\":{",
            "\"reconstructed\":{\"urn:a\":{\"Ok\":null},",
        ),
        WIRE.replace("\"Ok\":{\"Not\":", "\"Ok\":null,\"Ok\":{\"Not\":"),
        WIRE.replace(
            "\"declared_roots\":[\"urn:a\",\"urn:z\"]",
            "\"declared_roots\":[false]",
        ),
        "[[],[],{},[],[]]".to_owned(),
    ] {
        assert!(
            from_slice::<Observation>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let last_wins = WIRE.replace(
        "\"reconstructed\":{",
        "\"reconstructed\":{\"urn:a\":{\"Ok\":{\"And\":[]}},",
    );
    assert_eq!(
        to_vec(&from_slice::<Observation>(last_wins.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
}
