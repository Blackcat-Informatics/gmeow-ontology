// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic controls for native operator interpretation and source admission.

use super::*;
use crate::native_semantics::SemanticVocabulary;
use crate::physical::{JointProgram, NativeOutcome};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact};
use gmeow_logic_compile::ir::{ContextualScope, Formula, LogicModality, LogicProgram, Term};
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};
use std::collections::{BTreeMap, BTreeSet};

const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const DOMAIN: &str = "https://blackcatinformatics.ca/logic/domain";
const CLASS: &str = "https://blackcatinformatics.ca/logic/Class";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const EDGE: &str = "urn:semantic:edge";
const DONE: &str = "urn:semantic:done";

fn formula(predicate: &str, left: Term, right: Term) -> Formula {
    Formula::atom(Term::iri(predicate).unwrap(), vec![left, right]).unwrap()
}

fn rule(body: Formula, head: Formula) -> Formula {
    Formula::Forall {
        vars: vec!["x".into(), "y".into()],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    }
}

fn fact(subject: &str, predicate: &str, object: &str) -> Fact {
    Fact {
        subject: TermValue::iri(subject),
        predicate: predicate.into(),
        object: TermValue::iri(object),
    }
}

fn joint(
    rules: &[EvalRule],
    facts: Vec<Fact>,
    semantics: SemanticVocabulary,
) -> Vec<crate::rule_ir::DerivedRow> {
    let NativeOutcome::Decided(plan) =
        JointProgram::prepare_with_semantics(rules, &[], &[], &BTreeSet::new(), semantics).unwrap()
    else {
        panic!("finite synthetic program is admitted");
    };
    let NativeOutcome::Decided(result) = plan
        .materialize_facts(&BTreeMap::from([("urn:context".into(), facts)]), None)
        .unwrap()
    else {
        panic!("finite synthetic execution completes");
    };
    result.result.rows
}

#[test]
fn canonical_derived_domain_wakes_native_schema_and_canonical_consumer() {
    let var = |name| Term::var(name).unwrap();
    let named = |name| Term::iri(name).unwrap();
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![
        rule(
            formula(EDGE, var("x"), var("y")),
            formula(DOMAIN, named(EDGE), named("urn:class")),
        ),
        rule(
            formula(INSTANCE, var("x"), var("y")),
            formula(DONE, var("x"), var("y")),
        ),
    ]);
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&RdfQuad::new(
        RdfTerm::iri("urn:x"),
        EDGE,
        RdfTerm::iri("urn:y"),
    ));
    let input = builder.freeze().unwrap();
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let closure = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&input).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(closure.inferred.iter().any(|row| row.subject == "urn:x"
        && row.predicate == DONE
        && row.object.as_iri() == Some("urn:class")));
    let schema = closure
        .inferred
        .iter()
        .find(|row| row.rule_name.as_deref() == Some("dl:domain"))
        .unwrap();
    assert!(
        schema
            .premises
            .iter()
            .any(|(s, p, o)| s == EDGE && p == DOMAIN && o == "<urn:class>")
    );
    assert_eq!(
        input_facts(&input)
            .unwrap()
            .0
            .values()
            .map(Vec::len)
            .sum::<usize>(),
        1,
        "no alias facts are manufactured at ingestion"
    );
}

#[test]
fn derived_canonical_property_marker_is_visible_in_the_same_fixed_point() {
    let var = |name| Term::var(name).unwrap();
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![rule(
        formula(EDGE, var("x"), var("y")),
        formula(
            INSTANCE,
            Term::iri(EDGE).unwrap(),
            Term::iri("https://blackcatinformatics.ca/logic/transitiveProperty").unwrap(),
        ),
    )]);
    let mut builder = RdfDatasetBuilder::new();
    for (s, o) in [("urn:x", "urn:y"), ("urn:y", "urn:z")] {
        builder.push_owned_quad(&RdfQuad::new(RdfTerm::iri(s), EDGE, RdfTerm::iri(o)));
    }
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let closure = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&builder.freeze().unwrap()).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    let derived = closure
        .inferred
        .iter()
        .find(|row| {
            row.subject == "urn:x" && row.predicate == EDGE && row.object.as_iri() == Some("urn:z")
        })
        .unwrap();
    assert!(derived.premises.iter().any(|(s, p, o)| s == EDGE
        && p == INSTANCE
        && o == "<https://blackcatinformatics.ca/logic/transitiveProperty>"));
}

