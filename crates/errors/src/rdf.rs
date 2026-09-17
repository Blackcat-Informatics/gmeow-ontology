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

use purrdf_core::{LossLedger, RdfDiagnostic, RdfLocation, RdfSeverity};

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

#[path = "rdf.tests.rs"]
#[cfg(test)]
mod tests;
