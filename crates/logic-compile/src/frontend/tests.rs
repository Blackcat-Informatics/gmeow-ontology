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

#[test]
fn recovery_case_owns_its_formula_and_typed_term_carriers() {
    let (program, diagnostics) = parse(
        "ex:c a logic:Correspondence ;
            logic:correspondenceRelation logic:Subsumes ;
            logic:morphismClass logic:LossyLens ;
            logic:morphismKind logic:InstitutionMorphism ;
            logic:recoveryCase ex:case .
         ex:case a logic:RecoveryCase ; logic:recoveryTransform [
            a logic:Formula ;
            logic:quantifiedVariable [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable \"x\" ] ;
            logic:forall [ a logic:Formula ;
                logic:antecedent [ a logic:Formula ; logic:relation ex:source ; logic:argument
                    [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable \"x\" ] ,
                    [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:Source ] ] ;
                logic:consequent [ a logic:Formula ; logic:relation ex:view ; logic:argument
                    [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable \"x\" ] ,
                    [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:View ] ]
            ]
         ] .",
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error),
        "unexpected recovery parse diagnostics: {diagnostics:#?}"
    );
    assert_eq!(program.correspondences.len(), 1);
    assert_eq!(program.correspondences[0].recovery_cases.len(), 1);
    assert!(
        program.formulas.is_empty(),
        "a recovery transform must not also become a top-level formula"
    );
    assert!(
        program.axioms.iter().all(|axiom| {
            axiom.obj != logic_iri("TermCarrier")
                && axiom.obj != logic_iri("RecoveryCase")
                && !axiom.predicate.ends_with("recoveryCase")
                && !axiom.predicate.ends_with("recoveryTransform")
        }),
        "recovery/formula structure leaked into generic axioms: {:#?}",
        program.axioms
    );
}

#[test]
fn recovery_case_requires_named_identity() {
    let (program, diagnostics) = parse(
        "ex:c a logic:Correspondence ;
            logic:correspondenceRelation logic:Subsumes ;
            logic:morphismClass logic:LossyLens ;
            logic:morphismKind logic:InstitutionMorphism ;
            logic:recoveryCase [ a logic:RecoveryCase ] .",
    );
    assert!(program.correspondences.is_empty());
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "MALFORMED_CORRESPONDENCE"
                && diagnostic.message.contains("non-IRI logic:recoveryCase")
        }),
        "unnamed recovery evidence must not disappear silently: {diagnostics:#?}"
    );
}

#[test]
fn orphan_recovery_case_is_hard_failed() {
    // `ex:case` is typed `logic:RecoveryCase` but no `logic:Correspondence` reaches it via
    // `logic:recoveryCase`: unowned recovery evidence must be a hard Severity::Error finding,
    // not silently vanish from the parsed program.
    let (program, diagnostics) = parse(
        "ex:case a logic:RecoveryCase ; logic:recoveryTransform [
            a logic:Formula ;
            logic:relation ex:source ;
            logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termIri ex:Source ]
         ] .",
    );
    assert!(program.correspondences.is_empty());
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "ORPHAN_RECOVERY_CASE"
                && diagnostic.severity == Severity::Error
                && diagnostic.message.contains("/case")
        }),
        "unowned RecoveryCase must be hard-failed: {diagnostics:#?}"
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
    let cases = [
        (
            "same-path allValuesFrom + maxQualifiedCardinality/onClass",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:item ; owl:allValuesFrom g:Item ] ,
               [ a owl:Restriction ; owl:onProperty g:item ;
                 owl:maxQualifiedCardinality 1 ; owl:onClass g:Item ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:item ; logic:allValuesFrom g:Item ] ,
               [ a logic:Restriction ; logic:onProperty g:item ;
                 logic:maxQualifiedCardinality 1 ; logic:onClass g:Item ] ."#,
        ),
        (
            "someValuesFrom",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:someItem ; owl:someValuesFrom g:Item ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:someItem ; logic:someValuesFrom g:Item ] ."#,
        ),
        (
            "hasValue",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:state ; owl:hasValue g:active ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:state ; logic:hasValue g:active ] ."#,
        ),
        (
            "unqualified cardinalities",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:minItem ; owl:minCardinality 1 ] ,
               [ a owl:Restriction ; owl:onProperty g:maxItem ; owl:maxCardinality 2 ] ,
               [ a owl:Restriction ; owl:onProperty g:exactItem ; owl:cardinality 1 ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:minItem ; logic:minCardinality 1 ] ,
               [ a logic:Restriction ; logic:onProperty g:maxItem ; logic:maxCardinality 2 ] ,
               [ a logic:Restriction ; logic:onProperty g:exactItem ; logic:cardinality 1 ] ."#,
        ),
        (
            "qualified cardinalities with class qualifiers",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:exactMember ; owl:qualifiedCardinality 1 ; owl:onClass g:Item ] ,
               [ a owl:Restriction ; owl:onProperty g:minMember ; owl:minQualifiedCardinality 1 ; owl:onClass g:Item ] ,
               [ a owl:Restriction ; owl:onProperty g:maxMember ; owl:maxQualifiedCardinality 2 ; owl:onClass g:Item ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:exactMember ; logic:qualifiedCardinality 1 ; logic:onClass g:Item ] ,
               [ a logic:Restriction ; logic:onProperty g:minMember ; logic:minQualifiedCardinality 1 ; logic:onClass g:Item ] ,
               [ a logic:Restriction ; logic:onProperty g:maxMember ; logic:maxQualifiedCardinality 2 ; logic:onClass g:Item ] ."#,
        ),
        (
            "qualified cardinality with data-range qualifier",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:score ; owl:maxQualifiedCardinality 1 ; owl:onDataRange xsd:decimal ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:score ; logic:maxQualifiedCardinality 1 ; logic:onDataRange xsd:decimal ] ."#,
        ),
        (
            "unionOf filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:member ;
                 owl:allValuesFrom [ owl:unionOf ( g:Item g:OtherItem ) ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:member ;
                 logic:allValuesFrom [ logic:unionOf ( g:Item g:OtherItem ) ] ] ."#,
        ),
        (
            "disjointUnionOf filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:member ;
                 owl:allValuesFrom [ owl:disjointUnionOf ( g:Item g:OtherItem ) ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:member ;
                 logic:allValuesFrom [ logic:disjointUnionOf ( g:Item g:OtherItem ) ] ] ."#,
        ),
        (
            "oneOf filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:state ;
                 owl:allValuesFrom [ owl:oneOf ( g:active g:inactive ) ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:state ;
                 logic:allValuesFrom [ logic:oneOf ( g:active g:inactive ) ] ] ."#,
        ),
        (
            "complementOf filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:member ;
                 owl:allValuesFrom [ owl:complementOf g:ForbiddenItem ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:member ;
                 logic:allValuesFrom [ logic:complementOf g:ForbiddenItem ] ] ."#,
        ),
        (
            "faceted datatype onDatatype/withRestrictions filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:code ; owl:allValuesFrom
                 [ a rdfs:Datatype ; owl:onDatatype xsd:string ;
                   owl:withRestrictions ( [ xsd:minLength 2 ] ) ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:code ; logic:allValuesFrom
                 [ a rdfs:Datatype ; logic:onDatatype xsd:string ;
                   logic:withRestrictions ( [ xsd:minLength 2 ] ) ] ] ."#,
        ),
    ];

    let mut merged_logic_shapes = None;
    for (name, owl_ttl, logic_ttl) in cases {
        let owl = shape_dataset(owl_ttl);
        let logic = shape_dataset(logic_ttl);
        let owl_shapes = derive_validation_shapes(owl.as_ref())
            .unwrap_or_else(|error| panic!("derive OWL spelling for {name}: {error}"));
        let logic_shapes = derive_validation_shapes(logic.as_ref())
            .unwrap_or_else(|error| panic!("derive canonical logic spelling for {name}: {error}"));
        assert_eq!(
            logic_shapes, owl_shapes,
            "canonical logic: spelling must match its OWL projection for {name}"
        );
        if name.starts_with("same-path") {
            merged_logic_shapes = Some(logic_shapes);
        }
    }

    let logic_shapes = merged_logic_shapes.expect("the same-path merge case ran");
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
    // A functional characteristic is read from the canonical logic: carrier (a
    // logic:PropertyCharacteristicAssertion joining logic:characterizes + logic:characteristicSort),
    // NOT the deprecated owl:FunctionalProperty marker. With no rdfs:domain, only the
    // property-scoped SubjectsOf(P) cap is derived.
    let ds = shape_dataset(
        "[] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:id ; \
             logic:characteristicSort logic:functionalProperty .",
    );
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
fn derive_functional_property_deprecated_owl_marker_is_not_projected() {
    // The bare owl:FunctionalProperty marker no longer projects a cap: it is a deprecated source,
    // superseded by the logic: carrier. A property carrying ONLY the marker (no carrier record)
    // derives no functional shape at all.
    let ds = shape_dataset("g:id a owl:FunctionalProperty .");
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| matches!(
            &s.target,
            ShapeTarget::SubjectsOf(p) if p.ends_with("id")
        )),
        "deprecated owl:FunctionalProperty marker must not project a SubjectsOf cap: {shapes:?}"
    );
}

