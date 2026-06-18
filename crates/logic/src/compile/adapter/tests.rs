// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the OWL/gUFO adapter — the Rust mirror of
//! `tests/test_logic_adapter.py`, driven by Turtle source strings.

use super::*;
use crate::compile::frontend::parse_logic_str;

const PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gufo:  <http://purl.org/nemo/gufo#> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
";

fn adapt(ttl: &str) -> (LogicProgram, Vec<Diagnostic>) {
    adapt_legacy_str(&format!("{PREFIXES}{ttl}"), None).expect("adapt ok")
}

fn logic_prog(ttl: &str) -> LogicProgram {
    parse_logic_str(&format!("{PREFIXES}{ttl}"), None)
        .expect("parse ok")
        .0
}

// ── Error paths ──────────────────────────────────────────────────────────────

#[test]
fn adapt_empty_graph_raises() {
    let err = adapt_legacy_str(PREFIXES, None).unwrap_err();
    assert!(err.0.contains("empty"));
}

#[test]
fn adapt_nonexistent_file_raises() {
    let err = adapt_legacy_path(Path::new("/no/such/adapter-xyz.ttl"), None).unwrap_err();
    assert!(err.0.contains("does not exist"));
}

#[test]
fn adapt_invalid_turtle_raises() {
    let err = adapt_legacy_str("not turtle <<<", None).unwrap_err();
    assert!(err.0.contains("Failed to parse"));
}

// ── gUFO stereotype → logic: sort ────────────────────────────────────────────

#[test]
fn adapt_gufo_kind_to_logic_kind() {
    let (prog, _) = adapt("ex:Person a gufo:Kind .");
    assert_eq!(prog.axioms.len(), 1);
    let ax = &prog.axioms[0];
    assert!(ax.subject.ends_with("/Person"));
    assert_eq!(ax.predicate, RDF_TYPE);
    assert_eq!(ax.obj, logic("Kind"));
}

#[test]
fn adapt_gufo_event_and_situation_type() {
    let (prog, _) = adapt(
        "ex:Birth a gufo:EventType .
         ex:Snapshot a gufo:SituationType .",
    );
    assert!(prog.axioms.iter().any(|a| a.obj == logic("Event")));
    assert!(prog.axioms.iter().any(|a| a.obj == logic("Situation")));
}

#[test]
fn adapt_all_gufo_sortals() {
    let (prog, diags) = adapt(
        "ex:K a gufo:Kind . ex:Sk a gufo:SubKind . ex:Ph a gufo:Phase .
         ex:Ro a gufo:Role . ex:Ca a gufo:Category . ex:Mi a gufo:Mixin .
         ex:Rm a gufo:RoleMixin . ex:Pm a gufo:PhaseMixin . ex:Re a gufo:Relator .",
    );
    assert!(diags.is_empty());
    assert_eq!(prog.axioms.len(), 9);
    for sort in [
        "Kind",
        "SubKind",
        "Phase",
        "Role",
        "Category",
        "Mixin",
        "RoleMixin",
        "PhaseMixin",
        "Relator",
    ] {
        assert!(
            prog.axioms.iter().any(|a| a.obj == logic(sort)),
            "missing sort {sort}"
        );
    }
}

#[test]
fn adapt_blank_node_gufo_sort_emits_diagnostic() {
    let (prog, diags) = adapt("[] a gufo:Kind .");
    assert!(prog.axioms.is_empty());
    assert!(diags.iter().any(|d| d.code == "BLANK_NODE_GUFO_SORT"));
}

// ── OWL structural predicates ────────────────────────────────────────────────

#[test]
fn adapt_rdfs_subclass_of() {
    let (prog, _) = adapt("ex:Employee rdfs:subClassOf ex:Person .");
    let ax = &prog.axioms[0];
    assert_eq!(ax.predicate, logic("subClassOf"));
    assert!(ax.subject.ends_with("/Employee"));
    assert!(ax.obj.ends_with("/Person"));
}

#[test]
fn adapt_owl_equivalent_disjoint_inverse() {
    let (prog, _) = adapt(
        "ex:A owl:equivalentClass ex:B .
         ex:A owl:disjointWith ex:C .
         ex:p owl:inverseOf ex:q .",
    );
    assert!(prog
        .axioms
        .iter()
        .any(|a| a.predicate == logic("equivalentClass")));
    assert!(prog
        .axioms
        .iter()
        .any(|a| a.predicate == logic("disjointWith")));
    assert!(prog
        .axioms
        .iter()
        .any(|a| a.predicate == logic("inverseOf")));
}

