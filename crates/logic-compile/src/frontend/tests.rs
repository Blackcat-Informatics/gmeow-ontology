// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the front-end parser, driven by Turtle source strings. These are
//! the authoritative parser tests; they supersede the Python
//! `tests/test_logic_frontend.py`, which was retired.

use super::shape_derivation_tests::shape_dataset;
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

/// The selected surface retains literal facets and flags, matching full compilation.
#[test]
fn annotation_lift_preserves_selected_surface_and_full_compiler_parity() {
    let source = r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
g:Kept rdfs:label "name"@x-gmeow-english ;
    rdfs:comment "quoted \"value\"\n中"@x-gmeow-english--rtl ;
    skos:definition "definition"@x-gmeow-english ;
    skos:prefLabel "preferred"@x-gmeow-english ;
    skos:altLabel "alternative"@x-gmeow-english ;
    skos:scopeNote "scope"@x-gmeow-english ;
    rdfs:label "English"@en, "typed"^^xsd:string ;
    skos:example "not a selected predicate"@x-gmeow-english ;
    skos:definition g:NotALiteral .
_:interior rdfs:label "blank subject"@x-gmeow-english .
<https://example.org/foreign> rdfs:label "foreign subject"@x-gmeow-english .
g:graph { g:Kept rdfs:label "named graph"@x-gmeow-english . }
"#;
    let dataset = parse_dataset(source.as_bytes(), "application/trig", None)
        .expect("small annotation source");
    let (annotations, diagnostics) = lift_annotation_axioms(&dataset).expect("annotation lift");
    assert_eq!(annotations.len(), 6);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let predicates: BTreeSet<_> = annotations
        .iter()
        .map(|axiom| axiom.predicate.as_str())
        .collect();
    assert_eq!(predicates, ANNOTATION_LIFT_PREDS.iter().copied().collect());
    for axiom in &annotations {
        assert_eq!(axiom.subject, "https://blackcatinformatics.ca/gmeow/Kept");
        assert_eq!(axiom.node_kind, NodeKind::Annotation);
        assert_eq!(
            axiom.load_bearing,
            matches!(
                axiom.predicate.as_str(),
                "http://www.w3.org/2000/01/rdf-schema#comment"
                    | "http://www.w3.org/2004/02/skos/core#definition"
            )
        );
        assert_eq!(axiom.scope, ContextualScope::default());
        assert!(!axiom.negated);
        let literal = axiom.obj.as_literal().expect("literal annotation");
        assert_eq!(literal.language.as_deref(), Some(X_GMEOW_ENGLISH_TAG));
        assert!(literal.datatype.is_none(), "implied language datatype");
    }
    let comment = annotations
        .iter()
        .find(|axiom| axiom.predicate.ends_with("#comment"))
        .expect("retained comment")
        .obj
        .as_literal()
        .expect("literal comment");
    assert_eq!(comment.lexical_form, "quoted \"value\"\n中");
    assert_eq!(comment.direction, Some(purrdf::RdfTextDirection::Rtl));
    assert_eq!(
        comment.datatype_iri(),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"
    );
    let keys: Vec<_> = annotations.iter().map(LogicAxiom::sort_key).collect();
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));

    let (program, full_diagnostics) =
        parse_logic_dataset(&dataset, None).expect("small full compilation");
    let compiled: Vec<_> = program
        .axioms
        .into_iter()
        .filter(|axiom| axiom.node_kind == NodeKind::Annotation)
        .collect();
    assert_eq!(annotations, compiled);
    let annotation_diagnostics: Vec<_> = full_diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "MALFORMED_ANNOTATION")
        .collect();
    assert_eq!(diagnostics, annotation_diagnostics);
}

/// Unrelated structural growth and blank labels cannot alter named annotations.
#[test]
fn annotation_lift_is_independent_of_structural_blank_identity() {
    let annotation = "<https://blackcatinformatics.ca/gmeow/Term> \
        <http://www.w3.org/2004/02/skos/core#definition> \"retained\"@x-gmeow-english .";
    let original = format!("{annotation}\n_:left <urn:edge> _:right .");
    let changed = format!(
        "_:renamedRight <urn:edge> _:extra .\n{annotation}\n\
         _:renamedLeft <urn:edge> _:renamedRight .\n_:extra <urn:edge> _:renamedLeft ."
    );
    let original = parse_dataset(original.as_bytes(), "text/turtle", None).expect("small source");
    let changed =
        parse_dataset(changed.as_bytes(), "text/turtle", None).expect("structural growth");
    assert_eq!(
        lift_annotation_axioms(&original).expect("original annotations"),
        lift_annotation_axioms(&changed).expect("unchanged annotations")
    );
}

/// An empty source is refused, while a nonempty source may select no annotations.
#[test]
fn annotation_lift_distinguishes_empty_source_from_empty_selection() {
    let empty = purrdf::RdfDatasetBuilder::new()
        .freeze()
        .expect("empty dataset");
    let error = lift_annotation_axioms(&empty).expect_err("empty source must fail");
    assert!(
        error.to_string().contains("Source graph is empty"),
        "{error}"
    );
    let unrelated = parse_dataset(b"<urn:s> <urn:p> <urn:o> .", "text/turtle", None)
        .expect("nonempty unrelated source");
    let (annotations, diagnostics) = lift_annotation_axioms(&unrelated).expect("empty selection");
    assert!(annotations.is_empty());
    assert!(diagnostics.is_empty());
}

#[test]
fn native_structural_typing_extracts_rules_contracts_and_constraints_without_aliasing_domain_types()
{
    for typing in ["a", "logic:instanceOf"] {
        let source = format!(
            r#"_:rule5bae99cdS0_x a logic:TermCarrier ; logic:termVariable "x" .
ex:r {typing} logic:Rule ; logic:head [ rdf:reifies <<( _:rule5bae99cdS0_x ex:q ex:b )>> ] ; logic:body [ rdf:reifies <<( _:rule5bae99cdS0_x ex:p ex:a )>> ] .
ex:contract {typing} logic:ReasoningContract ; logic:modelSemantics logic:StableModelSemantics ; logic:negationOperator logic:DefaultNegation .
logic:StableModelSemantics {typing} logic:ModelSemantics .
logic:DefaultNegation {typing} logic:NegationOperator .
ex:range {typing} logic:ValueRangeConstraint ; logic:onClass ex:Probability ; logic:valuePath ex:magnitude ; logic:minInclusiveBound 0 ; logic:maxInclusiveBound 1 ; logic:formalizes ex:Probability .
ex:subject logic:instanceOf ex:DomainType ."#
        );
        let (program, diagnostics) = parse(&source);
        assert!(diagnostics.is_empty(), "{typing}: {diagnostics:?}");
        assert_eq!(program.rules.len(), 1, "{typing}");
        assert_eq!(program.contracts.len(), 1, "{typing}");
        assert_eq!(program.constraints.len(), 1, "{typing}");
        assert!(program.axioms.iter().all(
            |axiom| !axiom.subject.ends_with("/contract") && !axiom.subject.ends_with("/range")
        ));
        let domain: Vec<_> = program
            .axioms
            .iter()
            .filter(|axiom| axiom.subject.ends_with("/subject"))
            .collect();
        assert_eq!(domain.len(), 1);
        assert_eq!(domain[0].predicate, logic_iri("instanceOf"));
    }
}

