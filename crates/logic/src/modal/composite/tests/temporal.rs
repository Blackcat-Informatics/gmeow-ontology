// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::frontend::{FINITE_AT_OR_AFTER, FINITE_NEXT, FINITE_STRICTLY_BEFORE};

fn temporal(source: Term, variable: &str, operator: TemporalOperator, body: Formula) -> Formula {
    let guard = Formula::Atom {
        relation: Term::Iri(
            if operator == TemporalOperator::Next {
                FINITE_NEXT
            } else {
                FINITE_AT_OR_AFTER
            }
            .into(),
        ),
        args: vec![source, Term::Var(variable.into())],
    };
    if operator == TemporalOperator::Globally {
        Formula::Forall {
            vars: vec![variable.into()],
            body: Box::new(Formula::Implies(Box::new(guard), Box::new(body))),
        }
    } else {
        Formula::Exists {
            vars: vec![variable.into()],
            body: Box::new(Formula::And(vec![guard, body])),
        }
    }
}

fn until(source: Term, left: Formula, right: Formula) -> Formula {
    let v = Term::Var("v".into());
    let u = Term::Var("u".into());
    let interval = Formula::And(vec![
        Formula::Atom {
            relation: Term::Iri(FINITE_AT_OR_AFTER.into()),
            args: vec![source.clone(), u.clone()],
        },
        Formula::Atom {
            relation: Term::Iri(FINITE_STRICTLY_BEFORE.into()),
            args: vec![u, v],
        },
    ]);
    temporal(
        source,
        "v",
        TemporalOperator::Eventually,
        Formula::And(vec![
            right,
            Formula::Forall {
                vars: vec!["u".into()],
                body: Box::new(Formula::Implies(Box::new(interval), Box::new(left))),
            },
        ]),
    )
}

fn trace(evidence: &[(bool, bool)], finalized: bool) -> Model {
    let mut model = Model::default();
    let points = evidence
        .iter()
        .enumerate()
        .map(|(index, &(support, opposition))| {
            let context = format!("urn:w{index}");
            model.world(&context);
            model.evidence(&context, support, opposition);
            TemporalPoint {
                context,
                witnesses: vec![format!("urn:entry:{index}")],
            }
        })
        .collect::<Vec<_>>();
    let prefix = TemporalPrefix {
        identity: "urn:synthetic-prefix".into(),
        journal: "urn:journal".into(),
        enactment: "urn:enactment".into(),
        head: format!("urn:entry:{}", evidence.len() - 1),
        finalized,
        initial_head: "0".repeat(64),
        head_hash: "1".repeat(64),
    };
    for (index, point) in points.iter().enumerate() {
        model.traces.insert(
            point.context.clone(),
            TemporalTrace {
                points: points[index..].to_vec(),
                prefix: TemporalBasis::Journal(prefix.clone()),
            },
        );
    }
    model
}

fn formula(operator: TemporalOperator) -> Formula {
    temporal(
        Term::Iri("urn:w0".into()),
        "v",
        operator,
        atom(Term::Var("v".into())),
    )
}

fn bits(evaluation: &Evaluation) -> (bool, bool, bool) {
    (
        evaluation.evidence.support.is_some(),
        evaluation.evidence.opposition.is_some(),
        evaluation.evidence.complete,
    )
}

#[test]
fn strong_next_distinguishes_finalization_from_an_open_frontier() {
    let formula = formula(TemporalOperator::Next);
    assert_eq!(
        bits(&run(&formula, &trace(&[(true, false)], true), "urn:w0")),
        (false, true, true)
    );
    assert_eq!(
        bits(&run(&formula, &trace(&[(true, false)], false), "urn:w0")),
        (false, false, false)
    );
    assert_eq!(
        bits(&run(
            &formula,
            &trace(&[(false, true), (true, true)], false),
            "urn:w0"
        )),
        (true, true, true)
    );
}

