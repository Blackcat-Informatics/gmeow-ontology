// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    assert!(finding_iri.starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/finding/"));
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
