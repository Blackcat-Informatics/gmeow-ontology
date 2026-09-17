// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Verdict consumers interpret canonical operators without rewriting evidence.

use super::*;
use purrdf::TermValue;

/// Inspect every recorded source admission in these synthetic verdict controls.
fn source_admission_complete(world: &SourceCoverageWorld) -> bool {
    world
        .admissions
        .iter()
        .map(|admission| admission.selectors.len())
        .sum::<usize>()
        == world.constructs.len()
        && world.admissions.iter().all(|admission| {
            admission.completion == NativeFamilyCompletion::Complete
                && admission.obstructions.is_empty()
        })
}

/// Project DL diagnostics from the same native execution that produced `inferred`.
/// No source scan, refuter dispatch, second closure or artificial RDF status row
/// occurs here. Every retained incomplete obligation independently produces a gap.
///
/// # Errors
/// Refuses structurally invalid or differently scoped retained evidence.
fn verdict_from_native(
    inferred: &[InferredAxiom],
    execution: &crate::result::NativeExecutionEvidence,
) -> gmeow_errors::Result<DlVerdict> {
    execution.validate_structure()?;
    if inferred
        .iter()
        .any(|row| !execution.worlds.contains_key(&row.world))
    {
        return Err(invalid(
            "native verdict row belongs to an unselected execution world",
        ));
    }
    Ok(verdict_from_validated_native(inferred, execution))
}

const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const SUBCLASS: &str = "https://blackcatinformatics.ca/logic/subClassOf";
const NOTHING: &str = "https://blackcatinformatics.ca/logic/Nothing";

fn axiom(subject: &str, predicate: &str, object: TermValue) -> InferredAxiom {
    InferredAxiom {
        modal_evaluation: None,
        subject: subject.to_owned(),
        predicate: predicate.to_owned(),
        object,
        world: "urn:verdict:selected-world".to_owned(),
        is_edb: false,
        rule_name: Some("urn:verdict:source-law".to_owned()),
        premises: vec![(
            "urn:verdict:source".to_owned(),
            "urn:verdict:source-predicate".to_owned(),
            format!("<{NOTHING}>"),
        )],
    }
}

struct LocalVerdict {
    consistent: bool,
    unsatisfiable_classes: Vec<UnsatClass>,
    inconsistencies: Vec<InconsistencyWitness>,
}

fn verdict(rows: &[InferredAxiom]) -> LocalVerdict {
    let (unsatisfiable_classes, inconsistencies) = local_verdict(rows);
    LocalVerdict {
        consistent: inconsistencies.is_empty(),
        unsatisfiable_classes,
        inconsistencies,
    }
}

#[test]
fn canonical_membership_reports_a_clash_with_exact_world_and_source_premises() {
    for predicate in [INSTANCE, RDF_TYPE] {
        for nothing in [NOTHING, OWL_NOTHING] {
            let row = axiom("urn:verdict:individual", predicate, TermValue::iri(nothing));
            let result = verdict(std::slice::from_ref(&row));
            assert!(!result.consistent);
            assert_eq!(
                result.inconsistencies,
                vec![InconsistencyWitness {
                    individual: row.subject.clone(),
                    world: row.world.clone(),
                    premises: row.premises.clone(),
                }]
            );
            assert!(result.unsatisfiable_classes.is_empty());
        }
    }
}

#[test]
fn canonical_empty_classes_remain_distinct_from_populated_inconsistency() {
    for predicate in [SUBCLASS, RDFS_SUBCLASSOF] {
        for nothing in [NOTHING, OWL_NOTHING] {
            let row = axiom(
                "urn:verdict:empty-class",
                predicate,
                TermValue::iri(nothing),
            );
            let result = verdict(std::slice::from_ref(&row));
            assert!(result.consistent, "class emptiness never invents a member");
            assert!(result.inconsistencies.is_empty());
            assert_eq!(
                result.unsatisfiable_classes,
                vec![UnsatClass {
                    class: row.subject.clone(),
                    world: row.world.clone(),
                    premises: row.premises.clone(),
                }]
            );
            assert_eq!(
                unsatisfiable_from_inferred(&[row]),
                result.unsatisfiable_classes
            );
        }
    }
}