#[test]
fn marker_constants_are_interpreted_but_variable_data_identity_stays_exact() {
    let marker = EvalRule::positive(
        "urn:constant-marker",
        EvalAtom::positive(EvalTerm::var("?x"), DONE, EvalTerm::named("urn:constant")),
        vec![EvalAtom::positive(
            EvalTerm::var("?x"),
            INSTANCE,
            EvalTerm::named(CLASS),
        )],
    );
    let data = EvalRule::positive(
        "urn:data-binding",
        EvalAtom::positive(
            EvalTerm::var("?x"),
            "urn:data-result",
            EvalTerm::var("?value"),
        ),
        vec![
            EvalAtom::positive(EvalTerm::var("?x"), INSTANCE, EvalTerm::var("?value")),
            EvalAtom::positive(
                EvalTerm::var("?value"),
                "urn:label",
                EvalTerm::named("urn:yes"),
            ),
        ],
    );
    let rows = joint(
        &[marker.clone(), data],
        vec![
            fact("urn:x", TYPE, OWL_CLASS),
            fact(CLASS, "urn:label", "urn:yes"),
        ],
        SemanticVocabulary::GroundedLogicV1,
    );
    let marker_row = rows.iter().find(|row| row.predicate == DONE).unwrap();
    assert_eq!(marker_row.antecedents[0].predicate, TYPE);
    assert_eq!(marker_row.antecedents[0].object, TermValue::iri(OWL_CLASS));
    assert!(!rows.iter().any(|row| row.predicate == "urn:data-result"));
    assert!(
        !joint(
            &[marker],
            vec![fact("urn:x", TYPE, OWL_CLASS)],
            SemanticVocabulary::Exact
        )
        .iter()
        .any(|row| row.predicate == DONE)
    );
}

#[test]
fn ordinary_data_marker_and_marker_used_as_predicate_are_not_aliases() {
    let rules = [
        EvalRule::positive(
            "urn:ordinary-object",
            EvalAtom::positive(EvalTerm::var("?x"), DONE, EvalTerm::named("urn:object")),
            vec![EvalAtom::positive(
                EvalTerm::var("?x"),
                EDGE,
                EvalTerm::named(CLASS),
            )],
        ),
        EvalRule::positive(
            "urn:marker-predicate",
            EvalAtom::positive(EvalTerm::var("?x"), DONE, EvalTerm::named("urn:predicate")),
            vec![EvalAtom::positive(
                EvalTerm::var("?x"),
                CLASS,
                EvalTerm::var("?y"),
            )],
        ),
    ];
    let rows = joint(
        &rules,
        vec![
            fact("urn:x", EDGE, OWL_CLASS),
            fact("urn:x", OWL_CLASS, "urn:y"),
        ],
        SemanticVocabulary::GroundedLogicV1,
    );
    assert!(!rows.iter().any(|row| row.predicate == DONE));
}

#[test]
fn semantic_negative_cycle_is_refused_across_declared_predicate_spellings() {
    let mut absent = EvalAtom::positive(EvalTerm::var("?x"), TYPE, EvalTerm::var("?y"));
    absent.negated = true;
    let rule = EvalRule::positive(
        "urn:negative-cycle",
        EvalAtom::positive(EvalTerm::var("?x"), INSTANCE, EvalTerm::var("?y")),
        vec![
            EvalAtom::positive(EvalTerm::var("?x"), EDGE, EvalTerm::var("?y")),
            absent,
        ],
    );
    let plan = JointProgram::prepare_with_semantics(
        &[rule],
        &[],
        &[],
        &BTreeSet::new(),
        SemanticVocabulary::GroundedLogicV1,
    )
    .unwrap();
    assert!(matches!(
        plan,
        NativeOutcome::Unsupported(crate::physical::UnsupportedKind::NonStratifiable)
    ));
}

