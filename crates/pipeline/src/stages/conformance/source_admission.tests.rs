// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic::reason::refute::{
    CLASS_EXPRESSION_SOURCE_ADMISSION_ID, ClassAdmissionSourceWorld,
};
use std::collections::BTreeMap;

#[test]
fn shipped_admission_keeps_native_graph_receipt_and_external_status_without_semantic_comparison() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("expected")).unwrap();
    let bytes = b"<urn:s> <urn:ordinary> <urn:o> <urn:world> .\n";
    std::fs::write(directory.path().join("input.nq"), bytes).unwrap();
    let observation = SourceAdmissionObservation {
        input_blake3: *blake3::hash(bytes).as_bytes(),
        admission: ClassAdmissionObservation {
            contract: CLASS_EXPRESSION_SOURCE_ADMISSION_ID.to_owned(),
            selected_worlds: BTreeMap::new(),
            source_worlds: BTreeMap::from([(
                "urn:world".to_owned(),
                ClassAdmissionSourceWorld {
                    graph: Some(TermValue::iri("urn:world")),
                    assertions: 1,
                },
            )]),
        },
    };
    std::fs::write(
        directory.path().join("expected/verdicts.json"),
        serde_json::to_vec(&observation.admission).unwrap(),
    )
    .unwrap();
    let record = grade(
        "synthetic",
        "outside",
        directory.path(),
        "inconsistent".to_owned(),
        &observation,
    )
    .unwrap();
    let projected = emit(&[record]).unwrap();
    assert!(projected.contains("SourceAdmissionObservation>"));
    assert!(projected.contains("sourceAdmissionSelected> \"false\""));
    assert!(projected.contains("sourceAdmissionPublishedToken> \"inconsistent\""));
    assert!(projected.contains("sourceAdmissionAssertionCount> \"1\""));
    assert!(!projected.contains("ConformanceComparison>"));
    assert!(!projected.contains("comparisonNativeVerdict"));
    let encoded = projected
        .lines()
        .find(|line| line.contains("sourceAdmissionEvidence>"))
        .unwrap()
        .split('"')
        .nth(1)
        .unwrap();
    let bytes: Vec<_> = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let (schema, corpus, case, published, restored): (
        String,
        String,
        String,
        String,
        SourceAdmissionObservation,
    ) = ciborium::from_reader(bytes.as_slice()).unwrap();
    assert_eq!(
        (
            schema.as_str(),
            corpus.as_str(),
            case.as_str(),
            published.as_str()
        ),
        (RECEIPT_SCHEMA, "synthetic", "outside", "inconsistent")
    );
    assert_eq!(restored, observation);
    let mut wrong = observation;
    wrong.input_blake3 = [0; 32];
    assert!(
        grade(
            "synthetic",
            "outside",
            directory.path(),
            "inconsistent".to_owned(),
            &wrong
        )
        .is_err()
    );
}
