// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the front-end parser, driven by Turtle source strings. These are
//! the authoritative parser tests; they supersede the Python
//! `tests/test_logic_frontend.py`, which was retired.

use super::*;
use crate::ir::LogicModality;

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

// ── Trivially-Horn top-level formula routing (F2) ─────────────────────────────

#[test]
fn reified_trivially_horn_formula_routes_to_axioms_not_panics() {
    // A reified GROUND binary atom is a top-level `logic:Formula` whose reconstruction is a
    // trivially-Horn `Formula::Atom`. `LogicProgram::with_formulas` hard-asserts against such a
    // leaf, so the front-end MUST route it to `LogicProgram.axioms` (its Horn home) rather than
    // panic. This is the parse-layer root cause of the F2 conjecture-CLI panic.
    let (prog, _diags) = parse(
        "ex:phi a logic:Formula ;\n\
             logic:relation rdf:type ;\n\
             logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n\
             logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n",
    );
    // Routed to the fact home, NEVER kept as a formula (which would trip the assertion).
    assert!(
        prog.formulas.is_empty(),
        "a trivially-Horn leaf must not enter LogicProgram.formulas"
    );
    assert!(
        prog.axioms.iter().any(|a| {
            a.subject.ends_with("/a") && a.predicate.ends_with("#type") && a.obj.ends_with("/B")
        }),
        "the reified ground atom must be routed to LogicProgram.axioms, got {:?}",
        prog.axioms
    );
    assert!(
        !prog.axioms.iter().any(|a| {
            a.subject.ends_with("/phi") && a.predicate == RDF_TYPE && a.obj == logic_iri("Formula")
        }),
        "logic:Formula typing is owned by the formula extractor and must not be duplicated as a generic axiom: {:?}",
        prog.axioms
    );
}

// ── Minimal graph + reasoning contracts ───────────────────────────────

#[test]
fn parse_minimal_graph_succeeds() {
    let (prog, diags) = parse(
        "ex:Person a logic:Kind .
         logic:PositiveHornProfile a logic:ReasoningPreset .",
    );
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    assert!(has_axiom(&prog, "/Person", "#type", LogicModality::None));
    assert_eq!(prog.contracts.len(), 1);
    assert_eq!(
        prog.contracts[0].preset,
        Some(SemanticProfileId::PositiveHorn)
    );
    assert_eq!(prog.source_iri.as_deref(), Some("https://example.org/prog"));
}

#[test]
fn parse_multiple_contracts_with_complexity() {
    let (prog, _) = parse(
        "logic:PositiveHornProfile a logic:ReasoningPreset ;
            logic:complexityClass \"PTIME\" .
         logic:StableModelProfile a logic:ReasoningPreset .",
    );
    assert_eq!(prog.contracts.len(), 2);
    let horn = prog
        .contracts
        .iter()
        .find(|c| c.preset == Some(SemanticProfileId::PositiveHorn))
        .unwrap();
    assert_eq!(horn.complexity.as_ref().unwrap().to_string(), "PTIME");
}

#[test]
fn unknown_semantic_profile_emits_diagnostic() {
    let (prog, diags) = parse("ex:Bogus a logic:ReasoningPreset .");
    assert!(prog.contracts.is_empty());
    assert!(diags.iter().any(|d| d.code == "UNKNOWN_PROFILE"));
}

#[test]
fn unknown_semantic_profile_is_a_hard_error() {
    // Greenfield (reviewer C3): an unrecognised preset reference is a hard error,
    // not a fail-soft warning — otherwise it is a silent approximation.
    let (_, diags) = parse("ex:Bogus a logic:ReasoningPreset .");
    assert!(
        diags
            .iter()
            .any(|d| d.code == "UNKNOWN_PROFILE" && d.severity == Severity::Error)
    );
}

// ── Compatibility firewall (Task 3 / reviewer C3) ──────────────────────

#[test]
fn unsupported_contract_is_a_hard_compile_failure() {
    // A contract pairing logic:ProbabilisticMeasure with logic:StableModelSemantics
    // is a forbidden combination; it MUST surface as a Severity::Error so the
    // compile Report is not ok and the program is never treated as soundly
    // evaluable.  Parallel to the cut-confinement firewall discipline.
    let (_, diags) = parse(
        "ex:UnsupportedContract a logic:ReasoningContract ;
            logic:modelSemantics logic:StableModelSemantics ;
            logic:uncertaintyMeasure logic:ProbabilisticMeasure .
         logic:StableModelSemantics a logic:ModelSemantics .
         logic:ProbabilisticMeasure a logic:UncertaintyMeasure .",
    );
    let unsupported: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "UNSUPPORTED_CONTRACT")
        .collect();
    assert!(
        unsupported.iter().any(|d| d.severity == Severity::Error),
        "expected a Severity::Error UNSUPPORTED_CONTRACT finding; got: {diags:?}"
    );
}

#[test]
fn supported_contract_compiles_clean() {
    // A clean stable-model contract (no probabilistic measure) is supported.
    let (_, diags) = parse(
        "ex:CleanContract a logic:ReasoningContract ;
            logic:modelSemantics logic:StableModelSemantics ;
            logic:negationOperator logic:DefaultNegation .
         logic:StableModelSemantics a logic:ModelSemantics .
         logic:DefaultNegation a logic:NegationOperator .",
    );
    assert!(
        !diags.iter().any(|d| d.code == "UNSUPPORTED_CONTRACT"),
        "clean contract should not be flagged unsupported; got: {diags:?}"
    );
}

#[test]
fn probabilistic_measure_without_model_is_unsupported() {
    // Reviewer C4: a probabilistic measure with NO declared logic:ProbabilityModel
    // is a hard error (never a silent independence assumption).
    let (_, diags) = parse(
        "ex:ProbContract a logic:ReasoningContract ;
            logic:uncertaintyMeasure logic:ProbabilisticMeasure .
         logic:ProbabilisticMeasure a logic:UncertaintyMeasure .",
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "UNSUPPORTED_CONTRACT" && d.severity == Severity::Error),
        "probabilistic measure without a model must be a hard error; got: {diags:?}"
    );
}

#[test]
fn paraconsistent_valuation_under_counterfactual_revision_is_a_hard_compile_failure() {
    // Forbidden combo (RuleNoParaconsistentCounterfactualRevision), exercised end-to-end
    // through the front-end: a gap/glut-admitting admissible valuation cannot coexist with
    // counterfactual entrenchment revision. The closest-world generator that builds the
    // counterfactual states is undefined over gappy/glutty valuations, so the compile Report
    // must be not ok — never a silent approximation.
    let (_, diags) = parse(
        "ex:ParaCfContract a logic:ReasoningContract ;
            logic:admissibleValuation logic:AdmitAllFour ;
            logic:revision logic:EntrenchmentRevision .
         logic:AdmitAllFour a logic:AdmissibleValuationPolicy .
         logic:EntrenchmentRevision a logic:RevisionPolicy .",
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "UNSUPPORTED_CONTRACT" && d.severity == Severity::Error),
        "paraconsistent valuation under counterfactual revision must be a hard error; got: {diags:?}"
    );
}

#[test]
fn closed_world_closure_under_counterfactual_revision_is_a_hard_compile_failure() {
    // Forbidden combo (RuleNoClosedWorldInCounterfactual), exercised end-to-end: a
    // closed-world (negation-by-absence) default closure cannot coexist with counterfactual
    // entrenchment revision, whose generated states are open-ended. Reading absence as
    // falsehood inside them is unsound, so the compile Report must be not ok.
    let (_, diags) = parse(
        "ex:CwaCfContract a logic:ReasoningContract ;
            logic:defaultClosure logic:ClosedWorldClosure ;
            logic:revision logic:EntrenchmentRevision .
         logic:ClosedWorldClosure a logic:ClosureValue .
         logic:EntrenchmentRevision a logic:RevisionPolicy .",
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "UNSUPPORTED_CONTRACT" && d.severity == Severity::Error),
        "closed-world closure under counterfactual revision must be a hard error; got: {diags:?}"
    );
}

#[test]
fn probabilistic_measure_with_declared_model_is_supported() {
    // With a declared logic:ProbabilityModel the probabilistic measure is fine.
    let (_, diags) = parse(
        "ex:ProbContract a logic:ReasoningContract ;
            logic:uncertaintyMeasure logic:ProbabilisticMeasure .
         logic:ProbabilisticMeasure a logic:UncertaintyMeasure .
         ex:myModel a logic:ProbabilityModel .",
    );
    assert!(
        !diags.iter().any(|d| d.code == "UNSUPPORTED_CONTRACT"),
        "probabilistic measure with a declared model is supported; got: {diags:?}"
    );
}

// ── Meta-config does not leak into domain axioms (Gap 1) ───────────────

#[test]
fn contract_facet_config_does_not_leak_into_domain_axioms() {
    // A logic:ReasoningPreset's facet-config triples (expandsToFacet, defaultClosure,
    // …) are contract configuration consumed by extract_contracts — they MUST NOT
    // surface as domain LogicAxioms (which would pollute the Datalog / N3 / ledger
    // projections). A genuine domain triple alongside them still survives.
    let (prog, diags) = parse(
        "logic:PositiveHornProfile a logic:ReasoningPreset ;
            logic:expandsToFacet logic:ProceduralExecution ;
            logic:defaultClosure logic:OpenWorldClosure .
         logic:ProceduralExecution a logic:ResourcePolicy .
         logic:OpenWorldClosure a logic:ClosureValue .
         ex:Bird logic:subClassOf ex:Animal .",
    );
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    // The genuine domain triple survives.
    assert!(
        prog.axioms
            .iter()
            .any(|a| a.subject.ends_with("/Bird") && a.predicate.ends_with("subClassOf")),
        "domain axiom must survive; got: {:?}",
        prog.axioms
    );

    // ZERO facet-config axioms leaked.
    assert!(
        !prog
            .axioms
            .iter()
            .any(|a| a.predicate.ends_with("expandsToFacet")
                || a.predicate.ends_with("defaultClosure")),
        "no facet-config triple may leak into prog.axioms; got: {:?}",
        prog.axioms
    );
}

// ── Malformed ClosureEntry hard-fail (Gap 4) ───────────────────────────

#[test]
fn closure_entry_missing_value_is_a_hard_error() {
    // A closureEntry node missing logic:closureValue is malformed: emit a
    // MALFORMED_CLOSURE_ENTRY Severity::Error so the compile report is not ok,
    // never a silent skip.
    let (_, diags) = parse(
        "ex:BadClosure a logic:ReasoningContract ;
            logic:closureEntry [
                a logic:ClosureEntry ;
                logic:closureKey \"ex:pred\"
            ] .",
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "MALFORMED_CLOSURE_ENTRY" && d.severity == Severity::Error),
        "a closureEntry missing closureValue must be a hard error; got: {diags:?}"
    );
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

// ── Contract complexity guards ───────────────────────────────────────────────

#[test]
fn empty_complexity_class_emits_diagnostic() {
    let (prog, diags) = parse(
        "logic:PositiveHornProfile a logic:ReasoningPreset ;
            logic:complexityClass \"\" .",
    );
    assert!(diags.iter().any(|d| d.code == "INVALID_COMPLEXITY_CLASS"));
    // The contract is still recorded, just without a complexity class.
    assert_eq!(prog.contracts.len(), 1);
    assert!(prog.contracts[0].complexity.is_none());
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

// The diagnostics_report projection is tested in crate::logic_diagnostics
// (it returns the PyO3-tainted gmeow_errors::Report and lives runtime-side,
// out of the wasm-able compiler).

// ── Path shapes ──────────────────────────────────────────────────────

fn path_shape<'a>(prog: &'a LogicProgram, iri_suffix: &str) -> &'a PathShapeIr {
    prog.path_shapes
        .iter()
        .find(|s| s.iri.ends_with(iri_suffix))
        .unwrap_or_else(|| {
            panic!(
                "no path shape ending in {iri_suffix:?}: {:?}",
                prog.path_shapes
            )
        })
}

