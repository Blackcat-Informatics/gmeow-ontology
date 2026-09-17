// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Intrinsic domain laws execute in the same native producer graph as authored
//! rules and schema laws. Every input here is tiny and independently synthetic.

use super::*;
use crate::physical::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, WitnessOrigin,
};
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use purrdf::{BlankScope, DatasetView, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const THING: &str = "https://blackcatinformatics.ca/logic/Thing";
const NOTHING: &str = "https://blackcatinformatics.ca/logic/Nothing";
const SUBCLASS: &str = "https://blackcatinformatics.ca/logic/subClassOf";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

fn selected(graph: LogicalGraph, receipt: u8) -> SelectedLogicalWorld {
    SelectedLogicalWorld::new(
        graph,
        DomainProfile::NonemptyObjectDomainV1,
        "urn:test:selected-object-domain".to_owned(),
        [receipt; 32],
    )
    .unwrap()
}
fn domains(worlds: Vec<SelectedLogicalWorld>) -> SelectedDomains {
    SelectedDomains::new(worlds).unwrap()
}
fn data(rows: &[RdfQuad]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for row in rows {
        builder.push_owned_quad(row);
    }
    builder.freeze().unwrap()
}
fn fact(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o))
}
fn empty_program() -> LogicProgram {
    LogicProgram::new(vec![], vec![], vec![], None)
}
fn copying(from: &str, to: &str) -> LogicProgram {
    let atom = |predicate: &str| {
        Formula::atom(
            Term::iri(predicate).unwrap(),
            vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
        )
        .unwrap()
    };
    empty_program().with_formulas(vec![Formula::Forall {
        vars: vec!["x".to_owned(), "y".to_owned()],
        body: Box::new(Formula::Implies(Box::new(atom(from)), Box::new(atom(to)))),
    }])
}
fn run(
    program: &LogicProgram,
    input: &Arc<RdfDataset>,
    selection: &SelectedDomains,
    budget: Option<u64>,
) -> ProgramClosure {
    execute(
        &crate::program_analysis::prepare_program(program).unwrap(),
        crate::reason::program::prepare_reasoning_input(input).unwrap(),
        selection,
        budget,
    )
    .unwrap()
}
fn intrinsic(result: &ProgramClosure) -> Vec<&crate::physical::WitnessDerivation> {
    result
        .witnesses
        .iter()
        .filter(|w| matches!(w.scope.origin, WitnessOrigin::NonemptyDomain(_)))
        .collect()
}

#[test]
fn empty_selected_world_needs_no_prior_graph_declaration_or_fabricated_premise() {
    let input = data(&[]);
    let owned = selected(LogicalGraph::Named(TermValue::iri("urn:selected:empty")), 1);
    let selection = domains(vec![owned.clone()]);
    let result = run(
        &copying(INSTANCE, "urn:domain:observed"),
        &input,
        &selection,
        None,
    );
    let witnesses = intrinsic(&result);
    assert_eq!(witnesses.len(), 1);
    let witness = witnesses[0];
    assert_eq!(witness.scope.world, "urn:selected:empty");
    assert_eq!(witness.scope.origin, WitnessOrigin::NonemptyDomain(owned));
    assert_eq!(witness.heads.len(), 1);
    assert!(witness.heads[0].premises.is_empty());
    assert_eq!(witness.heads[0].statement.predicate, INSTANCE);
    assert_eq!(witness.heads[0].statement.object, TermValue::iri(THING));
    assert_eq!(
        witness.heads[0].positions,
        vec![crate::physical::WitnessPosition::Subject]
    );
    witness.validate().unwrap();
    assert_eq!(result.selected_domains, selection);
    assert!(result.inferred.iter().any(|r| r.subject == witness.witness
        && r.predicate == "urn:domain:observed"
        && r.world == "urn:selected:empty"));
    assert!(result.inferred.iter().all(|r| {
        r.premises
            .iter()
            .all(|(s, p, o)| !(s == THING && p == SUBCLASS && o == &format!("<{THING}>")))
    }));
    assert!(
        result.graphs.get("urn:selected:empty")
            == Some(&Some(TermValue::iri("urn:selected:empty")))
    );
}

