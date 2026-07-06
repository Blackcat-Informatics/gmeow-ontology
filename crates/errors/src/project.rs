// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Project the canonical ledger to the existing wire model.
//!
//! The [`DiagLedger`] is the canonical, Rust-owned structure; the [`Report`] /
//! [`Finding`] surface (and, through the renderers, JSON / SARIF / RDF / text /
//! HTML) is a lossy *projection* of it — the same direction as every other
//! projection in the ontology (a finding is a shadow of its witness node, not the
//! other way round).

use crate::ledger::{DiagLedger, DiagNode, fingerprint_iri};
use crate::model::{Finding, Location, Report};

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
        finding.add_location(self.source_ctx.location.clone());
        finding.tags = self.tags.clone();
        finding.attributions = self.attributions.clone();
        finding.suggestions = self.advice.iter().map(|a| a.text.clone()).collect();
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

#[cfg(test)]
mod tests {
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
}