#[test]
fn parse_wildcard_namespace_scoped_bounded_path_shape() {
    let (prog, diags) = parse(
        "ex:nearbyOrgs a logic:PathShape ;
            logic:pathWildcard true ;
            logic:pathNamespaceScope \"https://example.org/org/\"^^xsd:anyURI ;
            logic:pathMinDepth 1 ;
            logic:pathMaxDepth 2 ;
            logic:pathDepthParam \"maxDepth\" .",
    );
    assert!(
        diags.iter().all(|d| d.code != "MALFORMED_PATH_SHAPE"),
        "unexpected path-shape diagnostics: {diags:?}"
    );
    let s = path_shape(&prog, "/nearbyOrgs");
    assert_eq!(s.base, PathBase::Wildcard);
    assert_eq!(s.min_depth, 1);
    assert_eq!(s.max_depth, Some(2));
    assert_eq!(
        s.namespace_scope.as_deref(),
        Some("https://example.org/org/")
    );
    assert_eq!(s.depth_param.as_deref(), Some("maxDepth"));
}

#[test]
fn parse_named_predicate_bounded_path_shape_defaults_min_one() {
    // No logic:pathMinDepth → defaults to 1; named-predicate step.
    let (prog, _diags) = parse(
        "ex:ancestorsTo3 a logic:PathShape ;
            logic:pathStepPredicate ex:parentOf ;
            logic:pathMaxDepth 3 ;
            logic:pathDepthParam \"maxDepth\" .",
    );
    let s = path_shape(&prog, "/ancestorsTo3");
    assert_eq!(
        s.base,
        PathBase::NamedPredicate("https://example.org/test/parentOf".to_owned())
    );
    assert_eq!(s.min_depth, 1);
    assert_eq!(s.max_depth, Some(3));
    assert_eq!(s.namespace_scope, None);
}

#[test]
fn parse_unbounded_path_shape_has_no_max() {
    // No logic:pathMaxDepth → unbounded (transitive-closure reading).
    let (prog, _diags) = parse(
        "ex:reaches a logic:PathShape ;
            logic:pathStepPredicate ex:linksTo .",
    );
    let s = path_shape(&prog, "/reaches");
    assert_eq!(s.max_depth, None);
    assert_eq!(s.min_depth, 1);
}

#[test]
fn malformed_path_shape_both_named_and_wildcard_is_skipped_with_diagnostic() {
    let (prog, diags) = parse(
        "ex:bad a logic:PathShape ;
            logic:pathStepPredicate ex:p ;
            logic:pathWildcard true .",
    );
    assert!(
        prog.path_shapes.is_empty(),
        "malformed shape must be skipped: {:?}",
        prog.path_shapes
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "MALFORMED_PATH_SHAPE" && d.message.contains("BOTH")),
        "expected a BOTH-step diagnostic: {diags:?}"
    );
}

#[test]
fn malformed_path_shape_min_above_max_is_skipped_with_diagnostic() {
    let (prog, diags) = parse(
        "ex:inverted a logic:PathShape ;
            logic:pathWildcard true ;
            logic:pathMinDepth 3 ;
            logic:pathMaxDepth 1 .",
    );
    assert!(
        prog.path_shapes.is_empty(),
        "inverted range must be skipped"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "MALFORMED_PATH_SHAPE" && d.message.contains("must not exceed")),
        "expected a min>max diagnostic: {diags:?}"
    );
}

#[test]
fn malformed_path_shape_no_step_is_skipped_with_diagnostic() {
    let (prog, diags) = parse("ex:nostep a logic:PathShape ; logic:pathMaxDepth 2 .");
    assert!(prog.path_shapes.is_empty());
    assert!(
        diags
            .iter()
            .any(|d| d.code == "MALFORMED_PATH_SHAPE" && d.message.contains("neither")),
        "expected a neither-step diagnostic: {diags:?}"
    );
}

#[test]
fn path_shapes_are_canonically_ordered() {
    let (prog, _diags) = parse(
        "ex:zeta  a logic:PathShape ; logic:pathStepPredicate ex:p .
         ex:alpha a logic:PathShape ; logic:pathStepPredicate ex:p .",
    );
    let iris: Vec<&str> = prog.path_shapes.iter().map(|s| s.iri.as_str()).collect();
    let mut sorted = iris.clone();
    sorted.sort_unstable();
    assert_eq!(
        iris, sorted,
        "path shapes must be in canonical (sorted) order"
    );
}

// ── G7: pathWildcard boolean fidelity ───────────────────────────────────────

#[test]
fn path_shape_wildcard_accepts_xsd_boolean_one() {
    // G7: the xsd:boolean value "1" must be accepted as wildcard = true.
    let (prog, diags) = parse(
        "ex:wc a logic:PathShape ;
            logic:pathWildcard \"1\"^^xsd:boolean .",
    );
    assert!(
        diags.iter().all(|d| d.code != "MALFORMED_PATH_SHAPE"),
        "\"1\" must not produce a path-shape diagnostic: {diags:?}"
    );
    let s = path_shape(&prog, "/wc");
    assert_eq!(
        s.base,
        PathBase::Wildcard,
        "\"1\" must parse as wildcard=true"
    );
}

#[test]
fn path_shape_wildcard_rejects_unrecognized_literal() {
    // G7: an unrecognized boolean literal must produce a MALFORMED_PATH_SHAPE
    // diagnostic and the shape must be skipped (hard-fail, no silent coercion).
    let (prog, diags) = parse(
        "ex:bad a logic:PathShape ;
            logic:pathWildcard \"yes\" .",
    );
    assert!(
        prog.path_shapes.is_empty(),
        "shape with unrecognized wildcard literal must be skipped: {:?}",
        prog.path_shapes
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "MALFORMED_PATH_SHAPE" && d.message.contains("unrecognized")),
        "expected a MALFORMED_PATH_SHAPE diagnostic mentioning unrecognized: {diags:?}"
    );
}

#[test]
fn path_shape_wildcard_false_literal_yields_no_wildcard() {
    // G7: "false" is valid and must NOT mark the shape as wildcard.
    // Here we pair it with a step predicate to verify "false" is accepted cleanly
    // (i.e., does not trigger the unrecognized-literal hard-fail).
    let (prog, diags) = parse(
        "ex:notWild a logic:PathShape ;
            logic:pathStepPredicate ex:p ;
            logic:pathWildcard \"false\" .",
    );
    assert!(
        diags.iter().all(|d| d.code != "MALFORMED_PATH_SHAPE"),
        "\"false\" must not produce a path-shape diagnostic: {diags:?}"
    );
    let s = path_shape(&prog, "/notWild");
    assert_eq!(
        s.base,
        PathBase::NamedPredicate("https://example.org/test/p".to_owned()),
        "step predicate must win when wildcard is false"
    );
}

// ── CR1: a non-IRI logic:pathStepPredicate is rejected ───────────────────────

#[test]
fn path_shape_literal_step_predicate_is_skipped_with_diagnostic() {
    // CR1: a logic:pathStepPredicate that is a LITERAL (not an IRI named node)
    // would build a malformed predicate IRI downstream — reject the shape with a
    // MALFORMED_PATH_SHAPE diagnostic, never silently coerce it.
    let (prog, diags) = parse(
        "ex:litStep a logic:PathShape ;
            logic:pathStepPredicate \"not-an-iri\" .",
    );
    assert!(
        prog.path_shapes.is_empty(),
        "a literal step predicate must be skipped: {:?}",
        prog.path_shapes
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "MALFORMED_PATH_SHAPE" && d.message.contains("IRI named node")),
        "expected a MALFORMED_PATH_SHAPE diagnostic about an IRI named node: {diags:?}"
    );
}

// ── CR2: PathShapeIr::new() Err branches surface as MALFORMED_PATH_SHAPE + skip ──

#[test]
fn path_shape_empty_namespace_scope_is_skipped_with_diagnostic() {
    // CR2: an empty logic:pathNamespaceScope makes PathShapeIr::new() Err; the
    // front-end must surface it as MALFORMED_PATH_SHAPE and skip the shape.
    let (prog, diags) = parse(
        "ex:emptyNs a logic:PathShape ;
            logic:pathWildcard true ;
            logic:pathNamespaceScope \"\"^^xsd:anyURI .",
    );
    assert!(
        prog.path_shapes.is_empty(),
        "an empty namespace scope must be skipped: {:?}",
        prog.path_shapes
    );
    assert!(
        diags.iter().any(|d| d.code == "MALFORMED_PATH_SHAPE"),
        "expected a MALFORMED_PATH_SHAPE diagnostic for an empty namespace scope: {diags:?}"
    );
}

#[test]
fn path_shape_max_depth_above_cap_is_skipped_with_diagnostic() {
    // CR2: a logic:pathMaxDepth above MAX_PATH_DEPTH (1000) makes PathShapeIr::new()
    // Err; the front-end must surface it as MALFORMED_PATH_SHAPE and skip the shape.
    let (prog, diags) = parse(
        "ex:tooDeep a logic:PathShape ;
            logic:pathWildcard true ;
            logic:pathMinDepth 1 ;
            logic:pathMaxDepth 1001 .",
    );
    assert!(
        prog.path_shapes.is_empty(),
        "a max depth above the cap must be skipped: {:?}",
        prog.path_shapes
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "MALFORMED_PATH_SHAPE" && d.message.contains("hard cap")),
        "expected a MALFORMED_PATH_SHAPE diagnostic mentioning the hard cap: {diags:?}"
    );
}

// ── Quantifier binder hard-fails: a malformed/vacuous binder must NOT silently narrow ──

#[test]
fn quantifier_with_malformed_bound_var_is_malformed_not_silently_narrowed() {
    // A ∀ node whose binder carrier is missing its logic:termVariable must surface a
    // MALFORMED_FORMULA diagnostic and be skipped — never silently parsed as ∀{} or a
    // narrower binder (which would change the formula's meaning).
    let (prog, diags) = parse(
        "ex:f1 a logic:Formula ;
            logic:forall ex:body1 ;
            logic:quantifiedVariable ex:qv1 .
         ex:qv1 logic:termIndex 0 .
         ex:body1 a logic:Formula ;
            logic:relation ex:p ;
            logic:argument ex:arg1 .
         ex:arg1 logic:termIndex 0 ; logic:termVariable \"x\" .",
    );
    assert!(
        prog.formulas.is_empty(),
        "a malformed binder must be skipped, not narrowed: {:?}",
        prog.formulas
    );
    let malformed: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "MALFORMED_FORMULA")
        .collect();
    assert_eq!(malformed.len(), 1, "one binder defect must report once");
    assert_eq!(
        malformed[0].subject.as_deref(),
        Some("https://example.org/test/f1"),
        "the owning quantifier, not its term carrier, is the formula identity"
    );
    assert!(
        malformed[0].message.contains("https://example.org/test/f1")
            && malformed[0]
                .message
                .contains("https://example.org/test/qv1"),
        "the message must name both owning formula and malformed carrier: {malformed:?}"
    );
}

