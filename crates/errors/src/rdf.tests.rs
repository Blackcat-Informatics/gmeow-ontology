// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::borrow::Cow;

use super::*;
use crate::grade::{GateVerdict, gate};
use purrdf_core::{DiagnosticParameter, DiagnosticPresentation, DiagnosticValue, LossEntry};

#[test]
fn native_source_preserves_explicit_domain_semantics_and_original_anchor() {
    let native = RdfDiagnostic::new(
        RdfSeverity::Warning,
        "native.warning",
        "original observation",
    )
    .with_detail("original detail")
    .with_location(RdfLocation::logical("native-stage").with_subject("urn:native:subject"));
    let expected = native.clone();
    let code = register_code("gmeow.native.operation");
    let grade = Grade::new(
        Severity::Error,
        FindingCategory::PermittedEpistemicConflict,
        Standpoint::Advisory,
    );
    let graph = purrdf_core::BaseIri::parse("urn:world:test").unwrap();
    let diag = Diag::new(code, grade, "operation observation")
        .with_graph(graph.clone())
        .with_failure_class("urn:failure-class")
        .with_rdf_source(native);
    assert_eq!(diag.code(), code);
    assert_eq!(diag.grade(), grade);
    assert_eq!(diag.inner().message, "operation observation");
    assert_eq!(diag.inner().source_ctx.graph.as_ref(), Some(&graph));
    assert_eq!(
        diag.inner().failure_class.as_deref(),
        Some("urn:failure-class")
    );
    assert_eq!(
        diag.inner().source_ctx.location.logical.as_deref(),
        Some("native-stage")
    );
    assert_eq!(
        diag.inner().source_ctx.focus.as_ref().unwrap().0,
        "urn:native:subject"
    );
    assert_eq!(diag.downcast_ref::<RdfDiagnostic>(), Some(&expected));
    let mut ledger = DiagLedger::new();
    ledger.attach(diag, StageId::new("operation"));
    assert_ne!(ledger.verdict(), GateVerdict::Fatal);
    assert!(
        ledger.project_report("operation").findings[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("original detail")
    );
}

#[test]
fn native_subjects_remain_distinct_from_the_stage_location_through_replay() {
    let mut ledger = DiagLedger::new();
    for subject in ["urn:subject:a", "urn:subject:b"] {
        let mut location = RdfLocation::logical("parse-stage").with_subject(subject);
        location.path = Some("input.gts".to_owned());
        location.line = Some(3);
        location.column = Some(7);
        location.gts_quad_index = Some(0);
        location.gts_frame_index = Some(u64::MAX);
        let native = RdfDiagnostic::new(RdfSeverity::Error, "native.subject", "invalid subject")
            .with_location(location.clone());
        let diag = Diag::from_rdf(native);
        assert_eq!(
            diag.inner().source_ctx.location,
            Location::from_rdf(&location)
        );
        assert_eq!(diag.inner().source_ctx.focus.as_ref().unwrap().0, subject);
        ledger.attach(diag, StageId::new("parse"));
    }
    assert_eq!(ledger.len(), 2);
    let nodes = ledger.emit_sorted();
    assert_ne!(nodes[0].fingerprint, nodes[1].fingerprint);
    let report = ledger.project_report("native");
    for finding in &report.findings {
        assert_eq!(
            finding.primary_location().unwrap().logical.as_deref(),
            Some("parse-stage")
        );
        assert!(finding.locations.iter().any(|location| {
            matches!(
                location.logical.as_deref(),
                Some("urn:subject:a" | "urn:subject:b")
            )
        }));
    }
    let serialized = serde_json::to_vec(&nodes).unwrap();
    let mut replay = DiagLedger::new();
    replay.replay(serde_json::from_slice::<Vec<crate::DiagNode>>(&serialized).unwrap());
    assert_eq!(
        crate::render::to_json(&replay.project_report("native")).unwrap(),
        crate::render::to_json(&report).unwrap()
    );
}

#[test]
fn native_presentation_remains_typed_until_cause_lowering() {
    let detail = DiagnosticPresentation::new(
        "native.address.detail",
        "wire offset {offset}",
        vec![DiagnosticParameter::new(
            "offset",
            DiagnosticValue::Unsigned(u64::MAX),
        )],
    )
    .unwrap();
    let presentation = DiagnosticPresentation::new(
        "native.address",
        "invalid subject {subject}",
        vec![DiagnosticParameter::new(
            "subject",
            DiagnosticValue::Text("urn:subject:a".to_owned()),
        )],
    )
    .unwrap()
    .with_detail(detail)
    .unwrap();
    let native = RdfDiagnostic::new(RdfSeverity::Error, "native.address", "")
        .with_presentation(presentation)
        .with_location(RdfLocation::logical("parse-stage").with_subject("urn:subject:a"));
    let mut ledger = DiagLedger::new();
    let diag = Diag::from_rdf(native.clone());
    let cause = diag
        .downcast_ref::<RdfDiagnostic>()
        .expect("original native type retained");
    assert_eq!(cause, &native);
    let presentation = cause.presentation().unwrap();
    assert_eq!(presentation.message_id(), "native.address");
    assert_eq!(
        presentation.detail().unwrap().parameters()[0].value(),
        &DiagnosticValue::Unsigned(u64::MAX)
    );
    ledger.attach(diag, StageId::new("parse"));
    let nodes = ledger.emit_sorted();
    assert_eq!(nodes[0].frames.len(), 1);
    assert_eq!(nodes[0].frames[0].message, native.to_string());
    let report = ledger.project_report("native");
    assert!(
        report.findings[0]
            .detail
            .as_deref()
            .unwrap()
            .contains(&format!("wire offset {}", u64::MAX))
    );
    let serialized = serde_json::to_vec(&nodes).unwrap();
    let mut replay = DiagLedger::new();
    replay.replay(serde_json::from_slice::<Vec<crate::DiagNode>>(&serialized).unwrap());
    assert_eq!(
        crate::render::to_json(&replay.project_report("native")).unwrap(),
        crate::render::to_json(&report).unwrap()
    );
}

fn diagnostic_with_dropped_lang_tag() -> RdfDiagnostic {
    RdfDiagnostic::new(
        RdfSeverity::Warning,
        "lang.projection",
        "language tag dropped",
    )
}

fn losses_with_dropped_lang_tag() -> LossLedger {
    let mut losses = LossLedger::new();
    losses.record(LossEntry {
        code: Cow::Borrowed("dropped-language-tag"),
        from: Cow::Borrowed("rdf-1.2-dataset"),
        to: Cow::Borrowed("fixture-codec"),
        note: Cow::Borrowed("the @en language tag was dropped by the target codec"),
        location: None,
    });
    losses
}

#[test]
fn standalone_projection_loss_retains_original_direction_and_coordinates() {
    let loss = LossEntry {
        code: Cow::Borrowed("schema.unrepresentable"),
        from: Cow::Borrowed("shacl"),
        to: Cow::Borrowed("json-schema"),
        note: Cow::Borrowed("cross-node constraint is not representable"),
        location: Some(Box::new(RdfLocation {
            path: Some("authored.ttl".into()),
            line: Some(0),
            column: Some(0),
            gts_quad_index: Some(0),
            ..RdfLocation::logical("constraint").with_subject("urn:constraint")
        })),
    };
    let diag = Diag::from_rdf_loss(loss.clone());
    let original = &diag.downcast_ref::<NativeLoss>().unwrap().0;
    assert_eq!(original.code, loss.code);
    assert_eq!(original.from, loss.from);
    assert_eq!(original.to, loss.to);
    assert_eq!(original.note, loss.note);
    assert_eq!(original.location, loss.location);
    assert_eq!(diag.inner().source_ctx.location.line, Some(0));
    assert_eq!(diag.inner().source_ctx.location.column, Some(0));
    assert_eq!(diag.inner().source_ctx.location.gts_quad_index, Some(0));
    assert_eq!(
        diag.inner().source_ctx.focus.as_ref().unwrap().0,
        "urn:constraint"
    );
    let mut ledger = DiagLedger::new();
    ledger.attach(diag, StageId::new("stage-export-json-schema"));
    assert_ne!(ledger.verdict(), GateVerdict::Fatal);
    let finding = &ledger.project_report("json-schema").findings[0];
    let detail = finding.detail.as_ref().unwrap();
    assert!(detail.contains("shacl → json-schema"));
    assert!(detail.contains(loss.note.as_ref()));
}

#[test]
fn loss_subjects_remain_distinct_witnesses() {
    let mut losses = LossLedger::new();
    for subject in ["urn:subject:a", "urn:subject:b"] {
        losses.record(LossEntry {
            code: Cow::Borrowed("dropped-language-tag"),
            from: Cow::Borrowed("rdf-1.2-dataset"),
            to: Cow::Borrowed("fixture-codec"),
            note: Cow::Borrowed("language tag dropped"),
            location: Some(Box::new(
                RdfLocation::logical("projection-stage").with_subject(subject),
            )),
        });
    }
    let mut ledger = DiagLedger::new();
    let parent = Diag::from_rdf_with_losses(
        diagnostic_with_dropped_lang_tag(),
        &losses,
        &mut ledger,
        StageId::new("project"),
    );
    assert_eq!(ledger.len(), 2);
    ledger.attach(parent, StageId::new("project"));
    let nodes = ledger.emit_sorted();
    let parent = nodes
        .iter()
        .find(|node| node.code == "lang.projection")
        .unwrap();
    assert_eq!(parent.antecedents.len(), 2);
    assert_ne!(parent.antecedents[0], parent.antecedents[1]);
    for child in nodes
        .iter()
        .filter(|node| node.code == "dropped-language-tag")
    {
        assert_eq!(
            child.source_ctx.location.logical.as_deref(),
            Some("projection-stage")
        );
        assert!(matches!(
            child.source_ctx.focus.as_ref().unwrap().0.as_str(),
            "urn:subject:a" | "urn:subject:b"
        ));
    }
}

#[test]
fn losses_become_non_gating_projection_loss_children() {
    let mut ledger = DiagLedger::new();
    let parent = Diag::from_rdf_with_losses(
        diagnostic_with_dropped_lang_tag(),
        &losses_with_dropped_lang_tag(),
        &mut ledger,
        StageId::new("ingest"),
    );
    // One child witness was attached to the ledger.
    assert_eq!(ledger.len(), 1);
    let child_fingerprint = ledger.emit_sorted()[0].fingerprint;
    let child = ledger.emit_sorted()[0];
    assert_eq!(child.grade.category, FindingCategory::ProjectionLoss);
    // A projection loss never gates.
    assert_eq!(gate(child.grade), GateVerdict::Collected);
    // The live parent references the child as a DAG antecedent (one handle).
    assert_eq!(parent.inner().antecedents.len(), 1);

    // After attaching the parent, its node carries the child's fingerprint as a
    // content-addressed edge.
    ledger.attach(parent, StageId::new("ingest"));
    let parent_node = ledger
        .emit_sorted()
        .into_iter()
        .find(|n| n.code == "lang.projection")
        .expect("parent node present");
    assert_eq!(parent_node.antecedents.len(), 1);
    assert_eq!(parent_node.antecedents[0], child_fingerprint);
}

#[test]
fn double_lowering_across_two_stages_is_idempotent() {
    // R4: the same RDF diagnostic ingested at two DIFFERENT stages hash-conses
    // to one node (content address is identity, stage is not in the
    // fingerprint), its stage resolves deterministically to the lexicographic
    // minimum, and its frames are not doubled.
    let d = diagnostic_with_dropped_lang_tag();
    let losses = losses_with_dropped_lang_tag();
    let mut ledger = DiagLedger::new();
    let p_a = Diag::from_rdf_with_losses(d.clone(), &losses, &mut ledger, StageId::new("stage-a"));
    ledger.attach(p_a, StageId::new("stage-a"));
    let before = ledger.emit_sorted().len();
    let p_b = Diag::from_rdf_with_losses(d, &losses, &mut ledger, StageId::new("stage-b"));
    ledger.attach(p_b, StageId::new("stage-b"));
    let after = ledger.emit_sorted().len();
    // Identical content across stages hash-conses — no growth, no doubled frames.
    assert_eq!(before, after);
    // Cross-stage attribution is resolved deterministically to the min stage,
    // not dropped-by-first-writer.
    for node in ledger.emit_sorted() {
        assert_eq!(
            node.stage.as_str(),
            "stage-a",
            "merged stage must be the lexicographic minimum, not the first writer"
        );
    }
    for node in ledger.emit_sorted() {
        // Frames never accumulate duplicates across re-ingestion.
        let mut seen = std::collections::HashSet::new();
        for f in &node.frames {
            assert!(
                seen.insert(f.message.clone()),
                "frame doubled: {}",
                f.message
            );
        }
    }
}