#[test]
fn neither_spelling_of_the_empty_class_is_reported_as_an_authored_unsatisfiable_class() {
    let rows: Vec<_> = [SUBCLASS, RDFS_SUBCLASSOF]
        .into_iter()
        .flat_map(|predicate| {
            [NOTHING, OWL_NOTHING].into_iter().flat_map(move |subject| {
                [NOTHING, OWL_NOTHING]
                    .into_iter()
                    .map(move |object| axiom(subject, predicate, TermValue::iri(object)))
            })
        })
        .collect();
    let result = verdict(&rows);
    assert!(result.consistent);
    assert!(result.unsatisfiable_classes.is_empty());
}

#[test]
fn empty_class_names_in_data_literals_or_quotations_do_not_become_clashes() {
    let quoted = TermValue::Triple {
        s: Box::new(TermValue::iri("urn:verdict:quoted-subject")),
        p: Box::new(TermValue::iri(INSTANCE)),
        o: Box::new(TermValue::iri(NOTHING)),
    };
    let literal = TermValue::Literal {
        lexical_form: NOTHING.to_owned(),
        datatype: "http://www.w3.org/2001/XMLSchema#string".to_owned(),
        language: None,
        direction: None,
    };
    let rows = [
        axiom(
            "urn:verdict:data",
            "urn:verdict:mentions",
            TermValue::iri(NOTHING),
        ),
        axiom(
            "urn:verdict:data",
            "urn:verdict:instanceOf",
            TermValue::iri(NOTHING),
        ),
        axiom("urn:verdict:data", INSTANCE, quoted),
        axiom("urn:verdict:data", SUBCLASS, literal),
        axiom(
            "urn:verdict:data",
            INSTANCE,
            TermValue::iri("urn:verdict:Nothing"),
        ),
        axiom(NOTHING, "urn:verdict:mentions", TermValue::iri(OWL_NOTHING)),
    ];
    let result = verdict(&rows);
    assert!(result.consistent);
    assert!(result.inconsistencies.is_empty());
    assert!(result.unsatisfiable_classes.is_empty());
}

fn native_execution(rows: Vec<purrdf::RdfQuad>) -> crate::result::NativeExecutionEvidence {
    native_result(rows)
        .native_execution()
        .expect("required retained native observation")
        .clone()
}

