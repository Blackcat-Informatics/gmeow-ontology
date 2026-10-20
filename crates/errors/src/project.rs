// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Project the canonical ledger to the existing wire model.
//!
//! The [`DiagLedger`] is the canonical, Rust-owned structure; the [`Report`] /
//! [`Finding`] surface (and, through the renderers, JSON / SARIF / RDF / text /
//! HTML) is a lossy *projection* of it — the same direction as every other
//! projection in the ontology (a finding is a shadow of its witness node, not the
//! other way round).

use crate::ledger::{DiagFingerprint, DiagLedger, DiagNode, anchor_iri, fingerprint_iri};
use crate::model::{Finding, Location, RelatedLabel, Report};

impl DiagNode {
    /// Project this witness node to a wire [`Finding`]. The first observation is
    /// the headline message; any further observations (accumulated on a hash-cons
    /// merge) are folded into the detail so no observation is lost.
    pub fn to_finding(&self, tool: &str) -> Finding {
        let head = self
            .observations
            .first()
            .map(|o| o.message.as_str())
            .unwrap_or("");
        let mut finding = Finding::new(self.grade.severity, self.code.clone(), head)
            .with_tool(tool)
            .with_category(self.grade.category)
            .with_standpoint(self.grade.standpoint);
        let mut location = self.source_ctx.location.clone();
        let distinct_focus = self.source_ctx.focus.as_ref().filter(|focus| {
            location
                .logical
                .as_deref()
                .is_some_and(|logical| logical != focus.0)
        });
        if location.logical.is_none() {
            location.logical = self.source_ctx.focus.as_ref().map(|focus| focus.0.clone());
        }
        finding.add_location(location);
        if let Some(focus) = distinct_focus {
            finding.add_location(Location {
                logical: Some(focus.0.clone()),
                ..Location::default()
            });
        }
        finding.tags = self.tags.clone();
        finding.source_graph = self.source_ctx.graph.clone();
        finding.source_focus = self.source_ctx.focus.as_ref().map(|focus| focus.0.clone());
        finding.attributions = self.attributions.clone();
        // The documented-term attributions (a SHACL violation's constrained property,
        // etc.) ride onto the finding for the docs per-term diagnostics join.
        finding.documented_terms = self.documented_terms.clone();
        // The typed conformance-failure class the violated law declares — the SPECIFIC
        // failure the generic `code` (a shared SHACL component name) cannot name.
        finding.failure_class = self.failure_class.clone();
        finding.cited_iris = self.cited_iris.clone();
        // The flat text-only suggestion twin (kept so the existing suggestion
        // renderers are unchanged) AND the faithful structured projection that
        // preserves each advice's standpoint + outward help URI (the lossy step
        // this replaces dropped both, keeping only `.text`).
        finding.suggestions = self.advice.iter().map(|a| a.text.clone()).collect();
        finding.advice = self.advice.clone();
        // The registry-authored remediations — the "how to fix" payload for SARIF fixes
        // and the CLI/HTML remediation line.
        finding.remediation = self.remediation.clone();
        // Per-term usage guidance (howToUse/useWhen/avoidWhen), joined from the
        // bundle documentation graph — never fabricated, honest absence when the
        // witness's terms author none.
        finding.guidance = self.guidance.clone();
        // The explain-skeleton quad-derivation edges — a SEPARATE edge from the
        // finding-fingerprint antecedents projected just below.
        finding.derived_from_quads = self.derived_from_quads.clone();
        // Project guidance claims and quad-derivation citations as related labels
        // too, so the LSP's `DiagnosticRelatedInformation` surfaces the usage
        // guidance and reasoned-quad provenance alongside the primary message —
        // the same related_labels surface a witness Label rides (the loop below),
        // anchored at the finding's own primary location (an honest reuse: these
        // claims concern the finding as a whole, not a distinct secondary span).
        let primary_location = finding.primary_location().cloned().unwrap_or_default();
        for guidance in &finding.guidance {
            finding.related_labels.push(RelatedLabel {
                location: primary_location.clone(),
                message: format!("{}: {}", guidance.modality.label(), guidance.text),
            });
        }
        for quad_iri in &finding.derived_from_quads {
            finding.related_labels.push(RelatedLabel {
                location: primary_location.clone(),
                message: format!("derived via reasoned quad {quad_iri}"),
            });
        }
        // The canonical fingerprint IRI: the SAME IRI downstream findings' antecedent
        // edges point at, so the projected diagnostic graph's subject and
        // antecedent-object IRIs close (the join the declared meta-rules match on).
        finding.finding_iri = Some(fingerprint_iri(&self.fingerprint));
        // The code-blind source anchor + its non-triviality guard.
        let anchor = DiagFingerprint::anchor(&self.source_ctx);
        finding.anchor_iri = Some(anchor_iri(&anchor));
        finding.anchor_non_trivial = self.source_ctx.is_non_trivial();
        // The provenance-DAG antecedent edges, keyed on each cause's canonical
        // fingerprint IRI (the structured `gmeow:findingAntecedent` twin of the
        // related-location provenance chain emitted just below).
        finding.antecedents = self.antecedents.iter().map(fingerprint_iri).collect();
        // Secondary labelled spans (Rust-compiler-style "defined here" / SHACL
        // result-path / offending value) ride as related locations, so a
        // multi-anchor witness keeps every secondary anchor through the projection
        // instead of collapsing to its primary source context.
        for label in &self.labels {
            if !label.location.is_empty() {
                finding.related_locations.push(label.location.clone());
            }
            // AND the faithful text-bearing twin: keep the label MESSAGE beside its
            // location so a downstream consumer (the LSP's
            // `DiagnosticRelatedInformation`) has the prose, not just the anchor the
            // bare `related_locations` entry above carries. Guarded on a non-empty
            // message so a location-only label rides only as a related location.
            if !label.text.is_empty() {
                finding.related_labels.push(RelatedLabel {
                    location: label.location.clone(),
                    message: label.text.clone(),
                });
            }
        }
        // Project the content-addressed antecedent DAG edges as related locations
        // (the stable finding IRI of each cause), so `gmeow explain`, SARIF
        // relatedLocations, and LSP related-information get the provenance chain for
        // free. Content-addressed by fingerprint, so the projection encodes no arena
        // handle.
        for antecedent in self.antecedents.iter() {
            finding.related_locations.push(Location {
                logical: Some(fingerprint_iri(antecedent)),
                ..Location::default()
            });
        }

        // Extra observations (from merged witnesses) + the source-chain frames go
        // into the detail so nothing is dropped.
        let mut detail_lines: Vec<String> = Vec::new();
        for extra in self.observations.iter().skip(1) {
            detail_lines.push(extra.message.clone());
            // Additional observations share this witness's anchor. Keep them
            // on the labelled surface too: RDF does not project detail text.
            finding.related_labels.push(RelatedLabel {
                location: primary_location.clone(),
                message: extra.message.clone(),
            });
        }
        for frame in &self.frames {
            detail_lines.push(frame.message.clone());
        }
        if self.is_glut() {
            detail_lines.push("contradictory witnesses (glut) at this anchor".to_owned());
        }
        if !detail_lines.is_empty() {
            finding.detail = Some(detail_lines.join("\n"));
        }
        finding
    }
}