#[test]
fn adapt_rdfs_domain_and_range() {
    let (prog, _) = adapt(
        "ex:p rdfs:domain ex:A .
         ex:p rdfs:range ex:B .",
    );
    assert!(prog.axioms.iter().any(|a| a.predicate == logic("domain")));
    assert!(prog.axioms.iter().any(|a| a.predicate == logic("range")));
}

// ── OWL property characteristics ─────────────────────────────────────────────

#[test]
fn adapt_owl_characteristics() {
    let (prog, _) = adapt(
        "ex:p a owl:TransitiveProperty .
         ex:q a owl:SymmetricProperty .
         ex:r a owl:FunctionalProperty .
         ex:s a owl:InverseFunctionalProperty .",
    );
    for c in [
        "transitiveProperty",
        "symmetricProperty",
        "functionalProperty",
        "inverseFunctionalProperty",
    ] {
        assert!(
            prog.axioms.iter().any(|a| a.obj == logic(c)),
            "missing characteristic {c}"
        );
    }
}

// ── Unmapped constructs ──────────────────────────────────────────────────────

#[test]
fn blank_node_restriction_emits_unmapped_diagnostic() {
    let (prog, diags) = adapt(
        "ex:Bird rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasBeak ; owl:someValuesFrom ex:Beak ] .",
    );
    // The blank-node restriction object cannot be normalized.
    assert!(prog.axioms.iter().all(|a| !a.obj.is_empty()));
    let d = diags
        .iter()
        .find(|d| d.code == "UNMAPPED_OWL_CONSTRUCT")
        .expect("unmapped diagnostic");
    assert!(d.message.contains("restriction"), "msg: {}", d.message);
}

// ── IR isomorphism gate ──────────────────────────────────────────────────────

#[test]
fn isomorphic_identical_programs() {
    let a = adapt("ex:Person a gufo:Kind .").0;
    let b = adapt("ex:Person a gufo:Kind .").0;
    assert!(assert_ir_isomorphic(&a, &b).is_ok());
}

#[test]
fn isomorphic_divergent_programs_raise() {
    let a = adapt("ex:Person a gufo:Kind .").0;
    let b = adapt("ex:Person a gufo:Role .").0;
    let err = assert_ir_isomorphic(&a, &b).unwrap_err();
    assert!(err.0.contains("isomorphism gate FAILED"));
    assert!(err.0.contains("A has, B lacks (axiom)"));
    assert!(err.0.contains("B has, A lacks (axiom)"));
}

#[test]
fn isomorphic_order_independent() {
    let a = adapt("ex:P a gufo:Kind . ex:Q a gufo:Role .").0;
    let b = adapt("ex:Q a gufo:Role . ex:P a gufo:Kind .").0;
    assert!(assert_ir_isomorphic(&a, &b).is_ok());
}

// ── Round-trip: logic: ≡ owl/gufo ────────────────────────────────────────────

#[test]
fn roundtrip_gufo_kind_equals_logic_kind() {
    let prog_logic = logic_prog("ex:Person a logic:Kind .");
    let prog_gufo = adapt("ex:Person a gufo:Kind .").0;
    assert_eq!(prog_logic.axioms.len(), 1);
    assert_eq!(prog_gufo.axioms.len(), 1);
    assert!(assert_ir_isomorphic(&prog_logic, &prog_gufo).is_ok());
}

#[test]
fn roundtrip_owl_subclassof_equals_logic_subclassof() {
    let prog_logic = logic_prog("ex:Employee logic:subClassOf ex:Person .");
    let prog_owl = adapt("ex:Employee rdfs:subClassOf ex:Person .").0;
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn roundtrip_owl_transitive_property() {
    let prog_logic = logic_prog("ex:p a logic:transitiveProperty .");
    let prog_owl = adapt("ex:p a owl:TransitiveProperty .").0;
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn roundtrip_divergent_pair_raises() {
    let prog_logic = logic_prog("ex:Person a logic:Kind .");
    let prog_owl = adapt("ex:Person a gufo:Role .").0;
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_err());
}

// ── source_iri ───────────────────────────────────────────────────────────────

#[test]
fn adapt_source_iri_stored() {
    let (prog, _) = adapt_legacy_str(
        &format!("{PREFIXES}ex:P a gufo:Kind ."),
        Some("urn:src".to_owned()),
    )
    .unwrap();
    assert_eq!(prog.source_iri.as_deref(), Some("urn:src"));
}
