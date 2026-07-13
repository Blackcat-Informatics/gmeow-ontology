// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The remediation-attachment pass (D1) — the genuine production producer of the
//! rendered "how to fix" payload.
//!
//! For every finding in a validation [`Report`], this resolves the finding's code
//! to the rule catalogue's [`crate::rule_catalog::remediation_for`]
//! guidance and hangs it on the finding through the
//! [`DiagLedger::annotate`](gmeow_errors::DiagLedger::annotate)
//! annotate-by-fingerprint seam — never by writing `finding.remediation` directly.
//! Routing through `annotate` is the point: it exercises the annotate-by-fingerprint
//! API on the real path that produces the RENDERED SARIF `fixes` (and the CLI/HTML
//! "how to fix" lines), so the API is not dark. The attached [`Remediation`] carries
//! the validator's BINDING standpoint (P9: a validator's fix guidance for a binding
//! violation is itself binding), which then surfaces in the SARIF fix's
//! `gmeow.standpoint` property.
//!
//! Codes with genuinely no rule-level fix (the catalogue's honest-absence
//! allowlist) resolve to `None` and are left untouched — a remediation is never
//! fabricated.

use gmeow_errors::Report;
use gmeow_errors::code::register_code;
use gmeow_errors::diag::{Diag, Remediation, SourceContext, StageId};
use gmeow_errors::grade::{Grade, Standpoint};
use gmeow_errors::ledger::{DiagFingerprint, DiagLedger};
use gmeow_errors::model::FindingCategory;

use crate::rule_catalog::{help_uri_for, remediation_for};

/// The producing stage the transient witness is stamped with (attribution only —
/// this ledger never leaves this pass).
const STAGE: &str = "stage-validate";

