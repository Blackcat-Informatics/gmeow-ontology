// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic GMEOW rule interactions; no repository corpus or upstream suite.

use super::*;
use crate::rule_ir::{EvalAtom, EvalTerm};
use purrdf::TermValue;

pub(super) const WORLD: &str = "urn:joint:world";

pub(super) fn atom(subject: &str, predicate: &str, object: &str) -> EvalAtom {
    let term = |value: &str| {
        if value.starts_with('?') {
            EvalTerm::Var(value.to_owned())
        } else {
            EvalTerm::ConstNamed(value.to_owned())
        }
    };
    EvalAtom::positive(term(subject), predicate, term(object))
}

fn seed(worlds: &[&str]) -> crate::store::WorldStore {
    let store = crate::store::WorldStore::new();
    for world in worlds {
        store
            .insert_quad_terms(
                world,
                TermValue::iri("urn:a"),
                TermValue::iri("urn:seed"),
                TermValue::iri("urn:b"),
            )
            .unwrap();
    }
    store
}

pub(super) fn producer(body: Vec<EvalAtom>, head: Vec<EvalAtom>) -> ExistentialRule {
    ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "urn:invent".to_owned(),
        body,
        head,
        distinct: Vec::new(),
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    }
}

fn property(name: &str, head: [&str; 3], body: &[[&str; 3]]) -> PreparedPropertyRule {
    let terms = |values: [&str; 3]| {
        crate::physical::PropertyAtom(values.map(|value| {
            if value.starts_with('?') {
                EvalTerm::var(value)
            } else {
                EvalTerm::named(value)
            }
        }))
    };
    PreparedPropertyRule::new(crate::physical::PropertyRule {
        rule_iri: name.to_owned(),
        head: terms(head),
        body: body.iter().copied().map(terms).collect(),
        operation: None,
        guards: Vec::new(),
    })
    .unwrap()
}

fn with_properties(producer: ExistentialRule, properties: &[PreparedPropertyRule]) -> JointProgram {
    match JointProgram::prepare_with_properties(&[], &[producer], properties, &BTreeSet::new())
        .unwrap()
    {
        NativeOutcome::Decided(program) => program,
        NativeOutcome::Unsupported(kind) => panic!("unexpected preparation {kind:?}"),
    }
}

#[test]
fn data_selected_reader_and_invention_have_one_termination_proof() {
    let program = with_properties(
        producer(
            vec![atom("?x", "urn:seed", "?y")],
            vec![atom("?x", "urn:witness", "?z")],
        ),
        &[property(
            "urn:observe",
            ["?z", "urn:done", "?x"],
            &[["?x", "?p", "?z"]],
        )],
    );
    assert!(program.admission.admits_native(), "{:?}", program.admission);
    assert!(
        program
            .admission
            .to_finding()
            .message
            .contains("joint statement value-flow")
    );
    let result = run(&program, &[WORLD], None);
    assert_eq!(result.result.status, BudgetStatus::Ok);
    assert_eq!(result.witness_derivations.len(), 1);
    assert!(result.result.rows.iter().any(|row| {
        row.predicate == "urn:done"
            && row.subject == TermValue::iri(&result.witness_derivations[0].witness)
            && row
                .antecedents
                .iter()
                .any(|fact| fact.predicate == "urn:witness")
    }));
}

fn predicate_feedback() -> Vec<PreparedPropertyRule> {
    vec![
        property(
            "urn:publish-predicate",
            ["urn:signal", "?z", "urn:value"],
            &[["?x", "urn:witness", "?z"]],
        ),
        property(
            "urn:consume-predicate",
            ["?p", "urn:seed", "urn:value"],
            &[["urn:signal", "?p", "urn:value"]],
        ),
    ]
}

