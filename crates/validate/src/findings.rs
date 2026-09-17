// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bridge from the structured RDF/SHACL diagnostics into the canonical
//! [`gmeow_errors::Finding`] model.
//!
//! `gmeow-validate` depends on both `gmeow-errors` and the `purrdf` kernel,
//! so the conversion lives here. The Rust orphan rules forbid
//! `impl From<RdfDiagnostic> for Finding` in this crate (it owns neither type),
//! hence these are plain named functions.
//!
//! The whole point is *carry-through*: the GTS wire coordinates that
//! [`purrdf::RdfLocation`] records and the focus/path/shape structure that a
//! SHACL result carries survive into the [`Finding`], so SARIF, the `gmeow:`
//! RDF projection, and the content-addressed cache all anchor to the same
//! position inside a bundle.

use std::collections::BTreeMap;

use gmeow_errors::code::register_code;
use gmeow_errors::diag::{Diag, Label};
use gmeow_errors::grade::{Grade, Standpoint};
use gmeow_errors::model::FindingCategory;
use gmeow_errors::{Finding, Location, Severity};
use purrdf::shapes::report::{Severity as ShaclSeverity, ValidationResult};
use purrdf::shapes::term::Term;
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};

/// `gmeow:enforcesFailureClass` — the predicate a law (a `logic:Constraint`, an
/// OWL/RDFS restriction, or the shape either derives to) carries to name the TYPED
/// conformance-failure class it raises.
pub const GMEOW_ENFORCES_FAILURE_CLASS: &str =
    "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";

/// The `shape identity → gmeow:enforcesFailureClass IRI` index a SHACL result is
/// resolved against, so the emitted [`Finding`] can NAME the typed failure the
/// violated law declares instead of only the generic constraint-component code.
///
/// The derive pipeline projects `gmeow:enforcesFailureClass` directly onto the
/// generated shape node itself — the SAME identity a SHACL engine reports as a violation's
/// `sh:sourceShape` — for `logic:Constraint`-derived procedural shapes and
/// OWL-restriction-derived validation shapes alike, so one scan over the shapes graph
/// reaches every annotated node/property shape whichever derive path produced it. Nothing here is
/// keyed on a namespace: a shape carrying the annotation names its class, and one that
/// does not carries no class (an honest absence, never fabricated).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FailureClassIndex {
    by_shape: BTreeMap<String, String>,
}

impl FailureClassIndex {
    /// The empty index — a SHACL run whose shapes graph is not available names no
    /// failure class. Used only where there is genuinely no shapes graph to read.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Scan a parsed shapes graph for every `?shape gmeow:enforcesFailureClass ?class`
    /// triple with an IRI or scoped blank subject and an IRI object. Generated
    /// property shapes are often blank nodes; their identities use the same
    /// native term conversion as the SHACL parser and result producer.
    ///
    /// A shape carrying two distinct classes is rejected upstream by the derive
    /// frontend (`logic:Constraint {iri} has distinct gmeow:enforcesFailureClass
    /// values`), so this keeps the lexicographic minimum purely to stay a total,
    /// scan-order-independent function rather than to paper over that conflict.
    #[must_use]
    pub fn from_shapes_dataset(ds: &RdfDataset) -> Self {
        let mut by_shape: BTreeMap<String, String> = BTreeMap::new();
        let Some(enforces) = ds.term_id_by_value(&TermValue::iri(GMEOW_ENFORCES_FAILURE_CLASS))
        else {
            return Self { by_shape };
        };
        for quad in ds.quads_for_pattern(None, Some(enforces), None, GraphMatch::Any) {
            if !matches!(ds.resolve(quad.s), TermRef::Iri(_) | TermRef::Blank { .. }) {
                continue;
            }
            let TermRef::Iri(class) = ds.resolve(quad.o) else {
                continue;
            };
            let shape = purrdf::shapes::term::term_id_to_native(ds, quad.s).to_string();
            by_shape
                .entry(strip_angle(&shape).to_owned())
                .and_modify(|resident| {
                    if class < resident.as_str() {
                        *resident = class.to_owned();
                    }
                })
                .or_insert_with(|| class.to_owned());
        }
        Self { by_shape }
    }