#[test]
fn vacuous_quantifier_is_malformed() {
    // A ∀ node with a body but ZERO logic:quantifiedVariable carriers is a vacuous binder
    // — almost always a malformed source. It must surface MALFORMED_FORMULA, not parse as
    // ∀{}.
    let (prog, diags) = parse(
        "ex:f1 a logic:Formula ;
            logic:forall ex:body1 .
         ex:body1 a logic:Formula ;
            logic:relation ex:p ;
            logic:argument ex:arg1 .
         ex:arg1 logic:termIndex 0 ; logic:termVariable \"x\" .",
    );
    assert!(
        prog.formulas.is_empty(),
        "a vacuous quantifier must be skipped: {:?}",
        prog.formulas
    );
    assert!(
        diags.iter().any(|d| d.code == "MALFORMED_FORMULA"),
        "expected a MALFORMED_FORMULA diagnostic for a vacuous quantifier: {diags:?}"
    );
}

fn assert_malformed_formula_error(ttl: &str, expected_detail: &str) {
    let (prog, diags) = parse(ttl);
    assert!(
        prog.formulas.is_empty(),
        "a malformed formula must never enter the IR: {:?}",
        prog.formulas
    );
    assert!(
        diags.iter().any(|d| {
            d.code == "MALFORMED_FORMULA"
                && d.severity == Severity::Error
                && d.message.contains(expected_detail)
        }),
        "expected an error-grade MALFORMED_FORMULA containing {expected_detail:?}: {diags:?}"
    );
}

#[test]
fn formula_constructor_is_exclusive_and_cardinalities_are_strict() {
    let cases = [
        (
            "ex:f a logic:Formula ; logic:relation ex:p ; logic:not ex:child ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] .
             ex:child a logic:Formula ; logic:relation ex:q ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] .",
            "exactly one constructor family",
        ),
        (
            "ex:f a logic:Formula ; logic:and ex:child .
             ex:child a logic:Formula ; logic:relation ex:q ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] .",
            "at least two operands",
        ),
        (
            "ex:f a logic:Formula ; logic:iff ex:child .
             ex:child a logic:Formula ; logic:relation ex:q ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] .",
            "exactly two operands",
        ),
        (
            "ex:f a logic:Formula ; logic:antecedent ex:child .
             ex:child a logic:Formula ; logic:relation ex:q ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] .",
            "exactly one logic:consequent",
        ),
        (
            "ex:f a logic:Formula ; logic:not ex:left, ex:right .
             ex:left a logic:Formula ; logic:relation ex:p ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] .
             ex:right a logic:Formula ; logic:relation ex:q ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] .",
            "exactly one logic:not",
        ),
    ];

    for (ttl, expected_detail) in cases {
        assert_malformed_formula_error(ttl, expected_detail);
    }
}

#[test]
fn term_carrier_values_and_indices_are_total_and_unambiguous() {
    let cases = [
        (
            "ex:f a logic:Formula ; logic:relation ex:p ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ],
                            [ logic:termIndex 0 ; logic:termIri ex:a ] .",
            "unique and contiguous",
        ),
        (
            "ex:f a logic:Formula ; logic:relation ex:p ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ],
                            [ logic:termIndex 2 ; logic:termIri ex:a ] .",
            "unique and contiguous",
        ),
        (
            "ex:f a logic:Formula ; logic:relation ex:p ;
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ;
                              logic:termIri ex:a ] .",
            "exactly one term-value property",
        ),
        (
            "ex:f a logic:Formula ; logic:relation ex:p ;
             logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ;
                              logic:termLiteralDatatype xsd:string ] .",
            "only with logic:termLiteral",
        ),
        (
            "ex:f a logic:Formula ; logic:relation ex:p ;
             logic:argument [ logic:termIndex 0 ; logic:termLiteral \"a\" ;
                              logic:termLiteralDatatype \"xsd:string\" ] .",
            "IRI-valued logic:termLiteralDatatype",
        ),
    ];

    for (ttl, expected_detail) in cases {
        assert_malformed_formula_error(ttl, expected_detail);
    }
}

#[test]
fn recursive_formula_cycle_is_an_error_even_without_a_top_level_root() {
    assert_malformed_formula_error(
        "ex:left a logic:Formula ; logic:not ex:right .
         ex:right a logic:Formula ; logic:not ex:left .",
        "recursive constructor cycle",
    );
}

#[test]
fn malformed_child_reports_once_at_the_exact_child_not_a_prefix_colliding_ancestor() {
    let (prog, diags) = parse(
        "ex:f a logic:Formula ; logic:not ex:f1 .
         ex:f1 a logic:Formula .",
    );
    assert!(prog.formulas.is_empty());
    let malformed: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "MALFORMED_FORMULA")
        .collect();
    assert_eq!(malformed.len(), 1, "the child failure must not cascade");
    assert_eq!(
        malformed[0].subject.as_deref(),
        Some("https://example.org/test/f1")
    );
    assert!(malformed[0].message.contains("https://example.org/test/f1"));
}

#[test]
fn malformed_term_carrier_reports_once_at_its_owning_formula() {
    let (prog, diags) = parse(
        "ex:f a logic:Formula ; logic:relation ex:p ; logic:argument ex:arg .
         ex:arg logic:termIndex \"not-an-index\" ; logic:termVariable \"x\" .",
    );
    assert!(prog.formulas.is_empty());
    let malformed: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "MALFORMED_FORMULA")
        .collect();
    assert_eq!(malformed.len(), 1);
    assert_eq!(
        malformed[0].subject.as_deref(),
        Some("https://example.org/test/f")
    );
    assert!(
        malformed[0].message.contains("https://example.org/test/f")
            && malformed[0]
                .message
                .contains("https://example.org/test/arg"),
        "the diagnostic must identify both formula and carrier: {malformed:?}"
    );
}

#[test]
fn malformed_constraint_integrity_has_one_authoritative_formula_diagnostic() {
    let (prog, diags) = parse(
        "ex:c a logic:Constraint ;
             logic:integrity ex:integrity ;
             logic:severity \"Violation\" .
         ex:integrity a logic:Formula ; logic:not ex:badChild .
         ex:badChild a logic:Formula .",
    );
    assert!(prog.constraints.is_empty());
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_str(),
                "MALFORMED_FORMULA" | "MALFORMED_CONSTRAINT"
            )
        })
        .collect();
    assert_eq!(relevant.len(), 1, "formula cause must not be wrapped twice");
    assert_eq!(relevant[0].code, "MALFORMED_FORMULA");
    assert_eq!(
        relevant[0].subject.as_deref(),
        Some("https://example.org/test/badChild")
    );
}

#[test]
fn independent_constraint_defect_survives_formula_deduplication() {
    let (prog, diags) = parse(
        "ex:c a logic:Constraint ;
             logic:integrity ex:integrity ;
             logic:severity \"NotASeverity\" .
         ex:integrity a logic:Formula ; logic:not ex:badChild .
         ex:badChild a logic:Formula .",
    );
    assert!(prog.constraints.is_empty());
    assert_eq!(
        diags
            .iter()
            .filter(|d| d.code == "MALFORMED_FORMULA")
            .count(),
        1
    );
    assert!(diags.iter().any(|d| {
        d.code == "MALFORMED_CONSTRAINT"
            && d.subject.as_deref() == Some("https://example.org/test/c")
            && d.message.contains("NotASeverity")
    }));
}

#[test]
fn formula_cycle_identity_and_message_are_traversal_order_independent() {
    let (_, left_first) = parse(
        "ex:left a logic:Formula ; logic:not ex:right .
         ex:right a logic:Formula ; logic:not ex:left .",
    );
    let (_, right_first) = parse(
        "ex:right a logic:Formula ; logic:not ex:left .
         ex:left a logic:Formula ; logic:not ex:right .",
    );
    let select = |diags: &[Diagnostic]| {
        diags
            .iter()
            .filter(|d| d.code == "MALFORMED_FORMULA")
            .map(|d| (d.subject.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    };
    let left = select(&left_first);
    let right = select(&right_first);
    assert_eq!(left, right);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].0.as_deref(), Some("https://example.org/test/left"));
    assert!(
        left[0].1.contains("https://example.org/test/left")
            && left[0].1.contains("https://example.org/test/right")
    );
}

// ── Derived validation shapes (OWL restrictions → closed-world SHACL) ─────────

/// [`AUTHORING_NAMESPACES`] is THE single authoring-namespace authority — the derive's own
/// dogfooding boundary AND the `shape-migrate` injector's eligibility test (`gmeow-dev-cli`)
/// both consume this exact set. Pin it to exactly the four namespaces so an accidental
/// addition/removal reds here instead of silently drifting one of its two consumers out of
/// sync with the other.
#[test]
fn authoring_namespaces_is_pinned_to_the_four_dogfooded_namespaces() {
    assert_eq!(
        AUTHORING_NAMESPACES,
        [
            "https://blackcatinformatics.ca/gmeow/",
            "https://blackcatinformatics.ca/math/",
            "https://blackcatinformatics.ca/lang/",
            "https://blackcatinformatics.ca/logic/",
        ]
    );
}

#[test]
fn is_authoring_namespace_accepts_every_dogfooded_namespace_and_rejects_external() {
    for ns in AUTHORING_NAMESPACES {
        assert!(
            is_authoring_namespace(&format!("{ns}Example")),
            "{ns} must be accepted as an authoring namespace"
        );
    }
    assert!(!is_authoring_namespace("http://xmlns.com/foaf/0.1/Person"));
    assert!(!is_authoring_namespace(
        "https://ontologies.gufo.example/gufo#Object"
    ));
}

/// Parse a Turtle fragment into a dataset for [`derive_validation_shapes`]. The `g:` prefix is
/// the GMEOW authoring namespace, so its classes are in-scope for derivation.
fn shape_dataset(ttl: &str) -> std::sync::Arc<RdfDataset> {
    let full = format!(
        "@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix owl:  <http://www.w3.org/2002/07/owl#> .\n\
         @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix g:    <https://blackcatinformatics.ca/gmeow/> .\n{ttl}"
    );
    parse_dataset(full.as_bytes(), "text/turtle", None).expect("parse dataset ok")
}

/// Flatten a component and its nested inners (a `QualifiedValueShape`'s inner shape and a
/// `Not`'s inner component) into `out`, so a classification assertion still bites even after
/// `someValuesFrom` began wrapping its classification in a `QualifiedValueShape{min:1}`.
fn flatten_component(c: &ConstraintComponent, out: &mut Vec<ConstraintComponent>) {
    out.push(c.clone());
    match c {
        ConstraintComponent::QualifiedValueShape { shape, .. } => {
            for inner in shape {
                flatten_component(inner, out);
            }
        }
        ConstraintComponent::Not(inner) => flatten_component(inner, out),
        _ => {}
    }
}

/// Every constraint component of every shape — both path components and focus-node components —
/// flattened so nested `QualifiedValueShape` / `Not` inners are visible to a `matches!` assertion.
fn all_components(shapes: &[ValidationShapeIr]) -> Vec<ConstraintComponent> {
    let mut out = Vec::new();
    for s in shapes {
        for p in &s.properties {
            for c in &p.components {
                flatten_component(c, &mut out);
            }
        }
        for c in &s.node_components {
            flatten_component(c, &mut out);
        }
    }
    out
}