#[test]
fn witness_flow_through_predicate_positions_cannot_hide_an_invention_cycle() {
    let rule = producer(
        vec![atom("?x", "urn:seed", "?y")],
        vec![atom("?x", "urn:witness", "?z")],
    );
    assert!(ChaseAdmission::certify(std::slice::from_ref(&rule)).admits_native());
    let program = with_properties(rule, &predicate_feedback());
    assert!(!program.admission.admits_native());
    assert!(matches!(
        program.materialize(&seed(&[WORLD]), None).unwrap(),
        NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential)
    ));
    let bounded = run(&program, &[WORLD], Some(12));
    assert_eq!(bounded.result.status, BudgetStatus::Exhausted);
    assert_eq!(bounded.result.consumed_steps, 12);
    assert!(bounded.witness_derivations.len() > 1);
    assert!(bounded.result.rows.iter().any(|row| {
        bounded
            .witness_derivations
            .iter()
            .any(|witness| row.predicate == witness.witness)
    }));
}

#[test]
fn shared_witness_frontier_stays_finite_through_schema_feedback() {
    let mut rule = producer(
        vec![atom("?x", "urn:seed", "?y")],
        vec![atom("?x", "urn:witness", "?z")],
    );
    rule.witness_frontier = Some(Vec::new());
    let program = with_properties(rule, &predicate_feedback());
    assert!(program.admission.admits_native(), "{:?}", program.admission);
    let result = run(&program, &[WORLD], None);
    assert_eq!(result.result.status, BudgetStatus::Ok);
    assert_eq!(result.witness_derivations.len(), 1);
    assert!(result.result.rows.iter().any(|row| {
        row.predicate == "urn:seed"
            && row.subject == TermValue::iri(&result.witness_derivations[0].witness)
    }));
}

pub(super) fn prepare(rules: &[EvalRule], producers: &[ExistentialRule]) -> JointProgram {
    match JointProgram::prepare(rules, producers).unwrap() {
        NativeOutcome::Decided(program) => program,
        NativeOutcome::Unsupported(kind) => panic!("unexpected admission {kind:?}"),
    }
}

pub(super) fn run(
    program: &JointProgram,
    worlds: &[&str],
    budget: Option<u64>,
) -> JointMaterialization {
    match program.materialize(&seed(worlds), budget).unwrap() {
        NativeOutcome::Decided(result) => result,
        NativeOutcome::Unsupported(kind) => panic!("unexpected execution {kind:?}"),
    }
}

fn feedback() -> JointProgram {
    let ordinary = [
        EvalRule::positive(
            "urn:prepare",
            atom("?x", "urn:ready", "?y"),
            vec![atom("?x", "urn:seed", "?y")],
        ),
        EvalRule::positive(
            "urn:consume",
            atom("?z", "urn:done", "?x"),
            vec![atom("?x", "urn:witness", "?z")],
        ),
    ];
    prepare(
        &ordinary,
        &[producer(
            vec![atom("?x", "urn:ready", "?y")],
            vec![atom("?x", "urn:witness", "?z")],
        )],
    )
}

#[test]
fn ordinary_and_existential_feedback_shares_provenance_and_governor() {
    let program = feedback();
    let full = run(&program, &[WORLD], None);
    assert_eq!(full.result.status, BudgetStatus::Ok);
    assert_eq!(full.result.consumed_steps, 3);
    let done = full
        .result
        .rows
        .iter()
        .find(|r| r.predicate == "urn:done")
        .unwrap();
    assert_eq!(done.proof_height.get(), 3);
    assert_eq!(done.antecedents.len(), 1);
    assert_eq!(done.antecedents[0].predicate, "urn:witness");
    assert_eq!(full.witness_derivations.len(), 1);
    assert_eq!(
        done.subject,
        TermValue::iri(&full.witness_derivations[0].witness)
    );
    assert_eq!(
        full.witness_derivations[0].frontier,
        vec![TermValue::iri("urn:a")]
    );
    let partial = run(&program, &[WORLD], Some(2));
    assert_eq!(partial.result.status, BudgetStatus::Exhausted);
    assert_eq!(partial.result.consumed_steps, 2);
    assert!(
        !partial
            .result
            .rows
            .iter()
            .any(|r| r.predicate == "urn:done")
    );
    assert!(
        !partial
            .result
            .progress
            .saturated_preds
            .contains("urn:witness")
    );
    assert!(partial.result.progress.saturated_preds.contains("urn:seed"));
}

