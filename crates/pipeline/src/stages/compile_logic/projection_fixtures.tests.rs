// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn selected_projection_observation_retains_all_seven_outputs() {
    let (program, _) = gmeow_logic_compile::frontend::parse_logic_str(
        "<urn:C> <https://blackcatinformatics.ca/logic/subClassOf> <urn:D> .",
        None,
    )
    .unwrap();
    let outputs = project(&program).unwrap();
    assert_eq!(outputs.len(), 7);
    assert!(
        outputs["owl-dl"]
            .contains("<urn:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:D>")
    );
    let observation = ProjectionObservation {
        diagnostics: Vec::new(),
        projections: Ok(outputs.clone()),
    };
    let observations = BTreeMap::from([("authored".to_owned(), Ok(observation))]);
    let restored = json::from_bytes(
        &purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(&observations))
            .into_bytes(),
    )
    .unwrap();
    assert_eq!(
        restored["authored"]
            .as_ref()
            .unwrap()
            .projections
            .as_ref()
            .unwrap(),
        &outputs
    );
}

const PROJECTION_WIRE: &str = r#"{"a":{"Ok":{"diagnostics":[{"severity":"Info","code":"loss","message":"retained","subject":null}],"projections":{"Ok":{"n3":"line\n中","owl-dl":"<urn:a> <urn:p> <urn:b> ."}}}}}"#;

#[test]
fn native_projection_wire_preserves_terminals_and_both_causal_outcomes() {
    let mut observations = json::from_bytes(PROJECTION_WIRE.as_bytes()).unwrap();
    assert_eq!(
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(&observations))
            .into_bytes(),
        PROJECTION_WIRE.as_bytes()
    );
    let unknown = PROJECTION_WIRE.replace("\"diagnostics\":", "\"future\":null,\"diagnostics\":");
    assert_eq!(
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(
            &json::from_bytes(unknown.as_bytes()).unwrap()
        ))
        .into_bytes(),
        PROJECTION_WIRE.as_bytes()
    );
    let mut ledger = gmeow_errors::DiagLedger::new();
    let cause = ledger.attach(
        gmeow_errors::Diag::from(std::io::Error::other("authored source refusal"))
            .with_focus("urn:projection:source"),
        gmeow_errors::StageId::new("projection-input"),
    );
    let refusal = ledger.record(
        gmeow_errors::Diag::from(std::io::Error::other("authored projection refusal"))
            .with_focus("urn:projection:output")
            .with_antecedents([cause]),
        gmeow_errors::StageId::new("projection-output"),
    );
    observations
        .get_mut("a")
        .unwrap()
        .as_mut()
        .unwrap()
        .projections = Err(refusal.clone());
    observations.insert("b".into(), Err(refusal.clone()));
    let restored = json::from_bytes(
        &purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(&observations))
            .into_bytes(),
    )
    .unwrap();
    assert_eq!(
        restored["a"].as_ref().unwrap().diagnostics,
        observations["a"].as_ref().unwrap().diagnostics
    );
    assert_eq!(
        restored["a"]
            .as_ref()
            .unwrap()
            .projections
            .as_ref()
            .unwrap_err(),
        &refusal
    );
    assert_eq!(restored["b"].as_ref().unwrap_err(), &refusal);
    assert_eq!(refusal.causes.len(), 1);
}

#[test]
fn native_projection_admission_validates_shadowed_map_values_and_named_records() {
    for invalid in [
        PROJECTION_WIRE.replacen('{', "{\"a\":{\"Ok\":null},", 1),
        PROJECTION_WIRE.replace("\"diagnostics\":", "\"diagnostics\":[],\"diagnostics\":"),
        PROJECTION_WIRE.replace("\"projections\":", "\"projections\":null,\"projections\":"),
        PROJECTION_WIRE.replace("\"diagnostics\":[{\"severity\":\"Info\",\"code\":\"loss\",\"message\":\"retained\",\"subject\":null}],", ""),
        PROJECTION_WIRE.replace("\"n3\":", "\"n3\":false,\"n3\":"),
        PROJECTION_WIRE.replace("\"Ok\":{\"diagnostics\":", "\"Ok\":null,\"Ok\":{\"diagnostics\":"),
        r#"{"a":{"Ok":[[],{"Ok":{}}]}}"#.to_owned(),
    ] {
        assert!(json::from_bytes(invalid.as_bytes()).is_err(), "{invalid}");
    }
    let duplicate = PROJECTION_WIRE.replace("\"n3\":", "\"n3\":\"earlier\",\"n3\":");
    assert_eq!(
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(
            &json::from_bytes(duplicate.as_bytes()).unwrap()
        ))
        .into_bytes(),
        PROJECTION_WIRE.as_bytes()
    );
}
