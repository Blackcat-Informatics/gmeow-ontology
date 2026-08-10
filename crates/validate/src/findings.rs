// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bridge from the structured RDF/SHACL diagnostics into the canonical
//! [`gmeow_errors::Finding`] model.
//!
//! `gmeow-validate` depends on both `gmeow-errors` and the `gmeow-rdf` kernel,
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

/// The `sh:NodeShape IRI → gmeow:enforcesFailureClass IRI` index a SHACL result is
/// resolved against, so the emitted [`Finding`] can NAME the typed failure the
/// violated law declares instead of only the generic constraint-component code.
///
/// The derive pipeline projects `gmeow:enforcesFailureClass` directly onto the
/// generated shape node itself — the SAME IRI a SHACL engine reports as a violation's
/// `sh:sourceShape` — for `logic:Constraint`-derived procedural shapes and
/// OWL-restriction-derived validation shapes alike, so one scan over the shapes graph
/// reaches every annotated shape whichever derive path produced it. Nothing here is
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
    /// triple with an IRI subject and object.
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
            let (TermRef::Iri(shape), TermRef::Iri(class)) =
                (ds.resolve(quad.s), ds.resolve(quad.o))
            else {
                continue;
            };
            by_shape
                .entry(shape.to_owned())
                .and_modify(|resident| {
                    if class < resident.as_str() {
                        *resident = class.to_owned();
                    }
                })
                .or_insert_with(|| class.to_owned());
        }
        Self { by_shape }
    }

    /// The typed failure class the shape at `shape_iri` declares, if any.
    #[must_use]
    pub fn for_shape(&self, shape_iri: &str) -> Option<&str> {
        self.by_shape.get(shape_iri).map(String::as_str)
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

        let finding = finding_from_shacl(&result, &FailureClassIndex::empty());

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
        // The finding is attributed to the DOCUMENTED constrained property (the
        // `sh:path`), NOT the ABox focus node — the term whose "Diagnostics you
        // might hit" page this violation belongs on.
        assert_eq!(finding.documented_terms, vec!["https://ex/p".to_owned()]);
    }

    #[test]
    fn node_level_constraint_attributes_no_documented_term() {
        // A result with NO `sh:path` (a node-level / focus-node constraint) names no
        // single constrained property, so no documented term is fabricated.
        let mut result = min_count_result("https://ex/a");
        result.result_path = None;
        assert!(
            finding_from_shacl(&result, &FailureClassIndex::empty())
                .documented_terms
                .is_empty()
        );
        // Same honest absence when the path is a complex (blank-node) property path.
        let mut complex = min_count_result("https://ex/a");
        complex.result_path = Some(Term::BlankNode("b0".to_owned()));
        assert!(
            finding_from_shacl(&complex, &FailureClassIndex::empty())
                .documented_terms
                .is_empty()
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
            diag_from_shacl(
                &min_count_result("https://ex/a"),
                &FailureClassIndex::empty(),
            ),
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
        // The documented constrained property survives routing through the ledger onto
        // the projected finding — the docs per-term diagnostics join key.
        assert_eq!(finding.documented_terms, vec!["https://ex/p".to_owned()]);
    }

    #[test]
    fn routed_shacl_findings_at_distinct_foci_get_distinct_anchors() {
        // Two violations of the SAME constraint component on DIFFERENT focus nodes are
        // distinct witnesses at distinct anchors — the ledger does not collapse them,
        // and their code-blind anchor IRIs differ (the join key is per-focus).
        let mut ledger = DiagLedger::new();
        ledger.attach(
            diag_from_shacl(
                &min_count_result("https://ex/a"),
                &FailureClassIndex::empty(),
            ),
            StageId::new("stage-validate"),
        );
        ledger.attach(
            diag_from_shacl(
                &min_count_result("https://ex/b"),
                &FailureClassIndex::empty(),
            ),
            StageId::new("stage-validate"),
        );
        let findings = ledger.findings("shacl");
        assert_eq!(findings.len(), 2, "distinct foci → distinct witnesses");
        assert_ne!(findings[0].anchor_iri, findings[1].anchor_iri);
    }

    /// A shapes graph in which one node shape declares a typed failure class and a
    /// second, otherwise-identical shape declares none.
    fn shapes_with_failure_class() -> std::sync::Arc<purrdf::RdfDataset> {
        purrdf::parse_dataset(
            concat!(
                "<https://ex/shape> ",
                "<https://blackcatinformatics.ca/gmeow/enforcesFailureClass> ",
                "<https://ex/UntypedFreeVariable> .\n",
                "<https://ex/unannotated> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
                "<http://www.w3.org/ns/shacl#NodeShape> .\n"
            )
            .as_bytes(),
            "text/turtle",
            None,
        )
        .expect("shapes fixture parses")
    }

    #[test]
    fn the_violated_shapes_typed_failure_class_reaches_the_finding() {
        // F1: the shipped surface must NAME the failure. The component code is generic —
        // every cardinality gate in the ontology reports MinCountConstraintComponent —
        // so a consumer reading only `code` + `detail` cannot tell WHICH authored law
        // fired. The class is on the shape the engine already names as `sh:sourceShape`;
        // before this join it was parsed and then dropped on the floor.
        let classes = FailureClassIndex::from_shapes_dataset(&shapes_with_failure_class());
        assert_eq!(classes.len(), 1);
        let finding = finding_from_shacl(&min_count_result("https://ex/a"), &classes);
        assert_eq!(
            finding.failure_class.as_deref(),
            Some("https://ex/UntypedFreeVariable")
        );
    }

    #[test]
    fn a_shape_declaring_no_failure_class_names_none() {
        // Honest absence, never fabricated: an unannotated shape's violation carries no
        // class rather than a guessed one.
        let classes = FailureClassIndex::from_shapes_dataset(&shapes_with_failure_class());
        let mut result = min_count_result("https://ex/a");
        result.source_shape = Term::NamedNode(NamedNode::new_unchecked("https://ex/unannotated"));
        assert_eq!(finding_from_shacl(&result, &classes).failure_class, None);
    }

    #[test]
    fn the_failure_class_survives_the_ledger_projection() {
        // The pipeline and the `--deep`/data paths route SHACL results through the
        // DiagLedger rather than hand-building findings, so the class must ride the
        // witness node too — otherwise the class reaches only ONE of the two bridges and
        // the surface a consumer actually hits depends on which entry point ran.
        let classes = FailureClassIndex::from_shapes_dataset(&shapes_with_failure_class());
        let mut ledger = DiagLedger::new();
        ledger.attach(
            diag_from_shacl(&min_count_result("https://ex/a"), &classes),
            StageId::new("stage-validate"),
        );
        let findings = ledger.findings("shacl");
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].failure_class.as_deref(),
            Some("https://ex/UntypedFreeVariable")
        );
    }

    #[test]
    fn the_failure_class_is_payload_not_identity() {
        // Naming the class must not perturb the witness's content address: the same
        // violation with and without a resolvable class is the SAME witness, so the
        // blake3 finding IRI and the code-blind anchor are unchanged.
        let classes = FailureClassIndex::from_shapes_dataset(&shapes_with_failure_class());
        let mut with_class = DiagLedger::new();
        with_class.attach(
            diag_from_shacl(&min_count_result("https://ex/a"), &classes),
            StageId::new("stage-validate"),
        );
        let mut without_class = DiagLedger::new();
        without_class.attach(
            diag_from_shacl(
                &min_count_result("https://ex/a"),
                &FailureClassIndex::empty(),
            ),
            StageId::new("stage-validate"),
        );
        let a = &with_class.findings("shacl")[0];
        let b = &without_class.findings("shacl")[0];
        assert_eq!(a.finding_iri, b.finding_iri);
        assert_eq!(a.anchor_iri, b.anchor_iri);
        assert_ne!(a.failure_class, b.failure_class);
    }
}