#[test]
fn derive_inverse_functional_property_objects_of_inverted_max_one() {
    // Inverse-functional is likewise read from the logic: carrier (zero live instances in the
    // repo; a tested capability). With no rdfs:range, only the property-scoped ObjectsOf(P) cap.
    let ds = shape_dataset(
        "[] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:isbn ; \
             logic:characteristicSort logic:inverseFunctionalProperty .",
    );
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
fn derive_functional_carrier_caps_domain_class_node_shape() {
    // The functional cap must ALSO land on the domain CLASS node shape (sh:targetClass C), which is
    // what the declarative class-node reader (Pydantic/ShEx) consults to narrow the field to scalar.
    // A functional carrier record on P + P rdfs:domain C ⇒ the {C} Class shape carries a forward
    // maxCount=1 on P (in addition to the property-scoped SubjectsOf(P) cap).
    let ds = shape_dataset(
        "g:Book a owl:Class . \
         g:primaryAuthor rdfs:domain g:Book .\n\
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:primaryAuthor ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let class_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("Book")))
        .expect("a Class(Book) shape");
    let pc = class_shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("primaryAuthor"))
        .expect("Class(Book) carries a property on primaryAuthor");
    assert_eq!(
        pc.max_count,
        Some(1),
        "functional cap lands on the domain class node shape"
    );
    assert!(!pc.inverse, "a forward functional cap, not inverse");
    // And the property-scoped cap is still present.
    assert!(
        shapes.iter().any(
            |s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("primaryAuthor"))
                && s.properties
                    .iter()
                    .any(|p| p.path.ends_with("primaryAuthor") && p.max_count == Some(1))
        ),
        "the property-scoped SubjectsOf(primaryAuthor) cap is still emitted: {shapes:?}"
    );
}

#[test]
fn derive_functional_carrier_no_domain_fabricates_no_class() {
    // A functional property with NO rdfs:domain (the gmeow:unit shape) gets ONLY the property-scoped
    // cap — no class node shape is synthesized (there is no domain class to attach it to).
    let ds = shape_dataset(
        "[] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:unit ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        shapes
            .iter()
            .any(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("unit"))),
        "the property-scoped SubjectsOf(unit) cap is emitted"
    );
    assert!(
        !shapes
            .iter()
            .any(|s| matches!(&s.target, ShapeTarget::Class(_))),
        "no class node shape is fabricated for a domain-less functional property: {shapes:?}"
    );
}

#[test]
fn derive_inverse_functional_carrier_caps_range_class_node_shape() {
    // The inverse-functional cap must ALSO land on the range CLASS node shape as an INVERTED
    // maxCount=1: an inverse-functional carrier record on P + P rdfs:range C ⇒ the {C} Class shape
    // carries an inverted maxCount=1 on P.
    let ds = shape_dataset(
        "g:Isbn a owl:Class . \
         g:isbn rdfs:range g:Isbn .\n\
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:isbn ; \
             logic:characteristicSort logic:inverseFunctionalProperty .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let class_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("Isbn")))
        .expect("a Class(Isbn) shape");
    let pc = class_shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("isbn"))
        .expect("Class(Isbn) carries a property on isbn");
    assert_eq!(pc.max_count, Some(1), "inverse-functional cap → maxCount 1");
    assert!(pc.inverse, "inverse-functional cap is an inverted path");
}

#[test]
fn derive_colourspace_maxqualified_thing_idiom_unchanged_by_carrier() {
    // The colourspace idiom — a NON-functional property capped by a class-scoped
    // `logic:maxQualifiedCardinality 1 ; logic:onClass owl:Thing` restriction — is derived by the
    // FAMILY 1 restriction walk and is UNCHANGED by the functional-carrier rewrite (there is no
    // functional carrier record; the cap comes from the restriction, not a characteristic).
    let ds = shape_dataset(
        "g:Availability a owl:Class ; rdfs:subClassOf \
         [ a logic:Restriction ; logic:onProperty g:availabilityStatus ; \
           logic:maxQualifiedCardinality 1 ; logic:onClass owl:Thing ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let class_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("Availability")))
        .expect("a Class(Availability) shape");
    let pc = class_shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("availabilityStatus"))
        .expect("Class(Availability) carries a property on availabilityStatus");
    assert_eq!(
        pc.max_count,
        Some(1),
        "maxQualifiedCardinality 1 ; onClass owl:Thing degrades to a plain sh:maxCount 1"
    );
    assert!(!pc.inverse, "the restriction cap is a forward path");
    // No functional carrier record exists, so no property-scoped SubjectsOf cap is spuriously added.
    assert!(
        !shapes.iter().any(
            |s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("availabilityStatus"))
        ),
        "the non-functional property gets no SubjectsOf cap: {shapes:?}"
    );
}

#[test]
fn functional_completeness_invariant_flags_carrierless_and_clears_when_carried() {
    // An owl:FunctionalProperty declaration with NO logic: carrier record is an authoring gap: the
    // invariant returns the offending property.
    let gap = shape_dataset("g:legacyId a owl:FunctionalProperty .");
    let missing = functional_properties_missing_logic_carrier(gap.as_ref());
    assert!(
        missing.iter().any(|p| p.ends_with("legacyId")),
        "carrierless owl:FunctionalProperty is flagged: {missing:?}"
    );
    // Add the carrier record and the gap clears.
    let carried = shape_dataset(
        "g:legacyId a owl:FunctionalProperty .\n\
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:legacyId ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    assert!(
        functional_properties_missing_logic_carrier(carried.as_ref()).is_empty(),
        "a carrier record clears the authoring gap"
    );
}

#[test]
fn functional_carrier_integrity_flags_reintroduced_owl_marker() {
    // The retained RE-introduction guard: a bare owl:FunctionalProperty declaration with no
    // carrier is a ReintroducedOwlMarker violation — the invariant still bites if the deprecated
    // marker returns.
    let ds = shape_dataset("g:legacyId a owl:FunctionalProperty .");
    let violations = functional_carrier_integrity(ds.as_ref());
    assert!(
        violations.iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::ReintroducedOwlMarker { property } if property.ends_with("legacyId")
        )),
        "re-introduced owl:FunctionalProperty is a ReintroducedOwlMarker violation: {violations:?}"
    );
}

