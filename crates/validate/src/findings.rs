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

#[cfg(test)]
mod tests {
    use super::*;
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
}