#[test]
fn derive_logic_restrictions_matches_the_lowered_owl_spelling() {
    let owl = shape_dataset(
        "g:Record a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:item ; owl:allValuesFrom g:Item ] , \
         [ a owl:Restriction ; owl:onProperty g:item ; \
           owl:maxQualifiedCardinality 1 ; owl:onClass g:Item ] .",
    );
    let logic = shape_dataset(
        "g:Record a owl:Class ; logic:subClassOf \
         [ a logic:Restriction ; logic:onProperty g:item ; logic:allValuesFrom g:Item ] , \
         [ a logic:Restriction ; logic:onProperty g:item ; \
           logic:maxQualifiedCardinality 1 ; logic:onClass g:Item ] .",
    );

    let owl_shapes = derive_validation_shapes(owl.as_ref()).expect("derive OWL spelling");
    let logic_shapes =
        derive_validation_shapes(logic.as_ref()).expect("derive canonical logic spelling");

    assert_eq!(
        logic_shapes, owl_shapes,
        "canonical logic: restrictions must derive the same ValidationShapeIr as their OWL projection"
    );
    let record = logic_shapes
        .iter()
        .find(|shape| shape.iri.ends_with("/Record-shape"))
        .expect("logic-authored Record restriction must produce a class shape");
    assert_eq!(record.properties.len(), 1, "same-path restrictions merge");
    assert_eq!(
        record.properties[0].path,
        "https://blackcatinformatics.ca/gmeow/item"
    );
}

#[test]
fn derive_anonymous_one_of_filler_lowers_to_sh_in() {
    // An allValuesFrom whose filler is an anonymous enumeration `[ owl:oneOf ( a b ) ]` reads
    // closed-world as a value set on the path (`sh:in`), never a class-membership check.
    let ds = shape_dataset(
        "g:Trial a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:sidedness ; \
           owl:allValuesFrom [ owl:oneOf ( g:oneSided g:twoSided ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::In(vs) if vs.len() == 2
        )),
        "expected an sh:in over the two enumerated individuals: {comps:?}"
    );
}

#[test]
fn derive_union_of_bare_existential_restrictions_lowers_to_or_properties() {
    // `K ⊑ (∃p1.Thing ⊔ ∃p2.Thing)` — the either-of-these-properties existence obligation —
    // reads closed-world as the node-level property-alternatives disjunction.
    let ds = shape_dataset(
        "g:Framed a owl:Class ; rdfs:subClassOf [ owl:unionOf ( \
           [ a owl:Restriction ; owl:onProperty g:hasFrame ; owl:someValuesFrom owl:Thing ] \
           [ a owl:Restriction ; owl:onProperty g:hasModel ; owl:someValuesFrom owl:Thing ] ) ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let framed = shapes
        .iter()
        .find(|s| s.iri.contains("Framed"))
        .expect("Framed shape derived");
    assert!(
        framed.node_components.iter().any(|c| matches!(
            c,
            ConstraintComponent::OrProperties(paths)
                if paths.len() == 2
                    && paths.iter().any(|p| p.ends_with("hasFrame"))
                    && paths.iter().any(|p| p.ends_with("hasModel"))
        )),
        "expected an OrProperties node component: {:?}",
        framed.node_components
    );
    // The emitted SHACL carries the node-level sh:or over sh:path branches.
    let ttl = crate::projections::shapes::project_validation_shape_shacl(framed);
    assert!(
        ttl.contains(
            "sh:or ( [ sh:path <https://blackcatinformatics.ca/gmeow/hasFrame> ; sh:minCount 1 ]"
        ),
        "{ttl}"
    );
}

#[test]
fn derive_union_with_a_non_existential_member_stays_in_the_canon() {
    // A union carrying a NAMED-class member is a genuine class disjunction — it must NOT be
    // partially read as a property-alternatives disjunction.
    let ds = shape_dataset(
        "g:Mixed a owl:Class ; rdfs:subClassOf [ owl:unionOf ( \
           [ a owl:Restriction ; owl:onProperty g:hasFrame ; owl:someValuesFrom owl:Thing ] \
           g:NamedAlternative ) ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| s
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::OrProperties(_)))),
        "a mixed union must not derive a partial property disjunction: {shapes:?}"
    );
}

#[test]
fn derive_blank_restriction_domain_lowers_to_subjects_of_property_shape() {
    // A ClosedWorldClosure-opted-in property whose rdfs:domain is an anonymous restriction
    // `[ owl:onProperty q ; owl:minCardinality 1 ]` derives the required-companion condition on
    // the SubjectsOf(P) domain shape: every subject of P carries at least one q.
    let ds = shape_dataset(
        "g:lowersTo a owl:ObjectProperty ; \
           rdfs:domain [ a owl:Restriction ; owl:onProperty g:denotation ; owl:minCardinality 1 ] . \
         [ a <https://blackcatinformatics.ca/logic/ClosureEntry> ; \
           <https://blackcatinformatics.ca/logic/closureKey> \"https://blackcatinformatics.ca/gmeow/lowersTo\" ; \
           <https://blackcatinformatics.ca/logic/closureValue> <https://blackcatinformatics.ca/logic/ClosedWorldClosure> ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let domain_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("lowersTo")))
        .expect("a SubjectsOf(lowersTo) domain shape is derived");
    assert!(
        domain_shape
            .properties
            .iter()
            .any(|p| p.path.ends_with("denotation") && p.min_count == Some(1)),
        "expected a min-1 companion property on the domain shape: {domain_shape:?}"
    );
}

#[test]
fn derive_blank_restriction_domain_without_opt_in_derives_nothing() {
    // Without the ClosedWorldClosure opt-in, an anonymous restriction domain stays open-world:
    // no SubjectsOf shape is derived (domain/range are inference axioms by default).
    let ds = shape_dataset(
        "g:lowersTo a owl:ObjectProperty ; \
           rdfs:domain [ a owl:Restriction ; owl:onProperty g:denotation ; owl:minCardinality 1 ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes
            .iter()
            .any(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("lowersTo"))),
        "an un-opted-in anonymous domain must derive no shape: {shapes:?}"
    );
}

#[test]
fn derive_datatype_target_lowers_to_sh_datatype_not_sh_class() {
    // A someValuesFrom whose target is a datatype must NOT become sh:class — a literal is never
    // an instance of a class, so sh:class would flag every focus node. It becomes sh:datatype.
    let ds = shape_dataset(
        "g:Block a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:blockNumber ; owl:someValuesFrom xsd:integer ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(d) if d.ends_with("integer"))),
        "expected sh:datatype for an xsd:integer target: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(_))),
        "a datatype target must never emit sh:class: {comps:?}"
    );
}

#[test]
fn derive_class_target_stays_sh_class() {
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        all_components(&shapes)
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "a real class target must emit sh:class"
    );
}

#[test]
fn derived_shape_failure_class_dedupes_identical_values() {
    let ds = shape_dataset(
        "g:Widget a owl:Class ;
             g:enforcesFailureClass g:Failure, g:Failure ;
             rdfs:subClassOf [ a owl:Restriction ; owl:onProperty g:id ; owl:minCardinality 1 ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(
            |shape| matches!(&shape.target, ShapeTarget::Class(class) if class.ends_with("Widget")),
        )
        .expect("Widget shape");
    assert_eq!(
        shape.failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/Failure")
    );
}

#[test]
fn derived_shape_failure_class_rejects_distinct_values() {
    let ds = shape_dataset(
        "g:Widget a owl:Class ;
             g:enforcesFailureClass g:FailureA, g:FailureB ;
             rdfs:subClassOf [ a owl:Restriction ; owl:onProperty g:id ; owl:minCardinality 1 ] .",
    );
    let err = derive_validation_shapes(ds.as_ref()).expect_err("distinct metadata must fail");
    assert!(err.message().contains("distinct"), "{err}");
}

#[test]
fn derive_owl_thing_target_lowers_to_node_kind_not_sh_class() {
    // A someValuesFrom owl:Thing is an intentionally-open range ("any individual"). Under
    // spec-conformant SHACL, sh:class owl:Thing would demand a never-materialized rdf:type
    // owl:Thing edge and flag every value; the faithful projection is sh:nodeKind
    // sh:BlankNodeOrIRI (any resource, not a literal).
    let ds = shape_dataset(
        "g:Observation a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:observedFeature ; owl:someValuesFrom owl:Thing ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::BlankNodeOrIri)
        )),
        "an owl:Thing target must emit sh:nodeKind sh:BlankNodeOrIRI: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(_))),
        "an owl:Thing target must never emit sh:class: {comps:?}"
    );
}

#[test]
fn derive_rdfs_literal_target_lowers_to_node_kind_not_sh_datatype() {
    // A someValuesFrom rdfs:Literal is an intentionally-open literal range ("any literal").
    // Under spec-conformant SHACL, sh:datatype rdfs:Literal never matches a concrete literal
    // (rdfs:Literal is the class of all literals, not a lexical datatype), so it would flag
    // every value; the faithful projection is sh:nodeKind sh:Literal.
    let ds = shape_dataset(
        "g:Artifact a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:artifactMediaType ; owl:someValuesFrom rdfs:Literal ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::Literal)
        )),
        "an rdfs:Literal target must emit sh:nodeKind sh:Literal: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(_))),
        "an rdfs:Literal target must never emit sh:datatype: {comps:?}"
    );
}

#[test]
fn derive_owl_thing_target_lowers_to_node_kind_even_when_classified_as_datatype() {
    // Robustness guard for the universal-top projection. `is_datatype` treats anything typed
    // `a rdfs:Datatype` as a datatype range, so a quirk axiom `owl:Thing a rdfs:Datatype`
    // classifies the range with the datatype flag set. The projection must still emit the
    // faithful open node-kind (sh:BlankNodeOrIRI) rather than falling through to a vacuous
    // `sh:datatype owl:Thing` — i.e. the special-case guard is decoupled from the flag.
    let ds = shape_dataset(
        "owl:Thing a rdfs:Datatype .\n\
         g:Observation a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:observedFeature ; owl:someValuesFrom owl:Thing ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::BlankNodeOrIri)
        )),
        "an owl:Thing target must emit sh:nodeKind sh:BlankNodeOrIRI even when flagged a datatype: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(_))),
        "an owl:Thing target must never emit sh:datatype: {comps:?}"
    );
}

#[test]
fn derive_owl_cardinality_lifts_with_owl_provenance() {
    // Unqualified owl:min/maxCardinality lower to sh:minCount/sh:maxCount tagged as
    // OwlRestriction provenance — the open-world axiom read closed-world (ValidationOnly).
    let ds = shape_dataset(
        "g:Rec a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:id ; \
           owl:minCardinality \"1\"^^xsd:nonNegativeInteger ; \
           owl:maxCardinality \"3\"^^xsd:nonNegativeInteger ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let card = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .find(|p| p.min_count.is_some() || p.max_count.is_some())
        .expect("a cardinality property shape");
    assert_eq!(card.min_count, Some(1));
    assert_eq!(card.max_count, Some(3));
    assert_eq!(
        card.cardinality_provenance,
        Some(ConstraintProvenance::OwlRestriction),
        "OWL-restriction cardinality is a closed-world reading, never OptNative"
    );
}

#[test]
fn derive_merges_cardinality_and_class_on_same_path_into_one_property() {
    // A class that authors a cardinality restriction AND an owl:allValuesFrom class restriction on
    // ONE property must project to a SINGLE property shape carrying both — the conjunctive SHACL
    // reading of a single hand-authored `sh:property` block. Unmerged, the two same-path property
    // shapes would key distinctly from that block and defeat enforcement equivalence.
    let ds = shape_dataset(
        "g:StandpointClaim a owl:Class . \
         g:InferenceCommitment a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:conclusion ; \
           owl:cardinality \"1\"^^xsd:nonNegativeInteger ] , \
         [ a owl:Restriction ; owl:onProperty g:conclusion ; \
           owl:allValuesFrom g:StandpointClaim ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("InferenceCommitment")))
        .expect("a Class(InferenceCommitment) shape");
    let on_conclusion: Vec<_> = shape
        .properties
        .iter()
        .filter(|p| p.path.ends_with("conclusion"))
        .collect();
    assert_eq!(
        on_conclusion.len(),
        1,
        "cardinality + class on one path must merge into ONE property shape: {:?}",
        shape.properties
    );
    let p = on_conclusion[0];
    assert_eq!(p.min_count, Some(1), "exact cardinality gives min 1");
    assert_eq!(p.max_count, Some(1), "exact cardinality gives max 1");
    assert!(
        p.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("StandpointClaim"))),
        "the merged property carries the sh:class component: {:?}",
        p.components
    );
}

