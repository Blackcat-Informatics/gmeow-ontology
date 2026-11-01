// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn observations_bind_native_typing_and_selected_graph_without_text_extraction() {
    let dataset = purrdf::parse_dataset(
        br#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            logic:Example a logic:ReasoningPreset;
                logic:instanceOf logic:ReasoningPreset;
                logic:expandsToFacet logic:HornFragment;
                logic:resourcePolicy logic:ProceduralExecution .
            logic:Ignored logic:scopeNote "logic:ReasoningPreset";
                logic:someOtherProperty logic:ReasoningPreset .
            <urn:other> { logic:Other a logic:ReasoningPreset . }
        "#,
        "application/trig",
        None,
    )
    .unwrap();
    let mut artifacts = BTreeMap::new();
    record(&dataset, &mut artifacts).unwrap();
    let observation: Observation =
        purrdf_lex::json::record::from_slice(&artifacts[CHANNEL]).unwrap();
    assert_eq!(observation, observe(&dataset));
    assert_eq!(
        observation.members["ReasoningPreset"].as_ref().unwrap(),
        &BTreeSet::from([format!("{LOGIC_NAMESPACE}Example")])
    );
    assert_eq!(
        observation.preset_facets.unwrap()[&format!("{LOGIC_NAMESPACE}Example")],
        BTreeSet::from([
            format!("{LOGIC_NAMESPACE}HornFragment"),
            format!("{LOGIC_NAMESPACE}ProceduralExecution")
        ])
    );
    assert!(observation.members["NodeKind"].as_ref().unwrap().is_empty());
}

#[test]
fn malformed_members_and_facets_remain_explicit_observation_failures() {
    for source in [
        "_:member a logic:ReasoningPreset .",
        "logic:Example a logic:ReasoningPreset; logic:expandsToFacet \"ProceduralExecution\" .",
    ] {
        let dataset = purrdf::parse_dataset(
            format!("@prefix logic: <{LOGIC_NAMESPACE}> . {source}").as_bytes(),
            "text/turtle",
            None,
        )
        .unwrap();
        let mut artifacts = BTreeMap::new();
        record(&dataset, &mut artifacts).unwrap();
        let observation: Observation =
            purrdf_lex::json::record::from_slice(&artifacts[CHANNEL]).unwrap();
        assert!(
            observation
                .preset_facets
                .unwrap_err()
                .to_string()
                .contains("requires a named IRI")
        );
    }
}

const VOCABULARY_WIRE: &str = r#"{"source":"slices/grounding/logic/module.ttl","members":{"NodeKind":{"Ok":["urn:a","urn:z"]},"ReasoningPreset":{"Ok":["urn:Preset"]}},"preset_facets":{"Ok":{"urn:Preset":["urn:facet-a","urn:facet-z"]}}}"#;

