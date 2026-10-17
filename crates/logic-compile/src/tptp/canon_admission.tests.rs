// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::tptp::CanonAdmissionPublication;

#[test]
/// All original documents and ordinary domain statements stay in the producer
/// observation; byte hydration cannot restore the FOF writer's live authority.
fn canon_admission_roundtrip_preserves_complete_source_without_authorizing_proof() {
    let theory = compile_documents(&[
        ("domain.ttl", "<urn:s> <urn:p> <urn:o> .", None),
        (
            "classes.ttl",
            r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            <urn:union> a logic:Class; logic:unionOf (<urn:a> <urn:b>) .
            <urn:a> a logic:Class . <urn:b> a logic:Class .
        "#,
            None,
        ),
    ]);
    let publication = CanonAdmissionPublication::from_compiled_source(&theory);
    assert_eq!(
        publication.admission().selection,
        SourceSelection::all_classical_roots(&theory)
    );
    assert_eq!(
        project_tptp_fof(&theory, publication.admission()).status,
        FofProjectionStatus::Complete
    );
    let bytes = publication.to_bytes().unwrap();
    let loaded = CanonAdmissionPublication::from_bytes(&bytes).unwrap();
    assert_eq!(loaded.to_bytes().unwrap(), bytes);
    assert!(!loaded.admission().matches(&theory));
    let projection = project_tptp_fof(&theory, loaded.admission());
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(
        loaded
            .admission()
            .units
            .iter()
            .any(|unit| unit.documents.contains("domain.ttl"))
    );
    assert!(
        loaded
            .admission()
            .units
            .iter()
            .any(|unit| unit.documents.contains("classes.ttl"))
    );
    let second_theory = compile_documents(&[
        ("domain.ttl", "<urn:s> <urn:p> <urn:o> .", None),
        (
            "classes.ttl",
            r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            <urn:union> a logic:Class; logic:unionOf (<urn:a> <urn:b>) .
            <urn:a> a logic:Class . <urn:b> a logic:Class .
        "#,
            None,
        ),
    ]);
    assert_eq!(
        CanonAdmissionPublication::from_compiled_source(&second_theory)
            .to_bytes()
            .unwrap(),
        bytes
    );
}

#[test]
/// Unsupported selected owners remain visible rather than disappearing behind
/// a successful producer publication, and changed selection/profile is rejected.
fn canon_admission_keeps_refusals_and_rejects_incompatible_observations() {
    let theory = compile(
        r#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        <urn:domain> <urn:p> <urn:o> .
        <urn:restricted> a logic:Class; logic:onProperty <urn:p>;
            logic:allValuesFrom <http://www.w3.org/2001/XMLSchema#integer> .
    "#,
    );
    let bytes = CanonAdmissionPublication::from_compiled_source(&theory)
        .to_bytes()
        .unwrap();
    let loaded = CanonAdmissionPublication::from_bytes(&bytes).unwrap();
    assert!(loaded.admission().blockers().count() > 0);
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (path, value) in [
        (vec!["schema_version"], serde_json::json!(2)),
        (
            vec!["admission", "profile", "id"],
            serde_json::json!("unknown"),
        ),
        (
            vec!["admission", "selection", "owners"],
            serde_json::json!("None"),
        ),
        (
            vec!["admission", "selection_digest"],
            serde_json::json!("changed"),
        ),
    ] {
        let mut changed = original.clone();
        let mut target = &mut changed;
        for key in path {
            target = &mut target[key];
        }
        *target = value;
        assert!(
            CanonAdmissionPublication::from_bytes(&serde_json::to_vec(&changed).unwrap()).is_err()
        );
    }
    assert!(CanonAdmissionPublication::from_bytes(b"not JSON").is_err());
}
