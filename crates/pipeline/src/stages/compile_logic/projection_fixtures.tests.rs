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
    let bytes = serde_json::to_vec(&observation).unwrap();
    let restored: ProjectionObservation = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.projections.unwrap(), outputs);
}