#[test]
fn derive_contradictory_cardinality_hard_fails() {
    // min > max is structurally impossible: the derivation HARD-FAILS rather than silently
    // dropping the malformed restriction (no fail-soft).
    let ds = shape_dataset(
        "g:Bad a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:id ; \
           owl:minCardinality \"5\"^^xsd:nonNegativeInteger ; \
           owl:maxCardinality \"2\"^^xsd:nonNegativeInteger ] .",
    );
    assert!(
        derive_validation_shapes(ds.as_ref()).is_err(),
        "min > max cardinality must hard-fail, not drop the shape"
    );
}

// ── Full closed-world OWL fragment: per family ───────────────────────────────

#[test]
fn derive_some_values_from_uses_class_membership_under_approximation() {
    // owl:someValuesFrom is EXISTENTIAL ("K ⊑ ∃P.C"), but a validation shape is a
    // `logic:ValidationOnly` UNDER-approximation that must never over-claim: a `qualifiedMinCount 1`
    // existential would false-positive on the ontology's own open-world value-vocabulary
    // individuals (instances of a restricted class that legitimately do not populate the relation).
    // So the shape projects the class-membership under-approximation (a bare sh:class, vacuously
    // true when the property is absent); the existence obligation is carried in the canon, not the
    // shape. It must NOT be wrapped in a qualified value shape.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps: Vec<_> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "someValuesFrom emits the class-membership under-approximation (bare sh:class): {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. })),
        "someValuesFrom must NOT over-claim with a qualifiedMinCount existential: {comps:?}"
    );
}

#[test]
fn derive_class_scoped_closed_all_values_from_requires_the_path() {
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:allValuesFrom g:Doc ] . \
         [] a logic:ClosureEntry ; logic:onClass g:Article ; logic:closureKey g:cites ; \
            logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let cites = shapes
        .iter()
        .flat_map(|shape| &shape.properties)
        .find(|property| property.path.ends_with("cites"))
        .expect("class-scoped closed universal projects a cites property shape");
    assert_eq!(cites.min_count, Some(1));
    assert!(
        cites.components.iter().any(
            |component| matches!(component, ConstraintComponent::Class(iri) if iri.ends_with("Doc"))
        ),
        "class-scoped closed universal retains its value class: {:?}",
        cites.components
    );
}

#[test]
fn derive_all_values_from_emits_bare_class_not_wrapped() {
    // owl:allValuesFrom is UNIVERSAL: every value is a g:Doc → a bare sh:class on the path,
    // never wrapped in a qualified value shape.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:allValuesFrom g:Doc ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps: Vec<_> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "allValuesFrom emits a bare sh:class: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. })),
        "allValuesFrom must NOT wrap in a qualified value shape: {comps:?}"
    );
}

#[test]
fn derive_rdfs_domain_targets_subjects_of_with_node_class() {
    // domain/range are open-world by default (inference axioms); a shape is derived only for a
    // property opted IN via a `logic:ClosedWorldClosure` closure entry.
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . g:cites a owl:ObjectProperty ; rdfs:domain g:Doc .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:cites ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("cites")))
        .expect("a SubjectsOf(cites) shape");
    assert!(
        shape
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "domain must attach a node-level sh:class g:Doc: {:?}",
        shape.node_components
    );
}

#[test]
fn derive_rdfs_domain_range_is_open_world_by_default_without_optin() {
    // rdfs:domain/range are inference axioms → OPEN-WORLD by default: with NO ClosedWorldClosure
    // opt-in, neither a SubjectsOf(domain) nor an ObjectsOf(range) shape is derived. (Falsifiable
    // proof of the open-world default; the opt-in tests above show the closed reading fires.)
    let ds = shape_dataset(
        "g:Doc a owl:Class . g:cites a owl:ObjectProperty ; rdfs:domain g:Doc ; rdfs:range g:Doc .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| matches!(
            &s.target,
            ShapeTarget::SubjectsOf(p) | ShapeTarget::ObjectsOf(p) if p.ends_with("cites")
        )),
        "domain/range must derive NO shape without a ClosedWorldClosure opt-in: {:?}",
        shapes.iter().map(|s| &s.target).collect::<Vec<_>>()
    );
}

#[test]
fn derive_rdfs_range_targets_objects_of_class_and_datatype() {
    // A class range → sh:class node component on an ObjectsOf shape. domain/range are open-world
    // by default; opt the property IN with a `logic:ClosedWorldClosure` closure entry.
    let class_ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . g:cites a owl:ObjectProperty ; rdfs:range g:Doc .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:cites ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let class_shapes = derive_validation_shapes(class_ds.as_ref()).expect("derive ok");
    let class_shape = class_shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("cites")))
        .expect("an ObjectsOf(cites) shape");
    assert!(
        class_shape
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "class range → sh:class node component: {:?}",
        class_shape.node_components
    );
    // An xsd:integer range → sh:datatype node component.
    let dt_ds = shape_dataset_with_logic(
        "g:blockNumber a owl:DatatypeProperty ; rdfs:range xsd:integer .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:blockNumber ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let dt_shapes = derive_validation_shapes(dt_ds.as_ref()).expect("derive ok");
    let dt_shape = dt_shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("blockNumber")))
        .expect("an ObjectsOf(blockNumber) shape");
    assert!(
        dt_shape
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(d) if d.ends_with("integer"))),
        "datatype range → sh:datatype node component: {:?}",
        dt_shape.node_components
    );
}

#[test]
fn derive_rdfs_resource_range_domain_is_vacuous_no_class_component() {
    // rdfs:Resource is the UNIVERSAL TOP (the class of everything, literals included). A
    // range/domain of rdfs:Resource is VACUOUS: `sh:class rdfs:Resource` would demand a
    // never-materialized `rdf:type rdfs:Resource` edge and false-positive universally, so the
    // the derivation must emit NO node/class component for it.
    // Opt the property IN to closed-world domain/range reading so a shape IS derived and the
    // "no vacuous rdfs:Resource class component" assertion below stays falsifiable.
    let range_ds = shape_dataset_with_logic(
        "g:usesTerm a owl:ObjectProperty ; rdfs:range rdfs:Resource .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:usesTerm ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let range_shapes = derive_validation_shapes(range_ds.as_ref()).expect("derive ok");
    if let Some(shape) = range_shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("usesTerm")))
    {
        assert!(
            !shape.node_components.iter().any(|c| matches!(
                c,
                ConstraintComponent::Class(d) if d.ends_with("Resource")
            )),
            "rdfs:Resource range must emit NO sh:class node component (vacuous top): {:?}",
            shape.node_components
        );
    }
    // Same for a domain of rdfs:Resource.
    let domain_ds = shape_dataset_with_logic(
        "g:usesTerm a owl:ObjectProperty ; rdfs:domain rdfs:Resource .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:usesTerm ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let domain_shapes = derive_validation_shapes(domain_ds.as_ref()).expect("derive ok");
    if let Some(shape) = domain_shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("usesTerm")))
    {
        assert!(
            !shape.node_components.iter().any(|c| matches!(
                c,
                ConstraintComponent::Class(d) if d.ends_with("Resource")
            )),
            "rdfs:Resource domain must emit NO sh:class node component (vacuous top): {:?}",
            shape.node_components
        );
    }
}

#[test]
fn derive_functional_property_subjects_of_max_one() {
    let ds = shape_dataset("g:id a owl:FunctionalProperty .");
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("id")))
        .expect("a SubjectsOf(id) shape");
    let pc = shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("id"))
        .expect("a property on id");
    assert_eq!(pc.max_count, Some(1), "functional → sh:maxCount 1");
    assert!(!pc.inverse, "functional is a forward path, not inverse");
}

#[test]
fn derive_inverse_functional_property_objects_of_inverted_max_one() {
    let ds = shape_dataset("g:isbn a owl:InverseFunctionalProperty .");
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("isbn")))
        .expect("an ObjectsOf(isbn) shape");
    let pc = shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("isbn"))
        .expect("a property on isbn");
    assert_eq!(pc.max_count, Some(1), "inverse-functional → sh:maxCount 1");
    assert!(pc.inverse, "inverse-functional is an inverted path");
}

#[test]
fn derive_has_value_iri_emits_sh_has_value() {
    let ds = shape_dataset(
        "g:Publisher a owl:Class . \
         g:Book a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:publisher ; owl:hasValue g:Publisher ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        all_components(&shapes).iter().any(|c| matches!(
            c,
            ConstraintComponent::HasValue(crate::ir::ShapeValue::Iri(v)) if v.ends_with("Publisher")
        )),
        "owl:hasValue (IRI) → sh:hasValue: {:?}",
        all_components(&shapes)
    );
}

#[test]
fn derive_has_value_typed_literal_preserves_datatype() {
    // A typed fixed value `owl:hasValue "1"^^xsd:integer` must derive a TYPED sh:hasValue,
    // carrying the datatype IRI — never a bare untyped `"1"` (which would match the wrong
    // literal). This exercises the graphutil `Node::Lit` datatype-preservation fix.
    let ds = shape_dataset(
        "g:Prob a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:totalMass ; owl:hasValue \"1\"^^xsd:integer ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let has_typed = all_components(&shapes).iter().any(|c| matches!(
        c,
        ConstraintComponent::HasValue(crate::ir::ShapeValue::Literal { lexical, datatype, lang })
            if lexical == "1"
                && datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#integer")
                && lang.is_none()
    ));
    assert!(
        has_typed,
        "owl:hasValue \"1\"^^xsd:integer must derive a TYPED sh:hasValue (datatype preserved): {:?}",
        all_components(&shapes)
    );
    // The projected SHACL surface carries the datatype, not a bare untyped literal.
    let ttl = crate::projections::shapes::project_validation_shapes_shacl(
        &crate::ir::LogicProgram::new(vec![], vec![], vec![], None).with_validation_shapes(shapes),
    );
    assert!(
        ttl.contains("sh:hasValue \"1\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
        "the derived SHACL must carry the typed literal: {ttl}"
    );
}

#[test]
fn derive_has_value_plain_literal_stays_untyped() {
    // A plain `xsd:string` fixed value normalizes to an untyped carrier (datatype None), so an
    // authored `"foo"` and an equivalent `"foo"^^xsd:string` derive the same untyped sh:hasValue.
    let ds = shape_dataset(
        "g:Doc a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:label ; owl:hasValue \"foo\" ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let untyped = all_components(&shapes).iter().any(|c| matches!(
        c,
        ConstraintComponent::HasValue(crate::ir::ShapeValue::Literal { lexical, datatype, lang })
            if lexical == "foo" && datatype.is_none() && lang.is_none()
    ));
    assert!(
        untyped,
        "a plain literal must derive an untyped sh:hasValue (datatype None): {:?}",
        all_components(&shapes)
    );
}

#[test]
fn derive_has_value_blank_hard_fails() {
    // A fixed value cannot be an anonymous node — hard-fail, never a silent drop.
    let ds = shape_dataset(
        "g:Book a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:publisher ; owl:hasValue [ a owl:Thing ] ] .",
    );
    assert!(
        derive_validation_shapes(ds.as_ref()).is_err(),
        "owl:hasValue on a blank node must hard-fail"
    );
}

