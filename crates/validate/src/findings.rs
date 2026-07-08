// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bridge from the structured RDF/SHACL diagnostics into the canonical
//! [`gmeow_errors::Finding`] model.
//!
//! `gmeow-errors` links pyo3 unconditionally, so the PyO3-free
//! `gmeow-rdf` kernel must not depend on it. `gmeow-validate` already links
//! pyo3 and depends on both crates, so the conversion lives here. The Rust
//! orphan rules forbid `impl From<RdfDiagnostic> for Finding` in this crate
//! (it owns neither type), hence these are plain named functions.
//!
//! The whole point is *carry-through*: the GTS wire coordinates that
//! [`purrdf::RdfLocation`] records and the focus/path/shape structure that a
//! SHACL result carries survive into the [`Finding`], so SARIF, the `gmeow:`
//! RDF projection, and the content-addressed cache all anchor to the same
//! position inside a bundle.

use gmeow_errors::code::register_code;
use gmeow_errors::diag::{Diag, Label};
use gmeow_errors::grade::{Grade, Standpoint};
use gmeow_errors::model::FindingCategory;
use gmeow_errors::{Finding, Location, Severity};
use purrdf::shapes::report::{Severity as ShaclSeverity, ValidationResult};

/// Normalize a SHACL [`ShaclSeverity`] to the canonical diagnostics [`Severity`].
///
/// `sh:Violation` is the SHACL gate-failing level, so it maps to `error`.
fn severity_from_shacl(severity: &ShaclSeverity) -> Severity {
    match severity {
        ShaclSeverity::Violation => Severity::Error,
        ShaclSeverity::Warning => Severity::Warning,
        ShaclSeverity::Info => Severity::Info,
        // A custom `sh:severity` IRI purrdf preserves verbatim. gmeow's gate
        // treats an unrecognized severity as gate-failing (fail-closed).
        ShaclSeverity::Other(_) => Severity::Error,
    }
}

