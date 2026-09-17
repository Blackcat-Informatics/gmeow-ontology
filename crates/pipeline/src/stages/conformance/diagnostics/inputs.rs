// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Controlled diagnostic inputs for producer evaluation against authored rules.

use gmeow_errors::grade::{FindingCategory, Grade, Severity, Standpoint};
use gmeow_errors::{Finding, Report};
use gmeow_ns::{GMEOW_NS, LOGIC_NS};

/// The seeded `gmeow:severity*` individual IRI a [`Severity`] projects to.
pub(super) fn severity_iri(s: Severity) -> String {
    let local = match s {
        Severity::Info => "severityInfo",
        Severity::Note => "severityNote",
        Severity::Warning => "severityWarning",
        Severity::Error => "severityError",
    };
    format!("{GMEOW_NS}{local}")
}

/// The seeded `logic:Finding*` category individual IRI a [`FindingCategory`] projects to.
pub(super) fn category_iri(c: FindingCategory) -> String {
    let local = match c {
        FindingCategory::DataShapeViolation => "FindingDataShapeViolation",
        FindingCategory::ModelingDisciplineViolation => "FindingModelingDisciplineViolation",
        FindingCategory::ContradictionWitness => "FindingContradictionWitness",
        FindingCategory::PermittedEpistemicConflict => "FindingPermittedEpistemicConflict",
        FindingCategory::UnsupportedSemanticFeature => "FindingUnsupportedSemanticFeature",
        FindingCategory::IncompleteCheck => "FindingIncompleteCheck",
        FindingCategory::ProjectionLoss => "FindingProjectionLoss",
        FindingCategory::PolicyWarning => "FindingPolicyWarning",
        FindingCategory::Corroboration => "FindingCorroboration",
        FindingCategory::Transient => "FindingTransientChatter",
    };
    format!("{LOGIC_NS}{local}")
}

/// The seeded `gmeow:standpoint*` individual IRI a [`Standpoint`] projects to.
pub(super) fn standpoint_iri(p: Standpoint) -> String {
    let local = match p {
        Standpoint::Advisory => "standpointAdvisory",
        Standpoint::Perspectival => "standpointPerspectival",
        Standpoint::Binding => "standpointBinding",
    };
    format!("{GMEOW_NS}{local}")
}

/// Every grade in the finite bilattice, each paired with the stable finding IRI
/// that encodes it.
pub(super) fn all_grades() -> Vec<(Grade, String)> {
    let mut out = Vec::new();
    for &s in &Severity::ALL {
        for &c in &FindingCategory::ALL {
            for &p in &Standpoint::ALL {
                let iri = format!(
                    "{GMEOW_NS}examples/diagnostics/gate-conformance/g-{:?}-{:?}-{:?}",
                    s, c, p
                );
                out.push((Grade::new(s, c, p), iri));
            }
        }
    }
    out
}

fn witness_finding(
    iri: &str,
    code: &str,
    category: FindingCategory,
    antecedents: Vec<String>,
) -> Finding {
    let mut finding = Finding::new(Severity::Error, code, "boom")
        .with_tool("shacl")
        .with_category(category);
    finding.finding_iri = Some(iri.to_owned());
    finding.antecedents = antecedents;
    finding
}

fn finding_iri(local: &str) -> String {
    format!("{GMEOW_NS}diagnostics/finding/{local}")
}

pub(super) fn root_report() -> Report {
    let (root, effect_a, effect_b) = (
        finding_iri("rootF1"),
        finding_iri("effectF2"),
        finding_iri("effectF3"),
    );
    let mut report = Report::new("shacl");
    // Two effects that each derive from the ONE childless root.
    report.add_finding(witness_finding(
        &root,
        "discipline/relator-mediation",
        FindingCategory::ModelingDisciplineViolation,
        Vec::new(),
    ));
    report.add_finding(witness_finding(
        &effect_a,
        "shacl.MinCountConstraintComponent",
        FindingCategory::DataShapeViolation,
        vec![root.clone()],
    ));
    report.add_finding(witness_finding(
        &effect_b,
        "shacl.NodeKindConstraintComponent",
        FindingCategory::DataShapeViolation,
        vec![root.clone()],
    ));

    report
}

pub(super) fn glut_report() -> Report {
    let (supported, opposed) = (finding_iri("glutSupported"), finding_iri("glutOpposed"));
    let anchor = format!("{GMEOW_NS}diagnostics/anchor/shared0");

    // Two DIFFERENT-code findings at ONE non-trivial anchor whose category
    // polarities oppose (DataShapeViolation = Supported, PermittedEpistemicConflict
    // = Opposed).
    let mut supported_finding = Finding::new(
        Severity::Error,
        "shacl.MinCountConstraintComponent",
        "supported",
    )
    .with_tool("shacl")
    .with_category(FindingCategory::DataShapeViolation);
    supported_finding.finding_iri = Some(supported.clone());
    supported_finding.anchor_iri = Some(anchor.clone());
    supported_finding.anchor_non_trivial = true;

    let mut opposed_finding = Finding::new(
        Severity::Warning,
        "validate.deep.permitted-conflict",
        "opposed",
    )
    .with_tool("shacl")
    .with_category(FindingCategory::PermittedEpistemicConflict);
    opposed_finding.finding_iri = Some(opposed.clone());
    opposed_finding.anchor_iri = Some(anchor.clone());
    opposed_finding.anchor_non_trivial = true;

    let mut report = Report::new("shacl");
    report.add_finding(supported_finding);
    report.add_finding(opposed_finding);

    report
}