fn native_result(rows: Vec<purrdf::RdfQuad>) -> crate::result::ReasoningResult {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for row in rows {
        builder.push_owned_quad(&row);
    }
    let source = builder.freeze().expect("tiny native source");
    crate::reason::reason_all(
        crate::reason::prepare_reasoning_input(source.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("single native execution")
}

fn source_row(subject: &str, predicate: &str, object: &str, world: &str) -> purrdf::RdfQuad {
    purrdf::RdfQuad::new(
        purrdf::RdfTerm::iri(subject),
        predicate,
        purrdf::RdfTerm::iri(object),
    )
    .in_graph(purrdf::RdfTerm::iri(world))
}

#[test]
fn native_construct_selection_ignores_data_mentions_and_retains_exact_operator_source() {
    let complement = "https://blackcatinformatics.ca/logic/complementOf";
    let world = "urn:coverage:world";
    let execution = native_execution(vec![
        source_row("urn:coverage:C", complement, "urn:coverage:D", world),
        source_row(
            "urn:coverage:term",
            "urn:coverage:mentions",
            "http://www.w3.org/2002/07/owl#topObjectProperty",
            world,
        ),
        source_row(
            "urn:coverage:term",
            "urn:coverage:mentions",
            "http://www.w3.org/2002/07/owl#FunctionalProperty",
            world,
        ),
        source_row(
            "http://www.w3.org/2002/07/owl#oneOf",
            RDF_TYPE,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
            world,
        ),
    ]);
    execution
        .validate_structure()
        .expect("exact native source/proof framing");
    let selected = &execution.source_coverage.worlds[world].constructs;
    assert_eq!(
        selected.len(),
        1,
        "ordinary vocabulary mentions never become selected obligations"
    );
    assert_eq!(selected[0].family, DlConstructFamily::ComplementOf);
    assert_eq!(selected[0].statement.predicate, complement);
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == world)
        .unwrap();
    let source = ledger.source_leaves(&[selected[0].support]).unwrap();
    assert_eq!(source.len(), 1);
    let source = source.iter().next().unwrap();
    assert_eq!(source.predicate, complement);
    assert_eq!(source.graph, Some(TermValue::iri(world)));
    assert_eq!(source.object, TermValue::iri("urn:coverage:D"));
}

#[test]
fn vocabulary_in_literals_and_quoted_statements_does_not_select_constructs() {
    let marker = TermValue::iri("http://www.w3.org/2002/07/owl#FunctionalProperty");
    let quoted = TermValue::Triple {
        s: Box::new(TermValue::iri("urn:coverage:p")),
        p: Box::new(TermValue::iri(RDF_TYPE)),
        o: Box::new(marker.clone()),
    };
    for object in [quoted, TermValue::simple_literal(marker.as_iri().unwrap())] {
        assert!(
            construct_families(RDF_TYPE, &TermValue::iri("urn:coverage:source"), &object)
                .is_empty()
        );
    }
    assert_eq!(
        construct_families(RDF_TYPE, &TermValue::iri("urn:coverage:p"), &marker),
        vec![DlConstructFamily::FunctionalProperty]
    );
}

#[test]
fn one_complete_world_cannot_override_an_obstructed_sibling_construct() {
    let complement = "http://www.w3.org/2002/07/owl#complementOf";
    let mut execution = native_execution(vec![
        source_row(
            "urn:coverage:C",
            complement,
            "urn:coverage:D",
            "urn:coverage:a",
        ),
        source_row(
            "urn:coverage:C",
            complement,
            "urn:coverage:D",
            "urn:coverage:b",
        ),
    ]);
    let selected = &execution.source_coverage.worlds["urn:coverage:b"].constructs[0];
    let class = execution
        .classes
        .iter_mut()
        .find(|class| class.world == "urn:coverage:b")
        .unwrap();
    class.completion = NativeFamilyCompletion::Obstructed;
    class
        .obstructions
        .push(super::super::refute::NativeFamilyObstruction {
            kind: super::super::refute::NativeObstructionKind::UnsupportedCombination,
            detail: "selected synthetic model interaction".to_owned(),
            support: vec![selected.support],
        });
    let verdict = verdict_from_native(&[], &execution).unwrap();
    assert!(
        verdict.consistent,
        "capability refusal never fabricates a conflict"
    );
    assert_eq!(verdict.coverage.unsupported, vec!["complementOf"]);
    assert!(verdict.coverage.decided.is_empty());
    assert!(
        verdict
            .gaps
            .iter()
            .any(|gap| gap.code == "reason.dl-gap.native-class-model"
                && gap.message.contains("urn:coverage:b"))
    );
    assert!(!verdict.boundary_findings.is_empty());
}

#[test]
fn native_construct_inventory_rejects_foreign_contract_graph_and_proof_payload() {
    let original = native_execution(vec![source_row(
        "urn:coverage:C",
        "http://www.w3.org/2002/07/owl#complementOf",
        "urn:coverage:D",
        "urn:coverage:w",
    )]);
    let mut wrong_contract = original.clone();
    wrong_contract.source_coverage.input_contract[0] ^= 1;
    assert!(verdict_from_native(&[], &wrong_contract).is_err());
    let mut wrong_graph = original.clone();
    wrong_graph
        .source_coverage
        .worlds
        .get_mut("urn:coverage:w")
        .unwrap()
        .graph = Some(TermValue::iri("urn:foreign"));
    assert!(verdict_from_native(&[], &wrong_graph).is_err());
    let mut wrong_statement = original.clone();
    wrong_statement
        .source_coverage
        .worlds
        .get_mut("urn:coverage:w")
        .unwrap()
        .constructs[0]
        .statement
        .object = TermValue::iri("urn:changed");
    assert!(verdict_from_native(&[], &wrong_statement).is_err());
    let mut duplicate = original.clone();
    let constructs = &mut duplicate
        .source_coverage
        .worlds
        .get_mut("urn:coverage:w")
        .unwrap()
        .constructs;
    constructs.push(constructs[0].clone());
    assert!(verdict_from_native(&[], &duplicate).is_err());
}

#[test]
fn incomplete_native_frontier_retains_clashes_and_refuses_completed_coverage() {
    let result = native_result(vec![
        source_row(
            "urn:coverage:p",
            "http://www.w3.org/2000/01/rdf-schema#domain",
            "urn:coverage:C",
            "urn:coverage:w",
        ),
        source_row(
            "urn:coverage:x",
            "http://www.w3.org/2002/07/owl#sameAs",
            "urn:coverage:y",
            "urn:coverage:w",
        ),
        source_row(
            "urn:coverage:x",
            "http://www.w3.org/2002/07/owl#differentFrom",
            "urn:coverage:y",
            "urn:coverage:w",
        ),
    ]);
    let mut execution = result.native_execution().unwrap().clone();
    let original = verdict_from_native(result.inferred(), &execution).unwrap();
    assert!(
        !original.inconsistencies.is_empty(),
        "the positive control must carry a real committed local clash"
    );
    execution.frontier.completed = 0;
    execution.frontier.total = execution.frontier.total.max(1);
    execution.status = crate::reason::refute::native::NativeClosureStatus::Exhausted;
    let verdict = verdict_from_native(result.inferred(), &execution).unwrap();
    assert!(
        !verdict.consistent,
        "supported conflict and pending completion are independent"
    );
    assert_eq!(verdict.inconsistencies, original.inconsistencies);
    assert!(verdict.coverage.unsupported.contains(&"domain".to_owned()));
    assert!(
        verdict
            .gaps
            .iter()
            .any(|gap| gap.code == "reason.dl-gap.native-frontier")
    );
}

#[test]
fn malformed_chain_admission_retains_both_conflicting_cells_and_no_prefix_head() {
    let world = "urn:chain:world";
    let result = native_result(vec![
        source_row(
            "urn:chain:out",
            "http://www.w3.org/2002/07/owl#propertyChainAxiom",
            "urn:chain:list",
            world,
        ),
        source_row("urn:chain:list", FIRST, "urn:chain:p", world),
        source_row("urn:chain:list", FIRST, "urn:chain:q", world),
        source_row(
            "urn:chain:list",
            REST,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
            world,
        ),
        source_row("urn:chain:x", "urn:chain:p", "urn:chain:y", world),
    ]);
    let execution = result.native_execution().unwrap();
    let admission = execution.source_coverage.worlds[world]
        .admissions
        .iter()
        .find(|admission| admission.family == DlConstructFamily::PropertyChainAxiom)
        .unwrap();
    assert_eq!(
        admission.completion,
        NativeFamilyCompletion::Obstructed,
        "terminal={:?}; frontier={:?}; admissions={:#?}",
        execution.status,
        execution.frontier,
        execution.source_coverage.worlds[world].admissions
    );
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == world)
        .unwrap();
    let leaves = ledger.source_leaves(&admission.support).unwrap();
    for object in ["urn:chain:p", "urn:chain:q"] {
        assert!(
            leaves
                .iter()
                .any(|row| row.subject == TermValue::iri("urn:chain:list")
                    && row.predicate == FIRST
                    && row.object == TermValue::iri(object)
                    && row.graph == Some(TermValue::iri(world)))
        );
    }
    assert!(
        !result
            .inferred()
            .iter()
            .any(|row| row.predicate == "urn:chain:out")
    );
    assert!(!source_admission_complete(
        &execution.source_coverage.worlds[world]
    ));
}