#[test]
fn derive_disjoint_with_emits_not_class() {
    let ds =
        shape_dataset("g:Animal a owl:Class . g:Plant a owl:Class ; owl:disjointWith g:Animal .");
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        all_components(&shapes).iter().any(|c| matches!(
            c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(), ConstraintComponent::Class(d) if d.ends_with("Animal"))
        )),
        "owl:disjointWith → sh:not [ sh:class D ]: {:?}",
        all_components(&shapes)
    );
}

#[test]
fn derive_complement_of_emits_not_class() {
    let ds = shape_dataset("g:Dead a owl:Class . g:Alive a owl:Class ; owl:complementOf g:Dead .");
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        all_components(&shapes).iter().any(|c| matches!(
            c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(), ConstraintComponent::Class(d) if d.ends_with("Dead"))
        )),
        "owl:complementOf → sh:not [ sh:class D ]: {:?}",
        all_components(&shapes)
    );
}

#[test]
fn derive_one_of_emits_sh_in() {
    let ds = shape_dataset(
        "g:Suit a owl:Class ; owl:oneOf ( g:Hearts g:Spades ) . \
         g:Hearts a owl:Thing . g:Spades a owl:Thing .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let in_comp = all_components(&shapes)
        .into_iter()
        .find(|c| matches!(c, ConstraintComponent::In(_)))
        .expect("an sh:in component");
    match in_comp {
        ConstraintComponent::In(members) => {
            assert!(
                members
                    .iter()
                    .any(|v| matches!(v, crate::ir::ShapeValue::Iri(i) if i.ends_with("Hearts")))
                    && members.iter().any(
                        |v| matches!(v, crate::ir::ShapeValue::Iri(i) if i.ends_with("Spades"))
                    ),
                "sh:in must carry both enumerated members: {members:?}"
            );
        }
        other => panic!("expected In, got {other:?}"),
    }
}

#[test]
fn derive_all_disjoint_classes_cross_links_not_class() {
    let ds = shape_dataset(
        "g:A a owl:Class . g:B a owl:Class . \
         [] a owl:AllDisjointClasses ; owl:members ( g:A g:B ) .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let a_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("gmeow/A")))
        .expect("a shape for g:A");
    assert!(
        a_shape.node_components.iter().any(|c| matches!(
            c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(), ConstraintComponent::Class(d) if d.ends_with("gmeow/B"))
        )),
        "g:A must carry sh:not [ sh:class g:B ]: {:?}",
        a_shape.node_components
    );
    let b_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("gmeow/B")))
        .expect("a shape for g:B");
    assert!(
        b_shape.node_components.iter().any(|c| matches!(
            c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(), ConstraintComponent::Class(d) if d.ends_with("gmeow/A"))
        )),
        "g:B must carry sh:not [ sh:class g:A ]: {:?}",
        b_shape.node_components
    );
}

#[test]
fn derive_qualified_cardinality_emits_qualified_value_shape() {
    let ds = shape_dataset(
        "g:Wheel a owl:Class . \
         g:Car a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:hasPart ; owl:onClass g:Wheel ; \
           owl:qualifiedCardinality \"1\"^^xsd:nonNegativeInteger ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comp = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .find(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. }))
        .expect("a qualified value shape");
    match comp {
        ConstraintComponent::QualifiedValueShape { shape, min, max } => {
            assert_eq!(*min, Some(1));
            assert_eq!(*max, Some(1));
            assert!(
                shape
                    .iter()
                    .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Wheel"))),
                "qualified inner shape must be sh:class g:Wheel: {shape:?}"
            );
        }
        other => panic!("expected QualifiedValueShape, got {other:?}"),
    }
    // And it carries NO plain min/max_count (the count is on the qualified values).
    let pc = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .find(|p| p.path.ends_with("hasPart"))
        .expect("a property on hasPart");
    assert_eq!(
        pc.min_count, None,
        "qualified count is not a plain min_count"
    );
    assert_eq!(
        pc.max_count, None,
        "qualified count is not a plain max_count"
    );
}

#[test]
fn derive_qualified_cardinality_on_data_range_emits_plain_datatype_and_count() {
    // `owl:maxQualifiedCardinality 1 ; owl:onDataRange xsd:decimal` is the DATATYPE-qualified peer
    // of `owl:onClass`. It must read as a PLAIN `sh:datatype xsd:decimal` + `sh:maxCount 1` (a bare
    // datatype the JSON-Schema deriver can read), never a hard-fail for "requires owl:onClass".
    let ds = shape_dataset(
        "g:Measurement a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:appraisalValue ; \
           owl:maxQualifiedCardinality \"1\"^^xsd:nonNegativeInteger ; \
           owl:onDataRange xsd:decimal ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("onDataRange must derive, not fail");
    let pc = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .find(|p| p.path.ends_with("appraisalValue"))
        .expect("a property on appraisalValue");
    assert_eq!(
        pc.max_count,
        Some(1),
        "maxQualifiedCardinality 1 → sh:maxCount 1"
    );
    assert_eq!(pc.min_count, None, "no min on a max-qualified cardinality");
    assert!(
        pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(d)
                if d == "http://www.w3.org/2001/XMLSchema#decimal")),
        "onDataRange xsd:decimal → sh:datatype xsd:decimal: {:?}",
        pc.components
    );
    assert!(
        !pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. })),
        "a datatype filler degrades to a plain datatype, not a qualified value shape: {:?}",
        pc.components
    );
}

#[test]
fn derive_qualified_cardinality_omits_plain_class_without_a_backing_universal() {
    // `owl:maxQualifiedCardinality 1 ; owl:onClass g:C` counts ONLY the values that ARE a C; it
    // does NOT entail that EVERY value of the property is a C. So the faithful projection is the
    // `sh:qualifiedValueShape` ALONE — a bare `sh:class g:C` would over-claim the universal
    // (caught by the lift/derive round-trip `certify` invariant). No `rdfs:range`/`allValuesFrom`
    // backs the filler here, so NO plain class is emitted.
    let ds = shape_dataset(
        "g:C a owl:Class . \
         g:K a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:mediates ; \
           owl:maxQualifiedCardinality \"1\"^^xsd:nonNegativeInteger ; owl:onClass g:C ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let pc = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .find(|p| p.path.ends_with("mediates"))
        .expect("a property on mediates");
    assert!(
        !pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(_))),
        "a qualified cardinality with no backing universal must NOT emit a plain sh:class (over-claim): {:?}",
        pc.components
    );
    assert!(
        pc.components.iter().any(|c| matches!(
            c,
            ConstraintComponent::QualifiedValueShape { max, .. } if *max == Some(1)
        )),
        "the faithful qualified value shape carries the count: {:?}",
        pc.components
    );
}

#[test]
fn derive_qualified_cardinality_emits_plain_class_when_a_closed_range_backs_it() {
    // The bare `sh:class g:C` — which the JSON-Schema deriver / purrdf object-class node-ref path
    // read (they ignore the class nested inside a `sh:qualifiedValueShape`) — IS emitted for a
    // qualified cardinality when a genuine universal backs it: here a closed-world-opted-in
    // `rdfs:range g:mediates g:C`. Then both the plain class (the universal) and the qualified
    // value shape (the count) are present, and the round trip stays equivalent.
    let ds = shape_dataset_with_logic(
        "g:C a owl:Class . \
         g:mediates a owl:ObjectProperty ; rdfs:range g:C . \
         g:K a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:mediates ; \
           owl:maxQualifiedCardinality \"1\"^^xsd:nonNegativeInteger ; owl:onClass g:C ] .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:mediates ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let pc = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .filter(|p| p.path.ends_with("mediates"))
        .find(|p| {
            p.components
                .iter()
                .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. }))
        })
        .expect("the qualified property shape on mediates");
    assert!(
        pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("/C"))),
        "a closed-world range g:C must emit the plain sh:class for the JSON-Schema deriver: {:?}",
        pc.components
    );
    assert!(
        pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. })),
        "the faithful qualified value shape is still emitted: {:?}",
        pc.components
    );
}

#[test]
fn derive_single_property_has_key_emits_inverse_functional_shape() {
    // `K owl:hasKey ( P )` is the OWL 2 DL way to state a datatype/single-property key (an
    // owl:InverseFunctionalProperty on a datatype property would be OWL 2 Full). Its closed-world
    // reading is the same inverse sh:maxCount 1 the InverseFunctionalProperty arm emits.
    let ds = shape_dataset(
        "g:GTSSegment a owl:Class ; owl:hasKey ( g:gtsHeadId ) . \
         g:gtsHeadId a owl:DatatypeProperty ; rdfs:domain g:GTSSegment ; rdfs:range xsd:string .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("gtsHeadId")))
        .expect("an ObjectsOf(gtsHeadId) shape from owl:hasKey");
    let prop = &shape.properties[0];
    assert!(prop.inverse, "hasKey must derive an inverse-path shape");
    assert_eq!(
        prop.max_count,
        Some(1),
        "hasKey → each key value has ≤1 subject"
    );
}

#[test]
fn derive_composite_has_key_derives_no_single_path_shape() {
    // A COMPOSITE key (owl:hasKey ( P1 P2 )) asserts the TUPLE is unique, not each part — it has
    // no single-path SHACL form, so no per-part uniqueness shape may be derived.
    let ds = shape_dataset(
        "g:C a owl:Class ; owl:hasKey ( g:p1 g:p2 ) . \
         g:p1 a owl:DatatypeProperty . g:p2 a owl:DatatypeProperty .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| matches!(
            &s.target,
            ShapeTarget::ObjectsOf(p) if p.ends_with("/p1") || p.ends_with("/p2")
        )),
        "a composite key must derive no single-path uniqueness shape: {:?}",
        shapes.iter().map(|s| &s.target).collect::<Vec<_>>()
    );
}

#[test]
fn derive_min_qualified_cardinality_uses_the_owl2_standard_keyword() {
    // The OWL 2 RDF keyword is `owl:minQualifiedCardinality` (NOT `owl:qualifiedMinCardinality`).
    // Regression guard: the derivation must read the standard local name, else a real qualified
    // min-cardinality restriction is silently invisible and the arm ships zero shapes.
    let ds = shape_dataset(
        "g:Posting a owl:Class . \
         g:JournalEntry a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:entryPostings ; owl:onClass g:Posting ; \
           owl:minQualifiedCardinality \"2\"^^xsd:nonNegativeInteger ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comp = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .find(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. }))
        .expect("owl:minQualifiedCardinality must derive a qualified value shape");
    match comp {
        ConstraintComponent::QualifiedValueShape { shape, min, max } => {
            assert_eq!(
                *min,
                Some(2),
                "minQualifiedCardinality 2 → qualifiedMinCount 2"
            );
            assert_eq!(*max, None);
            assert!(
                shape
                    .iter()
                    .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Posting"))),
                "inner shape must be sh:class g:Posting: {shape:?}"
            );
        }
        other => panic!("expected QualifiedValueShape, got {other:?}"),
    }
}

#[test]
fn derive_all_values_from_faceted_datatype_emits_length_and_pattern_facets() {
    // An owl:allValuesFrom whose filler is a faceted rdfs:Datatype
    // (owl:onDatatype + owl:withRestrictions) reads as the SHACL length / pattern facets its
    // values must satisfy — the derivation's owl:withRestrictions arm.
    let ds = shape_dataset(
        "g:FinancialAccount a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:bic ; owl:allValuesFrom \
           [ a rdfs:Datatype ; owl:onDatatype xsd:string ; owl:withRestrictions ( \
              [ xsd:minLength \"8\"^^xsd:nonNegativeInteger ] \
              [ xsd:maxLength \"11\"^^xsd:nonNegativeInteger ] \
              [ xsd:pattern \"^[A-Z]{6}[A-Z0-9]{2}([A-Z0-9]{3})?$\" ] ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps: Vec<&ConstraintComponent> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .filter(|p| p.path.ends_with("bic"))
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::MinLength(8))),
        "xsd:minLength 8 → sh:minLength 8: {comps:?}"
    );
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::MaxLength(11))),
        "xsd:maxLength 11 → sh:maxLength 11: {comps:?}"
    );
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Pattern { regex, .. } if regex.starts_with("^[A-Z]{6}"))),
        "xsd:pattern → sh:pattern: {comps:?}"
    );
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::Datatype(d) if d.ends_with("string")
        )),
        "owl:onDatatype xsd:string → sh:datatype xsd:string: {comps:?}"
    );
}