#[test]
fn competing_chase_heads_keep_the_joint_proof_winner_in_either_order() {
    let mut first = producer(
        vec![atom("?x", "urn:seed", "?y")],
        vec![
            atom("?x", "urn:shared", "?y"),
            atom("?x", "urn:witness", "?z"),
        ],
    );
    first.rule_iri = "urn:z-producer".into();
    let mut second = first.clone();
    second.rule_iri = "urn:a-producer".into();
    let seed_fact = Fact {
        subject: TermValue::iri("urn:a"),
        predicate: "urn:seed".into(),
        object: TermValue::iri("urn:b"),
    };
    for producers in [[first.clone(), second.clone()], [second.clone(), first]] {
        let full = run(&prepare(&[], &producers), &[WORLD], None);
        assert_eq!(full.result.status, BudgetStatus::Ok);
        assert_eq!(full.result.consumed_steps, 3);
        assert_eq!(full.witness_derivations.len(), 2);
        let shared: Vec<_> = full
            .result
            .rows
            .iter()
            .filter(|row| row.predicate == "urn:shared")
            .collect();
        assert_eq!(shared.len(), 1);
        let row = shared[0];
        assert_eq!(row.rule_iri, second.rule_iri);
        assert_eq!(row.graph, WORLD);
        assert_eq!(row.subject, seed_fact.subject);
        assert_eq!(row.object, seed_fact.object);
        assert_eq!(row.proof_height.get(), 1);
        assert_eq!(row.antecedents.len(), 1);
        assert_eq!(row.antecedents[0].key(), seed_fact.key());
        assert_eq!(row.source_quad_ids, [seed_fact.reifier().unwrap()]);
        assert_eq!(
            row.derivation_id,
            crate::provenance::mint_derivation_id(&second.rule_iri, &[&row.source_quad_ids[0]])
        );
    }
}

#[test]
fn global_budget_preserves_unrun_world_inputs_and_withholds_heads() {
    let result = run(&feedback(), &["urn:world:a", "urn:world:b"], Some(2));
    assert_eq!(result.result.consumed_steps, 2);
    assert_eq!(result.result.status, BudgetStatus::Exhausted);
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|r| r.graph == "urn:world:b" && r.predicate == "urn:seed")
    );
    assert!(
        !result
            .result
            .rows
            .iter()
            .any(|r| r.graph == "urn:world:b" && r.predicate == "urn:witness")
    );
    assert_eq!(result.result.progress.completed, 0);
    assert!(
        !result
            .result
            .progress
            .saturated_preds
            .contains("urn:witness")
    );
}

#[test]
fn negation_waits_for_existential_producer_and_ordinary_consumers() {
    let mut absent = atom("?x", "urn:present", "?y");
    absent.negated = true;
    let rules = [
        EvalRule::positive(
            "urn:present-rule",
            atom("?x", "urn:present", "urn:b"),
            vec![atom("?x", "urn:witness", "?z")],
        ),
        EvalRule::positive(
            "urn:absence-rule",
            atom("?x", "urn:absent", "?y"),
            vec![atom("?x", "urn:seed", "?y"), absent],
        ),
    ];
    let program = prepare(
        &rules,
        &[producer(
            vec![atom("?x", "urn:seed", "?y")],
            vec![atom("?x", "urn:witness", "?z")],
        )],
    );
    let result = run(&program, &[WORLD], None);
    assert_eq!(result.result.progress.completed, 2);
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|r| r.predicate == "urn:present")
    );
    assert!(
        !result
            .result
            .rows
            .iter()
            .any(|r| r.predicate == "urn:absent")
    );
}