#[test]
fn functional_carrier_integrity_flags_orphan_carrier() {
    // A functional carrier whose logic:characterizes names an IRI declared by NO property type is
    // an OrphanCarrier violation (a misspelled / never-declared target).
    let ds = shape_dataset(
        "[] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:neverDeclaredProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let violations = functional_carrier_integrity(ds.as_ref());
    assert!(
        violations.iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::OrphanCarrier { property } if property.ends_with("neverDeclaredProp")
        )),
        "a carrier characterizing an undeclared IRI is an OrphanCarrier violation: {violations:?}"
    );
    // A declared property clears the orphan check (only the ledger-drift noise remains for it).
    let declared = shape_dataset(
        "g:realProp a owl:ObjectProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:realProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    assert!(
        !functional_carrier_integrity(declared.as_ref()).iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::OrphanCarrier { property } if property.ends_with("realProp")
        )),
        "a declared property is not an orphan"
    );
}

#[test]
fn functional_carrier_integrity_flags_duplicate_carrier() {
    // Two functional carrier records naming the same property is a DuplicateCarrier violation.
    let ds = shape_dataset(
        "g:dupProp a owl:ObjectProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:dupProp ; \
             logic:characteristicSort logic:functionalProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:dupProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let violations = functional_carrier_integrity(ds.as_ref());
    assert!(
        violations.iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::DuplicateCarrier { property, count }
                if property.ends_with("dupProp") && *count == 2
        )),
        "two carriers for one property is a DuplicateCarrier violation with count 2: {violations:?}"
    );
    // A single carrier for the same property does not trip the duplicate check.
    let single = shape_dataset(
        "g:dupProp a owl:ObjectProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:dupProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    assert!(
        !functional_carrier_integrity(single.as_ref()).iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::DuplicateCarrier { property, .. } if property.ends_with("dupProp")
        )),
        "a single carrier is not a duplicate"
    );
}

#[test]
fn functional_carrier_ledger_drift_names_missing_and_unexpected() {
    // Prove the completeness ledger is NON-VACUOUS: a small store carries NONE of the frozen
    // ledger's 719 properties, so every ledger entry surfaces as a LedgerMissing that NAMES it —
    // the exact "a property silently lost its carrier" hard-fail. The store's own lone carrier
    // (g:unexpectedProp, absent from the ledger) surfaces as a LedgerUnexpected that names it.
    let ds = shape_dataset(
        "g:unexpectedProp a owl:ObjectProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:unexpectedProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let violations = functional_carrier_integrity(ds.as_ref());
    let missing: Vec<&String> = violations
        .iter()
        .filter_map(|v| match v {
            FunctionalCarrierViolation::LedgerMissing { property } => Some(property),
            _ => None,
        })
        .collect();
    assert_eq!(
        missing.len(),
        719,
        "every frozen ledger entry with no live carrier is named as LedgerMissing"
    );
    assert!(
        missing.iter().any(|p| p.ends_with("acceptanceStatus")),
        "a specific ledger property is named as missing when its carrier is absent"
    );
    assert!(
        violations.iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::LedgerUnexpected { property } if property.ends_with("unexpectedProp")
        )),
        "an un-blessed carrier surfaces as a LedgerUnexpected naming it: {violations:?}"
    );
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
    // A `logic:KeyAssertion` (logic:keyClass C ; logic:keyProperty P) is the canonical carrier of a
    // datatype/single-property key — the greenfield replacement for `C owl:hasKey ( P )` (an
    // owl:InverseFunctionalProperty on a datatype property would be OWL 2 Full). Its closed-world
    // reading is the same inverse sh:maxCount 1 the InverseFunctionalProperty arm emits.
    let ds = shape_dataset(
        "g:GTSSegment a owl:Class . \
         g:gtsHeadId a owl:DatatypeProperty ; rdfs:domain g:GTSSegment ; rdfs:range xsd:string . \
         logic:gtsSegmentHeadKey a logic:KeyAssertion ; \
             logic:keyClass g:GTSSegment ; logic:keyProperty g:gtsHeadId .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("gtsHeadId")))
        .expect("an ObjectsOf(gtsHeadId) shape from the logic:KeyAssertion carrier");
    let prop = &shape.properties[0];
    assert!(
        prop.inverse,
        "a single-property key must derive an inverse-path shape"
    );
    assert_eq!(
        prop.max_count,
        Some(1),
        "key → each key value has ≤1 subject"
    );
}

#[test]
fn derive_composite_has_key_derives_no_single_path_shape() {
    // A COMPOSITE key (a logic:KeyAssertion naming several logic:keyProperty values) asserts the
    // TUPLE is unique, not each part — it has no single-path SHACL form, so no per-part uniqueness
    // shape may be derived.
    let ds = shape_dataset(
        "g:C a owl:Class . g:p1 a owl:DatatypeProperty . g:p2 a owl:DatatypeProperty . \
         logic:cCompositeKey a logic:KeyAssertion ; \
             logic:keyClass g:C ; logic:keyProperty g:p1 , g:p2 .",
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
    // A domain axiom (opted IN to closed-world reading) AND a functional carrier record on the
    // SAME property must fold into ONE SubjectsOf(P) shape carrying both the node Class and the
    // maxCount-1 property. (The functional maxCount derives regardless; the domain node-class
    // needs the ClosedWorldClosure opt-in since domain is open-world by default.)
    let ds = shape_dataset_with_logic(
        "g:Doc a owl:Class . \
         g:isbn rdfs:domain g:Doc .\n\
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:isbn ; \
             logic:characteristicSort logic:functionalProperty .\n\
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
fn join_aggregate_record_expands_and_contributes_nothing_to_the_reasoned_axiom_set() {
    // A well-formed two-leg join-aggregate record expands to exactly one canonical logic:Constraint
    // carrying the JoinAggregate satellite, targets the guarded class, and none of its structural
    // triples (joinPath / leg role predicates / the leg blank nodes) leak into prog.axioms.
    let (prog, diags) = parse(
        "ex:boundarySquareZero a logic:JoinAggregateConstraint ;\n\
           logic:onClass ex:TopCell ;\n\
           logic:aggFunction \"SUM\" ;\n\
           logic:aggComparator \"=\" ;\n\
           logic:aggThreshold 0 ;\n\
           logic:joinPath (\n\
             [ logic:legRecordType ex:Incidence ; logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
             [ logic:legRecordType ex:Incidence ; logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
           ) ;\n\
           logic:formalizes ex:BoundaryOperator .",
    );
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    assert_eq!(prog.constraints.len(), 1, "exactly one constraint expands");
    let c = &prog.constraints[0];
    let ja = c
        .join_aggregate
        .as_ref()
        .expect("the join-aggregate satellite must be present");
    assert_eq!(ja.function, "SUM");
    assert_eq!(ja.legs.len(), 2, "two legs — a genuine multi-hop join");
    assert_eq!(ja.threshold_lexical, "0");
    assert!(
        matches!(&c.target, ShapeTarget::Class(cl) if cl.ends_with("/TopCell")),
        "the join-aggregate must target the guarded class: {:?}",
        c.target
    );
    assert!(
        !prog.axioms.iter().any(|a| {
            a.predicate.contains("/legSource")
                || a.predicate.contains("/legTarget")
                || a.predicate.contains("/legValue")
                || a.predicate.contains("/legRecordType")
                || a.predicate.contains("/joinPath")
        }),
        "no join-aggregate structural triple may leak into prog.axioms; got: {:?}",
        prog.axioms
    );
}

#[test]
fn join_aggregate_leg_missing_a_value_predicate_is_a_malformed_record() {
    // A leg without logic:legValue cannot form the group product — fail-soft: one
    // MALFORMED_CONSTRAINT diagnostic, never a silent partial constraint.
    let (prog, diags) = parse(
        "ex:brokenLeg a logic:JoinAggregateConstraint ;\n\
           logic:onClass ex:TopCell ;\n\
           logic:aggFunction \"SUM\" ;\n\
           logic:aggComparator \"=\" ;\n\
           logic:aggThreshold 0 ;\n\
           logic:joinPath (\n\
             [ logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
             [ logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ]\n\
           ) ;\n\
           logic:formalizes ex:BoundaryOperator .",
    );
    assert!(prog.constraints.is_empty(), "no constraint may expand");
    assert!(
        diags.iter().any(|d| d.message.contains("legValue")),
        "a value-less leg must diagnose: {diags:?}"
    );
}

#[test]
fn join_aggregate_single_leg_is_not_a_join() {
    // One hop is a plain aggregate, not a JOIN — the record is malformed (needs ≥ 2 legs).
    let (prog, diags) = parse(
        "ex:oneHop a logic:JoinAggregateConstraint ;\n\
           logic:onClass ex:TopCell ;\n\
           logic:aggFunction \"SUM\" ;\n\
           logic:aggComparator \"=\" ;\n\
           logic:aggThreshold 0 ;\n\
           logic:joinPath (\n\
             [ logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
           ) ;\n\
           logic:formalizes ex:BoundaryOperator .",
    );
    assert!(prog.constraints.is_empty(), "no constraint may expand");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("at least two join legs")),
        "a single-leg record must diagnose: {diags:?}"
    );
}

