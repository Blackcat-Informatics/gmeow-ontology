// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::code::register_code;
use crate::diag::{Diag, StageId};
use crate::grade::{FindingCategory, Grade, Severity, Standpoint};
use crate::ledger::{DiagLedger, fingerprint_iri};
use crate::model::Location;

fn diag_at(code: &'static str, path: &str, grade: Grade) -> Diag {
    let c = register_code(code);
    Diag::new(c, grade, "msg").with_location(Location {
        path: Some(path.to_owned()),
        ..Location::default()
    })
}

#[test]
fn projected_finding_carries_the_grade_standpoint() {
    // U1: the projected finding carries the gating standpoint truth-axis, so the
    // RDF `gmeow:findingStandpoint` twin (and its SHACL up-set shape) is not
    // vacuous. Without this the gate morphism's binding-standpoint conjunct
    // would have nothing to read on a real projected finding.
    let mut ledger = DiagLedger::new();
    ledger.attach(
        diag_at(
            "test.project.standpoint",
            "s.ttl",
            Grade::new(
                Severity::Error,
                FindingCategory::DataShapeViolation,
                Standpoint::Binding,
            ),
        ),
        StageId::new("s"),
    );
    let finding = &ledger.findings("validate")[0];
    assert_eq!(finding.standpoint, Some(Standpoint::Binding));
    assert_eq!(finding.category, Some(FindingCategory::DataShapeViolation));
}

#[test]
fn projected_finding_carries_labels_as_related_locations() {
    // A multi-anchor witness (e.g. a SHACL result with a result-path / value,
    // or a "defined here / used there" lint) carries its secondary Label spans
    // through the projection as related locations — no secondary anchor is lost
    // to the single primary source context.
    use crate::diag::Label;
    use crate::model::Location as ModelLocation;
    let mut ledger = DiagLedger::new();
    let diag = diag_at(
        "test.project.labels",
        "s.ttl",
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
    )
    .with_label(Label {
        location: ModelLocation {
            logical: Some("path https://ex/p".to_owned()),
            ..ModelLocation::default()
        },
        text: "path".to_owned(),
    });
    ledger.attach(diag, StageId::new("s"));
    let finding = &ledger.findings("validate")[0];
    assert!(
        finding
            .related_locations
            .iter()
            .any(|l| l.logical.as_deref() == Some("path https://ex/p")),
        "a Label span must project to a related location"
    );
    // The label TEXT survives losslessly in related_labels, beside its location —
    // the message the LSP renders as DiagnosticRelatedInformation (the bare
    // related_locations twin above carries no message).
    let related_label = finding
        .related_labels
        .iter()
        .find(|l| l.location.logical.as_deref() == Some("path https://ex/p"))
        .expect("a text-bearing Label must project to a related label");
    assert_eq!(related_label.message, "path");
}

#[test]
fn projected_finding_carries_antecedents_as_related_locations() {
    // U2: a witness with an antecedent edge projects that edge as a related
    // location keyed on the cause's stable finding IRI — the provenance chain
    // Phase-5 remediation and `gmeow explain` consume.
    let mut ledger = DiagLedger::new();
    let cause = diag_at(
        "test.project.cause",
        "cause.ttl",
        Grade::new(
            Severity::Note,
            FindingCategory::ProjectionLoss,
            Standpoint::Perspectival,
        ),
    );
    let cause_ref = ledger.attach(cause, StageId::new("s"));
    let effect = diag_at(
        "test.project.effect",
        "effect.ttl",
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
    )
    .with_antecedents([cause_ref]);
    ledger.attach(effect, StageId::new("s"));

    let cause_iri = {
        // The cause node's stable finding IRI, recomputed from its identity.
        let node = ledger
            .emit_sorted()
            .into_iter()
            .find(|n| n.code == "test.project.cause")
            .expect("cause node present");
        fingerprint_iri(&node.fingerprint)
    };
    let effect_finding = ledger
        .findings("validate")
        .into_iter()
        .find(|f| f.code == "test.project.effect")
        .expect("effect finding present");
    assert!(
        effect_finding
            .related_locations
            .iter()
            .any(|l| l.logical.as_deref() == Some(cause_iri.as_str())),
        "the effect finding must relate to its antecedent cause by finding IRI"
    );
}

