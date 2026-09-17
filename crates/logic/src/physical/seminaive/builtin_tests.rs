// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Whole-solution publication and lineage through native post-join evaluation.

use super::*;
use crate::physical::builtin_eval::{NoCellResolver, Value};
use crate::query_ir::{ArithOp, CmpOp, QTerm};

fn variable(name: &str) -> QTerm {
    QTerm::Var(name.into())
}

fn solution(value: i64) -> Solution {
    Solution {
        bindings: vec![("value".into(), emit_term(&Value::Int(value)))],
        source_facts: vec![Fact {
            subject: purrdf::TermValue::iri("urn:source"),
            predicate: "urn:value".into(),
            object: emit_term(&Value::Int(value)),
        }],
    }
}

#[test]
fn native_post_join_generators_feed_later_filters_and_retain_lineage() {
    let builtins = [
        QBuiltin::Is {
            target: variable("result"),
            lhs: QTerm::Num(12),
            op: ArithOp::Div,
            rhs: variable("value"),
        },
        QBuiltin::Compare {
            lhs: variable("result"),
            op: CmpOp::Gt,
            rhs: QTerm::Num(3),
        },
    ];
    let mut gap = Vec::new();
    let mut input = Vec::with_capacity(8);
    input.extend([solution(2), solution(4), solution(3)]);
    let allocation = input.as_ptr();
    let output = apply_builtins(&builtins, input, &mut gap, &NoCellResolver, false);
    assert!(gap.is_empty());
    assert_eq!(output.len(), 2);
    assert_eq!(
        output.as_ptr(),
        allocation,
        "post-join filtering retains the existing allocation"
    );
    for (row, (source, result)) in output.iter().zip([(2, 6), (3, 4)]) {
        assert_eq!(row.get("result"), Some(&emit_term(&Value::Int(result))));
        assert_eq!(row.source_facts, solution(source).source_facts);
    }
}

#[test]
fn a_later_ordinary_builtin_gap_discards_all_prior_candidates() {
    let builtin = QBuiltin::Is {
        target: variable("result"),
        lhs: QTerm::Num(12),
        op: ArithOp::Div,
        rhs: variable("value"),
    };
    let mut gaps = Vec::new();
    let output = apply_builtins(
        &[builtin],
        vec![solution(2), solution(0), solution(3)],
        &mut gaps,
        &NoCellResolver,
        false,
    );
    assert!(output.is_empty());
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].op, "result is 12 // value");
    assert_eq!(
        gaps[0].bindings,
        [(
            "value".into(),
            crate::provenance::term_display(&emit_term(&Value::Int(0)))
        )]
    );
}

#[test]
fn tagged_violation_filters_retain_only_false_defined_candidates() {
    let builtin = QBuiltin::Compare {
        lhs: variable("value"),
        op: CmpOp::Gt,
        rhs: QTerm::Num(3),
    };
    let mut missing = solution(9);
    missing.bindings.clear();
    let mut gaps = Vec::new();
    let output = apply_builtins(
        &[builtin],
        vec![solution(2), missing, solution(4)],
        &mut gaps,
        &NoCellResolver,
        true,
    );
    assert!(gaps.is_empty());
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].source_facts, solution(2).source_facts);
}

#[test]
fn forward_and_demand_execution_refuse_a_failed_round_before_committing() {
    let mut rule = EvalRule::positive(
        "urn:divide-rule",
        EvalAtom::positive(
            EvalTerm::Var("?s".into()),
            "urn:result",
            EvalTerm::Var("result".into()),
        ),
        vec![EvalAtom::positive(
            EvalTerm::Var("?s".into()),
            "urn:value",
            EvalTerm::Var("value".into()),
        )],
    );
    rule.builtins.push(QBuiltin::Is {
        target: variable("result"),
        lhs: QTerm::Num(12),
        op: ArithOp::Div,
        rhs: variable("value"),
    });
    let sibling = EvalRule::positive(
        "urn:successful-sibling",
        EvalAtom::positive(
            EvalTerm::Var("?s".into()),
            "urn:copied",
            EvalTerm::Var("value".into()),
        ),
        rule.body.clone(),
    );
    let exe = crate::physical::plan::compile_cached(
        "native-builtin-failure",
        vec![rule.clone(), sibling],
    )
    .executable
    .unwrap();
    let facts = [
        solution(2).source_facts.remove(0),
        solution(0).source_facts.remove(0),
    ];
    let world = crate::store::WorldStore::new();
    for fact in &facts {
        world
            .insert_quad_terms(
                "urn:world",
                fact.subject.clone(),
                purrdf::TermValue::iri(&fact.predicate),
                fact.object.clone(),
            )
            .unwrap();
    }
    // The joint finite rule copies a body value; the arithmetic binding remains
    // a checked body operation rather than recursive numeric value invention.
    rule.head.object = EvalTerm::Var("value".into());
    let NativeOutcome::Decided(joint) = joint::JointProgram::prepare(&[rule], &[]).unwrap() else {
        panic!("finite copied-value rule must admit joint preparation")
    };
    for budget in [None, Some(10)] {
        let error = joint
            .materialize(&world, budget)
            .err()
            .expect("joint numeric gap must refuse publication");
        assert!(
            error.message().contains("ZeroDivisor"),
            "{}",
            error.message()
        );
        let error = materialize_native(&world, &exe, budget).unwrap_err();
        assert!(
            error.message().contains("ZeroDivisor"),
            "{}",
            error.message()
        );
        assert!(
            error.message().contains("result is 12 // value"),
            "{}",
            error.message()
        );
        let mut relation = RelationStore::new();
        for fact in &facts {
            relation.insert(&fact.predicate, &fact.subject, &fact.object);
        }
        let result = evaluate(relation, &exe, budget).unwrap();
        let NativeOutcome::Unsupported(UnsupportedKind::Arithmetic(gaps)) = result else {
            panic!("a failed numeric round cannot publish an answer")
        };
        assert_eq!(gaps.len(), 1);
        assert!(gaps[0].op.contains("12 // value"));
    }
    let mut governor = StepGovernor::new(None);
    assert!(
        eval_world_stratified(
            &facts,
            &exe,
            &mut governor,
            ProvenanceMode::Record,
            RoundExecution::Sequential
        )
        .is_err()
    );
    assert_eq!(
        governor.consumed, 0,
        "a failed round commits no successful sibling candidate"
    );
}
