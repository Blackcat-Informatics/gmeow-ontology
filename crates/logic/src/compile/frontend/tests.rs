// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the front-end parser — the Rust mirror of
//! `tests/test_logic_frontend.py`, driven by Turtle source strings.

use super::*;
use crate::compile::ir::LogicModality;

const PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

fn parse(ttl: &str) -> (LogicProgram, Vec<Diagnostic>) {
    let full = format!("{PREFIXES}{ttl}");
    parse_logic_str(&full, Some("https://example.org/prog".to_owned())).expect("parse ok")
}

fn has_axiom(
    prog: &LogicProgram,
    subj_suffix: &str,
    pred_local: &str,
    modality: LogicModality,
) -> bool {
    prog.axioms.iter().any(|a| {
        a.subject.ends_with(subj_suffix)
            && a.predicate.ends_with(pred_local)
            && a.scope.modality == modality
    })
}

// ── Empty / error paths ──────────────────────────────────────────────────────

#[test]
fn parse_empty_graph_raises() {
    let err = parse_logic_str(PREFIXES, None).unwrap_err();
    assert!(err.0.contains("empty"), "got: {}", err.0);
}

#[test]
fn parse_nonexistent_file_raises() {
    let err = parse_logic_path(Path::new("/no/such/file-xyz.ttl"), None).unwrap_err();
    assert!(err.0.contains("does not exist"));
}

#[test]
fn parse_invalid_turtle_raises() {
    let err = parse_logic_str("this is not turtle <<<", None).unwrap_err();
    assert!(err.0.contains("Failed to parse"));
}

// ── Minimal graph + profiles ─────────────────────────────────────────────────

#[test]
fn parse_minimal_graph_succeeds() {
    let (prog, diags) = parse(
        "ex:Person a logic:Kind .
         logic:PositiveHornProfile a logic:SemanticProfile .",
    );
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    assert!(has_axiom(&prog, "/Person", "#type", LogicModality::None));
    assert_eq!(prog.profiles.len(), 1);
    assert_eq!(prog.profiles[0].profile_id, SemanticProfileId::PositiveHorn);
    assert_eq!(prog.source_iri.as_deref(), Some("https://example.org/prog"));
}

#[test]
fn parse_multiple_profiles_with_complexity() {
    let (prog, _) = parse(
        "logic:PositiveHornProfile a logic:SemanticProfile ;
            logic:complexityClass \"PTIME\" .
         logic:StableModelProfile a logic:SemanticProfile .",
    );
    assert_eq!(prog.profiles.len(), 2);
    let horn = prog
        .profiles
        .iter()
        .find(|p| p.profile_id == SemanticProfileId::PositiveHorn)
        .unwrap();
    assert_eq!(horn.complexity.as_ref().unwrap().to_string(), "PTIME");
}

#[test]
fn unknown_semantic_profile_emits_diagnostic() {
    let (prog, diags) = parse("ex:Bogus a logic:SemanticProfile .");
    assert!(prog.profiles.is_empty());
    assert!(diags.iter().any(|d| d.code == "UNKNOWN_PROFILE"));
}

// ── Axioms ───────────────────────────────────────────────────────────────────

#[test]
fn parse_logic_relation_axiom() {
    let (prog, _) = parse("ex:Bird logic:subClassOf ex:Animal .");
    let ax = prog
        .axioms
        .iter()
        .find(|a| a.predicate.ends_with("subClassOf"))
        .unwrap();
    assert!(ax.subject.ends_with("/Bird"));
    assert!(ax.obj.ends_with("/Animal"));
    assert!(!ax.obj_is_literal);
}

#[test]
fn parse_literal_object_sets_flag() {
    let (prog, _) = parse("ex:s logic:confidence \"0.9\"^^xsd:decimal .");
    let ax = prog
        .axioms
        .iter()
        .find(|a| a.predicate.ends_with("confidence"))
        .unwrap();
    assert!(ax.obj_is_literal);
    assert_eq!(ax.obj, "0.9");
}

