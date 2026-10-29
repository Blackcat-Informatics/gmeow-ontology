// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic::reason::refute::{
    CLASS_EXPRESSION_SOURCE_ADMISSION_ID, ClassAdmissionSourceWorld, ClassAdmissionWorld,
    FragmentBoundary, RefutationPremise, RefutationSourceIssue,
};
use std::collections::{BTreeMap, BTreeSet};

/// Receipts retain bytes, graph scope and external status without semantic comparison.
#[test]
fn shipped_admission_keeps_native_graph_receipt_and_external_status_without_semantic_comparison() {
    // Frozen before replacing the original encoder: complete envelope bytes and
    // actual emitted identities. Line feeds separate fixture rows only.
    let assert_original = |case: &str, identity: &str, encoded: &str| {
        let expected = include_str!("fixtures/source-admission-cbor-v1.txt")
            .lines()
            .find(|line| line.split_once('|').is_some_and(|(name, _)| name == case))
            .expect("original receipt case must exist");
        assert_eq!(format!("{case}|{identity}|{encoded}"), expected);
    };
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("expected")).unwrap();
    let bytes = b"<urn:s> <urn:ordinary> <urn:o> <urn:world> .\n";
    std::fs::write(directory.path().join("input.nq"), bytes).unwrap();
    let observation = SourceAdmissionObservation {
        input_blake3: *purrdf_hash::blake3::hash(bytes).as_bytes(),
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
        purrdf_lex::json::record::to_vec(&observation.admission),
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
    let identity = projected
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    assert_original("outside", identity, encoded);
    let bytes = purrdf_hash::hex::decode(encoded).unwrap();
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

    // Independently authored receipt data exercises transport and scope admission,
    // not a parser, source preparation or a semantic consistency verdict.
    let graph = Some(TermValue::iri("urn:world"));
    let literal = TermValue::Literal {
        lexical_form: "ملاحظة\n\"verbatim\"".to_owned(),
        datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".to_owned(),
        language: Some("ar".to_owned()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    };
    let nested = TermValue::Triple {
        s: purrdf::TermBox::new(TermValue::Blank {
            label: "source-claim".to_owned(),
            scope: purrdf::BlankScope(u32::MAX),
        }),
        p: purrdf::TermBox::new(TermValue::iri("urn:outer")),
        o: purrdf::TermBox::new(TermValue::Triple {
            s: purrdf::TermBox::new(TermValue::iri("urn:inner")),
            p: purrdf::TermBox::new(TermValue::iri("urn:predicate")),
            o: purrdf::TermBox::new(literal.clone()),
        }),
    };
    let premise = RefutationPremise {
        subject: nested.clone(),
        predicate: "urn:definition".to_owned(),
        object: literal.clone(),
        graph: graph.clone(),
    };
    let boundaries = [
        RefutationSourceIssue::MalformedNil,
        RefutationSourceIssue::ConflictingListField {
            subject: "urn:list".to_owned(),
            predicate: "urn:first".to_owned(),
        },
        RefutationSourceIssue::ExpressionMultiplicity {
            subject: "urn:owner".to_owned(),
            predicate: "urn:definition".to_owned(),
        },
        RefutationSourceIssue::UnsupportedExpressionOwner {
            owner: nested,
            predicate: "urn:definition".to_owned(),
        },
        RefutationSourceIssue::UnsupportedExpressionOperand {
            owner: "urn:owner".to_owned(),
            predicate: "urn:definition".to_owned(),
            operand: literal,
        },
        RefutationSourceIssue::UnsupportedListMember {
            owner: "urn:owner".to_owned(),
            head: "urn:list".to_owned(),
        },
        RefutationSourceIssue::CyclicList {
            owner: "urn:owner".to_owned(),
            head: "urn:list".to_owned(),
        },
        RefutationSourceIssue::IncompleteList {
            owner: "urn:owner".to_owned(),
            head: "urn:list".to_owned(),
        },
    ]
    .into_iter()
    .map(|issue| FragmentBoundary::SourceAdmission {
        world: "urn:world".to_owned(),
        premises: vec![premise.clone()],
        issue,
    })
    .collect();
    let rich = SourceAdmissionObservation {
        input_blake3: *purrdf_hash::blake3::hash(b"independently authored receipt").as_bytes(),
        admission: ClassAdmissionObservation {
            contract: CLASS_EXPRESSION_SOURCE_ADMISSION_ID.to_owned(),
            source_worlds: BTreeMap::from([
                (
                    gmeow_logic::reason::rl::DEFAULT_WORLD.to_owned(),
                    ClassAdmissionSourceWorld {
                        graph: None,
                        assertions: 0,
                    },
                ),
                (
                    "urn:world".to_owned(),
                    ClassAdmissionSourceWorld {
                        graph: graph.clone(),
                        assertions: u64::MAX,
                    },
                ),
            ]),
            selected_worlds: BTreeMap::from([(
                "urn:world".to_owned(),
                ClassAdmissionWorld {
                    graph: graph.clone(),
                    definitions: vec![
                        premise,
                        RefutationPremise {
                            subject: TermValue::iri("urn:plain"),
                            predicate: "urn:definition".to_owned(),
                            object: TermValue::Literal {
                                lexical_form: "plain value".to_owned(),
                                datatype: "http://www.w3.org/2001/XMLSchema#string".to_owned(),
                                language: None,
                                direction: None,
                            },
                            graph: graph.clone(),
                        },
                        RefutationPremise {
                            subject: TermValue::Blank {
                                label: "left-claim".to_owned(),
                                scope: purrdf::BlankScope(0),
                            },
                            predicate: "urn:definition".to_owned(),
                            object: TermValue::Literal {
                                lexical_form: "left to right".to_owned(),
                                datatype:
                                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"
                                        .to_owned(),
                                language: Some("en".to_owned()),
                                direction: Some(purrdf::RdfTextDirection::Ltr),
                            },
                            graph,
                        },
                    ],
                    refusal: Some(FragmentBoundary::Combined(BTreeSet::from([
                        FragmentBoundary::Combined(boundaries),
                    ]))),
                },
            )]),
        },
    };
    rich.admission
        .validate()
        .expect("complete selected-world scope");
    let mut admitted = rich.clone();
    admitted
        .admission
        .selected_worlds
        .get_mut("urn:world")
        .unwrap()
        .refusal = None;
    for (case, observation) in [("rich-refused", rich), ("rich-admitted", admitted)] {
        observation
            .admission
            .validate()
            .expect("valid authored scope");
        let projected = emit(&[GradedAdmission {
            corpus: "synthetic".to_owned(),
            case: case.to_owned(),
            published: "external-status".to_owned(),
            observation: observation.clone(),
        }])
        .unwrap();
        assert!(projected.contains("sourceAdmissionSelected> \"true\""));
        assert!(projected.contains("sourceAdmissionAssertionCount> \"18446744073709551615\""));
        assert!(!projected.contains("ConformanceComparison>"));
        let encoded = projected
            .lines()
            .find(|line| line.contains("sourceAdmissionEvidence>"))
            .unwrap()
            .split('"')
            .nth(1)
            .unwrap();
        let identity = projected
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert_original(case, identity, encoded);
        let bytes = purrdf_hash::hex::decode(encoded).unwrap();
        let (schema, corpus, restored_case, published, restored): (
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
                restored_case.as_str(),
                published.as_str()
            ),
            (RECEIPT_SCHEMA, "synthetic", case, "external-status")
        );
        assert_eq!(restored, observation);
    }
}