#[test]
fn finite_unary_operators_preserve_independent_evidence_coordinates() {
    for (observed, eventually, globally) in [
        (
            vec![(true, false), (false, true)],
            (true, false, true),
            (false, true, true),
        ),
        (
            vec![(false, false), (false, true)],
            (false, false, true),
            (false, true, true),
        ),
        (vec![(true, true)], (true, true, true), (true, true, true)),
        (
            vec![(false, true), (false, true)],
            (false, true, true),
            (false, true, true),
        ),
    ] {
        let model = trace(&observed, true);
        assert_eq!(
            bits(&run(
                &formula(TemporalOperator::Eventually),
                &model,
                "urn:w0"
            )),
            eventually
        );
        assert_eq!(
            bits(&run(&formula(TemporalOperator::Globally), &model, "urn:w0")),
            globally
        );
    }
}

#[test]
fn an_open_prefix_can_witness_eventual_support_and_global_opposition() {
    let model = trace(&[(false, true), (true, false)], false);
    assert_eq!(
        bits(&run(
            &formula(TemporalOperator::Eventually),
            &model,
            "urn:w0"
        )),
        (true, false, false)
    );
    assert_eq!(
        bits(&run(&formula(TemporalOperator::Globally), &model, "urn:w0")),
        (false, true, false)
    );
}

#[test]
fn until_requires_its_left_operand_only_before_the_right_witness() {
    let formula = until(
        Term::Iri("urn:w0".into()),
        atom(Term::Var("u".into())),
        Formula::Not(Box::new(atom(Term::Var("v".into())))),
    );
    assert_eq!(
        bits(&run(
            &formula,
            &trace(&[(true, false), (false, true)], true),
            "urn:w0"
        )),
        (true, false, true)
    );
    assert_eq!(
        bits(&run(&formula, &trace(&[(false, true)], true), "urn:w0")),
        (true, false, true)
    );
    assert_eq!(
        bits(&run(
            &formula,
            &trace(&[(true, false), (true, false)], true),
            "urn:w0"
        )),
        (false, true, true)
    );
    assert_eq!(
        bits(&run(&formula, &trace(&[(true, false)], false), "urn:w0")),
        (false, false, false)
    );
}

#[test]
fn nested_temporal_queries_share_the_judgment_budget_and_memo_table() {
    let nested = temporal(
        Term::Iri("urn:w0".into()),
        "v",
        TemporalOperator::Eventually,
        temporal(
            Term::Var("v".into()),
            "u",
            TemporalOperator::Globally,
            atom(Term::Var("u".into())),
        ),
    );
    let model = trace(&[(true, false); 96], true);
    let program = Program::lower(&nested, "urn:w0").expect("temporal lowering");
    let complete = program
        .evaluate(&model, "urn:w0", None, None)
        .expect("evaluation");
    assert_eq!(bits(&complete), (true, false, true));
    assert_eq!(complete.consumed, 96 * 3);
    let bounded = program
        .evaluate(&model, "urn:w0", Some(complete.consumed - 1), None)
        .expect("bounded evaluation");
    assert_eq!(bounded.interrupted, Some(IncompleteCause::StepBudget));
    assert_eq!(bounded.consumed, complete.consumed - 1);
    assert!(
        bounded
            .anchors
            .iter()
            .all(|anchor| anchor.instruction != program.root.0 || anchor.context != "urn:w0")
    );
    assert_eq!(bounded.temporal_prefixes.len(), 1);
}

#[test]
fn cancellation_does_not_manufacture_a_temporal_boundary_verdict() {
    let model = trace(&[(true, false)], true);
    let flag = CancellationFlag::new();
    flag.cancel();
    let evaluation = Program::lower(&formula(TemporalOperator::Next), "urn:w0")
        .expect("lower")
        .evaluate(&model, "urn:w0", None, Some(&flag))
        .expect("cancelled evaluation");
    assert_eq!(evaluation.interrupted, Some(IncompleteCause::Cancelled));
    assert_eq!(evaluation.consumed, 0);
    assert!(evaluation.inferences.is_empty());
    assert!(evaluation.temporal_prefixes.is_empty());
}