/// Build a two-leg `logic:joinPath` Turtle fragment where `bad_pred`'s value on the FIRST leg is
/// replaced with `bad_value_ttl` (a literal or blank node), and every other structural predicate
/// on that leg — plus the whole second leg — is a well-formed IRI. Used to falsify Gap 12b: each
/// of `legSource`/`legTarget`/`legValue`/`legRecordType` must reject a non-IRI value rather than
/// silently stringify it.
fn join_leg_with_bad_value(bad_pred: &str, bad_value_ttl: &str) -> String {
    let mut fields = Vec::new();
    for (pred, iri) in [
        ("legSource", "ex:incidenceCoface"),
        ("legTarget", "ex:incidenceFace"),
        ("legValue", "ex:incidenceSign"),
    ] {
        if pred == bad_pred {
            fields.push(format!("logic:{pred} {bad_value_ttl}"));
        } else {
            fields.push(format!("logic:{pred} {iri}"));
        }
    }
    if bad_pred == "legRecordType" {
        fields.push(format!("logic:legRecordType {bad_value_ttl}"));
    }
    format!(
        "ex:badLeg a logic:JoinAggregateConstraint ;\n\
           logic:onClass ex:TopCell ;\n\
           logic:aggFunction \"SUM\" ;\n\
           logic:aggComparator \"=\" ;\n\
           logic:aggThreshold 0 ;\n\
           logic:joinPath (\n\
             [ {} ]\n\
             [ logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
           ) ;\n\
           logic:formalizes ex:BoundaryOperator .",
        fields.join(" ; "),
    )
}

#[test]
fn join_aggregate_leg_rejects_a_literal_value_for_every_structural_predicate() {
    // legSource/legTarget/legValue/legRecordType are record→endpoint/value PREDICATES; a
    // literal value must be rejected as malformed, not silently stringified (Gap 12b).
    for bad_pred in ["legSource", "legTarget", "legValue", "legRecordType"] {
        let (prog, diags) = parse(&join_leg_with_bad_value(bad_pred, "\"not-an-iri\""));
        assert!(
            prog.constraints.is_empty(),
            "a leg with a literal {bad_pred} must not expand: {:?}",
            prog.constraints
        );
        assert!(
            diags.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"
                && d.message.contains(bad_pred)
                && d.message.contains("must be an IRI")),
            "a literal {bad_pred} must diagnose as malformed: {diags:?}"
        );
    }
}

#[test]
fn join_aggregate_leg_rejects_a_blank_node_value_for_every_structural_predicate() {
    // Same as above, but the malformed value is a blank node rather than a literal — neither
    // is an IRI, and both must be rejected the same way (Gap 12b).
    for bad_pred in ["legSource", "legTarget", "legValue", "legRecordType"] {
        let (prog, diags) = parse(&join_leg_with_bad_value(bad_pred, "[ ]"));
        assert!(
            prog.constraints.is_empty(),
            "a leg with a blank-node {bad_pred} must not expand: {:?}",
            prog.constraints
        );
        assert!(
            diags.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"
                && d.message.contains(bad_pred)
                && d.message.contains("must be an IRI")),
            "a blank-node {bad_pred} must diagnose as malformed: {diags:?}"
        );
    }
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

// ── Compound function-term applications (logic:termApplication / logic:FunctionTerm) ──────────

#[test]
fn compound_function_term_parses_into_nested_term_app() {
    use crate::ir::{Formula, Term};
    // An atomic predication `p(H, cons(H, cons(1, nil)))` — its second argument carries
    // logic:termApplication onto a logic:FunctionTerm whose own second argument is again a
    // logic:termApplication, so the parser must reconstruct a NESTED Term::App with argument
    // order and kinds intact. The atom carries a compound function term, so it exceeds the
    // function-free Horn fragment and stays a logic:Formula (never routed to axioms).
    let (prog, diags) = parse(
        "ex:phi a logic:Formula ;
            logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"H\" ] ;
            logic:argument [ logic:termIndex 1 ; logic:termApplication ex:consOuter ] .
         ex:consOuter a logic:FunctionTerm ;
            logic:functionSymbol ex:cons ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"H\" ] ;
            logic:argument [ logic:termIndex 1 ; logic:termApplication ex:consInner ] .
         ex:consInner a logic:FunctionTerm ;
            logic:functionSymbol ex:cons ;
            logic:argument [ logic:termIndex 0 ; logic:termLiteral \"1\" ] ;
            logic:argument [ logic:termIndex 1 ; logic:termIri ex:nil ] .",
    );
    assert!(
        !diags.iter().any(|d| d.code == "MALFORMED_FORMULA"),
        "a well-formed compound term must not be flagged malformed: {diags:?}"
    );
    assert_eq!(
        prog.formulas.len(),
        1,
        "the function-term argument keeps the atom in LogicProgram.formulas: {:?}",
        prog.formulas
    );

    let ex = "https://example.org/test/";
    let cons = format!("{ex}cons");
    let expected_inner = Term::App {
        symbol: cons.clone(),
        args: vec![
            Term::Literal {
                lexical: "1".to_owned(),
                datatype: None,
            },
            Term::Iri(format!("{ex}nil")),
        ],
    };
    let expected_outer = Term::App {
        symbol: cons,
        args: vec![Term::Var("H".to_owned()), expected_inner],
    };

    let Formula::Atom { relation, args } = &prog.formulas[0] else {
        panic!("expected an atomic predication, got {:?}", prog.formulas[0]);
    };
    assert_eq!(*relation, Term::Iri(format!("{ex}p")), "relation preserved");
    assert_eq!(args.len(), 2, "atom arity preserved");
    assert_eq!(
        args[0],
        Term::Var("H".to_owned()),
        "argument 0 order preserved"
    );
    assert_eq!(
        args[1], expected_outer,
        "argument 1 is the nested cons(H, cons(1, nil)) application"
    );
}

#[test]
fn nullary_function_term_is_rejected() {
    // A logic:FunctionTerm with a symbol but ZERO logic:argument carriers is a nullary
    // application. A 0-ary function symbol is a constant (logic:termIri), so this is malformed
    // rather than a degenerate term — mirrors logic:FunctionTermArityConstraint.
    assert_malformed_formula_error(
        "ex:phi a logic:Formula ;
            logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;
            logic:argument [ logic:termIndex 1 ; logic:termApplication ex:empty ] .
         ex:empty a logic:FunctionTerm ; logic:functionSymbol ex:f .",
        "at least one argument",
    );
}

#[test]
fn function_term_without_symbol_is_rejected() {
    // A logic:FunctionTerm bearing arguments but no logic:functionSymbol is malformed —
    // mirrors logic:FunctionSymbolConstraint (exactly one reified symbol required).
    assert_malformed_formula_error(
        "ex:phi a logic:Formula ;
            logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;
            logic:argument [ logic:termIndex 1 ; logic:termApplication ex:ft ] .
         ex:ft a logic:FunctionTerm ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:z ] .",
        "exactly one logic:functionSymbol",
    );
}