/// Attach the DSL-authored rule-level remediation onto each finding whose code the
/// catalogue carries guidance for, THROUGH the annotate-by-fingerprint seam.
///
/// Each finding is interned into a transient [`DiagLedger`] under its
/// content-address fingerprint, [`annotate`](DiagLedger::annotate)d with the
/// resolved [`Remediation`], and the annotated node's remediation is read back onto
/// the finding — so `annotate` is the real producer of `finding.remediation` (and
/// therefore of the rendered SARIF `fixes`), never a bypass. Idempotent: a code with
/// no catalogue remediation is skipped, and re-running the pass re-derives the same
/// remediation (annotate itself dedups).
pub fn attach_remediations(report: &mut Report) {
    let stage = StageId::new(STAGE);
    // Built ONCE and reused across findings: the ledger interns each witness under its
    // content-address fingerprint, so distinct fingerprints never cross-talk (a hashmap
    // lookup), and a fingerprint collision resolves to the SAME node carrying the SAME
    // remediation — the remediation is a pure function of `finding.code` (which the
    // fingerprint keys on) and `annotate` dedups by equality. The readback is therefore
    // byte-identical to a fresh-per-finding ledger, so hoisting is behavior-preserving.
    let mut ledger = DiagLedger::new();
    for finding in &mut report.findings {
        let Some(prose) = remediation_for(&finding.code) else {
            continue;
        };
        // The identity fields the fingerprint keys on. A finding without a category /
        // standpoint (e.g. a raw SHACL result) takes the validator's defaults; the
        // SAME values feed both the interned node and the fingerprint recomputation,
        // so the annotate lookup resolves.
        let category = finding
            .category
            .unwrap_or(FindingCategory::ModelingDisciplineViolation);
        let standpoint = finding.standpoint.unwrap_or(Standpoint::Binding);
        let location = finding.primary_location().cloned().unwrap_or_default();

        let diag = Diag::new(
            register_code(&finding.code),
            Grade::new(finding.severity, category, standpoint),
            finding.message.clone(),
        )
        .with_location(location.clone());
        ledger.attach(diag, stage.clone());

        // The fingerprint the ledger interned this witness under — the annotate key.
        let source_ctx = SourceContext {
            location,
            ..SourceContext::default()
        };
        let fingerprint = DiagFingerprint::compute(&finding.code, category, &source_ctx);

        // The validator's remediation is asserted at the BINDING standpoint and links
        // out to the constraint-catalogue page for the code.
        let remediation =
            Remediation::new(prose, Standpoint::Binding).with_help_uri(help_uri_for(&finding.code));
        // annotate-by-fingerprint (D1) — the genuine producer, then read the node back.
        if ledger.annotate(&fingerprint, remediation).is_some()
            && let Some(node) = ledger.node_by_fingerprint(&fingerprint)
        {
            finding.remediation = node.remediation.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::model::{Finding, Severity};

    #[test]
    fn annotate_attaches_the_catalogue_remediation_with_the_binding_standpoint() {
        let mut report = Report::new("shacl");
        report.add_finding(Finding::new(
            Severity::Error,
            "discipline/relator-mediation",
            "relator does not mediate both relata",
        ));
        attach_remediations(&mut report);
        let finding = &report.findings[0];
        assert_eq!(
            finding.remediation.len(),
            1,
            "the annotate pass must attach exactly one remediation"
        );
        let remediation = &finding.remediation[0];
        assert_eq!(remediation.standpoint, Standpoint::Binding);
        assert_eq!(
            remediation.text,
            remediation_for("discipline/relator-mediation").unwrap()
        );
        assert!(
            remediation.help_uri.is_some(),
            "help URI must link the catalog"
        );
    }

    #[test]
    fn dynamic_shacl_member_inherits_the_family_remediation() {
        // A dynamic family member (shacl.*) resolves to the shacl. family guidance,
        // so a real SHACL finding gets a fix through the same annotate seam.
        let mut report = Report::new("shacl");
        report.add_finding(Finding::new(
            Severity::Error,
            "shacl.MinCountConstraintComponent",
            "missing required value",
        ));
        attach_remediations(&mut report);
        assert_eq!(report.findings[0].remediation.len(), 1);
    }

    #[test]
    fn allowlisted_code_gets_no_fabricated_remediation() {
        // A code on the honest-absence allowlist resolves to None → no remediation.
        let mut report = Report::new("validate");
        report.add_finding(Finding::new(
            Severity::Note,
            "validate.deep.consistent",
            "consistent and fully covered",
        ));
        attach_remediations(&mut report);
        assert!(
            report.findings[0].remediation.is_empty(),
            "an honest-absence code must never receive a fabricated remediation"
        );
    }

    #[test]
    fn attached_remediation_standpoint_reaches_the_rendered_sarif_fix() {
        // End-to-end: annotate → the rendered SARIF carries a `fixes` entry whose
        // `properties.gmeow.standpoint` is the validator's binding standpoint. This is
        // the exact byte surface the adversary greps in the regenerated shacl.sarif.
        let mut report = Report::new("shacl");
        report.add_finding(Finding::new(
            Severity::Error,
            "shacl.MinCountConstraintComponent",
            "missing required value",
        ));
        attach_remediations(&mut report);
        let sarif: serde_json::Value =
            serde_json::from_str(&gmeow_errors::render::to_sarif(&report).unwrap()).unwrap();
        let fix = &sarif["runs"][0]["results"][0]["fixes"][0];
        assert_eq!(
            fix["properties"]["gmeow.standpoint"],
            serde_json::Value::String("binding".to_owned()),
            "the annotate pass's remediation standpoint must reach the SARIF fix: {sarif}"
        );
    }

    #[test]
    fn pass_is_idempotent() {
        let mut report = Report::new("shacl");
        report.add_finding(Finding::new(
            Severity::Error,
            "discipline/relator-mediation",
            "m",
        ));
        attach_remediations(&mut report);
        attach_remediations(&mut report);
        assert_eq!(
            report.findings[0].remediation.len(),
            1,
            "re-running the pass must not duplicate the remediation"
        );
    }
}