// ── Classic reification with scope ───────────────────────────────────────────

#[test]
fn parse_classic_reification_with_scope() {
    let (prog, _) = parse(
        "ex:Bird a logic:SubKind .
         ex:Bird logic:subClassOf ex:Animal .
         ex:stmt1 a rdf:Statement ;
            rdf:subject ex:Animal ;
            rdf:predicate logic:subClassOf ;
            rdf:object ex:Organism ;
            logic:confidence \"0.9\"^^xsd:decimal ;
            logic:modality logic:epistemic .",
    );
    // The reified (Animal subClassOf Organism) axiom carries epistemic scope.
    let scoped = prog
        .axioms
        .iter()
        .find(|a| {
            a.subject.ends_with("/Animal")
                && a.predicate.ends_with("subClassOf")
                && a.obj.ends_with("/Organism")
        })
        .unwrap();
    assert_eq!(scoped.scope.modality, LogicModality::Epistemic);
    assert_eq!(scoped.scope.confidence, Some(0.9));
}

#[test]
fn parse_modality_annotation_strips_namespace() {
    let (prog, _) = parse(
        "ex:stmt a rdf:Statement ;
            rdf:subject ex:a ; rdf:predicate logic:subClassOf ; rdf:object ex:b ;
            logic:modality logic:deontic .",
    );
    let scoped = prog
        .axioms
        .iter()
        .find(|a| a.scope.modality == LogicModality::Deontic)
        .unwrap();
    assert_eq!(scoped.scope.modality, LogicModality::Deontic);
}

#[test]
fn malformed_reification_missing_predicate_emits_diagnostic() {
    let (_, diags) = parse(
        "ex:stmt a rdf:Statement ;
            rdf:subject ex:a ; rdf:object ex:b ;
            logic:modality logic:epistemic .",
    );
    assert!(diags.iter().any(|d| d.code == "MISSING_PREDICATE"));
}

#[test]
fn invalid_confidence_emits_diagnostic() {
    let (_, diags) = parse(
        "ex:stmt a rdf:Statement ;
            rdf:subject ex:a ; rdf:predicate logic:subClassOf ; rdf:object ex:b ;
            logic:confidence \"2.5\"^^xsd:decimal ;
            logic:modality logic:epistemic .",
    );
    assert!(diags.iter().any(|d| d.code == "INVALID_CONFIDENCE"));
}

// ── Profiles complexity guards ───────────────────────────────────────────────

#[test]
fn empty_complexity_class_emits_diagnostic() {
    let (prog, diags) = parse(
        "logic:PositiveHornProfile a logic:SemanticProfile ;
            logic:complexityClass \"\" .",
    );
    assert!(diags.iter().any(|d| d.code == "INVALID_COMPLEXITY_CLASS"));
    // The profile is still recorded, just without a complexity class.
    assert_eq!(prog.profiles.len(), 1);
    assert!(prog.profiles[0].complexity.is_none());
}

// ── Rules ────────────────────────────────────────────────────────────────────

#[test]
fn no_rule_nodes_yields_empty_rules() {
    let (prog, _) = parse("ex:Person a logic:Kind .");
    assert!(prog.rules.is_empty());
}

#[test]
fn rule_node_extracted() {
    let (prog, diags) = parse(
        "ex:r1 a logic:Rule ;
            logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:isA ; rdf:object ex:Animal ] ;
            logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:isA ; rdf:object ex:Bird ] .",
    );
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    assert_eq!(prog.rules.len(), 1);
    let rule = &prog.rules[0];
    assert!(rule.head.predicate.ends_with("isA"));
    assert_eq!(rule.body.len(), 1);
    assert!(!rule.body[0].negated);
}

#[test]
fn rule_missing_head_emits_diagnostic() {
    let (prog, diags) = parse(
        "ex:r1 a logic:Rule ;
            logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:isA ; rdf:object ex:Bird ] .",
    );
    assert!(prog.rules.is_empty());
    assert!(diags.iter().any(|d| d.code == "MISSING_RULE_HEAD"));
}