#[test]
fn negative_cycle_through_existential_head_is_refused() {
    let mut negative = atom("?x", "urn:witness", "?y");
    negative.negated = true;
    let rules = [EvalRule::positive(
        "urn:cycle",
        atom("?x", "urn:ready", "?y"),
        vec![atom("?x", "urn:seed", "?y"), negative],
    )];
    let result = JointProgram::prepare(
        &rules,
        &[producer(
            vec![atom("?x", "urn:ready", "?y")],
            vec![atom("?x", "urn:witness", "?z")],
        )],
    )
    .unwrap();
    assert!(matches!(
        result,
        NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable)
    ));
}

#[test]
fn termination_certificate_includes_ordinary_feedback_edges() {
    let existential = producer(
        vec![atom("?x", "urn:seed", "?y")],
        vec![atom("?x", "urn:witness", "?z")],
    );
    assert!(ChaseAdmission::certify(std::slice::from_ref(&existential)).admits_native());
    let program = prepare(
        &[EvalRule::positive(
            "urn:recur",
            atom("?z", "urn:seed", "urn:b"),
            vec![atom("?x", "urn:witness", "?z")],
        )],
        &[existential],
    );
    assert!(!program.admission.admits_native());
    assert!(matches!(
        program.materialize(&seed(&[WORLD]), None).unwrap(),
        NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential)
    ));
    let partial = run(&program, &[WORLD], Some(8));
    assert_eq!(partial.result.status, BudgetStatus::Exhausted);
    assert!(partial.result.consumed_steps <= 8);
}

#[test]
fn joint_rounds_are_deterministic_across_worker_counts() {
    let program = feedback();
    let execute = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| run(&program, &[WORLD], None))
    };
    let one = execute(1);
    let four = execute(4);
    assert!(super::super::same_budgeted_rows(&one.result, &four.result));
    assert_eq!(one.witness_derivations, four.witness_derivations);
}

#[test]
fn implicit_dimension_reads_wait_for_their_producers() {
    use crate::native_semantics::SemanticVocabulary;
    use crate::query_ir::{QBuiltin, QTerm};
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
    const DIMENSIONLESS: &str = "https://blackcatinformatics.ca/math/Dimensionless";
    for predicate in [RDF_TYPE, INSTANCE] {
        let typing = EvalRule::positive(
            "urn:dimension-typing",
            atom("urn:dimension", predicate, DIMENSIONLESS),
            vec![atom("?x", "urn:seed", "?y")],
        );
        // This is the actual head shape emitted by lower_constraint_violation_rules.
        let mut violation = EvalRule::positive(
            "urn:dimension-law",
            atom("?x", RDF_TYPE, "urn:FailureClass"),
            vec![atom("?x", "urn:seed", "?y")],
        );
        violation.constraint_tag = Some("urn:dimension-law".to_owned());
        violation.builtins.push(QBuiltin::DimEqual {
            d1: QTerm::Const("<https://blackcatinformatics.ca/math/lengthDimension>".to_owned()),
            d2: QTerm::Const("<urn:dimension>".to_owned()),
        });
        let rules = [typing, violation];
        let NativeOutcome::Decided(program) = JointProgram::prepare_with_semantics(
            &rules,
            &[],
            &[],
            &BTreeSet::new(),
            SemanticVocabulary::GroundedLogicV1,
        )
        .unwrap() else {
            panic!("unrelated failure markers cannot change dimension values")
        };
        assert!(
            program.admission.admits_native(),
            "a bound filter does not invent values"
        );
        let result = run(&program, &[WORLD], None);
        assert_eq!(result.result.progress.completed, 2);
        let failure = result
            .result
            .rows
            .iter()
            .find(|row| {
                row.predicate == RDF_TYPE && row.object == TermValue::iri("urn:FailureClass")
            })
            .expect("completed dimension comparison must publish its failure marker");
        assert_eq!(failure.subject, TermValue::iri("urn:a"));
        assert_eq!(failure.graph, WORLD);
        assert!(
            failure
                .antecedents
                .iter()
                .any(|fact| fact.predicate == "urn:seed")
        );
        let partial = run(&program, &[WORLD], Some(1));
        assert_eq!(partial.result.status, BudgetStatus::Exhausted);
        assert_eq!(partial.result.progress.completed, 1);
        for spelling in [RDF_TYPE, INSTANCE] {
            assert!(!partial.result.progress.saturated_preds.contains(spelling));
            assert!(result.result.progress.saturated_preds.contains(spelling));
        }
        // The ordinary forward path must retain the same last-writer frontier.
        if predicate == RDF_TYPE {
            let exe = crate::physical::plan::Parsed::uncached(&rules)
                .stratify()
                .unwrap()
                .plan()
                .into_executable();
            assert_eq!(exe.stratum_count(), 2);
            assert!(!exe.stratum_head_predicates(0).any(|p| p == RDF_TYPE));
            assert!(exe.stratum_head_predicates(1).any(|p| p == RDF_TYPE));
            let NativeOutcome::Decided(ordinary) =
                crate::physical::seminaive::materialize_native(&seed(&[WORLD]), &exe, Some(1))
                    .unwrap()
            else {
                panic!("ordinary constraint schedule must be admitted")
            };
            assert_eq!(ordinary.status, BudgetStatus::Exhausted);
            assert!(!ordinary.progress.saturated_preds.contains(RDF_TYPE));
        }
    }
}