#[test]
fn cyclic_function_term_is_rejected() {
    // A logic:FunctionTerm reachable from its own logic:argument expansion is an infinite
    // term; the parser's path guard rejects it rather than recursing forever.
    assert_malformed_formula_error(
        "ex:phi a logic:Formula ;
            logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;
            logic:argument [ logic:termIndex 1 ; logic:termApplication ex:loop ] .
         ex:loop a logic:FunctionTerm ;
            logic:functionSymbol ex:f ;
            logic:argument [ logic:termIndex 0 ; logic:termApplication ex:loop ] .",
        "cyclic",
    );
}

#[test]
fn term_application_carrier_excludes_other_value_kinds() {
    // logic:termApplication is the fifth mutually exclusive term-value kind: a carrier bearing
    // both logic:termApplication and logic:termIri violates the exactly-one rule (mirrors the
    // extended logic:TermCarrierValueConstraint).
    assert_malformed_formula_error(
        "ex:phi a logic:Formula ;
            logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;
            logic:argument [ logic:termIndex 1 ; logic:termApplication ex:ft ; logic:termIri ex:b ] .
         ex:ft a logic:FunctionTerm ;
            logic:functionSymbol ex:f ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:z ] .",
        "requires exactly one term-value property",
    );
}

// ── Reasoning programs (`logic:ReasoningProgram`) — R1 hard-fail (Task 3) ─────────────────

/// A well-formed `logic:ReasoningProgram`: a ground fact `add(z, Y, Y)`, a Horn rule
/// `add(s(X), Y, s(Z)) :- add(X, Y, Z), not blocked(X)` (a compound `Term::App` head over a
/// negated body literal), a single goal `add(A, B, Q)`, one `blocked(a)` verdict probe, and
/// a `logic:variableSort` on the rule's `X` carrier.
const REASONING_PROGRAM_TTL: &str = "\
    ex:prog1 a logic:ReasoningProgram ;
        logic:evaluationMode logic:BackwardEvaluation ;
        logic:programQuery [ a logic:Formula ;
            logic:relation ex:add ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"A\" ] ,
                           [ logic:termIndex 1 ; logic:termVariable \"B\" ] ,
                           [ logic:termIndex 2 ; logic:termVariable \"Q\" ]
        ] ;
        logic:verdictProbe [ a logic:Formula ;
            logic:relation ex:blocked ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ]
        ] ;
        logic:clause [ a logic:Formula ;
            logic:relation ex:add ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:z ] ,
                           [ logic:termIndex 1 ; logic:termVariable \"Y\" ] ,
                           [ logic:termIndex 2 ; logic:termVariable \"Y\" ]
        ] ;
        logic:clause [ a logic:Formula ;
            logic:antecedent [ a logic:Formula ;
                logic:and
                    [ a logic:Formula ;
                      logic:relation ex:add ;
                      logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ; logic:variableSort ex:Nat ] ,
                                     [ logic:termIndex 1 ; logic:termVariable \"Y\" ] ,
                                     [ logic:termIndex 2 ; logic:termVariable \"Z\" ]
                    ] ,
                    [ a logic:Formula ;
                      logic:not [ a logic:Formula ;
                          logic:relation ex:blocked ;
                          logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ]
                      ]
                    ]
            ] ;
            logic:consequent [ a logic:Formula ;
                logic:relation ex:add ;
                logic:argument [ logic:termIndex 0 ; logic:termApplication ex:sX ] ,
                               [ logic:termIndex 1 ; logic:termVariable \"Y\" ] ,
                               [ logic:termIndex 2 ; logic:termApplication ex:sZ ]
            ]
        ] .
    ex:sX a logic:FunctionTerm ;
        logic:functionSymbol ex:s ;
        logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] .
    ex:sZ a logic:FunctionTerm ;
        logic:functionSymbol ex:s ;
        logic:argument [ logic:termIndex 0 ; logic:termVariable \"Z\" ] .
";

#[test]
fn reasoning_program_with_compound_clause_and_negation_parses() {
    let ex = "https://example.org/test/";
    let (prog, diags) = parse(REASONING_PROGRAM_TTL);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "unexpected error diagnostics: {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 1);
    let rp = &prog.reasoning_programs[0];
    assert_eq!(rp.iri, format!("{ex}prog1"));
    assert_eq!(rp.mode, EvaluationMode::Backward);
    assert_eq!(
        rp.clauses.len(),
        2,
        "both the fact and the rule clause parsed: {:?}",
        rp.clauses
    );
    assert_eq!(rp.verdict_probes.len(), 1, "the probe parsed");
    assert!(
        matches!(&rp.query, Formula::Atom { relation, .. } if *relation == Term::Iri(format!("{ex}add"))),
        "query is the add/3 goal atom: {:?}",
        rp.query
    );
    assert!(
        rp.variable_sorts.iter().any(|(scope, v, s)| {
            matches!(scope, crate::ir::VariableSortScope::Clause { .. })
                && v == "X"
                && *s == format!("{ex}Nat")
        }),
        "the antecedent's X carrier's logic:variableSort must be captured, scoped to its OWN \
         clause: {:?}",
        rp.variable_sorts
    );

    // The clause/query/probe formula trees are owned by the reasoning program; they must
    // never also enter LogicProgram.formulas (the same one-fact-two-homes hazard
    // extract_formulas already guards for constraint integrity / recovery-transform roots).
    assert!(
        prog.formulas.is_empty(),
        "clause/query/probe formulas must not also enter LogicProgram.formulas: {:?}",
        prog.formulas
    );
    // Nor must the program's structural predicates/type leak into generic axioms.
    assert!(
        prog.axioms.iter().all(|a| {
            a.obj != logic_iri("ReasoningProgram")
                && !a.predicate.ends_with("/clause")
                && !a.predicate.ends_with("/programQuery")
                && !a.predicate.ends_with("/verdictProbe")
                && !a.predicate.ends_with("/evaluationMode")
        }),
        "reasoning-program structure leaked into generic axioms: {:#?}",
        prog.axioms
    );

    // The rule clause carries the nested compound Term::App head (s(X), s(Z)) and the
    // logic:not body literal.
    let rule_clause = rp
        .clauses
        .iter()
        .find(|f| matches!(f, Formula::Implies(..)))
        .expect("the rule clause must be present");
    let Formula::Implies(antecedent, consequent) = rule_clause else {
        unreachable!("matched above")
    };
    let Formula::Atom { args, .. } = consequent.as_ref() else {
        panic!("consequent must be an atom: {consequent:?}");
    };
    assert!(
        matches!(&args[0], Term::App { symbol, .. } if symbol == &format!("{ex}s")),
        "head arg0 must be s(X): {:?}",
        args[0]
    );
    assert!(
        matches!(&args[2], Term::App { symbol, .. } if symbol == &format!("{ex}s")),
        "head arg2 must be s(Z): {:?}",
        args[2]
    );
    assert!(
        matches!(
            antecedent.as_ref(),
            Formula::And(parts) if parts.iter().any(|p| matches!(p, Formula::Not(_)))
        ),
        "antecedent must carry a logic:not body literal: {antecedent:?}"
    );
}

#[test]
fn reasoning_program_parse_is_deterministic() {
    // The authored source triples ARE the canonical round-trip surface for reasoning-program
    // content (see the `projections::rdf::project_canonical_rdf12` doc comment): re-parsing
    // the identical source twice must yield byte-identical IR, since nothing downstream of
    // the frontend re-serializes and re-reads this content.
    let (first, _) = parse(REASONING_PROGRAM_TTL);
    let (second, _) = parse(REASONING_PROGRAM_TTL);
    assert_eq!(first.canonical_key(), second.canonical_key());
}

/// A `logic:ReasoningProgram` referencing a TYPED constant (`ex:one`, asserted `rdf:type`
/// `math:Integer` AND `math:PositiveNumber` — plural types, on purpose) nested one function
/// application deep (`s(one)`), plus an UNTYPED constant (`ex:untyped`) in the verdict
/// probe, to prove [`ReasoningProgramIr::constant_sorts`] captures the plain `rdf:type`
/// domain triple the stage's L3 fold otherwise drops, recurses into `Term::App` argument
/// carriers, and leaves an unsorted constant absent (never a hard fail).
const REASONING_PROGRAM_WITH_TYPED_CONSTANT_TTL: &str = "\
    @prefix math: <https://blackcatinformatics.ca/math/> .
    ex:one a math:Integer, math:PositiveNumber .
    ex:prog2 a logic:ReasoningProgram ;
        logic:evaluationMode logic:BackwardEvaluation ;
        logic:programQuery [ a logic:Formula ;
            logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ]
        ] ;
        logic:verdictProbe [ a logic:Formula ;
            logic:relation ex:q ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:untyped ]
        ] ;
        logic:clause [ a logic:Formula ;
            logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termApplication ex:sOne ]
        ] .
    ex:sOne a logic:FunctionTerm ;
        logic:functionSymbol ex:s ;
        logic:argument [ logic:termIndex 0 ; logic:termIri ex:one ] .
