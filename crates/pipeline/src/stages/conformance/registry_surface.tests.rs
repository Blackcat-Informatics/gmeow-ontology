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
    let observed: Result<RegistrySurface, gmeow_errors::RecordedDiag> =
        serde_json::from_slice(&artifacts[CHANNEL]).unwrap();
    assert_eq!(
        observed.unwrap(),
        RegistrySurface {
            pattern_ids: BTreeSet::from(["pattern".to_owned()]),
            decided_ids: BTreeSet::from(["decided".to_owned()]),
            boundary_ids: BTreeSet::from(["boundary".to_owned()]),
            source_admission_ids: BTreeSet::from(["admission".to_owned()]),
            source_admission_requirements: BTreeMap::from([(
                "admission".to_owned(),
                "selected owned list grammar".to_owned()
            )]),
            deciding_patterns: BTreeMap::from([("decided".to_owned(), "pattern".to_owned())]),
            completeness_bounds: BTreeMap::from([(
                "decided".to_owned(),
                "finite fragment".to_owned()
            )]),
            boundary_reasons: BTreeMap::from([(
                "boundary".to_owned(),
                "unbounded fragment".to_owned()
            )]),
        }
    );
}

#[test]
fn registry_refuses_ambiguous_or_nonliteral_characterizations() {
    for body in [
        r#"logic:x logic:fragmentCompletenessBound "one", "two" ."#,
        r#"logic:x logic:fragmentBoundaryReason "one", "two" ."#,
        "logic:x logic:fragmentCompletenessBound logic:Other .",
        "logic:x logic:fragmentBoundaryReason logic:Other .",
    ] {
        assert!(observe(&dataset(body)).is_err(), "accepted {body}");
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
        r#"logic:x a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement "grammar"@en ."#,
        r#"logic:x a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement "grammar"^^<urn:custom-datatype> ."#,
        r#"logic:x a logic:SourceAdmissionContract, logic:DecidedFragment; logic:sourceAdmissionRequirement "grammar" ."#,
        r#"logic:x a logic:SourceAdmissionContract, logic:RefutationPattern; logic:sourceAdmissionRequirement "grammar" ."#,
        r#"logic:x a logic:SourceAdmissionContract; logic:sourceAdmissionRequirement "grammar"; logic:expressivenessBoundary logic:Unsupported ."#,
    ] {
        assert!(observe(&dataset(body)).is_err(), "accepted {body}");
    }
}