#[test]
fn native_vocabulary_wire_retains_sorted_sets_maps_and_complete_causal_outcomes() {
    let observation: Observation = from_slice(VOCABULARY_WIRE.as_bytes()).unwrap();
    assert_eq!(observation.source, super::super::SOURCE_PATH);
    assert_eq!(to_vec(&observation), VOCABULARY_WIRE.as_bytes());
    let unsorted = VOCABULARY_WIRE
        .replace("[\"urn:a\",\"urn:z\"]", "[\"urn:z\",\"urn:a\",\"urn:z\"]")
        .replace(
            "[\"urn:facet-a\",\"urn:facet-z\"]",
            "[\"urn:facet-z\",\"urn:facet-a\",\"urn:facet-z\"]",
        );
    assert_eq!(
        to_vec(&from_slice::<Observation>(unsorted.as_bytes()).unwrap()),
        VOCABULARY_WIRE.as_bytes()
    );
    let repeated = VOCABULARY_WIRE
        .replace(
            "\"NodeKind\":{\"Ok\":[\"urn:a\",\"urn:z\"]}",
            "\"NodeKind\":{\"Ok\":[\"urn:discarded\"]},\"NodeKind\":{\"Ok\":[\"urn:last\"]}",
        )
        .replace(
            "\"urn:Preset\":[\"urn:facet-a\",\"urn:facet-z\"]",
            "\"urn:Preset\":[\"urn:discarded\"],\"urn:Preset\":[\"urn:last\"]",
        );
    let repeated: Observation = from_slice(repeated.as_bytes()).unwrap();
    assert_eq!(
        repeated.members["NodeKind"].as_ref().unwrap(),
        &BTreeSet::from(["urn:last".into()])
    );
    assert_eq!(
        repeated.preset_facets.as_ref().unwrap()["urn:Preset"],
        BTreeSet::from(["urn:last".into()])
    );

    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("authored premise")).with_focus("urn:premise"),
        StageId::new("vocabulary-input"),
    );
    let refusal = ledger.record(
        Diag::from(std::io::Error::other("authored refusal"))
            .with_focus("urn:member")
            .with_antecedents([premise]),
        StageId::new("vocabulary-observation"),
    );
    let observation = Observation {
        source: super::super::SOURCE_PATH.into(),
        members: BTreeMap::from([
            ("Refused".into(), Err(refusal.clone())),
            ("Empty".into(), Ok(BTreeSet::new())),
        ]),
        preset_facets: Err(refusal.clone()),
    };
    let restored: Observation = from_slice(&to_vec(&observation)).unwrap();
    assert_eq!(restored, observation);
    assert_eq!(restored.members["Refused"].as_ref().unwrap_err(), &refusal);
    let retained = restored.preset_facets.unwrap_err();
    assert_eq!(retained.causes.len(), 1);
    assert_eq!(
        retained.root.antecedents.as_ref(),
        &[retained.causes[0].fingerprint]
    );
}

#[test]
fn native_vocabulary_admission_refuses_ambiguous_records_and_shadowed_invalid_values() {
    let unknown = VOCABULARY_WIRE.replacen('{', "{\"future\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<Observation>(unknown.as_bytes()).unwrap()),
        VOCABULARY_WIRE.as_bytes()
    );
    let named = purrdf_lex::json::read(VOCABULARY_WIRE).unwrap();
    let positional = purrdf_lex::json::Value::from(
        named
            .as_object()
            .unwrap()
            .iter()
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>(),
    );
    assert!(from_slice::<Observation>(&to_vec(&positional)).is_err());
    for invalid in [
        VOCABULARY_WIRE.replace("\"source\":", "\"source\":\"duplicate\",\"source\":"),
        VOCABULARY_WIRE.replace(
            "\"preset_facets\":",
            "\"preset_facets\":null,\"preset_facets\":",
        ),
        VOCABULARY_WIRE.replace("\"source\":\"slices/grounding/logic/module.ttl\",", ""),
        VOCABULARY_WIRE.replace(
            "\"ReasoningPreset\":{\"Ok\":[\"urn:Preset\"]}",
            "\"ReasoningPreset\":{\"Ok\":[\"urn:Preset\"],\"Err\":null}",
        ),
        VOCABULARY_WIRE.replace(
            "\"Ok\":{\"urn:Preset\":[\"urn:facet-a\",\"urn:facet-z\"]}",
            "\"Err\":null",
        ),
        "{".into(),
    ] {
        assert!(
            from_slice::<Observation>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    for (invalid, pointer) in [
        (
            VOCABULARY_WIRE.replace(
                "\"NodeKind\":",
                "\"NodeKind\":{\"Ok\":[false]},\"NodeKind\":",
            ),
            "/members/NodeKind/Ok/0",
        ),
        (
            VOCABULARY_WIRE.replace(
                "\"urn:Preset\":[\"urn:facet-a\",\"urn:facet-z\"]",
                "\"urn:Preset\":[false],\"urn:Preset\":[\"urn:facet-a\",\"urn:facet-z\"]",
            ),
            "/preset_facets/Ok/urn:Preset/0",
        ),
    ] {
        let error = from_slice::<Observation>(invalid.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(error.contains(pointer), "{error}");
    }
}