#[test]
fn incomplete_key_is_observed_without_any_candidate_individuals() {
    let world = "urn:key:world";
    let result = native_result(vec![
        source_row(
            "urn:key:rule",
            RDF_TYPE,
            "https://blackcatinformatics.ca/logic/KeyAssertion",
            world,
        ),
        source_row("urn:key:rule", KEY_CLASS, "urn:key:Class", world),
    ]);
    let execution = result.native_execution().unwrap();
    let admission = execution.source_coverage.worlds[world]
        .admissions
        .iter()
        .find(|admission| admission.family == DlConstructFamily::HasKey)
        .unwrap();
    assert_eq!(
        admission.completion,
        NativeFamilyCompletion::Obstructed,
        "terminal={:?}; frontier={:?}; admissions={:#?}",
        execution.status,
        execution.frontier,
        execution.source_coverage.worlds[world].admissions
    );
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == world)
        .unwrap();
    let leaves = ledger.source_leaves(&admission.support).unwrap();
    assert!(
        leaves
            .iter()
            .any(|row| row.predicate == KEY_CLASS && row.object == TermValue::iri("urn:key:Class"))
    );
    assert!(
        leaves
            .iter()
            .all(|row| row.graph == Some(TermValue::iri(world)))
    );
    assert!(!result.inferred().iter().any(|row| {
        row.object
            .as_iri()
            .and_then(|object| EmptyClassAssertion::classify(&row.predicate, object))
            == Some(EmptyClassAssertion::Membership)
    }));
}

