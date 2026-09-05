// SPDX-License-Identifier: AGPL-3.0-only

//! Rust twin of the retired Python `shacl_engine` seam tests.
//!
//! These pin the native N-Triples → SHACL adapter contract:
//! - structured reports pass through faithfully,
//! - severity bucketing and the legacy `<focus>: <message>` line format are stable,
//! - parse errors hard-fail (never a silent `conforms`).

use std::sync::Arc;

use purrdf::shapes::engine::{parse_shapes, validate_dataset};
use purrdf::shapes::report::{Severity, ValidationReport, ValidationResult};
use purrdf::shapes::term::{NamedNode, Term};
use purrdf::{flat_dataset_from_quads, flat_rdf_quads_from_dataset, parse_dataset};

const NS: &str = "http://example.org/ns#";

const SHAPES_TTL: &str = r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <http://example.org/ns#> .
ex:PersonShape a sh:NodeShape ;
    sh:targetClass ex:Person ;
    sh:property [
        sh:path ex:name ;
        sh:minCount 1 ;
        sh:severity sh:Violation ;
        sh:message "name required" ;
    ] .
"#;

fn alice_nt(with_name: bool) -> String {
    let mut nt =
        format!("<{NS}alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{NS}Person> .\n");
    if with_name {
        nt.push_str(&format!("<{NS}alice> <{NS}name> \"Alice\" .\n"));
    }
    nt
}

fn nt_to_dataset(nt: &str) -> Arc<purrdf::RdfDataset> {
    let dataset = parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .unwrap_or_else(|e| panic!("N-Triples parse failed: {e}"));
    let mut quads = flat_rdf_quads_from_dataset(&dataset);
    for quad in &mut quads {
        quad.graph_name = None;
    }
    flat_dataset_from_quads(&quads).expect("flattened dataset must freeze")
}

fn validate_nt(data_nt: &str, shapes_ttl: &str) -> ValidationReport {
    let shapes = parse_shapes(shapes_ttl, None).expect("SHACL shapes must parse");
    let dataset = nt_to_dataset(data_nt);
    validate_dataset(&dataset, &shapes).expect("native SHACL validation must succeed")
}

// ── Legacy string-formatting helpers (mirrors the Python seam) ───────────────

fn term_to_str(term: Option<&str>) -> String {
    match term {
        None => "None".to_owned(),
        Some(t) if t.starts_with('<') && t.ends_with('>') => t[1..t.len() - 1].to_owned(),
        Some(t) if t.starts_with("_:") => t[2..].to_owned(),
        Some(t) => t.to_owned(),
    }
}

fn role_prefix(result: &ValidationResult) -> String {
    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    let labels: Vec<&str> = result
        .result_box_roles
        .iter()
        .map(|role| match role.as_str() {
            r if r == format!("{GMEOW}boxABox") => "ABox",
            r if r == format!("{GMEOW}boxTBox") => "TBox",
            r if r == format!("{GMEOW}boxRBox") => "RBox",
            r if r == format!("{GMEOW}boxCBox") => "CBox",
            r if r == format!("{GMEOW}boxConfigBox") => "ConfigBox",
            other => other
                .rsplit('/')
                .next()
                .unwrap_or(other)
                .rsplit('#')
                .next()
                .unwrap_or(other),
        })
        .collect();
    if labels.is_empty() {
        String::new()
    } else {
        format!("[{}] ", labels.join("/"))
    }
}

