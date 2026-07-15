// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The purrdf ingestion boundary.
//!
//! An [`RdfDiagnostic`] carries severity, code, message, and optional location;
//! conversion losses live in PurRDF's unified [`LossLedger`]. This module projects
//! both structures into the substrate: the diagnostic becomes a [`Diag`], its
//! [`RdfLocation`] becomes a [`Location`] (every GTS wire coordinate preserved),
//! and each loss becomes a `ProjectionLoss`-graded **child witness** attached to
//! the diagnostic ledger, so the loss evidence is a DAG of witnesses — not flat
//! text. This subsumes the bespoke
//! `finding_from_rdf`/`location_from_rdf` helpers that previously lived in
//! `gmeow-validate` (the orphan-rule detour is gone now that both the RDF types'
//! consumer and the diagnostics model live in one leaf crate).

use purrdf::{LossLedger, RdfDiagnostic, RdfLocation, RdfSeverity};

use crate::code::register_code;
use crate::diag::{Diag, StageId};
use crate::grade::{FindingCategory, Grade, Severity, Standpoint};
use crate::ledger::DiagLedger;
use crate::model::Location;

/// Normalize an [`RdfSeverity`] to the canonical [`Severity`].
pub fn severity_from_rdf(severity: RdfSeverity) -> Severity {
    match severity {
        RdfSeverity::Error => Severity::Error,
        RdfSeverity::Warning => Severity::Warning,
        RdfSeverity::Note => Severity::Note,
        RdfSeverity::Info => Severity::Info,
    }
}

/// The default category for an ingested RDF diagnostic of a given severity. An
/// error is a blocking structural defect; anything softer is a non-gating policy
/// note. A producer with more context can override the grade after ingestion.
fn default_category(severity: Severity) -> FindingCategory {
    match severity {
        Severity::Error => FindingCategory::ModelingDisciplineViolation,
        _ => FindingCategory::PolicyWarning,
    }
}

/// The default standpoint for an ingested RDF diagnostic of a given severity.
fn default_standpoint(severity: Severity) -> Standpoint {
    match severity {
        Severity::Error => Standpoint::Binding,
        Severity::Warning => Standpoint::Perspectival,
        Severity::Note | Severity::Info => Standpoint::Advisory,
    }
}

impl Location {
    /// Project an [`RdfLocation`] into a [`Location`], preserving every GTS wire
    /// coordinate (`usize` on the RDF side becomes the portable `u64` the
    /// diagnostics model serializes). Subsumes `validate::findings::location_from_rdf`.
    pub fn from_rdf(location: &RdfLocation) -> Location {
        let mut out = Location::new(
            location.path.clone(),
            location.line,
            location.column,
            location.logical.clone(),
        );
        if let Some(term_id) = location.gts_term_id {
            out = out.with_gts_term(term_id as u64);
        }
        if let Some(quad_index) = location.gts_quad_index {
            out = out.with_gts_quad(quad_index as u64);
        }
        if let Some(reifier_id) = location.gts_reifier_id {
            out = out.with_gts_reifier(reifier_id as u64);
        }
        if let Some(frame_index) = location.gts_frame_index {
            out = out.with_gts_frame(frame_index as u64);
        }
        if let Some(segment_index) = location.gts_segment_index {
            out = out.with_gts_segment(segment_index as u64);
        }
        out
    }
}

impl Diag {
    /// Project a purrdf [`RdfDiagnostic`] without conversion losses into a [`Diag`].
    /// The parent itself is returned unattached — the caller attaches it.
    pub fn from_rdf(diagnostic: &RdfDiagnostic, ledger: &mut DiagLedger, stage: StageId) -> Diag {
        Self::from_rdf_with_losses(diagnostic, &LossLedger::new(), ledger, stage)
    }

    /// Project a purrdf [`RdfDiagnostic`] and its unified [`LossLedger`] into a
    /// [`Diag`]. Each loss is attached to `ledger` as a non-gating
    /// `ProjectionLoss` child witness, and the returned parent carries those
    /// children as DAG antecedents.
    pub fn from_rdf_with_losses(
        diagnostic: &RdfDiagnostic,
        losses: &LossLedger,
        ledger: &mut DiagLedger,
        stage: StageId,
    ) -> Diag {
        // Losses become ProjectionLoss child witnesses, attached first so the
        // parent's antecedent handles are already resident.
        let mut antecedents = Vec::with_capacity(losses.entries().len());
        for loss in losses.entries() {
            let mut child = Diag::new(
                register_code(&loss.code),
                Grade::new(
                    Severity::Note,
                    FindingCategory::ProjectionLoss,
                    Standpoint::Perspectival,
                ),
                loss.note.to_string(),
            );
            if let Some(location) = &loss.location {
                child = child.with_location(Location::from_rdf(location));
            }
            antecedents.push(ledger.attach(child, stage.clone()));
        }

        let severity = severity_from_rdf(diagnostic.severity);
        let mut parent = Diag::new(
            register_code(&diagnostic.code),
            Grade::new(
                severity,
                default_category(severity),
                default_standpoint(severity),
            ),
            diagnostic.message.clone(),
        )
        .with_antecedents(antecedents);
        if let Some(location) = &diagnostic.location {
            parent = parent.with_location(Location::from_rdf(location));
        }
        parent
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::grade::{GateVerdict, gate};
    use purrdf::LossEntry;

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
}