#[test]
fn derive_all_values_from_faceted_datatype_emits_numeric_range_facets() {
    // An owl:allValuesFrom whose filler is a faceted rdfs:Datatype with numeric bound
    // restrictions (xsd:minInclusive / xsd:maxInclusive and their exclusive peers) reads as a
    // single SHACL NumericRange the values must satisfy — the closed unit interval [0, 1] and a
    // half-open lower-bound-exclusive interval.
    let ds = shape_dataset(
        "g:ConceptCategorization a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:typicality ; owl:allValuesFrom \
           [ a rdfs:Datatype ; owl:onDatatype xsd:decimal ; owl:withRestrictions ( \
              [ xsd:minInclusive \"0\"^^xsd:decimal ] \
              [ xsd:maxInclusive \"1\"^^xsd:decimal ] ) ] ] .\n\
         g:RationalValue a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:denominator ; owl:allValuesFrom \
           [ a rdfs:Datatype ; owl:onDatatype xsd:integer ; owl:withRestrictions ( \
              [ xsd:minExclusive \"0\"^^xsd:integer ] ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let typicality: Vec<&ConstraintComponent> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .filter(|p| p.path.ends_with("typicality"))
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        typicality.iter().any(|c| matches!(
            c,
            ConstraintComponent::NumericRange {
                min: Some(lo),
                max: Some(hi),
                min_inclusive: true,
                max_inclusive: true,
            } if *lo == 0.0 && *hi == 1.0
        )),
        "closed unit interval → sh:minInclusive 0 / sh:maxInclusive 1: {typicality:?}"
    );
    let denominator: Vec<&ConstraintComponent> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .filter(|p| p.path.ends_with("denominator"))
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        denominator.iter().any(|c| matches!(
            c,
            ConstraintComponent::NumericRange {
                min: Some(lo),
                max: None,
                min_inclusive: false,
                ..
            } if *lo == 0.0
        )),
        "lower-bound-exclusive → sh:minExclusive 0, no upper: {denominator:?}"
    );
}

#[test]
fn derive_non_faceted_blank_filler_is_skipped() {
    // A blank allValuesFrom filler that is neither a faceted datatype nor a readable class
    // expression (union / disjoint-union / complement) — here an `owl:intersectionOf`, which has
    // no faithful single-component shape form — is carried in the canon, never a bare blank shape.
    let ds = shape_dataset(
        "g:C a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:p ; owl:allValuesFrom \
           [ a owl:Class ; owl:intersectionOf ( g:A g:B ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes
            .iter()
            .flat_map(|s| &s.properties)
            .any(|p| p.path.ends_with("/p")),
        "a non-faceted blank filler must derive no property shape: {shapes:?}"
    );
}

#[test]
fn derive_qualified_cardinality_without_on_class_hard_fails() {
    // A qualified cardinality with no owl:onClass is malformed — hard-fail, never a silent drop.
    let ds = shape_dataset(
        "g:Car a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:hasPart ; \
           owl:qualifiedCardinality \"1\"^^xsd:nonNegativeInteger ] .",
    );
    assert!(
        derive_validation_shapes(ds.as_ref()).is_err(),
        "qualifiedCardinality without owl:onClass must hard-fail"
    );
}

#[test]
fn derive_domain_and_functional_merge_into_one_subjects_of_shape() {
    // A domain axiom (opted IN to closed-world reading) AND a functionality axiom on the SAME
    // property must fold into ONE SubjectsOf(P) shape carrying both the node Class and the
    // maxCount-1 property. (The functional maxCount derives regardless; the domain node-class
    // needs the ClosedWorldClosure opt-in since domain is open-world by default.)
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:isbn a owl:FunctionalProperty ; rdfs:domain g:Doc .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:isbn ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let subjects_of: Vec<_> = shapes
        .iter()
        .filter(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("isbn")))
        .collect();
    assert_eq!(
        subjects_of.len(),
        1,
        "domain + functional must produce exactly ONE SubjectsOf(isbn) shape: {subjects_of:?}"
    );
    let shape = subjects_of[0];
    assert!(
        shape
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "merged shape carries the domain node Class: {:?}",
        shape.node_components
    );
    assert!(
        shape
            .properties
            .iter()
            .any(|p| p.path.ends_with("isbn") && p.max_count == Some(1)),
        "merged shape carries the functional maxCount-1 property: {:?}",
        shape.properties
    );
}

// ── R3: the per-property / per-class closed-world-reading opt-out (closure selector) ──────

/// Parse a fragment that also declares the `logic:` prefix, for the closure-entry opt-out tests.
fn shape_dataset_with_logic(ttl: &str) -> std::sync::Arc<RdfDataset> {
    let full = format!(
        "@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix owl:   <http://www.w3.org/2002/07/owl#> .\n\
         @prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .\n\
         @prefix g:     <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n{ttl}"
    );
    parse_dataset(full.as_bytes(), "text/turtle", None).expect("parse dataset ok")
}

#[test]
fn derive_default_is_derive_all_without_any_closure_annotation() {
    // MAXIMAL UTILITY: with NO closure annotation, every eligible axiom derives a shape.
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        shapes.iter().any(|s| s.iri.contains("Article")),
        "default (no annotation) must derive the Article shape: {shapes:?}"
    );
}

#[test]
fn derive_property_optout_suppresses_that_property_shape() {
    // A closure entry keyed by the PROPERTY IRI with logic:OpenWorldClosure marks the property
    // genuinely open-world-only, so no closed-world shape is derived for it.
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] . \
         [] logic:closureKey <https://blackcatinformatics.ca/gmeow/cites> ; \
            logic:closureValue logic:OpenWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| s.iri.contains("Article")),
        "an OpenWorldClosure opt-out on g:cites must suppress the Article shape: {shapes:?}"
    );
}

#[test]
fn derive_class_optout_suppresses_the_whole_class_shape() {
    // A closure entry keyed by the CLASS IRI opts the entire class out of the closed-world reading.
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] . \
         [] logic:closureKey <https://blackcatinformatics.ca/gmeow/Article> ; \
            logic:closureValue logic:OpenWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| s.iri.contains("Article")),
        "an OpenWorldClosure opt-out on g:Article must suppress its shape entirely: {shapes:?}"
    );
}

#[test]
fn derive_closed_world_closure_entry_does_not_suppress() {
    // Only OpenWorldClosure is an opt-out; a ClosedWorldClosure entry (the default reading made
    // explicit) leaves derivation on — the shape is still produced.
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] . \
         [] logic:closureKey <https://blackcatinformatics.ca/gmeow/cites> ; \
            logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        shapes.iter().any(|s| s.iri.contains("Article")),
        "a ClosedWorldClosure entry is the default reading, not an opt-out: {shapes:?}"
    );
}

#[test]
fn derive_class_scoped_closed_entry_derives_no_global_domain_range_shape() {
    // A ClosedWorldClosure entry carrying logic:onClass is CLASS-SCOPED: it turns the class's
    // owl:allValuesFrom into a required (minCount 1) path on THAT class's shape, and it must NOT
    // leak into the property-global opt-in set — no corpus-wide sh:targetSubjectsOf /
    // sh:targetObjectsOf domain/range shape may be derived from it (closing a predicate on one
    // class asserts nothing about the predicate's other subjects/objects). The closureKey is a
    // string literal here, matching the authored slice form.
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:allValuesFrom g:Doc ] . \
         g:cites a owl:ObjectProperty ; rdfs:domain g:Article ; rdfs:range g:Doc . \
         [] a logic:ClosureEntry ; logic:onClass g:Article ; \
            logic:closureKey \"https://blackcatinformatics.ca/gmeow/cites\" ; \
            logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let article = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("Article")))
        .expect("the class-scoped entry keeps the Article class shape");
    assert!(
        article
            .properties
            .iter()
            .any(|p| p.path.ends_with("cites") && p.min_count == Some(1)),
        "class-scoped closed universal requires the path on the class shape: {:?}",
        article.properties
    );
    assert!(
        !shapes.iter().any(|s| matches!(
            &s.target,
            ShapeTarget::SubjectsOf(p) | ShapeTarget::ObjectsOf(p) if p.ends_with("cites")
        )),
        "a class-scoped entry must derive NO global domain/range shape: {:?}",
        shapes.iter().map(|s| &s.target).collect::<Vec<_>>()
    );
}

#[test]
fn derive_property_global_closed_entry_is_unaffected_by_a_class_scoped_sibling() {
    // The two entry scopes coexist without cross-talk: a property-GLOBAL entry (no logic:onClass)
    // still derives its domain/range shapes, while the class-scoped sibling on another property
    // derives none.
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:refs a owl:ObjectProperty ; rdfs:range g:Doc . \
         [] a logic:ClosureEntry ; logic:closureKey g:refs ; \
            logic:closureValue logic:ClosedWorldClosure . \
         g:cites a owl:ObjectProperty ; rdfs:range g:Doc . \
         [] a logic:ClosureEntry ; logic:onClass g:Article ; logic:closureKey g:cites ; \
            logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let refs = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("refs")))
        .expect("the property-global opt-in still derives its ObjectsOf(refs) range shape");
    assert!(
        refs.node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "the global entry's range shape carries its sh:class: {:?}",
        refs.node_components
    );
    assert!(
        !shapes
            .iter()
            .any(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("cites"))),
        "the class-scoped sibling must not derive a global range shape: {:?}",
        shapes.iter().map(|s| &s.target).collect::<Vec<_>>()
    );
}

#[test]
fn derive_class_scoped_open_entry_does_not_suppress_globally() {
    // The symmetric discrimination for the opt-out polarity: an OpenWorldClosure entry carrying
    // logic:onClass is class-scoped and must NOT sweep its key into the property-global opt-out
    // set — the closed-world reading of the property's restrictions on OTHER classes stays on.
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] . \
         [] a logic:ClosureEntry ; logic:onClass g:Other ; logic:closureKey g:cites ; \
            logic:closureValue logic:OpenWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        shapes.iter().any(|s| s.iri.contains("Article")),
        "a class-scoped OpenWorldClosure entry must not opt the property out corpus-wide: {shapes:?}"
    );
}

