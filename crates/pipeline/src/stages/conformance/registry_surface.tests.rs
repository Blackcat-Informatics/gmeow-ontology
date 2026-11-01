// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use gmeow_ns::LOGIC_NS;
use std::collections::BTreeSet;

use super::*;

fn dataset(body: &str) -> std::sync::Arc<RdfDataset> {
    purrdf::dataset_from_bytes(
        format!("@prefix logic: <{LOGIC_NS}> .\n{body}").as_bytes(),
        purrdf::NativeRdfFormat::Turtle,
    )
    .unwrap()
}

#[test]
fn registry_preserves_both_poles_and_their_characterizations() {
    let source = dataset(
        r#"logic:pattern a logic:RefutationPattern .
               logic:decided a logic:DecidedFragment;
                   logic:decidesUnderPattern logic:pattern;
                   logic:fragmentCompletenessBound "finite fragment" .
               logic:admission a logic:SourceAdmissionContract;
                   logic:sourceAdmissionRequirement "selected owned list grammar" .
               logic:boundary logic:expressivenessBoundary logic:Unsupported;
                   logic:fragmentBoundaryReason "unbounded fragment" ."#,
    );
    let mut artifacts = BTreeMap::new();
    record(&source, &mut artifacts).unwrap();
    assert_eq!(
        std::str::from_utf8(&artifacts[CHANNEL]).unwrap(),
        r#"{"Ok":{"pattern_ids":["pattern"],"decided_ids":["decided"],"source_admission_ids":["admission"],"source_admission_requirements":{"admission":[{"subject":"https://blackcatinformatics.ca/logic/admission","predicate":"https://blackcatinformatics.ca/logic/sourceAdmissionRequirement","object":{"lexical_form":"selected owned list grammar","datatype":null,"language":null,"direction":null}}]},"boundary_ids":["boundary"],"deciding_patterns":{"decided":"pattern"},"completeness_bounds":{"decided":[{"subject":"https://blackcatinformatics.ca/logic/decided","predicate":"https://blackcatinformatics.ca/logic/fragmentCompletenessBound","object":{"lexical_form":"finite fragment","datatype":null,"language":null,"direction":null}}]},"boundary_reasons":{"boundary":[{"subject":"https://blackcatinformatics.ca/logic/boundary","predicate":"https://blackcatinformatics.ca/logic/fragmentBoundaryReason","object":{"lexical_form":"unbounded fragment","datatype":null,"language":null,"direction":null}}]}}}"#
    );
    let observed: Observation = record::from_slice(&artifacts[CHANNEL]).unwrap();
    assert_eq!(
        observed.0.unwrap(),
        RegistrySurface {
            pattern_ids: BTreeSet::from(["pattern".to_owned()]),
            decided_ids: BTreeSet::from(["decided".to_owned()]),
            boundary_ids: BTreeSet::from(["boundary".to_owned()]),
            source_admission_ids: BTreeSet::from(["admission".to_owned()]),
            source_admission_requirements: BTreeMap::from([(
                "admission".to_owned(),
                vec![
                    gmeow_errors::SourceAnnotation::new(
                        purrdf::iri::BaseIri::parse(
                            "https://blackcatinformatics.ca/logic/admission"
                        )
                        .unwrap(),
                        purrdf::iri::BaseIri::parse(
                            "https://blackcatinformatics.ca/logic/sourceAdmissionRequirement"
                        )
                        .unwrap(),
                        purrdf::RdfLiteral::simple("selected owned list grammar"),
                    )
                    .unwrap()
                ]
            )]),
            deciding_patterns: BTreeMap::from([("decided".to_owned(), "pattern".to_owned())]),
            completeness_bounds: BTreeMap::from([(
                "decided".to_owned(),
                vec![
                    gmeow_errors::SourceAnnotation::new(
                        purrdf::iri::BaseIri::parse("https://blackcatinformatics.ca/logic/decided")
                            .unwrap(),
                        purrdf::iri::BaseIri::parse(
                            "https://blackcatinformatics.ca/logic/fragmentCompletenessBound"
                        )
                        .unwrap(),
                        purrdf::RdfLiteral::simple("finite fragment"),
                    )
                    .unwrap()
                ]
            )]),
            boundary_reasons: BTreeMap::from([(
                "boundary".to_owned(),
                vec![
                    gmeow_errors::SourceAnnotation::new(
                        purrdf::iri::BaseIri::parse(
                            "https://blackcatinformatics.ca/logic/boundary"
                        )
                        .unwrap(),
                        purrdf::iri::BaseIri::parse(
                            "https://blackcatinformatics.ca/logic/fragmentBoundaryReason"
                        )
                        .unwrap(),
                        purrdf::RdfLiteral::simple("unbounded fragment"),
                    )
                    .unwrap()
                ]
            )]),
        }
    );
}

