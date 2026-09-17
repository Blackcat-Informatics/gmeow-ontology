// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::RdfDataset;

#[test]
fn corpus_logic_module_common_logic_roundtrips() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = crate::fixture::authenticated_artifact(&root, "stage-compile-logic", CHANNEL)
        .expect("producer-authenticated Common Logic observation");
    let observation: Observation = serde_json::from_slice(&bytes).expect("decode observation");
    assert_eq!(observation.source, super::super::SOURCE_PATH);
    let dialects = observation
        .dialects
        .expect("canonical reference was produced");
    for (expected, dialect) in ["clif", "cgif", "xcl"].into_iter().zip(dialects) {
        assert_eq!(dialect.dialect, expected);
        dialect
            .result
            .unwrap_or_else(|error| panic!("{expected}: {error}"));
    }
}

fn synthetic() -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(
        b"<urn:example:A> <https://blackcatinformatics.ca/logic/subClassOf> <urn:example:B> .",
        "application/n-triples",
        None,
    )
    .unwrap()
}

#[test]
fn native_reference_records_every_dialect() {
    let mut artifacts = BTreeMap::new();
    let (program, _) = parse_logic_dataset(&synthetic(), None).unwrap();
    record(&program, &mut artifacts).unwrap();
    let observation: Observation = serde_json::from_slice(&artifacts[CHANNEL]).unwrap();
    let dialects = observation.dialects.unwrap();
    for (name, observation) in ["clif", "cgif", "xcl"].into_iter().zip(dialects) {
        assert_eq!(observation.dialect, name);
        observation.result.unwrap();
    }
}

#[test]
fn malformed_projection_is_recorded_without_aborting_the_other_observations() {
    let (program, _) = parse_logic_dataset(&synthetic(), None).unwrap();
    let mut projection = xcl::project_xcl(&program).unwrap();
    projection.content = "<broken>".to_owned();
    let result = dialect("xcl", &program, Ok(projection), |text| {
        xcl::parse_xcl_str(text, None).map_err(gmeow_errors::Diag::from)
    });
    assert!(result.result.is_err());
    assert!(observe(&program).unwrap().iter().all(|r| r.result.is_ok()));
}

#[test]
fn complete_ir_comparison_records_source_drift() {
    let (program, _) = parse_logic_dataset(&synthetic(), None).unwrap();
    let mut changed = program.clone();
    changed.source_iri = Some("urn:other-source".to_owned());
    assert!(
        exact(&program, &changed)
            .unwrap_err()
            .message()
            .contains("source_iri differs")
    );
}