#[test]
fn derive_grounding_namespaces_are_authoring_ground() {
    // declarative-migration wave 1: the dogfooded grounding slices (math:, lang:, logic:) are authoring ground
    // too — their hand-authored shapes migrate to derived projections, so a restriction on a
    // math:/lang:/logic: class must derive a shape (not be skipped as an imported ontology).
    let ttl = "@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
               @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
               @prefix owl:  <http://www.w3.org/2002/07/owl#> .\n\
               @prefix math: <https://blackcatinformatics.ca/math/> .\n\
               @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
               @prefix logic:<https://blackcatinformatics.ca/logic/> .\n\
               math:NumberSystem a owl:Class .\n\
               math:Number a owl:Class ; rdfs:subClassOf \
               [ a owl:Restriction ; owl:onProperty math:inNumberSystem ; \
                 owl:qualifiedCardinality 1 ; owl:onClass math:NumberSystem ] .\n\
               lang:Form a owl:Class ; rdfs:subClassOf \
               [ a owl:Restriction ; owl:onProperty lang:inSignSystem ; owl:cardinality 1 ] .\n\
               logic:Plan a owl:Class ; rdfs:subClassOf \
               [ a owl:Restriction ; owl:onProperty logic:planGoal ; owl:cardinality 1 ] .";
    let ds = parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse dataset ok");
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    for expect in ["math/Number", "lang/Form", "logic/Plan"] {
        assert!(
            shapes.iter().any(|s| s.iri.contains(expect)),
            "grounding class {expect} must derive a validation shape: {shapes:?}"
        );
    }
}

// ── Shape-migration additions (sh:or / range facets / complement / value-keyed) ──

#[test]
fn derive_union_of_filler_lowers_to_sh_or() {
    // A someValuesFrom whose filler is an anonymous `owl:unionOf ( A B )` class expression
    // reads closed-world as `sh:or ( [ sh:class A ] [ sh:class B ] )`.
    let ds = shape_dataset(
        "g:A a owl:Class . g:B a owl:Class . \
         g:Attack a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:attackTarget ; \
           owl:someValuesFrom [ owl:unionOf ( g:A g:B ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let or = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .find_map(|c| match c {
            ConstraintComponent::Or(branches) => Some(branches.clone()),
            _ => None,
        })
        .expect("expected an Or component");
    assert_eq!(or.len(), 2, "two union branches: {or:?}");
    assert!(
        or.iter().all(
            |b| matches!(b, ConstraintComponent::Class(c) if c.ends_with("/A") || c.ends_with("/B"))
        ),
        "each branch is an sh:class over A/B: {or:?}"
    );
}

#[test]
fn derive_range_facets_lower_to_numeric_range() {
    // A faceted datatype filler carrying xsd:minInclusive/maxExclusive lowers to ONE NumericRange.
    let ds = shape_dataset(
        "g:M a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:magnitude ; owl:allValuesFrom \
           [ a rdfs:Datatype ; owl:onDatatype xsd:decimal ; owl:withRestrictions \
             ( [ xsd:minInclusive 0 ] [ xsd:maxExclusive 100 ] ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let range = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .find_map(|c| match c {
            ConstraintComponent::NumericRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } => Some((*min, *max, *min_inclusive, *max_inclusive)),
            _ => None,
        })
        .expect("expected a NumericRange");
    assert_eq!(range.0, Some(0.0));
    assert_eq!(range.1, Some(100.0));
    assert!(range.2, "minInclusive");
    assert!(!range.3, "maxExclusive");
}

#[test]
fn derive_complement_has_value_lowers_to_not_has_value() {
    // owl:allValuesFrom [ owl:complementOf [ owl:hasValue v ] ] → sh:not [ sh:hasValue v ].
    let ds = shape_dataset(
        "g:C a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:p ; owl:allValuesFrom \
           [ owl:complementOf [ owl:hasValue g:forbidden ] ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let found = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .any(|c| {
            matches!(c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(),
                    ConstraintComponent::HasValue(ShapeValue::Iri(v)) if v.ends_with("/forbidden")))
        });
    assert!(found, "expected Not(HasValue(forbidden)): {shapes:?}");
}

#[test]
fn derive_pinned_forbidden_pattern_record_lowers_to_not_has_value_on_the_class_shape() {
    // FAMILY 7: a `logic:ForbiddenPatternConstraint` with a PINNED `logic:forbiddenValue` is the
    // decidable, validation-only authoring form of the value-complement pattern. It must lower to
    // the exact component the legacy shapes carried — `sh:not [ sh:hasValue v ]` on the forbidden
    // path of the `{C}-shape` — merged with the class's other conditions on that path.
    let ds = shape_dataset(
        "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         g:Cell a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:denom ; \
           owl:maxQualifiedCardinality 1 ; owl:onDataRange xsd:integer ] .\n\
         g:cellDenomNonZero a logic:ForbiddenPatternConstraint ;\n\
           logic:onClass g:Cell ;\n\
           logic:forbiddenPredicate g:denom ;\n\
           logic:forbiddenValue \"0\"^^xsd:integer ;\n\
           logic:formalizes g:Cell .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let cell_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("/Cell")))
        .expect("a g:Cell class shape must derive");
    let denom = cell_shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("/denom"))
        .expect("a g:denom property shape must derive");
    assert!(
        denom.components.iter().any(|c| matches!(c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(),
                    ConstraintComponent::HasValue(ShapeValue::Literal { lexical, .. })
                        if lexical == "0"))),
        "expected sh:not [ sh:hasValue 0 ] on the g:denom path: {cell_shape:?}"
    );
    // The pinned-value component MERGES with the restriction-derived cardinality on the same
    // path (one property shape per path, never a colliding second shape).
    assert_eq!(denom.max_count, Some(1), "same-path conditions must merge");
}

#[test]
fn derive_unpinned_and_iri_pinned_forbidden_pattern_records_derive_no_component() {
    // The UNPINNED form ("C must not carry P at all") has no per-value SHACL-Core lowering
    // here, and the IRI-pinned form is the class-negation idiom whose declarative home is
    // the node-level sh:not [ sh:class … ] — neither derives a property component; their
    // canonical constraint expansions carry them procedurally.
    let ds = shape_dataset(
        "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         g:cellNoLegacy a logic:ForbiddenPatternConstraint ;\n\
           logic:onClass g:Cell ;\n\
           logic:forbiddenPredicate g:legacyCode ;\n\
           logic:formalizes g:Cell .\n\
         g:cellNotOther a logic:ForbiddenPatternConstraint ;\n\
           logic:onClass g:Cell ;\n\
           logic:forbiddenPredicate rdf:type ;\n\
           logic:forbiddenValue g:OtherKind ;\n\
           logic:formalizes g:Cell .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        shapes.is_empty(),
        "unpinned / IRI-pinned forbidden patterns must derive no declarative shape: {shapes:?}"
    );
}

#[test]
fn derive_value_range_record_lowers_to_min_and_max_inclusive_on_the_class_shape() {
    // FAMILY 8: a `logic:ValueRangeConstraint` is the decidable, validation-only authoring
    // form of a bounded numeric range (the OWL faceted-datatype filler is undecidable for the
    // native reasoner once a literal is asserted on the path). It must lower to the exact
    // components the legacy facet carried — `sh:minInclusive`/`sh:maxInclusive` on the value
    // path of the `{C}-shape`.
    let ds = shape_dataset(
        "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         g:unitInterval a logic:ValueRangeConstraint ;\n\
           logic:onClass g:Probability ;\n\
           logic:valuePath g:magnitude ;\n\
           logic:minInclusiveBound 0 ;\n\
           logic:maxInclusiveBound 1 ;\n\
           logic:formalizes g:Probability .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("/Probability")))
        .expect("a g:Probability class shape must derive");
    let prop = shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("/magnitude"))
        .expect("a g:magnitude property shape must derive");
    assert!(
        prop.components.iter().any(|c| matches!(c,
            ConstraintComponent::NumericRange {
                min: Some(lo),
                max: Some(hi),
                min_inclusive: true,
                max_inclusive: true,
            } if *lo == 0.0 && *hi == 1.0)),
        "expected sh:minInclusive 0 / sh:maxInclusive 1 on g:magnitude: {shape:?}"
    );
}

#[test]
fn value_range_record_expands_to_one_constraint_and_leaks_no_axiom() {
    // The record expands to exactly one canonical logic:Constraint whose integrity is the
    // guarded range formula, and its structural triples never enter `prog.axioms` — a
    // validates-but-does-not-entail obligation is never a reasoning-core axiom.
    let (prog, diags) = parse(
        "ex:unitInterval a logic:ValueRangeConstraint ;\n\
           logic:onClass ex:Probability ;\n\
           logic:valuePath ex:magnitude ;\n\
           logic:minInclusiveBound 0 ;\n\
           logic:maxInclusiveBound 1 ;\n\
           logic:formalizes ex:Probability .",
    );
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    assert_eq!(
        prog.constraints.len(),
        1,
        "exactly one canonical constraint must expand"
    );
    let c = &prog.constraints[0];
    assert!(
        matches!(&c.target, ShapeTarget::Class(cl) if cl.ends_with("/Probability")),
        "the range constraint must target the guarded class: {:?}",
        c.target
    );
    assert!(
        !prog
            .axioms
            .iter()
            .any(|a| a.subject.ends_with("/unitInterval")),
        "no record triple may leak into prog.axioms; got: {:?}",
        prog.axioms
    );
}

#[test]
fn value_range_record_without_any_bound_is_a_malformed_record() {
    // A range record naming no bound constrains nothing — fail-soft like the other sugar
    // readers: a MALFORMED_CONSTRAINT diagnostic, never a silent no-op constraint.
    let (prog, diags) = parse(
        "ex:noBounds a logic:ValueRangeConstraint ;\n\
           logic:onClass ex:Probability ;\n\
           logic:valuePath ex:magnitude ;\n\
           logic:formalizes ex:Probability .",
    );
    assert!(prog.constraints.is_empty(), "no constraint may expand");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("minInclusiveBound")),
        "a missing-bound record must diagnose: {diags:?}"
    );
}

#[test]
fn pinned_forbidden_pattern_record_contributes_nothing_to_the_reasoned_axiom_set() {
    // The record is a validation descriptor: it expands to exactly one canonical
    // logic:Constraint and its structural triples MUST NOT leak into `prog.axioms`
    // (the reasoned axiom set) — a validates-but-does-not-entail obligation never
    // becomes a reasoning-core axiom.
    let (prog, diags) = parse(
        "ex:cellDenomNonZero a logic:ForbiddenPatternConstraint ;\n\
           logic:onClass ex:Cell ;\n\
           logic:forbiddenPredicate ex:denom ;\n\
           logic:forbiddenValue \"0\"^^xsd:integer ;\n\
           logic:formalizes ex:Cell .",
    );
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    assert_eq!(
        prog.constraints.len(),
        1,
        "exactly one canonical constraint must expand"
    );
    assert!(
        !prog
            .axioms
            .iter()
            .any(|a| a.subject.ends_with("/cellDenomNonZero")),
        "no record triple may leak into prog.axioms; got: {:?}",
        prog.axioms
    );
}

#[test]
fn derive_value_keyed_general_class_inclusion() {
    // `[ owl:onProperty mode ; owl:hasValue modeAbduction ] rdfs:subClassOf
    //  [ owl:onProperty explanandum ; owl:minCardinality 1 ]` → a ValueKeyed(mode, modeAbduction)
    // shape requiring explanandum ≥ 1 (the modes-ride-one-class idiom).
    let ds = shape_dataset(
        "[ a owl:Restriction ; owl:onProperty g:inferenceModeOf ; owl:hasValue g:modeAbduction ] \
           rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:explanandum ; owl:minCardinality 1 ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let vk = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ValueKeyed { .. }))
        .expect("expected a value-keyed shape");
    match &vk.target {
        ShapeTarget::ValueKeyed { predicate, value } => {
            assert!(predicate.ends_with("/inferenceModeOf"), "{predicate}");
            assert!(value.ends_with("/modeAbduction"), "{value}");
        }
        other => panic!("expected ValueKeyed, got {other:?}"),
    }
    assert!(
        vk.properties
            .iter()
            .any(|p| p.path.ends_with("/explanandum") && p.min_count == Some(1)),
        "explanandum minCount 1: {:?}",
        vk.properties
    );
}