#[test]
fn template_admission_preserves_and_refuses_unexecuted_source_scope() {
    use gmeow_logic_compile::ir::{AtomicTerm, LogicAxiom, LogicRule};
    let axiom = |predicate: &str| {
        LogicAxiom::new(
            "?x",
            predicate,
            AtomicTerm::Var("?y".into()),
            false,
            ContextualScope::default(),
        )
        .unwrap()
    };
    for scope in [
        ContextualScope {
            standpoint: Some("urn:C".into()),
            ..Default::default()
        },
        ContextualScope {
            time: Some("2026".into()),
            ..Default::default()
        },
        ContextualScope {
            module: Some("urn:module".into()),
            ..Default::default()
        },
        ContextualScope {
            modality: LogicModality::Deontic,
            ..Default::default()
        },
    ] {
        let source_rule = LogicRule::new(axiom(DONE), vec![axiom(EDGE)], vec![], scope.clone());
        let program =
            LogicProgram::new(vec![], vec![source_rule], vec![], Some("urn:source".into()));
        let prepared = crate::program_analysis::prepare_program(&program).unwrap();
        assert_eq!(prepared.admission.scopes[0].scope, scope);
        let input = RdfDatasetBuilder::new().freeze().unwrap();
        let error = match execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&input).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None,
        ) {
            Ok(_) => panic!("scoped source must not become a universal template"),
            Err(error) => error,
        };
        assert!(
            error
                .message()
                .contains("world-local template admission refuses")
        );
        assert!(error.message().contains("rule/0"));
    }
}

#[test]
fn late_projected_type_wakes_canonical_existential_body_with_original_premise() {
    use crate::physical::{ExistentialRule, WitnessPolicy};
    let ordinary = EvalRule::positive(
        "urn:publish-type",
        EvalAtom::positive(EvalTerm::var("?x"), TYPE, EvalTerm::named(OWL_CLASS)),
        vec![EvalAtom::positive(
            EvalTerm::var("?x"),
            EDGE,
            EvalTerm::var("?y"),
        )],
    );
    let existential = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "urn:invent-after-marker".into(),
        body: vec![EvalAtom::positive(
            EvalTerm::var("?x"),
            INSTANCE,
            EvalTerm::named(CLASS),
        )],
        head: vec![EvalAtom::positive(
            EvalTerm::var("?x"),
            DONE,
            EvalTerm::var("?z"),
        )],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let NativeOutcome::Decided(plan) = JointProgram::prepare_with_semantics(
        &[ordinary],
        &[existential],
        &[],
        &BTreeSet::new(),
        SemanticVocabulary::GroundedLogicV1,
    )
    .unwrap() else {
        panic!("positive finite preparation");
    };
    let NativeOutcome::Decided(result) = plan
        .materialize_facts(
            &BTreeMap::from([("urn:C".into(), vec![fact("urn:x", EDGE, "urn:y")])]),
            None,
        )
        .unwrap()
    else {
        panic!("finite chase");
    };
    let done = result
        .result
        .rows
        .iter()
        .find(|row| row.predicate == DONE)
        .unwrap();
    assert_eq!(done.graph, "urn:C");
    assert_eq!(done.antecedents.len(), 1);
    assert_eq!(done.antecedents[0].predicate, TYPE);
    assert_eq!(done.antecedents[0].object, TermValue::iri(OWL_CLASS));
    assert_eq!(result.witness_derivations.len(), 1);
}