#[test]
fn dynamic_completion_preserves_later_fixed_writers() {
    let dynamic = property(
        "urn:dynamic",
        ["urn:signal", "?p", "urn:failure"],
        &[["urn:select", "urn:predicate", "?p"]],
    );
    let mut absent = atom("?x", "urn:absent", "?y");
    absent.negated = true;
    let fixed = EvalRule::positive(
        "urn:fixed",
        atom("?x", "urn:tracked", "?y"),
        vec![atom("?x", "urn:seed", "?y"), absent],
    );
    let NativeOutcome::Decided(program) =
        JointProgram::prepare_with_properties(&[fixed], &[], &[dynamic], &BTreeSet::new()).unwrap()
    else {
        panic!("data-selected and fixed writes must be schedulable")
    };
    let source = seed(&[WORLD]);
    source.insert_quad(WORLD, "urn:select", "urn:predicate", "urn:tracked");
    let NativeOutcome::Decided(partial) = program.materialize(&source, Some(1)).unwrap() else {
        panic!("bounded native execution must be admitted")
    };
    assert_eq!(partial.result.status, BudgetStatus::Exhausted);
    assert_eq!(partial.result.progress.completed, 1);
    assert!(
        !partial
            .result
            .progress
            .saturated_preds
            .contains("urn:tracked")
    );
    assert!(
        partial
            .result
            .progress
            .saturated_preds
            .contains("urn:predicate")
    );
    let NativeOutcome::Decided(complete) = program.materialize(&source, None).unwrap() else {
        panic!("finite native execution must be admitted")
    };
    assert_eq!(complete.result.status, BudgetStatus::Ok);
    assert!(
        complete
            .result
            .progress
            .saturated_preds
            .contains("urn:tracked")
    );
    assert_eq!(
        complete
            .result
            .rows
            .iter()
            .filter(|row| row.predicate == "urn:tracked")
            .count(),
        2
    );
}

#[test]
fn shared_ordinary_commit_preserves_parallel_and_budget_parity() {
    crate::cost::run_rule_parallel_evidence().expect(
        "native ordinary closure and every budget cut must match forced sequential execution",
    );
}