";

#[test]
fn reasoning_program_captures_constant_rdf_type_as_sort_declarations() {
    let ex = "https://example.org/test/";
    let math = "https://blackcatinformatics.ca/math/";
    let (prog, diags) = parse(REASONING_PROGRAM_WITH_TYPED_CONSTANT_TTL);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "unexpected error diagnostics: {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 1);
    let rp = &prog.reasoning_programs[0];

    // Both asserted rdf:type IRIs on the constant nested inside s(one) are captured — not
    // just the first — since a constant may legitimately carry several sorts at once.
    assert!(
        rp.constant_sorts
            .contains(&(format!("{ex}one"), format!("{math}Integer"))),
        "ex:one's math:Integer type must be captured from a Term::App argument: {:?}",
        rp.constant_sorts
    );
    assert!(
        rp.constant_sorts
            .contains(&(format!("{ex}one"), format!("{math}PositiveNumber"))),
        "ex:one's SECOND asserted type (math:PositiveNumber) must ALSO be captured, not just \
         the first: {:?}",
        rp.constant_sorts
    );
    assert_eq!(
        rp.constant_sorts.len(),
        2,
        "exactly the two asserted types on ex:one — the untyped verdict-probe constant \
         (ex:untyped) contributes NO entry, and it is not a hard fail: {:?}",
        rp.constant_sorts
    );

    // Deterministic: sorted, deduplicated.
    let mut sorted = rp.constant_sorts.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        rp.constant_sorts, sorted,
        "constant_sorts is already sorted and deduplicated by ReasoningProgramIr::new"
    );
}

/// Assert that `ttl` fails to yield any [`ReasoningProgramIr`] and instead emits an
/// error-grade `MALFORMED_REASONING_PROGRAM` diagnostic containing `expected_detail` —
/// mirrors [`assert_malformed_formula_error`] for the reasoning-program surface.
fn assert_malformed_reasoning_program(ttl: &str, expected_detail: &str) {
    let (prog, diags) = parse(ttl);
    assert!(
        prog.reasoning_programs.is_empty(),
        "a malformed reasoning program must never enter the IR: {:?}",
        prog.reasoning_programs
    );
    assert!(
        diags.iter().any(|d| {
            d.code == "MALFORMED_REASONING_PROGRAM"
                && d.severity == Severity::Error
                && d.message.contains(expected_detail)
        }),
        "expected an error-grade MALFORMED_REASONING_PROGRAM containing {expected_detail:?}: {diags:?}"
    );
}

#[test]
fn reasoning_program_with_zero_clauses_is_hard_failed() {
    assert_malformed_reasoning_program(
        "ex:prog a logic:ReasoningProgram ;
            logic:evaluationMode logic:BackwardEvaluation ;
            logic:programQuery [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ]
            ] .",
        "at least one logic:clause",
    );
}

#[test]
fn reasoning_program_missing_query_is_hard_failed() {
    assert_malformed_reasoning_program(
        "ex:prog a logic:ReasoningProgram ;
            logic:evaluationMode logic:BackwardEvaluation ;
            logic:clause [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] .",
        "exactly one logic:programQuery",
    );
}