#[test]
fn dual_typed_orphan_recovery_is_rejected_once_and_never_becomes_a_domain_axiom() {
    for typing in [
        "a logic:RecoveryCase",
        "logic:instanceOf logic:RecoveryCase",
        "a logic:RecoveryCase ; logic:instanceOf logic:RecoveryCase",
    ] {
        let (program, diagnostics) = parse(&format!("ex:orphan {typing} ."));
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "ORPHAN_RECOVERY_CASE"
                    && diagnostic.severity == Severity::Error)
                .count(),
            1,
            "{typing}: {diagnostics:?}"
        );
        assert!(
            program
                .axioms
                .iter()
                .all(|axiom| !axiom.subject.ends_with("/orphan"))
        );
    }
}

#[test]
fn native_member_condition_stays_owned_and_exposes_its_evaluation_boundary() {
    let (program, diagnostics) = parse(
        "ex:set <https://blackcatinformatics.ca/math/memberCondition> ex:condition .
         ex:condition logic:instanceOf logic:Formula ;
             logic:relation ex:inRing ; logic:argument
             [ logic:termIndex 0 ; logic:termVariable \"x\" ],
             [ logic:termIndex 1 ; logic:termIri ex:ring ] .",
    );
    assert!(program.formulas.is_empty());
    assert!(
        program
            .axioms
            .iter()
            .all(|axiom| !axiom.subject.starts_with('?') && !axiom.subject.ends_with("/condition"))
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
    );
    let boundary = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "OWNER_SCOPED_MEMBER_CONDITION")
        .expect("owned definition cannot disappear from coverage");
    assert_eq!(
        boundary.subject.as_deref(),
        Some("https://example.org/test/condition")
    );
    assert!(boundary.message.contains("https://example.org/test/set"));
}

#[test]
fn dual_typing_does_not_duplicate_rules_or_promote_rdf_reification() {
    let (program, diagnostics) = parse(
        r#"_:rule4d484603S0_x a logic:TermCarrier ; logic:termVariable "x" .
ex:r a logic:Rule ; logic:instanceOf logic:Rule ;
             logic:head [ rdf:reifies <<( _:rule4d484603S0_x ex:q ex:b )>> ] ;
             logic:body [ rdf:reifies <<( _:rule4d484603S0_x ex:p ex:a )>> ] .
         ex:statement logic:instanceOf rdf:Statement ;
             rdf:subject ex:s ; rdf:predicate logic:subClassOf ; rdf:object ex:C ."#,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(program.rules.len(), 1);
    assert!(program.axioms.iter().all(
        |axiom| !(axiom.subject.ends_with("/s") && axiom.predicate == logic_iri("subClassOf"))
    ));
}

/// Sugar owners reject missing structure and malformed mode literals without emitting weaker constraints.
#[test]
fn every_constraint_sugar_reports_malformed_native_typed_records() {
    for class in [
        "ChoiceGroupConstraint",
        "GuardedImplicationConstraint",
        "DisjunctiveRequirednessConstraint",
        "PathValueTypeConstraint",
        "CrossNodeConstraint",
        "ForbiddenPatternConstraint",
        "ValueRangeConstraint",
        "AggregateConstraint",
        "JoinAggregateConstraint",
        "AggregateBalanceConstraint",
        "ComparisonConstraint",
        "PathNodeKindConstraint",
        "SelfJoinUniquenessConstraint",
        "InverseExistenceConstraint",
        "TransitiveReachabilityConstraint",
        "AcyclicConstraint",
        "ValueSetMembershipConstraint",
        "StringPatternConstraint",
    ] {
        let (_, expected) = parse(&format!("ex:c a logic:{class} ."));
        let (native, actual) = parse(&format!("ex:c logic:instanceOf logic:{class} ."));
        assert!(
            !expected.is_empty(),
            "{class} must reject its absent required fields"
        );
        assert_eq!(
            actual, expected,
            "{class} must retain every admission diagnostic"
        );
        assert!(
            native
                .axioms
                .iter()
                .all(|axiom| !axiom.subject.ends_with("/c"))
        );
    }
    for (class, predicate, fields, code) in [
        (
            "ChoiceGroupConstraint",
            "choiceMode",
            "logic:choicePredicate ex:a, ex:b ;",
            "exactly-one",
        ),
        (
            "CrossNodeConstraint",
            "crossMode",
            "logic:roleA ex:a ; logic:roleB ex:b ;",
            "co-occur",
        ),
    ] {
        for object in [
            None,
            Some("ex:CODE"),
            Some("_:CODE"),
            Some("\"CODE\"^^xsd:normalizedString"),
            Some("\"CODE\"@en"),
            Some("\"CODE\"@en--ltr"),
            Some("\" CODE\"^^xsd:token"),
            Some("\"CODE \"^^xsd:token"),
            Some("\"\\tCODE\"^^xsd:token"),
            Some("\"CODE\\uFFFF\"^^xsd:token"),
            Some("\"unknown\""),
            Some("\"CODE\", \"CODE\"^^xsd:token"),
            Some("\"CODE\"^^xsd:token, \"CODE\""),
            Some("\"CODE\", \"unknown\""),
        ] {
            let selector = object.map_or_else(String::new, |object| {
                format!("logic:{predicate} {} ;", object.replace("CODE", code))
            });
            let (program, diagnostics) = parse(&format!(
                "ex:c a logic:{class} ; logic:onClass ex:Widget ;
                 {fields} {selector} logic:formalizes ex:Widget ."
            ));
            assert!(program.constraints.is_empty(), "{predicate}: {object:?}");
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == "MALFORMED_CONSTRAINT"
                        && diagnostic.subject.as_deref() == Some("https://example.org/test/c")
                        && diagnostic.message.contains(predicate)
                }),
                "{predicate}: {object:?}: {diagnostics:?}"
            );
        }
    }
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
    assert!(err.message().contains("empty"), "got: {}", err.message());
    assert!(err.is::<crate::error::Frontend>());
    assert_eq!(
        gmeow_errors::code::code_str(err.code()),
        "logic-compile.frontend"
    );
}