#[test]
fn advice_standpoint_and_help_uri_survive_the_projection() {
    // D2a: the lossy advice projection used to keep only `.text`; the faithful
    // projection keeps each advice's standpoint AND outward help URI on the
    // structured `finding.advice`, while the flat `suggestions` text twin still
    // renders for the existing surfaces.
    use crate::diag::Advice;
    use crate::grade::Standpoint;
    let mut ledger = DiagLedger::new();
    let diag = diag_at(
        "test.project.advice",
        "a.ttl",
        Grade::new(
            Severity::Warning,
            FindingCategory::PolicyWarning,
            Standpoint::Advisory,
        ),
    )
    .with_advice(Advice {
        standpoint: Standpoint::Perspectival,
        text: "prefer a rigid sortal".to_owned(),
        help_uri: Some("https://ex/help#sortal".to_owned()),
    });
    ledger.attach(diag, StageId::new("s"));
    let finding = &ledger.findings("validate")[0];
    // Flat text twin preserved (existing renderers).
    assert_eq!(
        finding.suggestions,
        vec!["prefer a rigid sortal".to_owned()]
    );
    // Structured advice keeps the standpoint + help URI the old projection dropped.
    assert_eq!(finding.advice.len(), 1);
    assert_eq!(finding.advice[0].standpoint, Standpoint::Perspectival);
    assert_eq!(
        finding.advice[0].help_uri.as_deref(),
        Some("https://ex/help#sortal")
    );
}

#[test]
fn remediation_projects_onto_the_finding() {
    // D2a: an authored Remediation on the witness rides onto the projected
    // finding's `remediation` (the SARIF-fixes / CLI "how to fix" payload).
    use crate::diag::{ArtifactChange, Region, Remediation};
    use crate::grade::Standpoint;
    let mut ledger = DiagLedger::new();
    let diag = diag_at(
        "test.project.remediation",
        "a.ttl",
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
    )
    .with_remediation(
        Remediation::new(
            "attach both relata through gmeow:mediates",
            Standpoint::Binding,
        )
        .with_artifact_change(ArtifactChange {
            artifact_uri: "core/x.ttl".to_owned(),
            region: Region {
                start_line: Some(12),
                ..Region::default()
            },
            replacement: "gmeow:mediates ex:r .".to_owned(),
        }),
    );
    ledger.attach(diag, StageId::new("s"));
    let finding = &ledger.findings("validate")[0];
    assert_eq!(finding.remediation.len(), 1);
    assert_eq!(
        finding.remediation[0].text,
        "attach both relata through gmeow:mediates"
    );
    assert!(finding.remediation[0].artifact_change.is_some());
}

#[test]
fn projected_subject_and_antecedent_object_iris_close() {
    // D3 join-closure: build two findings where one is the other's antecedent,
    // project, and assert the child's antecedent-edge object IRI textually
    // equals the parent finding's OWN subject IRI (`finding_iri`). This is the
    // equality the declared root-cause / cluster meta-rules match on: subject and
    // antecedent-object must be the SAME blake3 fingerprint IRI.
    let mut ledger = DiagLedger::new();
    let cause = diag_at(
        "test.project.close.cause",
        "cause.ttl",
        Grade::new(
            Severity::Note,
            FindingCategory::ProjectionLoss,
            Standpoint::Perspectival,
        ),
    );
    let cause_ref = ledger.attach(cause, StageId::new("s"));
    let effect = diag_at(
        "test.project.close.effect",
        "effect.ttl",
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
    )
    .with_antecedents([cause_ref]);
    ledger.attach(effect, StageId::new("s"));

    let findings = ledger.findings("validate");
    let cause_finding = findings
        .iter()
        .find(|f| f.code == "test.project.close.cause")
        .expect("cause finding");
    let effect_finding = findings
        .iter()
        .find(|f| f.code == "test.project.close.effect")
        .expect("effect finding");
    // The parent's own canonical subject IRI.
    let parent_subject = cause_finding
        .finding_iri
        .as_deref()
        .expect("ledger witness carries a fingerprint IRI");
    // The child's structured antecedent edge object.
    assert_eq!(
        effect_finding.antecedents,
        vec![parent_subject.to_owned()],
        "the antecedent-edge object IRI must equal the parent finding's subject IRI"
    );
    // And it closes textually with the ledger's fingerprint_iri for the cause.
    let node = ledger
        .emit_sorted()
        .into_iter()
        .find(|n| n.code == "test.project.close.cause")
        .expect("cause node");
    assert_eq!(parent_subject, fingerprint_iri(&node.fingerprint));
}