#[test]
fn registry_native_json_requires_a_complete_named_inventory() {
    const EMPTY: &str = r#"{"pattern_ids":[],"decided_ids":[],"source_admission_ids":[],"source_admission_requirements":{},"boundary_ids":[],"deciding_patterns":{},"completeness_bounds":{},"boundary_reasons":{}}"#;
    let decode = |json: &str| record::from_slice::<RegistrySurface>(json.as_bytes());
    assert_eq!(decode(EMPTY).unwrap(), RegistrySurface::default());
    for invalid in [
        EMPTY.replace(r#""pattern_ids":[],"#, ""),
        EMPTY.replace(r#""boundary_reasons":{}"#, r#""unrelated":{}"#),
        EMPTY.replace(r#""pattern_ids":[]"#, r#""pattern_ids":null"#),
        EMPTY.replace(r#""boundary_reasons":{}"#, r#""boundary_reasons":[]"#),
        EMPTY.replace(
            r#""pattern_ids":[]"#,
            r#""pattern_ids":[],"pattern_ids":[]"#,
        ),
        "[[],[],[],{},[],{},{},{}]".to_owned(),
        EMPTY.replace(
            r#""deciding_patterns":{}"#,
            r#""deciding_patterns":{"decided":null,"decided":"pattern"}"#,
        ),
    ] {
        assert!(decode(&invalid).is_err(), "accepted {invalid}");
    }
    let extended = EMPTY.replace(
        r#""deciding_patterns":{}"#,
        r#""unknown":true,"deciding_patterns":{"decided":"old","decided":"pattern"}"#,
    );
    assert_eq!(
        decode(&extended).unwrap().deciding_patterns,
        BTreeMap::from([("decided".to_owned(), "pattern".to_owned())])
    );
    // Both outcomes would lose the source refusal if an ambiguous envelope won.
    for invalid in [
        format!(r#"{{"Ok":{EMPTY},"Err":null}}"#),
        format!(r#"{{"Ok":{EMPTY},"Ok":{EMPTY}}}"#),
    ] {
        assert!(record::from_slice::<Observation>(invalid.as_bytes()).is_err());
    }
}

#[test]
fn registry_refuses_ambiguous_or_nonliteral_characterizations() {
    for body in [
        r#"logic:x logic:fragmentCompletenessBound "one", "two" ."#,
        r#"logic:x logic:fragmentBoundaryReason "one", "two" ."#,
        "logic:x logic:fragmentCompletenessBound logic:Other .",
        "logic:x logic:fragmentBoundaryReason logic:Other .",
    ] {
        let source = dataset(body);
        let expected = super::super::record_failure(observe(&source).unwrap_err());
        let mut artifacts = BTreeMap::new();
        record(&source, &mut artifacts).unwrap();
        let observed: Observation = record::from_slice(&artifacts[CHANNEL]).unwrap();
        assert_eq!(observed.0.unwrap_err(), expected, "accepted {body}");
    }
}

#[test]
fn registry_refuses_ambiguous_or_invalid_pattern_targets() {
    for body in [
        "logic:x logic:decidesUnderPattern logic:One, logic:Two .",
        "logic:x logic:decidesUnderPattern <urn:foreign:pattern> .",
        "logic:x logic:decidesUnderPattern [] .",
        r#"logic:x logic:decidesUnderPattern "pattern" ."#,
    ] {
        assert!(observe(&dataset(body)).is_err(), "accepted {body}");
    }
}

#[test]
fn registry_refuses_incomplete_ambiguous_or_semantically_misclassified_admission() {
    for body in [
        "logic:x a logic:SourceAdmissionContract .",
        r#"[] a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement "grammar" ."#,
        r#"<urn:foreign> a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement "grammar" ."#,
        r#"logic:x logic:sourceAdmissionRequirement "untyped" ."#,
        r#"logic:x a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement "one", "two" ."#,
        "logic:x a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement logic:Other .",
        r#"logic:x a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement " " ."#,
        r#"logic:x a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement "grammar"^^<urn:custom-datatype> ."#,
        r#"logic:x a logic:SourceAdmissionContract, logic:DecidedFragment; logic:sourceAdmissionRequirement "grammar" ."#,
        r#"logic:x a logic:SourceAdmissionContract, logic:RefutationPattern; logic:sourceAdmissionRequirement "grammar" ."#,
        r#"logic:x a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement "grammar"; logic:expressivenessBoundary logic:Unsupported ."#,
    ] {
        assert!(observe(&dataset(body)).is_err(), "accepted {body}");
    }
}