fn partition_results(results: &[ValidationResult]) -> (Vec<String>, Vec<String>) {
    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    for r in results {
        let focus = term_to_str(Some(&r.focus_node.to_string()));
        let prefix = role_prefix(r);
        let line = match &r.message {
            Some(msg) => format!("{prefix}{focus}: {msg}"),
            None => format!("{prefix}{focus}"),
        };
        match r.severity {
            Severity::Warning | Severity::Info => warnings.push(line),
            _ => violations.push(line),
        }
    }
    (violations, warnings)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn version_is_reported() {
    assert!(!purrdf::shapes::VERSION.is_empty());
}

#[test]
fn conforming_graph_has_no_results() {
    let report = validate_nt(&alice_nt(true), SHAPES_TTL);
    assert!(report.conforms);
    assert!(report.results.is_empty());
}

#[test]
fn violation_partitions_to_errors_with_stable_line() {
    let report = validate_nt(&alice_nt(false), SHAPES_TTL);
    assert!(!report.conforms);
    let (violations, warnings) = partition_results(&report.results);
    assert!(warnings.is_empty());
    assert_eq!(violations, vec![format!("{NS}alice: name required")]);
}

// The RDF-1.2 reifier obligation the validation-shape projector now emits (a property-level
// `sh:reifierShape` + `sh:reificationRequired` block — the exact form
// generated/shapes/validation-shapes.ttl carries for gmeow:StandpointTenure → gmeow:tenurePosition)
// is genuinely ENFORCED by the native engine on the production surface: a held position asserted as
// a bare, unreified triple is a finding; a properly reified one whose reifier conforms passes.
const REIFIER_SHAPES_TTL: &str = r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <http://example.org/ns#> .
ex:TenureShape a sh:NodeShape ;
    sh:targetClass ex:Tenure ;
    sh:property [
        sh:path ex:tenurePosition ;
        sh:reifierShape ex:ReifierShape ;
        sh:reificationRequired true ;
        sh:severity sh:Violation ;
        sh:message "a tenure position must be a reified statement" ;
    ] .
ex:ReifierShape a sh:NodeShape ;
    sh:property [
        sh:path ex:vantage ;
        sh:minCount 1 ;
    ] .
"#;

const REIFIER_COMPONENT: &str = "http://www.w3.org/ns/shacl#ReifierShapeConstraintComponent";

fn turtle_to_dataset(ttl: &str) -> Arc<purrdf::RdfDataset> {
    let dataset = parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .unwrap_or_else(|e| panic!("Turtle parse failed: {e}"));
    let mut quads = flat_rdf_quads_from_dataset(&dataset);
    for quad in &mut quads {
        quad.graph_name = None;
    }
    flat_dataset_from_quads(&quads).expect("flattened dataset must freeze")
}

fn validate_ttl(data_ttl: &str, shapes_ttl: &str) -> ValidationReport {
    let shapes = parse_shapes(shapes_ttl, None).expect("SHACL shapes must parse");
    let dataset = turtle_to_dataset(data_ttl);
    validate_dataset(&dataset, &shapes).expect("native SHACL validation must succeed")
}

#[test]
fn reification_required_flags_a_bare_held_position() {
    // A Tenure that records its position as a bare, unreified triple is a finding.
    let data = "@prefix ex: <http://example.org/ns#> .\n\
                ex:t1 a ex:Tenure ; ex:tenurePosition ex:claim1 .\n";
    let report = validate_ttl(data, REIFIER_SHAPES_TTL);
    assert!(
        !report.conforms,
        "a bare (unreified) held position must be a finding"
    );
    assert!(
        report
            .results
            .iter()
            .any(|r| r.source_constraint_component.as_str() == REIFIER_COMPONENT),
        "the finding must be a ReifierShapeConstraintComponent: {:?}",
        report.results
    );
}

#[test]
fn a_reified_position_with_a_conforming_reifier_passes() {
    // The position is reified, and the reifier conforms to the reifier shape (it has a vantage).
    let data = "@prefix ex: <http://example.org/ns#> .\n\
                @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
                ex:t2 a ex:Tenure ; ex:tenurePosition ex:claim2 .\n\
                ex:r2 rdf:reifies <<( ex:t2 ex:tenurePosition ex:claim2 )>> ; ex:vantage ex:sp .\n";
    let report = validate_ttl(data, REIFIER_SHAPES_TTL);
    assert!(
        report.conforms,
        "a reified position with a conforming reifier must pass: {:?}",
        report.results
    );
}

#[test]
fn a_reifier_that_violates_its_shape_is_flagged() {
    // The position IS reified, but the reifier violates the reifier shape (it has no vantage).
    let data = "@prefix ex: <http://example.org/ns#> .\n\
                @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
                ex:t3 a ex:Tenure ; ex:tenurePosition ex:claim3 .\n\
                ex:r3 rdf:reifies <<( ex:t3 ex:tenurePosition ex:claim3 )>> .\n";
    let report = validate_ttl(data, REIFIER_SHAPES_TTL);
    assert!(
        !report.conforms,
        "a reifier that violates the reifier shape must be a finding: {:?}",
        report.results
    );
    assert!(
        report
            .results
            .iter()
            .any(|r| r.source_constraint_component.as_str() == REIFIER_COMPONENT),
        "the finding must be a ReifierShapeConstraintComponent: {:?}",
        report.results
    );
}

#[test]
fn warning_severity_buckets_to_warnings() {
    let shapes = SHAPES_TTL.replace("sh:Violation", "sh:Warning");
    let report = validate_nt(&alice_nt(false), &shapes);
    let (violations, warnings) = partition_results(&report.results);
    assert!(violations.is_empty());
    assert_eq!(warnings, vec![format!("{NS}alice: name required")]);
}

#[test]
fn partition_results_prefixes_box_roles_when_present() {
    let result = ValidationResult {
        focus_node: Term::NamedNode(NamedNode::new_unchecked(format!("{NS}stmt"))),
        result_path: None,
        path_structure: None,
        value: None,
        source_constraint_component: NamedNode::new_unchecked(
            "http://www.w3.org/ns/shacl#ReifierShapeConstraintComponent",
        ),
        source_shape: Term::NamedNode(NamedNode::new_unchecked(format!("{NS}Shape"))),
        severity: Severity::Violation,
        message: Some("context required".to_owned()),
        source_box_roles: Vec::new(),
        path_box_roles: Vec::new(),
        result_box_roles: vec![NamedNode::new_unchecked(
            "https://blackcatinformatics.ca/gmeow/boxCBox",
        )],
        attributions: Vec::new(),
    };
    let (violations, warnings) = partition_results(&[result]);
    assert_eq!(
        violations,
        vec![format!("[CBox] {NS}stmt: context required")]
    );
    assert!(warnings.is_empty());
}

#[test]
fn partition_results_uses_hash_iri_local_name_for_unknown_roles() {
    let result = ValidationResult {
        focus_node: Term::NamedNode(NamedNode::new_unchecked(format!("{NS}stmt"))),
        result_path: None,
        path_structure: None,
        value: None,
        source_constraint_component: NamedNode::new_unchecked(
            "http://www.w3.org/ns/shacl#ConstraintComponent",
        ),
        source_shape: Term::NamedNode(NamedNode::new_unchecked(format!("{NS}Shape"))),
        severity: Severity::Violation,
        message: Some("context required".to_owned()),
        source_box_roles: Vec::new(),
        path_box_roles: Vec::new(),
        result_box_roles: vec![NamedNode::new_unchecked(
            "http://example.org/roles#NovelRole",
        )],
        attributions: Vec::new(),
    };
    let (violations, warnings) = partition_results(&[result]);
    assert_eq!(
        violations,
        vec![format!("[NovelRole] {NS}stmt: context required")]
    );
    assert!(warnings.is_empty());
}

#[test]
fn parse_error_hard_fails() {
    let err = parse_shapes("this is not valid turtle @@@", None).expect_err("must fail");
    assert!(!err.is_empty());
}

#[test]
fn term_normalization() {
    assert_eq!(term_to_str(Some("<http://x>")), "http://x");
    assert_eq!(term_to_str(Some("_:b0")), "b0");
    assert_eq!(term_to_str(Some("\"literal\"")), "\"literal\"");
    assert_eq!(term_to_str(None), "None");
}