#[test]
fn reasoning_program_with_two_queries_is_hard_failed() {
    assert_malformed_reasoning_program(
        "ex:prog a logic:ReasoningProgram ;
            logic:evaluationMode logic:BackwardEvaluation ;
            logic:clause [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] ;
            logic:programQuery [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] ,
            [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,
                               [ logic:termIndex 1 ; logic:termVariable \"Y\" ]
            ] .",
        "exactly one logic:programQuery",
    );
}

#[test]
fn reasoning_program_with_non_atomic_verdict_probe_is_hard_failed() {
    assert_malformed_reasoning_program(
        "ex:prog a logic:ReasoningProgram ;
            logic:evaluationMode logic:BackwardEvaluation ;
            logic:clause [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] ;
            logic:programQuery [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] ;
            logic:verdictProbe [ a logic:Formula ;
                logic:and
                    [ a logic:Formula ;
                      logic:relation ex:p ;
                      logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,
                                     [ logic:termIndex 1 ; logic:termIri ex:b ] ] ,
                    [ a logic:Formula ;
                      logic:relation ex:q ;
                      logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,
                                     [ logic:termIndex 1 ; logic:termIri ex:b ] ]
            ] .",
        "must be an atomic logic:Formula",
    );
}

#[test]
fn reasoning_program_with_unknown_evaluation_mode_is_hard_failed() {
    assert_malformed_reasoning_program(
        "ex:prog a logic:ReasoningProgram ;
            logic:evaluationMode logic:FooBarEvaluation ;
            logic:clause [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] ;
            logic:programQuery [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] .",
        "not a recognized logic:EvaluationMode value",
    );
}

#[test]
fn reasoning_program_with_non_ground_verdict_probe_is_hard_failed() {
    // A `logic:verdictProbe` reports ONE ground atom's three-valued well-founded verdict, so a
    // variable-bearing probe (`win(X)`) has no single verdict to report — it would lower to
    // `win(?0)` whose truth is a silent MISREPORT (`false`). `ReasoningProgramIr::new` hard-fails
    // it instead.
    assert_malformed_reasoning_program(
        "ex:prog a logic:ReasoningProgram ;
            logic:evaluationMode logic:BackwardEvaluation ;
            logic:clause [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] ;
            logic:programQuery [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] ;
            logic:verdictProbe [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] .",
        "must be a GROUND atom",
    );
}

#[test]
fn reasoning_program_with_conflicting_variable_sorts_within_one_scope_is_hard_failed() {
    // The same variable `X` is assigned two DIFFERENT sorts on its two carriers WITHIN ONE
    // clause — an ambiguous order-sort context the unifier cannot seed deterministically
    // (`ReasoningProgramIr::new`'s per-scope conflict guard, not a `module.ttl` cardinality
    // rule). Both `X` occurrences are the SAME variable (one clause is one scope), so the two
    // sorts genuinely conflict.
    assert_malformed_reasoning_program(
        "ex:prog a logic:ReasoningProgram ;
            logic:evaluationMode logic:BackwardEvaluation ;
            logic:clause [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ; logic:variableSort ex:Nat ] ,
                               [ logic:termIndex 1 ; logic:termVariable \"X\" ; logic:variableSort ex:Str ]
            ] ;
            logic:programQuery [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] .",
        "two distinct sorts",
    );
}

#[test]
fn reasoning_program_same_name_different_sorts_across_scopes_is_accepted() {
    // The SAME authored name `X` carries `ex:Nat` in the clause and `ex:Str` in the query.
    // These are DIFFERENT scopes (each clause / the query is a fresh variable scope), so the
    // two `X`s are UNRELATED variables and may legitimately carry different sorts — this must
    // NOT be a conflict. Both declarations are captured, each tagged with its owning scope.
    let ex = "https://example.org/test/";
    let (prog, diags) = parse(
        "ex:prog a logic:ReasoningProgram ;
            logic:evaluationMode logic:BackwardEvaluation ;
            logic:clause [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ; logic:variableSort ex:Nat ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] ;
            logic:programQuery [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ; logic:variableSort ex:Str ] ,
                               [ logic:termIndex 1 ; logic:termIri ex:b ]
            ] .",
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "same-named vars in different scopes must NOT conflict: {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 1);
    let rp = &prog.reasoning_programs[0];
    assert!(
        rp.variable_sorts.iter().any(|(scope, v, s)| {
            matches!(scope, crate::ir::VariableSortScope::Clause { .. })
                && v == "X"
                && *s == format!("{ex}Nat")
        }),
        "the clause's X:Nat is captured under its clause scope: {:?}",
        rp.variable_sorts
    );
    assert!(
        rp.variable_sorts.iter().any(|(scope, v, s)| {
            *scope == crate::ir::VariableSortScope::Query && v == "X" && *s == format!("{ex}Str")
        }),
        "the query's X:Str is captured under the query scope: {:?}",
        rp.variable_sorts
    );
}

#[test]
fn reasoning_program_identical_clauses_distinct_sorts_are_accepted_and_scoped() {
    // Two STRUCTURALLY-IDENTICAL clauses `p(X)` are authored, one declaring `X:Nat` and the
    // other `X:Real`. They share a `Formula::content_key` (a `logic:variableSort` is harvested
    // separately and is NOT part of the clause AST), so the ONLY thing that keeps their scopes
    // apart is the occurrence-index disambiguation. This must be ACCEPTED — the two `X`s are
    // unrelated variables in two distinct clause scopes — not falsely rejected as an
    // intra-scope sort conflict (the residual bug this fix closes: a content_key-only scope key
    // collapsed both clauses into one scope and hard-failed them).
    let ex = "https://example.org/test/";
    let (prog, diags) = parse(
        "ex:prog a logic:ReasoningProgram ;
            logic:evaluationMode logic:BackwardEvaluation ;
            logic:clause [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ; logic:variableSort ex:Nat ]
            ] ;
            logic:clause [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ; logic:variableSort ex:Real ]
            ] ;
            logic:programQuery [ a logic:Formula ;
                logic:relation ex:p ;
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"R\" ]
            ] .",
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "two identical clauses with different variable sorts must be accepted (distinct scopes, \
         no false conflict): {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 1);
    let rp = &prog.reasoning_programs[0];
    assert_eq!(
        rp.clauses.len(),
        2,
        "both structurally-identical clauses are retained: {:?}",
        rp.clauses
    );

    // Both clause-scoped `X` declarations are present, keyed by the SAME content_key but
    // DIFFERENT occurrence indices {0, 1}, carrying the two distinct sorts {Nat, Real}.
    let clause_x: Vec<(&str, usize, &str)> = rp
        .variable_sorts
        .iter()
        .filter_map(|(scope, v, s)| match scope {
            crate::ir::VariableSortScope::Clause { key, occurrence } if v == "X" => {
                Some((key.as_str(), *occurrence, s.as_str()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        clause_x.len(),
        2,
        "both clause-scoped X declarations are captured: {:?}",
        rp.variable_sorts
    );
    assert_eq!(
        clause_x[0].0, clause_x[1].0,
        "the two clauses share one content_key (they are structurally identical)"
    );
    let occurrences: std::collections::BTreeSet<usize> =
        clause_x.iter().map(|(_, occ, _)| *occ).collect();
    assert_eq!(
        occurrences,
        [0, 1].into_iter().collect(),
        "the two identical clauses are disambiguated by occurrence index 0 and 1: {:?}",
        rp.variable_sorts
    );
    let sorts: std::collections::BTreeSet<&str> = clause_x.iter().map(|(_, _, s)| *s).collect();
    assert_eq!(
        sorts,
        [format!("{ex}Nat"), format!("{ex}Real")]
            .iter()
            .map(String::as_str)
            .collect(),
        "each occurrence carries its OWN authored sort (Nat and Real): {:?}",
        rp.variable_sorts
    );
}

// ── Modal operators: parse-time STANDARD-TRANSLATION into the FOL Formula IR ───────────────────

const ACTUAL_WORLD: &str = "https://blackcatinformatics.ca/logic/actualWorld";
const EPISTEMICALLY_POSSIBLE: &str = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
const DOXASTICALLY_ACCESSIBLE: &str = "https://blackcatinformatics.ca/logic/doxasticallyAccessible";

#[test]
fn box_atom_expands_via_standard_translation() {
    use crate::ir::{Formula, Term};
    // □P(a) over logic:epistemicallyPossible ↦ ∀ __w0 . R(actualWorld, __w0) → P(__w0, a).
    // No new Formula IR variant — the modal node expands into Forall/Implies/Atom at parse time,
    // and the body atom must NOT ALSO surface as a top-level formula.
    let (prog, diags) = parse(
        "ex:nec a logic:Formula ;
            logic:necessarily [ a logic:Formula ; logic:relation ex:P ;
                logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termIri ex:a ] ] ;
            logic:overAccessibility logic:epistemicallyPossible .",
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "a well-formed □ node must not emit any error diagnostic: {diags:?}"
    );
    assert_eq!(
        prog.formulas.len(),
        1,
        "the modal node is the sole top-level formula; its body is a component: {:?}",
        prog.formulas
    );

    let ex = "https://example.org/test/";
    let Formula::Forall { vars, body } = &prog.formulas[0] else {
        panic!("□ expands to a Forall, got {:?}", prog.formulas[0]);
    };
    assert_eq!(vars, &vec!["__w0".to_owned()], "box binds a fresh world var");
    let Formula::Implies(acc, head) = body.as_ref() else {
        panic!("□ body is an Implies(accessibility, head), got {body:?}");
    };
    let Formula::Atom { relation, args } = acc.as_ref() else {
        panic!("accessibility guard is an atom, got {acc:?}");
    };
    assert_eq!(
        *relation,
        Term::Iri(EPISTEMICALLY_POSSIBLE.to_owned()),
        "the guard uses the pinned typed accessibility relation"
    );
    assert_eq!(
        args,
        &vec![
            Term::Iri(ACTUAL_WORLD.to_owned()),
            Term::Var("__w0".to_owned())
        ],
        "the guard runs from the actual world to the bound world var"
    );
    let Formula::Atom { relation, args } = head.as_ref() else {
        panic!("head is the relativized atom, got {head:?}");
    };
    assert_eq!(*relation, Term::Iri(format!("{ex}P")), "relation preserved");
    assert_eq!(args.len(), 2, "the world is prepended to the atom arguments");
    assert_eq!(
        args[0],
        Term::Var("__w0".to_owned()),
        "the prepended world is the bound world var, not the actual world"
    );
    assert_eq!(
        args[1],
        Term::Iri(format!("{ex}a")),
        "the original argument follows the prepended world"
    );
}

#[test]
fn diamond_atom_expands_via_standard_translation() {
    use crate::ir::{Formula, Term};
    // ◇P(a) over logic:epistemicallyPossible ↦ ∃ __w0 . R(actualWorld, __w0) ∧ P(__w0, a).
    let (prog, diags) = parse(
        "ex:pos a logic:Formula ;
            logic:possibly [ a logic:Formula ; logic:relation ex:P ;
                logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termIri ex:a ] ] ;
            logic:overAccessibility logic:epistemicallyPossible .",
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "a well-formed ◇ node must not emit any error diagnostic: {diags:?}"
    );
    assert_eq!(
        prog.formulas.len(),
        1,
        "the modal node is the sole top-level formula: {:?}",
        prog.formulas
    );

    let ex = "https://example.org/test/";
    let Formula::Exists { vars, body } = &prog.formulas[0] else {
        panic!("◇ expands to an Exists, got {:?}", prog.formulas[0]);
    };
    assert_eq!(
        vars,
        &vec!["__w0".to_owned()],
        "diamond binds a fresh world var"
    );
    let Formula::And(parts) = body.as_ref() else {
        panic!("◇ body is an And(accessibility, head), got {body:?}");
    };
    assert_eq!(parts.len(), 2, "the ∧ pairs the guard with the body");
    let Formula::Atom { relation, args } = &parts[0] else {
        panic!("first conjunct is the accessibility guard, got {:?}", parts[0]);
    };
    assert_eq!(*relation, Term::Iri(EPISTEMICALLY_POSSIBLE.to_owned()));
    assert_eq!(
        args,
        &vec![
            Term::Iri(ACTUAL_WORLD.to_owned()),
            Term::Var("__w0".to_owned())
        ],
    );
    let Formula::Atom { relation, args } = &parts[1] else {
        panic!("second conjunct is the relativized atom, got {:?}", parts[1]);
    };
    assert_eq!(*relation, Term::Iri(format!("{ex}P")));
    assert_eq!(args.len(), 2);
    assert_eq!(args[0], Term::Var("__w0".to_owned()));
    assert_eq!(args[1], Term::Iri(format!("{ex}a")));
}

#[test]
fn nested_modal_threads_inner_world() {
    use crate::ir::{Formula, Term};
    // □◇P(a): the outer □ binds __w0, and the inner ◇ (at depth 1) binds __w1. The INNER
    // accessibility atom must run from the OUTER bound world __w0, never from the actual-world
    // constant — that is what "threading the inner world" means.
    let (prog, diags) = parse(
        "ex:outer a logic:Formula ;
            logic:necessarily ex:inner ;
            logic:overAccessibility logic:epistemicallyPossible .
         ex:inner a logic:Formula ;
            logic:possibly [ a logic:Formula ; logic:relation ex:P ;
                logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termIri ex:a ] ] ;
            logic:overAccessibility logic:doxasticallyAccessible .",
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "a well-formed □◇ nest must not emit any error diagnostic: {diags:?}"
    );
    assert_eq!(
        prog.formulas.len(),
        1,
        "only the outer modal node is top-level: {:?}",
        prog.formulas
    );

    let ex = "https://example.org/test/";
    let Formula::Forall { vars, body } = &prog.formulas[0] else {
        panic!("outer □ is a Forall, got {:?}", prog.formulas[0]);
    };
    assert_eq!(vars, &vec!["__w0".to_owned()]);
    let Formula::Implies(acc_outer, inner_st) = body.as_ref() else {
        panic!("outer □ body is an Implies, got {body:?}");
    };
    let Formula::Atom { relation, args } = acc_outer.as_ref() else {
        panic!("outer guard is an atom, got {acc_outer:?}");
    };
    assert_eq!(*relation, Term::Iri(EPISTEMICALLY_POSSIBLE.to_owned()));
    assert_eq!(
        args,
        &vec![
            Term::Iri(ACTUAL_WORLD.to_owned()),
            Term::Var("__w0".to_owned())
        ],
        "the outer guard runs from the actual world to __w0"
    );

    let Formula::Exists { vars, body } = inner_st.as_ref() else {
        panic!("inner ◇ is an Exists at depth 1, got {inner_st:?}");
    };
    assert_eq!(vars, &vec!["__w1".to_owned()], "the inner ◇ binds __w1");
    let Formula::And(parts) = body.as_ref() else {
        panic!("inner ◇ body is an And, got {body:?}");
    };
    assert_eq!(parts.len(), 2);
    let Formula::Atom { relation, args } = &parts[0] else {
        panic!("inner guard is an atom, got {:?}", parts[0]);
    };
    assert_eq!(*relation, Term::Iri(DOXASTICALLY_ACCESSIBLE.to_owned()));
    assert_eq!(
        args[0],
        Term::Var("__w0".to_owned()),
        "the inner accessibility atom's source world is the OUTER bound world __w0, not the actual world"
    );
    assert_eq!(args[1], Term::Var("__w1".to_owned()));
    let Formula::Atom { relation, args } = &parts[1] else {
        panic!("inner head is an atom, got {:?}", parts[1]);
    };
    assert_eq!(*relation, Term::Iri(format!("{ex}P")));
    assert_eq!(args.len(), 2);
    assert_eq!(
        args[0],
        Term::Var("__w1".to_owned()),
        "the head is relativized to the innermost world __w1"
    );
    assert_eq!(args[1], Term::Iri(format!("{ex}a")));
}

#[test]
#[allow(non_snake_case)] // `accessibleFrom` names the rejected logic: property verbatim.
fn modal_over_accessibleFrom_is_hard_error() {
    // logic:accessibleFrom is the bare superproperty; the standard translation must be taken
    // over a single TYPED accessibility relation, never the blurred union, so pinning it is a
    // hard error, not a value to translate over.
    let (prog, diags) = parse(
        "ex:nec a logic:Formula ;
            logic:necessarily [ a logic:Formula ; logic:relation ex:P ;
                logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termIri ex:a ] ] ;
            logic:overAccessibility logic:accessibleFrom .",
    );
    assert!(
        prog.formulas.is_empty(),
        "a modal node pinning the bare superproperty must never enter the IR: {:?}",
        prog.formulas
    );
    assert!(
        diags.iter().any(|d| {
            d.code == "MALFORMED_FORMULA"
                && d.severity == Severity::Error
                && d.message.contains("accessibleFrom")
        }),
        "expected an error-grade MALFORMED_FORMULA rejecting logic:accessibleFrom: {diags:?}"
    );
}