#[test]
fn parse_nonexistent_file_raises() {
    let path = Path::new("/no/such/file-xyz.ttl");
    let err = parse_logic_path(path, None).unwrap_err();
    assert_eq!(
        gmeow_errors::code::code_str(err.code()),
        "logic-compile.source-read"
    );
    let failure = err.downcast_ref::<crate::error::SourceRead>().unwrap();
    assert_eq!(
        err.inner().failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/logic/CompilerSourceRefusal")
    );
    assert_eq!(failure.path, path);
    assert_eq!(failure.cause.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn parse_invalid_turtle_raises() {
    let err = parse_logic_str("this is not turtle <<<", None).unwrap_err();
    assert!(err.message().contains("Failed to parse"));
    assert_eq!(
        gmeow_errors::code::code_str(err.code()),
        "logic-compile.source-parse"
    );
    let failure = err.downcast_ref::<crate::error::SourceParse>().unwrap();
    assert_eq!(
        err.inner().failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/logic/CompilerSourceRefusal")
    );
    assert_eq!(failure.format, "Turtle");
    assert!(!failure.cause.to_string().is_empty());
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
            a.subject.ends_with("/a")
                && a.predicate.ends_with("#type")
                && a.obj.as_iri().is_some_and(|iri| iri.ends_with("/B"))
        }),
        "the reified ground atom must be routed to LogicProgram.axioms, got {:?}",
        prog.axioms
    );
    assert!(
        !prog.axioms.iter().any(|a| {
            a.subject.ends_with("/phi")
                && a.predicate == RDF_TYPE
                && a.obj.as_iri() == Some(logic_iri("Formula").as_str())
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
            axiom.obj.as_iri() != Some(logic_iri("TermCarrier").as_str())
                && axiom.obj.as_iri() != Some(logic_iri("RecoveryCase").as_str())
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
                && diagnostic.message.contains("non-IRI")
                && diagnostic.message.contains(&logic_iri("recoveryCase"))
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
    // Greenfield: an unrecognised preset reference is a hard error,
    // not a fail-soft warning — otherwise it is a silent approximation.
    let (_, diags) = parse("ex:Bogus a logic:ReasoningPreset .");
    assert!(
        diags
            .iter()
            .any(|d| d.code == "UNKNOWN_PROFILE" && d.severity == Severity::Error)
    );
}

// ── Compatibility firewall ─────────────────────────────────────────────

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
fn probabilistic_measure_declaration_without_model_preserves_configuration() {
    let (program, diagnostics) = parse(
        "ex:ProbContract a logic:ReasoningContract ;
            logic:uncertaintyMeasure logic:ProbabilisticMeasure .
         logic:ProbabilisticMeasure a logic:UncertaintyMeasure .",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(program.contracts.len(), 1);
    assert!(
        program.contracts[0]
            .uncertainty_measures
            .contains("ProbabilisticMeasure")
    );
}

#[test]
fn probabilistic_preset_declaration_does_not_request_inference() {
    for typing in ["a", "logic:instanceOf"] {
        let (program, diagnostics) = parse(&format!(
            "logic:ProbabilisticProfile {typing} logic:ReasoningPreset ;
                logic:expandsToFacet logic:ProbabilisticMeasure .
             logic:ProbabilisticMeasure {typing} logic:UncertaintyMeasure ."
        ));
        assert!(diagnostics.is_empty(), "{typing}: {diagnostics:?}");
        assert_eq!(program.contracts.len(), 1);
        assert_eq!(
            program.contracts[0].preset,
            Some(crate::ir::SemanticProfileId::Probabilistic)
        );
        assert!(
            program.contracts[0]
                .uncertainty_measures
                .contains("ProbabilisticMeasure")
        );
    }
}

#[test]
fn redundant_contract_typing_does_not_turn_a_preset_declaration_into_execution() {
    for typing in ["a", "logic:instanceOf"] {
        let (program, diagnostics) = parse(&format!(
            "logic:ProbabilisticProfile {typing} logic:ReasoningPreset, logic:ReasoningContract ;
                logic:expandsToFacet logic:ProbabilisticMeasure .
             logic:ProbabilisticMeasure {typing} logic:UncertaintyMeasure ."
        ));
        assert!(diagnostics.is_empty(), "{typing}: {diagnostics:?}");
        assert_eq!(program.contracts.len(), 1);
        assert_eq!(
            program.contracts[0].preset,
            Some(crate::ir::SemanticProfileId::Probabilistic)
        );
    }
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
    // Both source spellings admit the declared contract. This structural check
    // does not select a model for, or execute, a probabilistic query.
    for typing in ["a", "logic:instanceOf"] {
        let (program, diagnostics) = parse(&format!(
            "ex:ProbContract {typing} logic:ReasoningContract ;
                logic:uncertaintyMeasure logic:ProbabilisticMeasure .
             logic:ProbabilisticMeasure {typing} logic:UncertaintyMeasure .
             ex:myModel {typing} logic:ProbabilityModel ."
        ));
        assert!(diagnostics.is_empty(), "{typing}: {diagnostics:?}");
        assert_eq!(program.contracts.len(), 1, "{typing}");
        assert!(
            program.contracts[0]
                .uncertainty_measures
                .contains("ProbabilisticMeasure"),
            "{typing}: the selected measure must survive compilation"
        );
    }
}

#[test]
fn unselected_probability_model_does_not_change_a_contract_declaration() {
    for typing in ["a", "logic:instanceOf"] {
        let source = format!(
            "{PREFIXES}
             ex:ProbContract {typing} logic:ReasoningContract ;
                 logic:uncertaintyMeasure logic:ProbabilisticMeasure .
             logic:ProbabilisticMeasure {typing} logic:UncertaintyMeasure .
             GRAPH ex:unselected {{ ex:myModel {typing} logic:ProbabilityModel . }}"
        );
        let dataset = purrdf::parse_dataset(source.as_bytes(), "application/trig", None)
            .expect("synthetic scoped contract source");
        let (program, diagnostics) =
            parse_logic_dataset(&dataset, None).expect("compile the synthetic source");
        assert!(diagnostics.is_empty(), "{typing}: {diagnostics:?}");
        let (without_model, without_diagnostics) = parse(&format!(
            "ex:ProbContract {typing} logic:ReasoningContract ;
                logic:uncertaintyMeasure logic:ProbabilisticMeasure .
             logic:ProbabilisticMeasure {typing} logic:UncertaintyMeasure ."
        ));
        assert!(without_diagnostics.is_empty());
        assert_eq!(program.contracts, without_model.contracts);
    }
}

// ── Meta-config does not leak into domain axioms ───────────────────────

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

/// INVARIANT: a term that is configuration in one role is not skipped where it appears in
/// another role. A closure entry is configuration only for the closure-entry predicates and a
/// contract only for the facet predicates, so a facet-predicate row whose subject is merely
/// a closure entry stays a domain axiom, while the same row on the contract still does not leak.
#[test]
fn contract_config_roles_do_not_collide_across_closure_entries() {
    let (prog, _) = parse(
        "ex:Contract a logic:ReasoningContract ;
            logic:defaultClosure logic:OpenWorldClosure ;
            logic:closureEntry ex:Entry .
         ex:Entry a logic:ClosureEntry ;
            logic:closureKey ex:pred ;
            logic:closureValue logic:ClosedWorldClosure ;
            logic:defaultClosure logic:OpenWorldClosure .
         logic:OpenWorldClosure a logic:ClosureValue .
         logic:ClosedWorldClosure a logic:ClosureValue .",
    );
    let default_closure_of = |suffix: &str| {
        prog.axioms
            .iter()
            .any(|a| a.subject.ends_with(suffix) && a.predicate.ends_with("defaultClosure"))
    };
    assert!(
        default_closure_of("/Entry"),
        "a facet-predicate row on a node that is only a closure entry is not contract \
         configuration; got: {:?}",
        prog.axioms
    );
    assert!(
        !default_closure_of("/Contract"),
        "the facet predicate on the contract itself must not leak; got: {:?}",
        prog.axioms
    );
}

/// INVARIANT: a closure entry owned by a preset is configuration even when it is untyped, so its
/// key and value rows do not leak into the domain axiom set.
#[test]
fn untyped_closure_entry_of_a_preset_does_not_leak() {
    let (prog, _) = parse(
        "ex:Preset a logic:ReasoningPreset ;
            logic:closureEntry ex:Entry .
         ex:Entry logic:closureKey ex:pred ;
            logic:closureValue logic:ClosedWorldClosure .
         logic:ClosedWorldClosure a logic:ClosureValue .",
    );
    assert!(
        !prog
            .axioms
            .iter()
            .any(|a| a.predicate.ends_with("closureKey") || a.predicate.ends_with("closureValue")),
        "an untyped closure entry of a preset must not leak; got: {:?}",
        prog.axioms
    );
}

// ── Malformed ClosureEntry hard-fail ───────────────────────────────────

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
    assert!(ax.obj.as_iri().is_some_and(|iri| iri.ends_with("/Animal")));
    assert!(!ax.obj.is_literal());
}

#[test]
fn parse_literal_object_sets_flag() {
    let (prog, _) = parse("ex:s logic:confidence \"0.9\"^^xsd:decimal .");
    let ax = prog
        .axioms
        .iter()
        .find(|a| a.predicate.ends_with("confidence"))
        .unwrap();
    assert!(ax.obj.is_literal());
    assert_eq!(ax.obj.as_literal().unwrap().lexical_form, "0.9");
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
                && a.obj.as_iri().is_some_and(|iri| iri.ends_with("/Organism"))
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
    let (program, diags) = parse(
        "ex:stmt a rdf:Statement ;
            rdf:subject ex:a ; rdf:predicate logic:subClassOf ; rdf:object ex:b ;
            logic:confidence \"2.5\"^^xsd:decimal ;
            logic:modality logic:epistemic .",
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "INVALID_CONFIDENCE" && d.severity == Severity::Error)
    );
    assert!(
        !program
            .axioms
            .iter()
            .any(|a| a.subject.ends_with("/a") && a.predicate == logic_iri("subClassOf"))
    );
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
        r#"_:ruledf798fb2S0_x a logic:TermCarrier ; logic:termVariable "x" .
ex:r1 a logic:Rule ;
            logic:head [ rdf:reifies <<( _:ruledf798fb2S0_x logic:isA ex:Animal )>> ] ;
            logic:body [ rdf:reifies <<( _:ruledf798fb2S0_x logic:isA ex:Bird )>> ] ."#,
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
        r#"_:rule02f107f4S0_x a logic:TermCarrier ; logic:termVariable "x" .
ex:r1 a logic:Rule ;
            logic:body [ rdf:reifies <<( _:rule02f107f4S0_x logic:isA ex:Bird )>> ] ."#,
    );
    assert!(prog.rules.is_empty());
    assert!(diags.iter().any(|d| d.code == "MISSING_RULE_HEAD"));
}