#[test]
fn ambiguous_negative_assertion_keeps_every_target_without_inventing_a_clash() {
    let world = "urn:npa:world";
    let result = native_result(vec![
        source_row("urn:npa:rule", SOURCE_INDIVIDUAL, "urn:npa:x", world),
        source_row("urn:npa:rule", ASSERTION_PROPERTY, "urn:npa:p", world),
        source_row("urn:npa:rule", TARGET_INDIVIDUAL, "urn:npa:y", world),
        source_row("urn:npa:rule", TARGET_INDIVIDUAL, "urn:npa:z", world),
        source_row("urn:npa:x", "urn:npa:p", "urn:npa:y", world),
    ]);
    let execution = result.native_execution().unwrap();
    let admission = execution.source_coverage.worlds[world]
        .admissions
        .iter()
        .find(|admission| admission.family == DlConstructFamily::NegativePropertyAssertion)
        .unwrap();
    assert_eq!(
        admission.selectors.len(),
        4,
        "all source selections share one admitted owner"
    );
    assert_eq!(admission.completion, NativeFamilyCompletion::Obstructed);
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == world)
        .unwrap();
    let leaves = ledger.source_leaves(&admission.support).unwrap();
    for target in ["urn:npa:y", "urn:npa:z"] {
        assert!(
            leaves
                .iter()
                .any(|row| row.predicate == TARGET_INDIVIDUAL
                    && row.object == TermValue::iri(target))
        );
    }
    assert!(!result.inferred().iter().any(|row| {
        row.object
            .as_iri()
            .and_then(|object| EmptyClassAssertion::classify(&row.predicate, object))
            == Some(EmptyClassAssertion::Membership)
    }));
}