/// The local name of an IRI string (the part after the last `#` or `/`), used
/// to build stable, short diagnostic codes from constraint-component IRIs.
fn iri_local(iri: &str) -> &str {
    iri.rsplit(['#', '/'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(iri)
}

/// Strip the angle brackets oxigraph's N-Triples `Display` wraps around IRIs, so
/// a focus node / path / value stored in a [`Location`] is the bare IRI (its
/// identity), not its serialization. This keeps the SARIF projection (where a
/// bracketed URI is invalid), the flat JSON report, and the `gmeow:` RDF graph
/// all anchored on the same clean identifier. Blank nodes and literals lack the
/// brackets and pass through unchanged.
fn strip_angle(term: &str) -> &str {
    term.strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(term)
}

/// Convert a SHACL [`ValidationResult`] into a canonical [`Finding`].
///
/// The focus node becomes the primary (logical) location; the result path and
/// offending value become related locations; the source shape rides in the
/// detail field. The code is `shacl.<ConstraintComponentLocalName>` so SARIF
/// rules stay stable and short.
pub fn finding_from_shacl(result: &ValidationResult) -> Finding {
    let code = format!(
        "{}{}",
        crate::codes::SHACL_FAMILY,
        iri_local(result.source_constraint_component.as_str())
    );
    let message = result
        .message
        .clone()
        .unwrap_or_else(|| "SHACL constraint violated".to_owned());
    let mut finding =
        Finding::new(severity_from_shacl(&result.severity), code, message).with_tool("shacl");

    finding.add_location(Location {
        logical: Some(strip_angle(&result.focus_node.to_string()).to_owned()),
        ..Location::default()
    });

    if let Some(path) = &result.result_path {
        finding.related_locations.push(Location {
            logical: Some(format!("path {}", strip_angle(&path.to_string()))),
            ..Location::default()
        });
    }
    if let Some(value) = &result.value {
        finding.related_locations.push(Location {
            logical: Some(format!("value {}", strip_angle(&value.to_string()))),
            ..Location::default()
        });
    }
    finding.detail = Some(format!(
        "source shape: {}",
        strip_angle(&result.source_shape.to_string())
    ));
    finding
}

/// The gating STANDPOINT a SHACL result of a given severity asserts — the leg the
/// `logic:ruleGateFatalVerdict` up-set derivation reads alongside severity and the
/// category's blocking projection. This mirrors `gmeow_errors::rdf::default_standpoint`
/// (the mapping the run-ledger's `Diag::from_rdf` fold uses), so a routed SHACL finding
/// carries the SAME standpoint the forward run-ledger node does: an `sh:Violation`
/// (Error) is Binding — the only standpoint that can join the gate-fatal up-set — while
/// warnings/info are non-binding and never gate.
fn standpoint_from_shacl(severity: Severity) -> Standpoint {
    match severity {
        Severity::Error => Standpoint::Binding,
        Severity::Warning => Standpoint::Perspectival,
        Severity::Note | Severity::Info => Standpoint::Advisory,
    }
}

/// Lower a SHACL [`ValidationResult`] into a canonical [`Diag`] — the ledger-native
/// twin of [`finding_from_shacl`].
///
/// Where [`finding_from_shacl`] hand-builds a wire [`Finding`] carrying NO content
/// address, this builds a [`Diag`] the [`DiagLedger`](gmeow_errors::DiagLedger) interns,
/// so the projected finding gains a stable blake3 `finding_iri` AND a code-blind
/// `anchor_iri` (with `anchor_non_trivial`), the cross-node-glut join key. The mapping
/// is faithful to `finding_from_shacl`:
///
/// * the focus node rides in the [`SourceContext`](gmeow_errors::diag::SourceContext)'s
///   `location.logical` — the SAME field `finding_from_shacl` uses for the primary
///   location (so span-enrichment's bare-IRI join still matches) AND the field the
///   anchor fingerprint keys on (so a real focus node is a `gmeow:NonTrivialAnchor`);
/// * the result path and offending value ride as secondary [`Label`]s, projected to the
///   finding's related locations (the SARIF/JSON secondary anchors);
/// * the source shape rides as a context frame, projected into the finding detail;
/// * the category is [`DataShapeViolation`](FindingCategory::DataShapeViolation) — the
///   honest SHACL kind (matching `report_bridge`), whose `Supported` polarity makes the
///   `gmeow:categoryPolarity` join the meta-rules read correct — and the standpoint is
///   [`standpoint_from_shacl`].
///
/// SHACL violations are independent (no antecedent DAG among them), so the built diag
/// carries no antecedents — anchor + grade only.
pub fn diag_from_shacl(result: &ValidationResult) -> Diag {
    let code = format!(
        "{}{}",
        crate::codes::SHACL_FAMILY,
        iri_local(result.source_constraint_component.as_str())
    );
    let message = result
        .message
        .clone()
        .unwrap_or_else(|| "SHACL constraint violated".to_owned());
    let severity = severity_from_shacl(&result.severity);
    let grade = Grade::new(
        severity,
        FindingCategory::DataShapeViolation,
        standpoint_from_shacl(severity),
    );
    let mut diag = Diag::new(register_code(&code), grade, message).with_location(Location {
        logical: Some(strip_angle(&result.focus_node.to_string()).to_owned()),
        ..Location::default()
    });
    if let Some(path) = &result.result_path {
        diag = diag.with_label(Label {
            location: Location {
                logical: Some(format!("path {}", strip_angle(&path.to_string()))),
                ..Location::default()
            },
            text: "path".to_owned(),
        });
    }
    if let Some(value) = &result.value {
        diag = diag.with_label(Label {
            location: Location {
                logical: Some(format!("value {}", strip_angle(&value.to_string()))),
                ..Location::default()
            },
            text: "value".to_owned(),
        });
    }
    // The source shape rides as a context frame so the projection folds it into the
    // finding detail — the SAME `source shape: <iri>` string `finding_from_shacl` sets.
    diag.with_context(format!(
        "source shape: {}",
        strip_angle(&result.source_shape.to_string())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::{DiagLedger, StageId};
    use purrdf::shapes::term::{NamedNode, Term};

    #[test]
    fn shacl_result_carries_focus_node_and_component() {
        let result = ValidationResult {
            focus_node: Term::NamedNode(NamedNode::new_unchecked("https://ex/a")),
            result_path: Some(Term::NamedNode(NamedNode::new_unchecked("https://ex/p"))),
            path_structure: None,
            value: None,
            source_constraint_component: NamedNode::new_unchecked(
                "http://www.w3.org/ns/shacl#MinCountConstraintComponent",
            ),
            source_shape: Term::NamedNode(NamedNode::new_unchecked("https://ex/shape")),
            severity: ShaclSeverity::Violation,
            message: Some("missing required property".to_owned()),
            source_box_roles: Vec::new(),
            path_box_roles: Vec::new(),
            result_box_roles: Vec::new(),
            attributions: vec![],
        };

        let finding = finding_from_shacl(&result);

        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.code, "shacl.MinCountConstraintComponent");
        // IRIs are stored bare (identity, not N-Triples serialization), so the
        // SARIF projection emits a valid `artifactLocation.uri` (a bracketed
        // `<https://…>` is rejected by GitHub code-scanning).
        assert_eq!(
            finding
                .primary_location()
                .and_then(|l| l.logical.as_deref()),
            Some("https://ex/a")
        );
        assert!(
            finding
                .related_locations
                .iter()
                .any(|l| l.logical.as_deref() == Some("path https://ex/p"))
        );
        assert_eq!(
            finding.detail.as_deref(),
            Some("source shape: https://ex/shape")
        );
    }

    fn min_count_result(focus: &str) -> ValidationResult {
        ValidationResult {
            focus_node: Term::NamedNode(NamedNode::new_unchecked(focus)),
            result_path: Some(Term::NamedNode(NamedNode::new_unchecked("https://ex/p"))),
            path_structure: None,
            value: None,
            source_constraint_component: NamedNode::new_unchecked(
                "http://www.w3.org/ns/shacl#MinCountConstraintComponent",
            ),
            source_shape: Term::NamedNode(NamedNode::new_unchecked("https://ex/shape")),
            severity: ShaclSeverity::Violation,
            message: Some("missing required property".to_owned()),
            source_box_roles: Vec::new(),
            path_box_roles: Vec::new(),
            result_box_roles: Vec::new(),
            attributions: vec![],
        }
    }

    #[test]
    fn routed_shacl_finding_carries_finding_iri_and_nontrivial_anchor() {
        // The production path: route the SHACL result through a `DiagLedger` and read
        // back the projected finding. It must carry the stable blake3 identity the
        // hand-built `Finding` lacked — a non-empty `finding_iri`, an `anchor_iri`, and
        // `anchor_non_trivial == true` (the focus node IS a genuine, joinable anchor) —
        // so the cross-node-glut meta-rule has an anchor to join on. Same grade shape:
        // an `sh:Violation` is a Binding DataShapeViolation (the gate-fatal up-set leg).
        let mut ledger = DiagLedger::new();
        ledger.attach(
            diag_from_shacl(&min_count_result("https://ex/a")),
            StageId::new("stage-validate"),
        );
        let findings = ledger.findings("shacl");
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.code, "shacl.MinCountConstraintComponent");
        assert_eq!(finding.category, Some(FindingCategory::DataShapeViolation));
        assert_eq!(finding.standpoint, Some(Standpoint::Binding));
        // The focus node still rides in the primary location's logical anchor — the
        // bare-IRI join key span-enrichment matches on — AND it is the finding's anchor.
        assert_eq!(
            finding
                .primary_location()
                .and_then(|l| l.logical.as_deref()),
            Some("https://ex/a")
        );
        let finding_iri = finding
            .finding_iri
            .as_deref()
            .expect("a routed finding carries a blake3 finding IRI");
        assert!(
            finding_iri.starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/finding/")
        );
        assert!(
            finding
                .anchor_iri
                .as_deref()
                .expect("a routed finding carries an anchor IRI")
                .starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/anchor/")
        );
        assert!(
            finding.anchor_non_trivial,
            "a real focus node is a NonTrivial anchor the glut join can fire on"
        );
        // The result path survives as a secondary related location, the source shape as
        // detail — no SHACL structure is dropped by routing through the ledger.
        assert!(
            finding
                .related_locations
                .iter()
                .any(|l| l.logical.as_deref() == Some("path https://ex/p"))
        );
        assert_eq!(
            finding.detail.as_deref(),
            Some("source shape: https://ex/shape")
        );
    }

    #[test]
    fn routed_shacl_findings_at_distinct_foci_get_distinct_anchors() {
        // Two violations of the SAME constraint component on DIFFERENT focus nodes are
        // distinct witnesses at distinct anchors — the ledger does not collapse them,
        // and their code-blind anchor IRIs differ (the join key is per-focus).
        let mut ledger = DiagLedger::new();
        ledger.attach(
            diag_from_shacl(&min_count_result("https://ex/a")),
            StageId::new("stage-validate"),
        );
        ledger.attach(
            diag_from_shacl(&min_count_result("https://ex/b")),
            StageId::new("stage-validate"),
        );
        let findings = ledger.findings("shacl");
        assert_eq!(findings.len(), 2, "distinct foci → distinct witnesses");
        assert_ne!(findings[0].anchor_iri, findings[1].anchor_iri);
    }
}