#[test]
fn physical_import_report_and_compiler_graphs_do_not_select_domains() {
    let mut builder = RdfDatasetBuilder::new();
    for graph in [
        "urn:carrier:import",
        "urn:carrier:report",
        "urn:carrier:compiler",
    ] {
        let graph = builder.intern_iri(graph);
        builder.declare_named_graph(graph);
    }
    let input = builder.freeze().unwrap();
    let none = domains(vec![]);
    assert!(
        run(&empty_program(), &input, &none, None)
            .witnesses
            .is_empty()
    );
    let selection = domains(vec![selected(
        LogicalGraph::Named(TermValue::iri("urn:logical:chosen")),
        2,
    )]);
    let result = run(&empty_program(), &input, &selection, None);
    assert_eq!(intrinsic(&result).len(), 1);
    assert!(
        result
            .inferred
            .iter()
            .filter(|r| !r.is_edb)
            .all(|r| r.world == "urn:logical:chosen")
    );
}

#[test]
fn selected_worlds_and_contracts_have_distinct_order_independent_witnesses() {
    let a = selected(LogicalGraph::Default, 3);
    let b = selected(LogicalGraph::Named(TermValue::iri("urn:logical:b")), 3);
    let input = data(&[]);
    let first = run(
        &empty_program(),
        &input,
        &domains(vec![a.clone(), b.clone()]),
        None,
    );
    let reordered = run(&empty_program(), &input, &domains(vec![b, a.clone()]), None);
    assert_eq!(first.witnesses, reordered.witnesses);
    assert_eq!(first.witnesses.len(), 2);
    assert_ne!(first.witnesses[0].witness, first.witnesses[1].witness);
    assert_ne!(
        first.witnesses[0].heads[0].derivation_id,
        first.witnesses[1].heads[0].derivation_id
    );
    let original = run(&empty_program(), &input, &domains(vec![a]), None);
    let changed = run(
        &empty_program(),
        &input,
        &domains(vec![selected(LogicalGraph::Default, 4)]),
        None,
    );
    assert_ne!(original.witnesses[0].witness, changed.witnesses[0].witness);
    assert_ne!(
        original.certificates[0].input_contract,
        changed.certificates[0].input_contract
    );
}

#[test]
fn blank_graph_scope_is_retained_in_intrinsic_evidence_and_output() {
    let a = TermValue::Blank {
        label: "world".to_owned(),
        scope: BlankScope(71),
    };
    let b = TermValue::Blank {
        label: "world".to_owned(),
        scope: BlankScope(72),
    };
    let selection = domains(vec![
        selected(LogicalGraph::Named(a.clone()), 5),
        selected(LogicalGraph::Named(b.clone()), 5),
    ]);
    let result = crate::reason::reason_program(
        &empty_program(),
        prepare_reasoning_input(&data(&[])).unwrap(),
        &selection,
    )
    .unwrap();
    let execution = result.native_execution().unwrap();
    let origins: std::collections::BTreeSet<_> = execution
        .witness_derivations
        .iter()
        .filter(|w| matches!(w.scope.origin, WitnessOrigin::NonemptyDomain(_)))
        .map(|w| {
            let WitnessOrigin::NonemptyDomain(domain) = &w.scope.origin else {
                unreachable!()
            };
            domain.graph().graph().unwrap().clone()
        })
        .collect();
    assert_eq!(
        origins,
        std::collections::BTreeSet::from([a.clone(), b.clone()])
    );
    let output = crate::reason::native_closure_to_dataset(&result).unwrap();
    let graphs: std::collections::BTreeSet<_> = output
        .quads()
        .filter_map(|q| q.g.map(|g| crate::reason::dataset::native(&output, g)))
        .collect();
    assert_eq!(graphs, std::collections::BTreeSet::from([a, b]));
    assert_ne!(
        execution.witness_derivations[0].witness,
        execution.witness_derivations[1].witness
    );
}

