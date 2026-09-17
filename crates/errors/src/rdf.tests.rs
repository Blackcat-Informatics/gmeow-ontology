// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::borrow::Cow;

use super::*;
use crate::grade::{GateVerdict, gate};
use purrdf_core::LossEntry;

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
fn losses_become_non_gating_projection_loss_children() {
    let mut ledger = DiagLedger::new();
    let parent = Diag::from_rdf_with_losses(
        &diagnostic_with_dropped_lang_tag(),
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
    let p_a = Diag::from_rdf_with_losses(&d, &losses, &mut ledger, StageId::new("stage-a"));
    ledger.attach(p_a, StageId::new("stage-a"));
    let before = ledger.emit_sorted().len();
    let p_b = Diag::from_rdf_with_losses(&d, &losses, &mut ledger, StageId::new("stage-b"));
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