#[test]
fn negated_body_atom_yields_negated_axiom() {
    let (prog, _) = parse(
        r#"_:rulef14e645cS0_x a logic:TermCarrier ; logic:termVariable "x" .
ex:r1 a logic:Rule ;
            logic:head [ rdf:reifies <<( _:rulef14e645cS0_x logic:isA ex:Live )>> ] ;
            logic:body [ rdf:reifies <<( _:rulef14e645cS0_x logic:isA ex:Bird )>> ] ;
            logic:negatedBody [ rdf:reifies <<( _:rulef14e645cS0_x logic:isA ex:Dead )>> ] ."#,
    );
    assert_eq!(prog.rules.len(), 1);
    let rule = &prog.rules[0];
    let negated: Vec<_> = rule.body.iter().filter(|b| b.negated).collect();
    let positive: Vec<_> = rule.body.iter().filter(|b| !b.negated).collect();
    assert_eq!(negated.len(), 1);
    assert_eq!(positive.len(), 1);
    assert!(
        negated[0]
            .obj
            .as_iri()
            .is_some_and(|iri| iri.ends_with("/Dead"))
    );
}

#[test]
fn distinct_body_guard_requires_variables() {
    let (prog, diags) = parse(
        r#"_:rule9c8d4740S0_x a logic:TermCarrier ; logic:termVariable "x" .
_:rule9c8d4740S0_y a logic:TermCarrier ; logic:termVariable "y" .
ex:r1 a logic:Rule ;
            logic:head [ rdf:reifies <<( _:rule9c8d4740S0_x logic:rel _:rule9c8d4740S0_y )>> ] ;
            logic:body [ rdf:reifies <<( _:rule9c8d4740S0_x logic:rel _:rule9c8d4740S0_y )>> ] ;
            logic:distinctBody [ logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "x" ], [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "y" ] ] ."#,
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
        r#"_:rulebf1f628eS0_x a logic:TermCarrier ; logic:termVariable "x" .
_:rulebf1f628eS0_y a logic:TermCarrier ; logic:termVariable "y" .
ex:r1 a logic:Rule ;
            logic:head [ rdf:reifies <<( _:rulebf1f628eS0_x logic:rel _:rulebf1f628eS0_y )>> ] ;
            logic:distinctBody [ logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "x" ], [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:constant ] ] ."#,
    );
    assert!(prog.rules.is_empty());
    assert!(
        diags
            .iter()
            .any(|d| d.code == "MALFORMED_RULE_BODY" && d.severity == Severity::Error)
    );
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
    // A synthetic mix of ordinary typing, subsumption and a scoped assertion.
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
    // Three type facts, one global subsumption and one scoped subsumption.
    assert_eq!(prog.axioms.len(), 5, "axioms: {:#?}", prog.axioms);
    // The scoped reified axiom carries epistemic modality.
    assert!(has_axiom(
        &prog,
        "/Animal",
        "subClassOf",
        LogicModality::Epistemic
    ));
    // Scope coordinates belong to the reification envelope, never the domain theory.
    assert!(
        prog.axioms
            .iter()
            .all(|axiom| !axiom.subject.ends_with("/stmt1"))
    );
    let scoped = prog
        .axioms
        .iter()
        .find(|axiom| axiom.scope.modality == LogicModality::Epistemic)
        .unwrap();
    assert_eq!(scoped.scope.confidence, Some(0.9));
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