#[test]
fn zero_budget_retains_selection_without_publishing_an_uncommitted_domain() {
    let selection = domains(vec![selected(
        LogicalGraph::Named(TermValue::iri("urn:logical:empty")),
        6,
    )]);
    let result = run(
        &copying(INSTANCE, "urn:late:consumer"),
        &data(&[]),
        &selection,
        Some(0),
    );
    assert_eq!(result.status, crate::seam::BudgetStatus::Exhausted);
    assert_eq!(result.selected_domains, selection);
    assert!(result.inferred.is_empty());
    assert!(result.witnesses.is_empty());
    assert_eq!(
        result.native_status,
        crate::reason::refute::native::NativeClosureStatus::Exhausted
    );
    assert_eq!(result.consumed_steps, 0);
    assert_eq!(result.frontier.consumed_steps, 0);
    assert!(result.frontier.completed < result.frontier.total);
}

#[test]
fn top_emptiness_and_authored_consumer_share_the_domain_fixed_point() {
    for (thing, subclass, nothing) in [
        (THING, SUBCLASS, NOTHING),
        (
            "http://www.w3.org/2002/07/owl#Thing",
            "http://www.w3.org/2000/01/rdf-schema#subClassOf",
            OWL_NOTHING,
        ),
    ] {
        let rows = [fact(thing, subclass, nothing)];
        let selection = domains(vec![selected(LogicalGraph::Default, 7)]);
        let result = run(
            &copying(TYPE, "urn:empty:observed"),
            &data(&rows),
            &selection,
            None,
        );
        let witness = intrinsic(&result)[0];
        let membership = result
            .inferred
            .iter()
            .find(|r| {
                r.subject == witness.witness
                    && r.rule_name.as_deref() == Some("dl:universal-class-subclass")
                    && r.object.as_iri() == Some(nothing)
            })
            .expect("intrinsic member must reach the actual top-subclass law");
        assert_eq!(membership.premises.len(), 2);
        assert!(membership.premises.contains(&(
            thing.to_owned(),
            subclass.to_owned(),
            format!("<{nothing}>")
        )));
        assert!(membership.premises.contains(&(
            witness.witness.clone(),
            INSTANCE.to_owned(),
            format!("<{THING}>")
        )));
        assert!(result.inferred.iter().any(|r| r.subject == witness.witness
            && r.predicate == "urn:empty:observed"
            && r.object.as_iri() == Some(nothing)));
    }
}

#[test]
fn intrinsic_domain_reaches_minimum_and_the_same_governor_budget_prefix() {
    let mut rows = vec![
        fact(THING, SUBCLASS, "urn:restriction"),
        fact(
            "urn:restriction",
            "https://blackcatinformatics.ca/logic/onProperty",
            "urn:required:edge",
        ),
        fact(
            "urn:restriction",
            "https://blackcatinformatics.ca/logic/onClass",
            "urn:filler",
        ),
    ];
    rows.push(RdfQuad::new(
        RdfTerm::iri("urn:restriction"),
        "https://blackcatinformatics.ca/logic/minQualifiedCardinality",
        RdfTerm::Literal(RdfLiteral::typed(
            "1",
            "http://www.w3.org/2001/XMLSchema#integer",
        )),
    ));
    let selection = domains(vec![selected(LogicalGraph::Default, 8)]);
    let program = copying("urn:required:edge", "urn:edge:observed");
    let input = data(&rows);
    let full = run(&program, &input, &selection, None);
    let witness = intrinsic(&full)[0];
    let edge = full
        .inferred
        .iter()
        .find(|r| r.subject == witness.witness && r.predicate == "urn:required:edge")
        .expect("minimum obligation awakened by domain law");
    assert!(full.inferred.iter().any(|r| r.subject == witness.witness
        && r.predicate == "urn:edge:observed"
        && r.object == edge.object));
    assert!(
        full.witnesses
            .iter()
            .any(|w| w.witness == edge.object.as_iri().unwrap()
                && matches!(w.scope.origin, WitnessOrigin::Rule))
    );
    let cut = run(&program, &input, &selection, Some(1));
    assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
    assert_eq!(cut.frontier.consumed_steps, 1);
    for receipt in &cut.witnesses {
        receipt.validate().unwrap();
        assert!(
            receipt
                .heads
                .iter()
                .all(
                    |head| cut.inferred.iter().any(|r| r.world == receipt.scope.world
                        && r.subject == head.statement.subject.as_iri().unwrap()
                        && r.predicate == head.statement.predicate
                        && r.object == head.statement.object)
                )
        );
    }
    assert!(
        cut.inferred
            .iter()
            .all(|r| r.predicate != "urn:edge:observed")
    );
}