#[test]
fn negated_body_atom_yields_negated_axiom() {
    let (prog, _) = parse(
        "ex:r1 a logic:Rule ;
            logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:isA ; rdf:object ex:Live ] ;
            logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:isA ; rdf:object ex:Bird ] ;
            logic:negatedBody [ rdf:subject \"?x\" ; rdf:predicate logic:isA ; rdf:object ex:Dead ] .",
    );
    assert_eq!(prog.rules.len(), 1);
    let rule = &prog.rules[0];
    let negated: Vec<_> = rule.body.iter().filter(|b| b.negated).collect();
    let positive: Vec<_> = rule.body.iter().filter(|b| !b.negated).collect();
    assert_eq!(negated.len(), 1);
    assert_eq!(positive.len(), 1);
    assert!(negated[0].obj.ends_with("/Dead"));
}

#[test]
fn distinct_body_guard_requires_variables() {
    let (prog, diags) = parse(
        "ex:r1 a logic:Rule ;
            logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
            logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
            logic:distinctBody [ rdf:subject \"?x\" ; rdf:object \"?y\" ] .",
    );
    assert_eq!(prog.rules.len(), 1);
    assert_eq!(
        prog.rules[0].distinct_pairs,
        vec![("?x".to_owned(), "?y".to_owned())]
    );
    assert!(diags.is_empty());
}

#[test]
fn distinct_body_constant_term_rejected() {
    let (prog, diags) = parse(
        "ex:r1 a logic:Rule ;
            logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
            logic:distinctBody [ rdf:subject \"?x\" ; rdf:object ex:constant ] .",
    );
    assert!(prog.rules[0].distinct_pairs.is_empty());
    assert!(diags.iter().any(|d| d.code == "MALFORMED_RULE_BODY"));
}

// ── Order independence ───────────────────────────────────────────────────────

#[test]
fn parse_is_order_independent() {
    let a = parse(
        "ex:Person a logic:Kind .
         ex:Animal a logic:Kind .",
    )
    .0;
    let b = parse(
        "ex:Animal a logic:Kind .
         ex:Person a logic:Kind .",
    )
    .0;
    assert_eq!(a.canonical_key(), b.canonical_key());
}

// ── Real conformance-case round-trip (the byte-parity anchor) ────────────────

#[test]
fn confidence_scoped_axiom_case_produces_expected_ir() {
    // Mirrors conformance/logic/cases/projections/confidence-scoped-axiom.
    let (prog, diags) = parse_logic_str(
        "@prefix logic: <https://blackcatinformatics.ca/logic/> .
         @prefix ex:    <https://example.org/confidence-scoped-axiom/> .
         @prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
         @prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
         ex:Organism a logic:Kind .
         ex:Animal   a logic:Kind .
         ex:Bird     a logic:SubKind .
         ex:Bird     logic:subClassOf ex:Animal .
         ex:stmt1 a rdf:Statement ;
            rdf:subject   ex:Animal ;
            rdf:predicate logic:subClassOf ;
            rdf:object    ex:Organism ;
            logic:confidence \"0.9\"^^xsd:decimal ;
            logic:modality   logic:epistemic .",
        None,
    )
    .unwrap();
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    // Exactly the 7 axioms the datalog golden emits.
    assert_eq!(prog.axioms.len(), 7, "axioms: {:#?}", prog.axioms);
    // The scoped reified axiom carries epistemic modality.
    assert!(has_axiom(
        &prog,
        "/Animal",
        "subClassOf",
        LogicModality::Epistemic
    ));
    // The plain stmt1 confidence/modality triples are default-context axioms.
    assert!(has_axiom(
        &prog,
        "/stmt1",
        "confidence",
        LogicModality::None
    ));
    assert!(has_axiom(&prog, "/stmt1", "modality", LogicModality::None));
    // The three rdf:type and one plain subClassOf are default context.
    assert!(has_axiom(&prog, "/Bird", "subClassOf", LogicModality::None));
}