#[test]
fn one_member_chain_refuses_before_native_composition() {
    let world = "urn:short-chain:world";
    let result = native_result(vec![
        source_row(
            "urn:short-chain:out",
            "http://www.w3.org/2002/07/owl#propertyChainAxiom",
            "urn:short-chain:list",
            world,
        ),
        source_row("urn:short-chain:list", FIRST, "urn:short-chain:p", world),
        source_row(
            "urn:short-chain:list",
            REST,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
            world,
        ),
        source_row(
            "urn:short-chain:x",
            "urn:short-chain:p",
            "urn:short-chain:y",
            world,
        ),
    ]);
    let execution = result.native_execution().unwrap();
    let admission = execution.source_coverage.worlds[world]
        .admission(
            DlConstructFamily::PropertyChainAxiom,
            &TermValue::iri("urn:short-chain:out"),
        )
        .unwrap();
    assert_eq!(
        admission.completion,
        NativeFamilyCompletion::Obstructed,
        "terminal={:?}; frontier={:?}; admissions={:#?}",
        execution.status,
        execution.frontier,
        execution.source_coverage.worlds[world].admissions
    );
    assert!(
        admission
            .obstructions
            .iter()
            .any(|obstruction| obstruction.detail.contains("at least two"))
    );
    assert!(
        !result
            .inferred()
            .iter()
            .any(|row| row.predicate == "urn:short-chain:out")
    );
}

#[test]
fn canonical_key_cannot_hide_a_selected_projected_key_list() {
    let world = "urn:mixed-key:world";
    let result = native_result(vec![
        source_row(
            "urn:mixed-key:owner",
            RDF_TYPE,
            "https://blackcatinformatics.ca/logic/KeyAssertion",
            world,
        ),
        source_row("urn:mixed-key:owner", KEY_CLASS, "urn:mixed-key:C", world),
        source_row(
            "urn:mixed-key:owner",
            KEY_PROPERTY,
            "urn:mixed-key:p",
            world,
        ),
        source_row(
            "urn:mixed-key:owner",
            "http://www.w3.org/2002/07/owl#hasKey",
            "urn:mixed-key:broken",
            world,
        ),
        source_row("urn:mixed-key:broken", FIRST, "urn:mixed-key:q", world),
    ]);
    let execution = result.native_execution().unwrap();
    let admission = execution.source_coverage.worlds[world]
        .admission(
            DlConstructFamily::HasKey,
            &TermValue::iri("urn:mixed-key:owner"),
        )
        .unwrap();
    assert_eq!(admission.selectors.len(), 2);
    assert_eq!(admission.completion, NativeFamilyCompletion::Obstructed);
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == world)
        .unwrap();
    let leaves = ledger.source_leaves(&admission.support).unwrap();
    assert!(leaves.iter().any(
        |row| row.predicate == KEY_PROPERTY && row.object == TermValue::iri("urn:mixed-key:p")
    ));
    assert!(
        leaves
            .iter()
            .any(|row| row.subject == TermValue::iri("urn:mixed-key:broken")
                && row.predicate == FIRST
                && row.object == TermValue::iri("urn:mixed-key:q"))
    );
    assert!(!source_admission_complete(
        &execution.source_coverage.worlds[world]
    ));
}

#[test]
fn atomic_bad_class_operand_never_publishes_a_derived_membership() {
    let world = "urn:bad-domain:world";
    let predicate = "http://www.w3.org/2000/01/rdf-schema#domain";
    let mut malformed = source_row("urn:bad-domain:p", predicate, "urn:unused", world);
    malformed.object = purrdf::RdfTerm::Literal(purrdf::RdfLiteral::simple("not a class"));
    let result = native_result(vec![
        malformed,
        source_row(
            "urn:bad-domain:x",
            "urn:bad-domain:p",
            "urn:bad-domain:y",
            world,
        ),
    ]);
    let execution = result.native_execution().unwrap();
    let admission = execution.source_coverage.worlds[world]
        .admission(
            DlConstructFamily::Domain,
            &TermValue::iri("urn:bad-domain:p"),
        )
        .unwrap();
    assert_eq!(admission.completion, NativeFamilyCompletion::Obstructed);
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == world)
        .unwrap();
    assert!(
        ledger
            .source_leaves(&admission.support)
            .unwrap()
            .iter()
            .any(|row| row.predicate == predicate
                && row.object == TermValue::simple_literal("not a class"))
    );
    assert!(!result.inferred().iter().any(|row| !row.is_edb
        && row.predicate == RDF_TYPE
        && matches!(&row.object, TermValue::Literal { .. })));
}