#[test]
fn existing_member_satisfies_the_domain_without_redundant_null() {
    let input = data(&[fact("urn:already", INSTANCE, THING)]);
    let selection = domains(vec![selected(LogicalGraph::Default, 9)]);
    let result = run(&empty_program(), &input, &selection, None);
    assert!(intrinsic(&result).is_empty());
    assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
}

#[test]
fn invalid_selection_and_tampered_intrinsic_evidence_fail_closed() {
    assert!(
        SelectedLogicalWorld::new(
            LogicalGraph::Named(TermValue::simple_literal("not a graph")),
            DomainProfile::NonemptyObjectDomainV1,
            "urn:authority".to_owned(),
            [1; 32]
        )
        .is_err()
    );
    let owner = selected(LogicalGraph::Default, 10);
    assert!(SelectedDomains::new([owner.clone(), owner.clone()]).is_err());
    let result = run(&empty_program(), &data(&[]), &domains(vec![owner]), None);
    let receipt = &result.witnesses[0];
    assert_eq!(
        *receipt,
        crate::physical::WitnessDerivation::from_wire(&receipt.to_wire().unwrap()).unwrap()
    );
    let mut altered = receipt.clone();
    altered.scope.origin = WitnessOrigin::Rule;
    assert!(altered.validate().is_err());
    let mut altered = receipt.clone();
    altered.heads[0]
        .premises
        .push(crate::physical::WitnessStatement {
            subject: TermValue::iri(THING),
            predicate: SUBCLASS.to_owned(),
            object: TermValue::iri(THING),
        });
    assert!(altered.validate().is_err());
    let collision = domains(vec![selected(
        LogicalGraph::Named(TermValue::iri(super::super::rl::DEFAULT_WORLD)),
        11,
    )]);
    assert!(
        execute(
            &crate::program_analysis::prepare_program(&empty_program()).unwrap(),
            crate::reason::program::prepare_reasoning_input(&data(&[])).unwrap(),
            &collision,
            None
        )
        .is_err()
    );
}

#[test]
fn top_emptiness_in_one_selected_world_does_not_cross_into_its_sibling() {
    let mut empty_top = fact(THING, SUBCLASS, NOTHING);
    empty_top.graph_name = Some(RdfTerm::iri("urn:logical:a"));
    let selection = domains(vec![
        selected(LogicalGraph::Named(TermValue::iri("urn:logical:a")), 12),
        selected(LogicalGraph::Named(TermValue::iri("urn:logical:b")), 12),
    ]);
    let result = run(
        &copying(TYPE, "urn:clash:consumer"),
        &data(&[empty_top]),
        &selection,
        None,
    );
    let a = intrinsic(&result)
        .into_iter()
        .find(|w| w.scope.world == "urn:logical:a")
        .unwrap();
    let b = intrinsic(&result)
        .into_iter()
        .find(|w| w.scope.world == "urn:logical:b")
        .unwrap();
    assert!(result.inferred.iter().any(|r| r.subject == a.witness
        && r.world == "urn:logical:a"
        && r.object.as_iri() == Some(NOTHING)));
    assert!(result.inferred.iter().all(|r| !(r.world == "urn:logical:b"
        && matches!(r.object.as_iri(), Some(NOTHING | OWL_NOTHING)))));
    assert!(
        result
            .inferred
            .iter()
            .all(|r| !(r.world == "urn:logical:a" && r.subject == b.witness))
    );
}

#[test]
fn retained_domain_authority_obeys_the_existing_template_payload_bound() {
    let domain = SelectedLogicalWorld::new(
        LogicalGraph::Default,
        DomainProfile::NonemptyObjectDomainV1,
        "x".repeat(1024 * 1024 + 1),
        [13; 32],
    )
    .unwrap();
    let selection = domains(vec![domain]);
    let template = crate::physical::JointTemplate::with_sources(
        &[],
        &[],
        &[],
        crate::native_semantics::SemanticVocabulary::GroundedLogicV1,
        &[],
        None,
        &selection,
    )
    .unwrap();
    assert!(
        !template.cacheable(),
        "retained domain authority cannot escape the native metadata byte budget"
    );
}