    /// The typed failure class declared by an IRI or scope-qualified blank shape.
    #[must_use]
    pub fn for_shape(&self, shape: &str) -> Option<&str> {
        self.by_shape.get(shape).map(String::as_str)
    }

    /// The typed failure class a SHACL result's `sh:sourceShape` declares, if any.
    #[must_use]
    pub fn for_result(&self, result: &ValidationResult) -> Option<&str> {
        self.for_shape(strip_angle(&result.source_shape.to_string()))
    }

    /// Whether the index resolved no shape at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_shape.is_empty()
    }

    /// How many shapes declare a failure class.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_shape.len()
    }
}

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

/// The DOCUMENTED source term a SHACL result honestly concerns: its CONSTRAINED
/// PROPERTY — the `sh:path` — when that path is a plain IRI (a documented `gmeow:`
/// property term), NOT the ABox focus node that tripped the shape. The focus node is
/// a data individual that names no documented ontology term, so it can never join the
/// docs per-term "Diagnostics you might hit" surface; the constrained property is the
/// documented term whose page the violation genuinely belongs on ("set this required
/// property or you hit a MinCount violation"). A node-level constraint (no `sh:path`)
/// or a complex property-path expression (a blank-node path) yields `None`: there is
/// no single documented property to attribute to and one is never fabricated (the
/// docs join simply renders no per-term row for that finding). The violated shape's
/// target class would be the honest fallback, but a `ValidationResult` carries only
/// the shape IRI, not its resolved `sh:targetClass`, so that fallback is not available
/// at this construction site without the shapes graph.
fn documented_constrained_property(result: &ValidationResult) -> Option<&str> {
    match &result.result_path {
        Some(Term::NamedNode(iri)) => Some(iri.as_str()),
        _ => None,
    }
}

/// Convert a SHACL [`ValidationResult`] into a canonical [`Finding`].
///
/// The focus node becomes the primary (logical) location; the result path and
/// offending value become related locations; the source shape rides in the
/// detail field. The code is `shacl.<ConstraintComponentLocalName>` so SARIF
/// rules stay stable and short.
///
/// `classes` resolves the violated shape to the TYPED conformance-failure class it
/// declares (`gmeow:enforcesFailureClass`), which rides onto
/// [`Finding::failure_class`]. The component code alone cannot name it — every
/// cardinality gate in the ontology reports `shacl.MinCountConstraintComponent` —
/// so without this join the authored class never reaches a consumer at all.
pub fn finding_from_shacl(result: &ValidationResult, classes: &FailureClassIndex) -> Finding {
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
    // Attribute the finding to the DOCUMENTED constrained property (the `sh:path`) it
    // concerns — the documented term whose "Diagnostics you might hit" page this
    // violation belongs on — distinct from the ABox focus node in its primary
    // location. Absent for node-level / complex-path constraints (honest absence).
    if let Some(property) = documented_constrained_property(result) {
        finding.documented_terms.push(property.to_owned());
    }
    // The TYPED failure class the violated shape declares — the specific failure the
    // generic component code cannot name. Absent when the shape declares none.
    if let Some(class) = classes.for_result(result) {
        finding.failure_class = Some(class.to_owned());
    }
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
///
/// `classes` supplies the same shape → `gmeow:enforcesFailureClass` join
/// [`finding_from_shacl`] performs, so both bridges name the typed failure identically.
pub fn diag_from_shacl(result: &ValidationResult, classes: &FailureClassIndex) -> Diag {
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
    diag = diag.with_context(format!(
        "source shape: {}",
        strip_angle(&result.source_shape.to_string())
    ));
    // Attribute to the DOCUMENTED constrained property (the `sh:path`), symmetric with
    // `finding_from_shacl`. Payload, not an identity field, so the witness's blake3
    // fingerprint / anchor are unchanged — the attribution is purely additive and only
    // the projected finding's `documented_terms` (the docs per-term join key) grows.
    if let Some(property) = documented_constrained_property(result) {
        diag = diag.with_documented_term(property.to_owned());
    }
    // The TYPED failure class the violated shape declares. Payload, not an identity
    // field, so the witness's blake3 fingerprint / anchor are unchanged.
    if let Some(class) = classes.for_result(result) {
        diag = diag.with_failure_class(class.to_owned());
    }
    diag
}

#[path = "findings.tests.rs"]
#[cfg(test)]
mod tests;