/// Optional depth names retain exact spelling and reject malformed supplied literals as whole shapes.
#[test]
fn parse_wildcard_namespace_scoped_bounded_path_shape() {
    let source = "ex:nearbyOrgs a logic:PathShape ;
            logic:pathWildcard true ;
            logic:pathNamespaceScope \"https://example.org/org/\"^^xsd:anyURI ;
            logic:pathMinDepth 1 ;
            logic:pathMaxDepth 2 ;
            logic:pathDepthParam \"maxDepth\" .";
    let (prog, diags) = parse(source);
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
    assert_eq!(prog.path_shapes.len(), 1);
    let path_key = LogicProgram::new(vec![], vec![], vec![], None)
        .with_path_shapes(prog.path_shapes.clone())
        .canonical_key();
    for parameter in ["\"maxDepth\"^^xsd:token", "\"maxDepth\", \"maxDepth\""] {
        let (typed, diagnostics) = parse(&source.replace("\"maxDepth\"", parameter));
        assert_eq!(typed.path_shapes, prog.path_shapes, "{diagnostics:?}");
        assert_eq!(
            LogicProgram::new(vec![], vec![], vec![], None)
                .with_path_shapes(typed.path_shapes)
                .canonical_key(),
            path_key
        );
    }
    let absent = source.replace(
        "logic:pathMaxDepth 2 ;\n            logic:pathDepthParam \"maxDepth\"",
        "logic:pathMaxDepth 2",
    );
    let (absent, diagnostics) = parse(&absent);
    assert_eq!(absent.path_shapes.len(), 1, "{diagnostics:?}");
    assert_eq!(path_shape(&absent, "/nearbyOrgs").depth_param, None);
    assert_ne!(
        LogicProgram::new(vec![], vec![], vec![], None)
            .with_path_shapes(absent.path_shapes)
            .canonical_key(),
        path_key
    );
    for (literal, name) in [
        ("\" Max Depth \"", " Max Depth "),
        ("\"Max Depth\"^^xsd:token", "Max Depth"),
    ] {
        let (named, diagnostics) = parse(&source.replace("\"maxDepth\"", literal));
        assert_eq!(named.path_shapes.len(), 1, "{diagnostics:?}");
        assert_eq!(
            path_shape(&named, "/nearbyOrgs").depth_param.as_deref(),
            Some(name)
        );
        assert_ne!(
            LogicProgram::new(vec![], vec![], vec![], None)
                .with_path_shapes(named.path_shapes)
                .canonical_key(),
            path_key
        );
    }
    for parameter in [
        "ex:maxDepth",
        "_:maxDepth",
        "\"maxDepth\"^^xsd:normalizedString",
        "\"maxDepth\"@en",
        "\"maxDepth\"@en--ltr",
        "\" maxDepth\"^^xsd:token",
        "\"\\tmaxDepth\"^^xsd:token",
        "\"maxDepth\\uFFFF\"^^xsd:token",
        "\"\"",
        "\"  \"",
        "\"\"^^xsd:token",
        "\"maxDepth\", \"otherDepth\"",
        "\"maxDepth\", \"maxDepth\"^^xsd:token",
        "\"maxDepth\"^^xsd:token, \"maxDepth\"",
    ] {
        let (malformed, diagnostics) = parse(&source.replace("\"maxDepth\"", parameter));
        assert!(malformed.path_shapes.is_empty(), "{parameter}");
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "MALFORMED_PATH_SHAPE"
                    && diagnostic.subject.as_deref() == Some("https://example.org/test/nearbyOrgs")
                    && (diagnostic.message.contains("pathDepthParam")
                        || diagnostic.message.contains("depth_param"))
            }),
            "{parameter}: {diagnostics:?}"
        );
    }
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

/// Typed binder names preserve exact binding; malformed carriers refuse the owning quantifier.
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
    for datatype in ["string", "token"] {
        let (program, diagnostics) = parse(&format!(
            r#"ex:f1 a logic:Formula ; logic:forall ex:body1 ; logic:quantifiedVariable ex:qv1 .
            ex:qv1 logic:termIndex 0 ; logic:termVariable "?X:中"^^xsd:{datatype} .
            ex:body1 a logic:Formula ; logic:relation ex:p ; logic:argument [
                logic:termIndex 0 ; logic:termVariable "?X:中" ] ."#
        ));
        assert!(diagnostics.is_empty(), "{datatype}: {diagnostics:?}");
        let Formula::Forall { vars, body } = &program.formulas[0] else {
            panic!("expected quantifier");
        };
        assert_eq!(vars, &["?X:中"]);
        assert_eq!(
            **body,
            Formula::atom(
                Term::Iri("https://example.org/test/p".to_owned()),
                vec![Term::Var("?X:中".to_owned())]
            )
            .unwrap()
        );
    }

    for field in [
        r#"logic:termVariable "x"@en"#,
        r#"logic:termVariable "x"^^xsd:anyURI"#,
        r#"logic:termVariable " x"^^xsd:token"#,
        r#"logic:termVariable "x", "x"^^xsd:token"#,
        r#"logic:termSequenceMarker "x"^^xsd:token"#,
    ] {
        let (program, diagnostics) = parse(&format!(
            "ex:f1 a logic:Formula ; logic:forall ex:body1 ; logic:quantifiedVariable ex:qv1 . ex:qv1 logic:termIndex 0 ; {field} . ex:body1 a logic:Formula ; logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ."
        ));
        assert!(program.formulas.is_empty(), "{field}");
        let malformed: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "MALFORMED_FORMULA")
            .collect();
        assert_eq!(malformed.len(), 1, "{field}: {diagnostics:?}");
        assert_eq!(
            malformed[0].subject.as_deref(),
            Some("https://example.org/test/f1")
        );
        assert!(
            malformed[0]
                .message
                .contains("https://example.org/test/qv1"),
            "{field}: {malformed:?}"
        );
    }
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