#[test]
fn canonical_membership_is_visible_to_native_list_intersection_with_exact_evidence() {
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in [
        (
            "urn:intersection",
            "https://blackcatinformatics.ca/logic/intersectionOf",
            "urn:list",
        ),
        (
            "urn:list",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
            "urn:member-class",
        ),
        (
            "urn:list",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
        ),
        ("urn:x", INSTANCE, "urn:member-class"),
    ] {
        builder.push_owned_quad(&RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)));
    }
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![rule(
        formula(
            INSTANCE,
            Term::var("x").unwrap(),
            Term::iri("urn:intersection").unwrap(),
        ),
        formula(DONE, Term::var("x").unwrap(), Term::iri("urn:yes").unwrap()),
    )]);
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let closure = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&builder.freeze().unwrap()).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        closure
            .inferred
            .iter()
            .any(|row| row.subject == "urn:x" && row.predicate == DONE)
    );
    let membership = closure
        .inferred
        .iter()
        .find(|row| {
            row.subject == "urn:x" && row.rule_name.as_deref() == Some("dl:intersection-membership")
        })
        .unwrap();
    assert!(
        membership
            .premises
            .iter()
            .any(|(s, p, o)| s == "urn:x" && p == INSTANCE && o == "<urn:member-class>")
    );
    assert!(
        membership
            .premises
            .iter()
            .any(|(_, p, _)| p == "http://www.w3.org/1999/02/22-rdf-syntax-ns#first")
    );
}

#[test]
fn incomplete_canonical_producer_cannot_publish_projected_completion() {
    let rule = EvalRule::positive(
        "urn:produce-type",
        EvalAtom::positive(EvalTerm::var("?x"), INSTANCE, EvalTerm::var("?y")),
        vec![EvalAtom::positive(
            EvalTerm::var("?x"),
            EDGE,
            EvalTerm::var("?y"),
        )],
    );
    let NativeOutcome::Decided(plan) = JointProgram::prepare_with_semantics(
        &[rule],
        &[],
        &[],
        &BTreeSet::new(),
        SemanticVocabulary::GroundedLogicV1,
    )
    .unwrap() else {
        panic!("finite plan");
    };
    let NativeOutcome::Decided(result) = plan
        .materialize_facts(
            &BTreeMap::from([(
                "urn:C".into(),
                vec![
                    fact("urn:x", EDGE, "urn:y"),
                    fact("urn:other", TYPE, "urn:class"),
                ],
            )]),
            Some(0),
        )
        .unwrap()
    else {
        panic!("budgeted native result");
    };
    assert_eq!(result.result.status, crate::seam::BudgetStatus::Exhausted);
    assert!(!result.result.progress.saturated_preds.contains(INSTANCE));
    assert!(!result.result.progress.saturated_preds.contains(TYPE));
    assert!(result.result.progress.saturated_preds.contains(EDGE));
}

#[test]
fn head_and_body_scopes_are_not_erased_and_source_evidence_survives_publication() {
    use gmeow_logic_compile::ir::{AtomicTerm, LogicAxiom, LogicRule};
    let axiom = |predicate: &str| {
        LogicAxiom::new(
            "?x",
            predicate,
            AtomicTerm::Var("?y".into()),
            false,
            ContextualScope::default(),
        )
        .unwrap()
    };
    for head_scope in [true, false] {
        let mut head = axiom(DONE);
        let mut body = axiom(EDGE);
        let scoped = if head_scope { &mut head } else { &mut body };
        scoped.scope.standpoint = Some("urn:C".into());
        scoped.scope.provenance = Some("urn:assertor".into());
        scoped.scope.confidence = Some(0.75);
        let program = LogicProgram::new(
            vec![],
            vec![LogicRule::new(
                head,
                vec![body],
                vec![],
                ContextualScope::default(),
            )],
            vec![],
            Some("urn:source-document".into()),
        );
        let prepared = crate::program_analysis::prepare_program(&program)
            .unwrap()
            .publication();
        assert_eq!(
            prepared.admission.source_iri.as_deref(),
            Some("urn:source-document")
        );
        let original = prepared
            .admission
            .scopes
            .iter()
            .find(|scope| scope.scope.standpoint.is_some())
            .unwrap();
        assert_eq!(
            original.owner,
            if head_scope {
                "rule/0/head"
            } else {
                "rule/0/body/0"
            }
        );
        assert_eq!(original.scope.provenance.as_deref(), Some("urn:assertor"));
        assert_eq!(original.scope.confidence, Some(0.75));
        assert!(prepared.admission.admit_world_local_template().is_err());
    }
}
