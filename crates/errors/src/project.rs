// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Project the canonical ledger to the existing wire model.
//!
//! The [`DiagLedger`] is the canonical, Rust-owned structure; the [`Report`] /
//! [`Finding`] surface (and, through the renderers, JSON / SARIF / RDF / text /
//! HTML) is a lossy *projection* of it — the same direction as every other
//! projection in the ontology (a finding is a shadow of its witness node, not the
//! other way round).

use crate::ledger::{DiagLedger, DiagNode};
use crate::model::{Finding, Report};

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
            .with_category(self.grade.category);
        finding.add_location(self.source_ctx.location.clone());
        finding.tags = self.tags.clone();
        finding.attributions = self.attributions.clone();
        finding.suggestions = self.advice.iter().map(|a| a.text.clone()).collect();

        // Extra observations (from merged witnesses) + the source-chain frames go
        // into the detail so nothing is dropped.
        let mut detail_lines: Vec<String> = Vec::new();
        for extra in self.observations.iter().skip(1) {
            detail_lines.push(extra.message.clone());
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
    /// Project the whole ledger to a [`Report`], in the ledger's total
    /// deterministic `(stage, fingerprint)` order. Built from the same
    /// [`findings`](DiagLedger::findings) projection so the two surfaces can never
    /// diverge.
    pub fn project_report(&self, tool: &str) -> Report {
        let mut report = Report::new(tool);
        for finding in self.findings(tool) {
            report.add_finding(finding);
        }
        report
    }

    /// Every finding this ledger projects, in deterministic order — the single
    /// finding-projection surface, produced directly without building an
    /// intermediate [`Report`]. [`project_report`](DiagLedger::project_report)
    /// reuses this.
    pub fn findings(&self, tool: &str) -> Vec<Finding> {
        self.emit_sorted()
            .iter()
            .map(|n| n.to_finding(tool))
            .collect()
    }
}