impl DiagLedger {
    /// Borrow every observation at a selected severity in deterministic order,
    /// including observations joined under the same witness identity.
    pub fn messages(&self, severity: crate::Severity) -> impl Iterator<Item = &str> {
        self.emit_sorted()
            .into_iter()
            .filter(move |node| node.grade.severity == severity)
            .flat_map(|node| {
                node.observations
                    .iter()
                    .map(|observation| observation.message.as_str())
            })
    }

    /// Project the whole ledger to a [`Report`], in the ledger's total
    /// deterministic `(stage, fingerprint)` order. Built from the same
    /// [`iter_findings`](DiagLedger::iter_findings) projection so the exit surfaces never
    /// diverge.
    pub fn project_report(&self, tool: &str) -> Report {
        let mut report = Report::new(tool);
        for finding in self.iter_findings(tool) {
            report.add_finding(finding);
        }
        report
    }

    /// Every finding this ledger projects, in deterministic order — the single
    /// owned form of [`iter_findings`](DiagLedger::iter_findings), produced directly
    /// without building an intermediate [`Report`].
    pub fn findings(&self, tool: &str) -> Vec<Finding> {
        self.iter_findings(tool).collect()
    }

    /// Project findings only as an exit consumer requests them. Domain reports can
    /// borrow this view without retaining a second finding store beside the ledger.
    pub fn iter_findings<'a>(&'a self, tool: &'a str) -> impl Iterator<Item = Finding> + 'a {
        self.emit_sorted()
            .into_iter()
            .map(move |node| node.to_finding(tool))
    }
}

#[path = "project.tests.rs"]
#[cfg(test)]
mod tests;
