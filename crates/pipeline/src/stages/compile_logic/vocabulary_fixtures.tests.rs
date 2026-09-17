// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

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
    let observation: Observation = serde_json::from_slice(&artifacts[CHANNEL]).unwrap();
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
        let observation = observe(&dataset);
        assert!(
            observation
                .preset_facets
                .unwrap_err()
                .to_string()
                .contains("requires a named IRI")
        );
    }
}