/// Term carriers retain one complete native name and kind with unambiguous positional structure.
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
    for field in ["termVariable", "termSequenceMarker"] {
        for (value, name) in [
            (r#""x""#, "x"),
            (r#""x"^^xsd:string"#, "x"),
            (r#""x"^^xsd:token"#, "x"),
            (r#""?X:中""#, "?X:中"),
            (r#"" x ""#, " x "),
            (r#""X", "X""#, "X"),
        ] {
            let (program, diagnostics) = parse(&format!(
                "ex:f a logic:Formula ; logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:{field} {value} ] ."
            ));
            assert!(diagnostics.is_empty(), "{field} {value}: {diagnostics:?}");
            let expected = if field == "termVariable" {
                Term::Var(name.to_owned())
            } else {
                Term::SequenceMarker(name.to_owned())
            };
            assert_eq!(
                program.formulas,
                vec![
                    Formula::atom(
                        Term::Iri("https://example.org/test/p".to_owned()),
                        vec![expected]
                    )
                    .unwrap()
                ]
            );
        }
        for value in [
            "ex:x",
            "_:x",
            "<<( ex:s ex:p ex:o )>>",
            r#""x"@en"#,
            r#""x"@en--ltr"#,
            r#""x"^^xsd:anyURI"#,
            r#"" x"^^xsd:token"#,
            r#""x\uFFFF""#,
            r#""""#,
            r#"" ""#,
            r#"""^^xsd:token"#,
            r#""x", "y""#,
            r#""x", "x"^^xsd:token"#,
        ] {
            let (program, diagnostics) = parse(&format!(
                "ex:f a logic:Formula ; logic:relation ex:p ; logic:argument ex:carrier . ex:carrier logic:termIndex 0 ; logic:{field} {value} ."
            ));
            assert!(program.formulas.is_empty(), "{field} {value}");
            assert!(
                diagnostics.iter().any(|d| d.code == "MALFORMED_FORMULA"
                    && d.subject.as_deref() == Some("https://example.org/test/f")
                    && d.message.contains("https://example.org/test/carrier")),
                "{field} {value}: {diagnostics:?}"
            );
        }
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

/// Malformed severity remains independently attributed when an integrity formula also fails.
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
    for severity in [
        "ex:Violation",
        "_:Violation",
        "\"Violation\"^^xsd:normalizedString",
        "\"Violation\"@en",
        "\"Violation\"@en--ltr",
        "\" Violation\"^^xsd:token",
        "\"Violation \"",
        "\"violation\"",
        "\"Violation\", \"Info\"",
        "\"Violation\", \"Violation\"^^xsd:token",
        "\"Violation\"^^xsd:token, \"Violation\"",
    ] {
        let (program, diagnostics) = parse(&format!(
            "ex:c a logic:Constraint ; logic:integrity ex:integrity ;
                 logic:severity {severity} .
             ex:integrity a logic:Formula ; logic:not ex:badChild .
             ex:badChild a logic:Formula ."
        ));
        assert!(program.constraints.is_empty(), "{severity}");
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "MALFORMED_FORMULA")
                .count(),
            1,
            "{severity}: {diagnostics:?}"
        );
        assert!(
            diagnostics.iter().any(|d| {
                d.code == "MALFORMED_CONSTRAINT"
                    && d.subject.as_deref() == Some("https://example.org/test/c")
                    && d.message.contains("logic:severity")
            }),
            "{severity}: {diagnostics:?}"
        );
    }
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

/// Numeric datatype facets preserve their endpoint values and projected inclusivity.
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
    let projected = shapes
        .iter()
        .map(crate::projections::shapes::project_validation_shape_shacl)
        .collect::<String>();
    assert!(projected.contains("sh:minInclusive 0"), "{projected}");
    assert!(projected.contains("sh:maxExclusive 100"), "{projected}");
}

/// Unsupported supplied facets refuse derivation even when another component is preferred.
#[test]
fn supplied_datatype_range_bounds_cannot_disappear_from_derived_shapes() {
    for (predicate, bound, other) in [
        ("minInclusive", "\"PT0S\"^^xsd:duration", ""),
        (
            "maxInclusive",
            "\"PT30M\"^^xsd:duration",
            "xsd:minInclusive 0 ;",
        ),
        (
            "minExclusive",
            "\"-PT1S\"^^xsd:duration",
            "xsd:minInclusive 0 ;",
        ),
        (
            "maxExclusive",
            "\"broken\"^^xsd:decimal",
            "xsd:maxInclusive 1 ;",
        ),
        ("minInclusive", "g:notALiteral", ""),
        ("maxExclusive", "\"NaN\"^^xsd:double", ""),
        ("minInclusive", "\"0\"^^xsd:string", ""),
        ("minInclusive", "0, \"PT0S\"^^xsd:duration", ""),
    ] {
        let source = format!(
            "g:M a logic:Class ; logic:subClassOf \
             [ a logic:Restriction ; logic:onProperty g:value ; logic:allValuesFrom \
               [ a logic:Datarange ; logic:onDatatype xsd:decimal ; \
                 logic:withRestrictions ( g:facet ) ] ] . \
             g:facet {other} xsd:{predicate} {bound} ."
        );
        let error = derive_validation_shapes(shape_dataset(&source).as_ref())
            .expect_err("a supplied unsupported bound must refuse the whole derivation");
        let message = error.message();
        assert!(message.contains("/facet"), "{message}");
        assert!(message.contains(predicate), "{message}");
        assert!(message.contains("finite RDF numeric"), "{message}");
        assert!(message.contains("supplied"), "{message}");
    }
    let qualified_union = shape_dataset(
        "g:M a logic:Class ; logic:subClassOf \
         [ a logic:Restriction ; logic:onProperty g:value ; \
           logic:minQualifiedCardinality 1 ; logic:onDataRange \
           [ a logic:Datarange ; logic:unionOf ( xsd:decimal xsd:integer ) ; \
             logic:onDatatype xsd:decimal ; logic:withRestrictions ( g:facet ) ] ] . \
         g:facet xsd:minInclusive \"PT0S\"^^xsd:duration .",
    );
    let error = derive_validation_shapes(qualified_union.as_ref())
        .expect_err("a recognized qualifying union must not hide an unsupported facet");
    assert!(error.message().contains("/facet"), "{error}");
    assert!(error.message().contains("minInclusive"), "{error}");
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
                    ConstraintComponent::HasValue(ShapeValue::Literal(purrdf::RdfLiteral { lexical_form: lexical, .. }))
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

/// Unsupported bounds refuse both shape projection and every procedural extraction entry.
#[test]
fn supplied_value_range_bounds_refuse_shape_and_source_extraction_together() {
    for fields in [
        "logic:minInclusiveBound \"PT0S\"^^xsd:duration",
        "logic:minInclusiveBound 0 ; logic:maxInclusiveBound \"PT30M\"^^xsd:duration",
        "logic:minInclusiveBound 0 ; logic:minExclusiveBound \"-PT1S\"^^xsd:duration",
        "logic:maxInclusiveBound 1 ; logic:maxExclusiveBound \"broken\"^^xsd:decimal",
        "logic:minInclusiveBound g:notALiteral",
        "logic:maxExclusiveBound \"NaN\"^^xsd:double",
        "logic:minInclusiveBound \"0\"^^xsd:string",
        "logic:minInclusiveBound 0, \"PT0S\"^^xsd:duration",
    ] {
        let source = format!(
            "g:range a logic:ValueRangeConstraint ; logic:onClass g:M ; \
             logic:valuePath g:value ; logic:formalizes g:M ; {fields} ."
        );
        let dataset = shape_dataset(&source);
        let shape_error = derive_validation_shapes(dataset.as_ref())
            .expect_err("unsupported bounds must not leave weaker shapes");
        let extraction_error = extract_all_constraints(dataset.as_ref())
            .expect_err("mandatory extraction must not warn and skip the range");
        let parse_error = parse_logic_dataset(dataset.as_ref(), None)
            .expect_err("source compilation must refuse unsupported bounds");
        let prepared = PreparedLogicSource::new(dataset.as_ref()).expect("prepared source");
        let prepared_error = prepared
            .constraints()
            .expect_err("producer fixture extraction must propagate the same refusal");
        for error in [shape_error, extraction_error, parse_error, prepared_error] {
            let message = error.message();
            assert!(message.contains("/range"), "{message}");
            assert!(message.contains("Bound"), "{message}");
            assert!(message.contains("finite RDF numeric"), "{message}");
            assert!(message.contains("supplied"), "{message}");
        }
    }
}

/// Missing endpoints stay unbounded and admitted inclusive peers retain declared precedence.
#[test]
fn value_range_absent_endpoints_and_inclusive_precedence_remain_supported() {
    for (fields, present, absent) in [
        ("logic:minExclusiveBound 0", "sh:minExclusive 0", "sh:max"),
        ("logic:maxInclusiveBound 1", "sh:maxInclusive 1", "sh:min"),
        (
            "logic:minInclusiveBound 0 ; logic:minExclusiveBound 1",
            "sh:minInclusive 0",
            "sh:minExclusive",
        ),
    ] {
        let source = format!(
            "g:range a logic:ValueRangeConstraint ; logic:onClass g:M ; \
             logic:valuePath g:value ; logic:formalizes g:M ; {fields} ."
        );
        let dataset = shape_dataset(&source);
        let shapes = derive_validation_shapes(dataset.as_ref()).expect("supported numeric range");
        let projected = shapes
            .iter()
            .map(crate::projections::shapes::project_validation_shape_shacl)
            .collect::<String>();
        assert!(projected.contains(present), "{projected}");
        assert!(!projected.contains(absent), "{projected}");
        let (program, diagnostics) =
            parse_logic_dataset(dataset.as_ref(), None).expect("supported source range");
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(program.constraints.len(), 1);
    }
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

/// Aggregate selectors retain canonical meaning while malformed complete terms emit no record.
#[test]
fn join_aggregate_record_expands_and_contributes_nothing_to_the_reasoned_axiom_set() {
    // A well-formed two-leg join-aggregate record expands to exactly one canonical logic:Constraint
    // carrying the JoinAggregate satellite, targets the guarded class, and none of its structural
    // triples (joinPath / leg role predicates / the leg blank nodes) leak into prog.axioms.
    let join_source = "ex:boundarySquareZero a logic:JoinAggregateConstraint ;\n\
           logic:onClass ex:TopCell ;\n\
           logic:aggFunction \"SUM\" ;\n\
           logic:aggComparator \"=\" ;\n\
           logic:aggThreshold 0 ;\n\
           logic:joinPath (\n\
             [ logic:legRecordType ex:Incidence ; logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
             [ logic:legRecordType ex:Incidence ; logic:legSource ex:incidenceCoface ; logic:legTarget ex:incidenceFace ; logic:legValue ex:incidenceSign ]\n\
           ) ;\n\
           logic:formalizes ex:BoundaryOperator .";
    let (prog, diags) = parse(join_source);
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
    let ordinary_source = "ex:boundarySquareZero a logic:AggregateConstraint ;
        logic:onClass ex:TopCell ; logic:aggFunction \"SUM\" ;
        logic:aggPath ex:incidenceSign ; logic:aggComparator \"=\" ;
        logic:aggCompareTo 0 ; logic:formalizes ex:BoundaryOperator .";
    for source in [ordinary_source, join_source] {
        let (baseline, diagnostics) = parse(source);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let expected = &baseline.constraints[0];
        for (function, comparator) in [
            ("\"sum\"", "\" == \""),
            ("\"sUm\"^^xsd:string", "\"==\"^^xsd:string"),
            ("\"sum\"^^xsd:token", "\"=\"^^xsd:token"),
            ("\"SUM\", \"SUM\"", "\"=\", \"=\""),
        ] {
            let selected = source
                .replace(
                    "logic:aggFunction \"SUM\"",
                    &format!("logic:aggFunction {function}"),
                )
                .replace(
                    "logic:aggComparator \"=\"",
                    &format!("logic:aggComparator {comparator}"),
                );
            let (actual, diagnostics) = parse(&selected);
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            assert_eq!(actual.constraints.len(), 1);
            assert_eq!(actual.constraints[0].integrity, expected.integrity);
            assert_eq!(actual.constraints[0].content_key(), expected.content_key());
        }
        for (predicate, code, alias) in
            [("aggFunction", "SUM", "sum"), ("aggComparator", "=", "==")]
        {
            for object in [
                None,
                Some("ex:resource"),
                Some("_:selector"),
                Some("<<( ex:s ex:p ex:o )>>"),
                Some("\"CODE\"^^xsd:normalizedString"),
                Some("\"CODE\"@en"),
                Some("\"CODE\"@en--ltr"),
                Some("\" CODE\"^^xsd:token"),
                Some("\"\\tCODE\"^^xsd:token"),
                Some("\"CODE\\uFFFF\""),
                Some("\"CODE\", \"CODE\"^^xsd:token"),
                Some("\"CODE\", \"ALIAS\""),
                Some("\"unknown\""),
            ] {
                let selector = object.map_or_else(String::new, |object| {
                    format!(
                        "logic:{predicate} {} ;",
                        object.replace("CODE", code).replace("ALIAS", alias)
                    )
                });
                let selected =
                    source.replace(&format!("logic:{predicate} \"{code}\" ;"), &selector);
                let (actual, diagnostics) = parse(&selected);
                assert!(actual.constraints.is_empty(), "{predicate}: {object:?}");
                assert!(
                    diagnostics.iter().any(|diagnostic| {
                        diagnostic.code == "MALFORMED_CONSTRAINT"
                            && diagnostic.subject.as_deref()
                                == Some("https://example.org/test/boundarySquareZero")
                    }),
                    "{predicate}: {object:?}: {diagnostics:?}"
                );
            }
        }
        // Function grammar uppercases without trimming; comparator whitespace above remains valid.
        let (actual, diagnostics) = parse(&source.replace("\"SUM\"", "\" SUM \""));
        assert!(actual.constraints.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MALFORMED_CONSTRAINT")
        );
    }
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
/// on that leg — plus the whole second leg — is a well-formed IRI. Each
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
    // literal value must be rejected as malformed, not silently stringified.
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
    // is an IRI, and both must be rejected the same way.
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
            Term::literal("1".to_owned(), None).unwrap(),
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

// ── Reasoning programs (`logic:ReasoningProgram`) — R1 hard-fail ──────────────────────────

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
            a.obj.as_iri() != Some(logic_iri("ReasoningProgram").as_str())
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
    // □P(a) ↦ ∀ __w0 . R(actualWorld, __w0) → contextualApplication(__w0, P, a).
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
    assert_eq!(
        vars,
        &vec!["__w0".to_owned()],
        "box binds a fresh world var"
    );
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
    assert_eq!(
        *relation,
        Term::Iri(crate::ir::CONTEXTUAL_APPLICATION_IRI.to_owned())
    );
    assert_eq!(
        args.len(),
        3,
        "context and relation precede the original atom arguments"
    );
    assert_eq!(
        args[0],
        Term::Var("__w0".to_owned()),
        "the context coordinate is the bound world variable"
    );
    assert_eq!(
        args[1],
        Term::Iri(format!("{ex}P")),
        "the original relation has its own coordinate"
    );
    assert_eq!(args[2], Term::Iri(format!("{ex}a")));
}

#[test]
fn diamond_atom_expands_via_standard_translation() {
    use crate::ir::{Formula, Term};
    // ◇P(a) ↦ ∃ __w0 . R(actualWorld, __w0) ∧ contextualApplication(__w0, P, a).
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
        panic!(
            "first conjunct is the accessibility guard, got {:?}",
            parts[0]
        );
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
        panic!(
            "second conjunct is the relativized atom, got {:?}",
            parts[1]
        );
    };
    assert_eq!(
        *relation,
        Term::Iri(crate::ir::CONTEXTUAL_APPLICATION_IRI.to_owned())
    );
    assert_eq!(args.len(), 3);
    assert_eq!(args[0], Term::Var("__w0".to_owned()));
    assert_eq!(args[1], Term::Iri(format!("{ex}P")));
    assert_eq!(args[2], Term::Iri(format!("{ex}a")));
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
    assert_eq!(
        *relation,
        Term::Iri(crate::ir::CONTEXTUAL_APPLICATION_IRI.into())
    );
    assert_eq!(args.len(), 3);
    assert_eq!(
        args[0],
        Term::Var("__w1".to_owned()),
        "the head is relativized to the innermost world __w1"
    );
    assert_eq!(args[1], Term::Iri(format!("{ex}P")));
    assert_eq!(args[2], Term::Iri(format!("{ex}a")));
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

/// A broken conjunct or guard must not license the head by disappearing from its
/// antecedent. Every case keeps an independent valid rule to check local attribution.
#[test]
fn malformed_rule_parts_exclude_the_entire_rule() {
    let cases = [
        r#"logic:body []"#,
        r#"logic:body [ rdf:reifies "not a triple" ]"#,
        r#"logic:body [ rdf:reifies ex:notATriple ]"#,
        r#"logic:body [ rdf:reifies <<( ex:s ex:p ex:o )>>, <<( ex:s ex:q ex:o )>> ]"#,
        r#"logic:body [ rdf:reifies <<( ex:literalSubject ex:p ex:o )>> ]"#,
        r#"logic:body [ rdf:reifies <<( ex:emptyCarrier ex:p ex:o )>> ]"#,
        r#"logic:body [ rdf:reifies <<( ex:s ex:emptyCarrier ex:o )>> ]"#,
        r#"logic:body [ rdf:reifies <<( ex:s ex:p ex:emptyCarrier )>> ]"#,
        r#"logic:negatedBody "broken""#,
        r#"logic:body [ rdf:reifies <<( ex:s ex:p <<( ex:s ex:p ex:o )>> )>> ]"#,
        r#"logic:distinctBody [ logic:argument [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termIri ex:constant ], [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "y" ] ]"#,
        r#"logic:aggregateFunction <<( ex:s ex:p ex:o )>>; logic:aggregateVariable "?y"; logic:aggregateResult "?total""#,
        r#"logic:distinctBody "broken""#,
        r#"logic:distinctBody [ logic:argument [ logic:termIndex 0; logic:termVariable "x" ], [ logic:termIndex 0; logic:termVariable "y" ] ]"#,
        r#"logic:aggregateFunction "SUM""#,
        r#"logic:groupKey "?x""#,
        r#"logic:aggregateFunction "SUM", "COUNT"; logic:aggregateVariable "?y"; logic:aggregateResult "?total""#,
        r#"logic:head [ rdf:reifies <<( _:rulede2f30d5S0_x ex:other _:rulede2f30d5S0_y )>> ]"#,
    ];
    for broken in cases {
        let (program, diagnostics) = parse(&format!(
            r#"ex:literalSubject a logic:TermCarrier ; logic:termLiteral "subject" .
ex:emptyCarrier a logic:TermCarrier .
_:rulede2f30d5S0_x a logic:TermCarrier ; logic:termVariable "x" .
_:rulede2f30d5S0_y a logic:TermCarrier ; logic:termVariable "y" .
ex:broken logic:instanceOf logic:Rule;
                logic:head [ rdf:reifies <<( _:rulede2f30d5S0_x ex:forbidden _:rulede2f30d5S0_y )>> ];
                {broken} .
               _:rulede2f30d5S1_x a logic:TermCarrier ; logic:termVariable "x" .
_:rulede2f30d5S1_y a logic:TermCarrier ; logic:termVariable "y" .
ex:valid logic:instanceOf logic:Rule;
                logic:head [ rdf:reifies <<( _:rulede2f30d5S1_x ex:allowed _:rulede2f30d5S1_y )>> ];
                logic:body [ rdf:reifies <<( _:rulede2f30d5S1_x ex:given _:rulede2f30d5S1_y )>> ] ."#
        ));
        assert_eq!(program.rules.len(), 1, "{broken}: {diagnostics:?}");
        assert!(program.rules[0].head.predicate.ends_with("/allowed"));
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error
                    && diagnostic
                        .subject
                        .as_deref()
                        .is_some_and(|subject| subject.ends_with("/broken"))),
            "missing attributed error for {broken}: {diagnostics:?}"
        );
    }
}

#[test]
fn rule_source_declaration_has_one_semantic_home_on_round_trip() {
    let (program, diagnostics) = parse(
        r#"
        _:rulee9905b70S0_x a logic:TermCarrier ; logic:termVariable "x" .
_:rulee9905b70S0_y a logic:TermCarrier ; logic:termVariable "y" .
ex:r a logic:Rule; logic:provenance ex:r;
            logic:head [ rdf:reifies <<( _:rulee9905b70S0_x ex:derived _:rulee9905b70S0_y )>> ];
            logic:body [ rdf:reifies <<( _:rulee9905b70S0_x ex:given _:rulee9905b70S0_y )>> ] .
    "#,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(
        program.axioms.is_empty(),
        "rule structure is not a domain assertion"
    );
    let clif = crate::clif::writer::project_clif(&program).expect("CLIF projection");
    let (restored, diagnostics) =
        crate::clif::parse_clif_str(&clif.content, None).expect("CLIF parse");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(restored.rules, program.rules);
    assert!(restored.axioms.is_empty());
}
