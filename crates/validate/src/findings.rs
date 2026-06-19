// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bridge from the structured RDF/SHACL diagnostics into the canonical
//! [`gmeow_diagnostics::Finding`] model (#654).
//!
//! `gmeow-diagnostics` links pyo3 unconditionally, so the PyO3-free
//! `gmeow-rdf` kernel must not depend on it. `gmeow-validate` already links
//! pyo3 and depends on both crates, so the conversion lives here. The Rust
//! orphan rules forbid `impl From<RdfDiagnostic> for Finding` in this crate
//! (it owns neither type), hence these are plain named functions.
//!
//! The whole point is *carry-through*: the GTS wire coordinates that
//! [`gmeow_rdf::RdfLocation`] records and the focus/path/shape structure that a
//! SHACL result carries survive into the [`Finding`], so SARIF, the `gmeow:`
//! RDF projection, and the content-addressed cache all anchor to the same
//! position inside a bundle.

use gmeow_diagnostics::{Finding, Location, Severity};
use gmeow_rdf::{RdfDiagnostic, RdfLocation, RdfSeverity};
use gmeow_shacl::report::{Severity as ShaclSeverity, ValidationResult};

/// Normalize an [`RdfSeverity`] to the canonical diagnostics [`Severity`].
fn severity_from_rdf(severity: RdfSeverity) -> Severity {
    match severity {
        RdfSeverity::Error => Severity::Error,
        RdfSeverity::Warning => Severity::Warning,
        RdfSeverity::Note => Severity::Note,
        RdfSeverity::Info => Severity::Info,
    }
}

/// Normalize a SHACL [`ShaclSeverity`] to the canonical diagnostics [`Severity`].
///
/// `sh:Violation` is the SHACL gate-failing level, so it maps to `error`.
fn severity_from_shacl(severity: ShaclSeverity) -> Severity {
    match severity {
        ShaclSeverity::Violation => Severity::Error,
        ShaclSeverity::Warning => Severity::Warning,
        ShaclSeverity::Info => Severity::Info,
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

/// Convert an [`RdfLocation`] into a diagnostics [`Location`], preserving every
/// GTS wire coordinate (`usize` on the RDF side becomes the portable `u64` the
/// diagnostics model serializes).
pub fn location_from_rdf(location: &RdfLocation) -> Location {
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

/// Convert an [`RdfDiagnostic`] into a canonical [`Finding`].
///
/// Conversion losses recorded on the diagnostic become related locations and
/// human-readable suggestions, so nothing the adapter knew is dropped.
pub fn finding_from_rdf(diagnostic: &RdfDiagnostic) -> Finding {
    let mut finding = Finding::new(
        severity_from_rdf(diagnostic.severity),
        diagnostic.code.clone(),
        diagnostic.message.clone(),
    )
    .with_tool("rdf");
    finding.detail = diagnostic.detail.clone();
    if let Some(location) = &diagnostic.location {
        finding.add_location(location_from_rdf(location));
    }
    for loss in &diagnostic.losses {
        finding
            .suggestions
            .push(format!("{}: {}", loss.code, loss.message));
        if let Some(location) = &loss.location {
            let related = location_from_rdf(location);
            if !related.is_empty() {
                finding.related_locations.push(related);
            }
        }
    }
    finding
}

/// Convert a SHACL [`ValidationResult`] into a canonical [`Finding`].
///
/// The focus node becomes the primary (logical) location; the result path and
/// offending value become related locations; the source shape rides in the
/// detail field. The code is `shacl.<ConstraintComponentLocalName>` so SARIF
/// rules stay stable and short.
pub fn finding_from_shacl(result: &ValidationResult) -> Finding {
    let code = format!(
        "shacl.{}",
        iri_local(result.source_constraint_component.as_str())
    );
    let message = result
        .message
        .clone()
        .unwrap_or_else(|| "SHACL constraint violated".to_owned());
    let mut finding =
        Finding::new(severity_from_shacl(result.severity), code, message).with_tool("shacl");

    finding.add_location(Location {
        logical: Some(result.focus_node.to_string()),
        ..Location::default()
    });

    if let Some(path) = &result.result_path {
        finding.related_locations.push(Location {
            logical: Some(format!("path {path}")),
            ..Location::default()
        });
    }
    if let Some(value) = &result.value {
        finding.related_locations.push(Location {
            logical: Some(format!("value {value}")),
            ..Location::default()
        });
    }
    finding.detail = Some(format!("source shape: {}", result.source_shape));
    finding
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::{NamedNode, Term};

    #[test]
    fn rdf_diagnostic_carries_wire_coordinates_into_finding() {
        let diagnostic = RdfDiagnostic::error("gts.fold", "unexpected reifier")
            .with_location(RdfLocation::logical("gts:quad").with_gts_quad(42));

        let finding = finding_from_rdf(&diagnostic);

        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.code, "gts.fold");
        let location = finding.primary_location().expect("a location");
        assert_eq!(location.gts_quad_index, Some(42));
        assert_eq!(location.logical.as_deref(), Some("gts:quad"));
    }

    #[test]
    fn rdf_losses_become_suggestions_and_related_locations() {
        use gmeow_rdf::RdfLoss;
        let mut diagnostic =
            RdfDiagnostic::new(RdfSeverity::Warning, "gts.lossy", "language tag dropped");
        diagnostic.add_loss(
            RdfLoss::new("lang", "dropped @en")
                .with_location(RdfLocation::logical("gts:term").with_gts_term(7)),
        );

        let finding = finding_from_rdf(&diagnostic);

        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.suggestions, ["lang: dropped @en"]);
        assert_eq!(finding.related_locations.len(), 1);
        assert_eq!(finding.related_locations[0].gts_term_id, Some(7));
    }

    #[test]
    fn shacl_result_carries_focus_node_and_component() {
        let result = ValidationResult {
            focus_node: Term::NamedNode(NamedNode::new_unchecked("https://ex/a")),
            result_path: Some(Term::NamedNode(NamedNode::new_unchecked("https://ex/p"))),
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
        };

        let finding = finding_from_shacl(&result);

        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.code, "shacl.MinCountConstraintComponent");
        // oxigraph Term Display is the N-Triples form, so IRIs are angle-bracketed.
        assert_eq!(
            finding
                .primary_location()
                .and_then(|l| l.logical.as_deref()),
            Some("<https://ex/a>")
        );
        assert!(finding
            .related_locations
            .iter()
            .any(|l| l.logical.as_deref() == Some("path <https://ex/p>")));
        assert_eq!(
            finding.detail.as_deref(),
            Some("source shape: <https://ex/shape>")
        );
    }
}
