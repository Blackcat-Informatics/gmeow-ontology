// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::sparql::CancellationFlag;

mod temporal;

#[derive(Default)]
struct Model {
    contexts: BTreeMap<String, Context>,
    atoms: BTreeMap<String, Evidence>,
    edges: BTreeMap<(String, Accessibility), Successors>,
    traces: BTreeMap<String, TemporalTrace>,
}

impl Model {
    fn world(&mut self, world: &str) {
        self.contexts.insert(
            world.into(),
            Context {
                world: world.into(),
                standpoint: "urn:shared-standpoint".into(),
                enactment: None,
                journal_position: None,
                norm_scope: None,
                protocol_scope: None,
            },
        );
    }

    fn edge(&mut self, source: &str, axis: usize, target: &str, closed: bool) {
        let set = self
            .edges
            .entry((source.into(), Accessibility(axis)))
            .or_default();
        set.transitions.push(Transition {
            destination: target.into(),
            witness: format!("urn:edge:{source}:{axis}:{target}"),
        });
        set.closure_witness = closed.then(|| format!("urn:closed:{source}:{axis}"));
    }

    fn evidence(&mut self, world: &str, support: bool, opposition: bool) {
        self.atoms.insert(
            world.into(),
            Evidence {
                support: support.then(|| format!("{world}/positive-witness")),
                opposition: opposition.then(|| format!("{world}/negative-witness")),
                complete: true,
            },
        );
    }
}

impl Frame for Model {
    fn temporal(&self, context: &str) -> Result<TemporalTrace, AdmissionError> {
        let mut trace =
            self.traces.get(context).cloned().ok_or_else(|| {
                AdmissionError::Malformed("missing explicit synthetic trace".into())
            })?;
        trace.points.truncate(2);
        Ok(trace)
    }
    fn context(&self, identity: &str) -> Result<&Context, AdmissionError> {
        self.contexts
            .get(identity)
            .ok_or_else(|| AdmissionError::Malformed("missing context".into()))
    }
    fn atom(&self, context: &str, _: &str, _: &Term, _: &Term) -> Result<Evidence, AdmissionError> {
        Ok(self.atoms.get(context).cloned().unwrap_or(Evidence {
            complete: true,
            ..Evidence::default()
        }))
    }
    fn successors(&self, context: &str, axis: Accessibility) -> Result<Successors, AdmissionError> {
        Ok(self
            .edges
            .get(&(context.into(), axis))
            .cloned()
            .unwrap_or_default())
    }
}

fn atom(world: Term) -> Formula {
    Formula::Atom {
        relation: Term::Iri("urn:predicate".into()),
        args: vec![
            world,
            Term::Iri("urn:subject".into()),
            Term::Iri("urn:object".into()),
        ],
    }
}

fn necessity(source: Term, variable: &str, axis: usize, body: Formula) -> Formula {
    Formula::Forall {
        vars: vec![variable.into()],
        body: Box::new(Formula::Implies(
            Box::new(Formula::Atom {
                relation: Term::Iri(TYPED_ACCESSIBILITY[axis].into()),
                args: vec![source, Term::Var(variable.into())],
            }),
            Box::new(body),
        )),
    }
}

fn run(formula: &Formula, model: &Model, world: &str) -> Evaluation {
    Program::lower(formula, world)
        .expect("admitted formula")
        .evaluate(model, world, None, None)
        .expect("admitted model")
}

#[test]
fn typed_edges_preserve_every_unselected_coordinate() {
    let mut model = Model::default();
    model.world("urn:w0");
    model.world("urn:w1");
    let base = model.contexts["urn:w0"].clone();
    let mut next = model.contexts["urn:w1"].clone();
    assert!(base.validate_transition(Accessibility(0), &next).is_ok());
    next.standpoint = "urn:another-standpoint".into();
    assert!(base.validate_transition(Accessibility(0), &next).is_err());
    assert!(base.validate_transition(Accessibility(5), &next).is_err());
    next.world.clone_from(&base.world);
    assert!(base.validate_transition(Accessibility(5), &next).is_ok());
    next.protocol_scope = Some(ProtocolScope {
        protocol: "urn:protocol".into(),
        role: "urn:role".into(),
    });
    assert!(base.validate_transition(Accessibility(5), &next).is_err());
    let mut at_step = base;
    at_step.enactment = Some("urn:enactment".into());
    at_step.journal_position = Some(("urn:journal".into(), 1));
    next = at_step.clone();
    next.journal_position = Some(("urn:journal".into(), 2));
    assert!(at_step.validate_transition(Accessibility(3), &next).is_ok());
    assert!(
        at_step
            .validate_transition(Accessibility(0), &next)
            .is_err()
    );
    next.journal_position = Some(("urn:another-journal".into(), 2));
    assert!(
        at_step
            .validate_transition(Accessibility(3), &next)
            .is_err()
    );
    assert!(
        at_step
            .validate_transition(Accessibility(3), &at_step)
            .is_err()
    );
}

#[test]
fn modal_order_selects_different_contexts() {
    let mut model = Model::default();
    for world in ["urn:w0", "urn:w1", "urn:w2", "urn:w3"] {
        model.world(world);
    }
    model.edge("urn:w0", 0, "urn:w1", true);
    model.edge("urn:w1", 1, "urn:w2", true);
    model.edge("urn:w0", 1, "urn:w3", true);
    model.edge("urn:w3", 0, "urn:w3", true);
    model.evidence("urn:w2", true, false);
    model.evidence("urn:w3", false, true);
    let compose = |first, second| {
        necessity(
            Term::Iri("urn:w0".into()),
            "u",
            first,
            necessity(
                Term::Var("u".into()),
                "v",
                second,
                atom(Term::Var("v".into())),
            ),
        )
    };
    let forward = run(&compose(0, 1), &model, "urn:w0");
    let reverse = run(&compose(1, 0), &model, "urn:w0");
    assert!(forward.evidence.support.is_some() && forward.evidence.opposition.is_none());
    assert!(reverse.evidence.support.is_none() && reverse.evidence.opposition.is_some());
    assert!(forward.evidence.complete && reverse.evidence.complete);
}

#[test]
fn contradiction_and_missing_information_remain_local() {
    let mut model = Model::default();
    model.world("urn:w0");
    model.world("urn:w1");
    model.evidence("urn:w0", true, true);
    let both = run(
        &Formula::Not(Box::new(atom(Term::Iri("urn:w0".into())))),
        &model,
        "urn:w0",
    );
    assert!(both.evidence.support.is_some() && both.evidence.opposition.is_some());
    let neither = run(&atom(Term::Iri("urn:w1".into())), &model, "urn:w1");
    assert_eq!(
        neither.evidence,
        Evidence {
            complete: true,
            ..Evidence::default()
        }
    );
}

#[test]
fn open_successors_cannot_certify_universal_support() {
    let mut model = Model::default();
    model.world("urn:w0");
    model.world("urn:w1");
    model.edge("urn:w0", 0, "urn:w1", false);
    model.evidence("urn:w1", true, false);
    let formula = necessity(
        Term::Iri("urn:w0".into()),
        "u",
        0,
        atom(Term::Var("u".into())),
    );
    let open = run(&formula, &model, "urn:w0");
    assert_eq!(open.evidence, Evidence::default());
    assert!(
        !open
            .inferences
            .iter()
            .any(|proof| proof.rule.ends_with("necessity-support"))
    );
    model.evidence("urn:w1", true, true);
    let counterexample = run(&formula, &model, "urn:w0");
    assert!(counterexample.evidence.opposition.is_some());
    assert!(!counterexample.evidence.complete);
}

#[test]
fn deontic_scope_is_required_and_empty_ideals_are_not_vacuous() {
    let mut model = Model::default();
    model.world("urn:w0");
    let formula = necessity(
        Term::Iri("urn:w0".into()),
        "u",
        2,
        atom(Term::Var("u".into())),
    );
    let program = Program::lower(&formula, "urn:w0").unwrap();
    assert!(matches!(
        program.evaluate(&model, "urn:w0", None, None),
        Err(AdmissionError::Malformed(_))
    ));
    model.contexts.get_mut("urn:w0").unwrap().norm_scope = Some(NormScope {
        issuer: "urn:issuer".into(),
        bearer: "urn:bearer".into(),
        policy: "urn:policy".into(),
    });
    model.edges.insert(
        ("urn:w0".into(), Accessibility(2)),
        Successors {
            transitions: vec![],
            closure_witness: Some("urn:closed-empty".into()),
        },
    );
    let empty = program.evaluate(&model, "urn:w0", None, None).unwrap();
    assert_eq!(empty.evidence, Evidence::default());
}

#[test]
fn budgets_and_cancellation_do_not_become_semantic_absence() {
    let mut model = Model::default();
    model.world("urn:w0");
    model.evidence("urn:w0", true, false);
    let program = Program::lower(&atom(Term::Iri("urn:w0".into())), "urn:w0").unwrap();
    let cut = program.evaluate(&model, "urn:w0", Some(0), None).unwrap();
    assert_eq!(cut.interrupted, Some(IncompleteCause::StepBudget));
    assert_eq!(cut.consumed, 0);
    assert_eq!(cut.evidence, Evidence::default());
    let exact = program.evaluate(&model, "urn:w0", Some(1), None).unwrap();
    assert_eq!(exact.consumed, 1);
    assert!(exact.interrupted.is_none() && exact.evidence.support.is_some());
    let stop = CancellationFlag::new();
    stop.cancel();
    let cancelled = program
        .evaluate(&model, "urn:w0", None, Some(&stop))
        .unwrap();
    assert_eq!(cancelled.interrupted, Some(IncompleteCause::Cancelled));
    assert_eq!(cancelled.consumed, 0);
}

#[test]
fn attribution_changes_the_derived_assessment_identity() {
    let mut model = Model::default();
    model.world("urn:w0");
    model.evidence("urn:w0", true, false);
    let formula = atom(Term::Iri("urn:w0".into()));
    let first = run(&formula, &model, "urn:w0");
    model.contexts.get_mut("urn:w0").unwrap().standpoint = "urn:rival-standpoint".into();
    let second = run(&formula, &model, "urn:w0");
    assert_ne!(first.evidence.support, second.evidence.support);
}

#[test]
fn explicit_context_selection_does_not_read_the_enclosing_world() {
    let mut model = Model::default();
    model.world("urn:w0");
    model.world("urn:w1");
    model.evidence("urn:w0", false, true);
    model.evidence("urn:w1", true, false);
    // A constant context term in the standard translation is an explicit
    // logic:inContext selection, even inside an expression assessed at w0.
    let program = Program::lower(&atom(Term::Iri("urn:w1".into())), "urn:w0").unwrap();
    let evaluation = program.evaluate(&model, "urn:w0", None, None).unwrap();
    assert!(evaluation.evidence.support.is_some() && evaluation.evidence.opposition.is_none());
    assert!(
        evaluation
            .inferences
            .iter()
            .any(|proof| proof.context == "urn:w1")
    );
    assert!(
        evaluation
            .inferences
            .iter()
            .any(|proof| proof.rule.ends_with("context-selection-support"))
    );
}

#[test]
fn a_general_quantifier_and_wrong_world_cannot_enter_the_fragment() {
    let formula = Formula::Forall {
        vars: vec!["person".into()],
        body: Box::new(atom(Term::Iri("urn:w0".into()))),
    };
    assert!(matches!(
        Program::lower(&formula, "urn:w0"),
        Err(AdmissionError::OutsideFragment(_))
    ));
    let mut model = Model::default();
    model.world("urn:w1");
    let program = Program::lower(&atom(Term::Iri("urn:w0".into())), "urn:w0").unwrap();
    assert!(matches!(
        program.evaluate(&model, "urn:w1", None, None),
        Err(AdmissionError::Malformed(_))
    ));
}

#[test]
fn all_sixteen_information_pairs_obey_bilateral_connectives() {
    for left in 0_u8..4 {
        for right in 0_u8..4 {
            let mut model = Model::default();
            model.world("urn:left");
            model.world("urn:right");
            model.evidence("urn:left", left & 1 != 0, left & 2 != 0);
            model.evidence("urn:right", right & 1 != 0, right & 2 != 0);
            let a = atom(Term::Iri("urn:left".into()));
            let b = atom(Term::Iri("urn:right".into()));
            for (formula, positive, negative) in [
                (
                    Formula::And(vec![a.clone(), b.clone()]),
                    left & right & 1 != 0,
                    (left | right) & 2 != 0,
                ),
                (
                    Formula::Or(vec![a.clone(), b.clone()]),
                    (left | right) & 1 != 0,
                    left & right & 2 != 0,
                ),
                (
                    Formula::Implies(Box::new(a.clone()), Box::new(b.clone())),
                    left & 2 != 0 || right & 1 != 0,
                    left & 1 != 0 && right & 2 != 0,
                ),
            ] {
                let result = run(&formula, &model, "urn:left");
                assert_eq!(
                    (
                        result.evidence.support.is_some(),
                        result.evidence.opposition.is_some()
                    ),
                    (positive, negative),
                    "{left} {right} {formula:?}"
                );
                assert!(result.evidence.complete);
            }
        }
    }
}

fn possibility(source: Term, variable: &str, axis: usize, body: Formula) -> Formula {
    Formula::Exists {
        vars: vec![variable.into()],
        body: Box::new(Formula::And(vec![
            Formula::Atom {
                relation: Term::Iri(TYPED_ACCESSIBILITY[axis].into()),
                args: vec![source, Term::Var(variable.into())],
            },
            body,
        ])),
    }
}

#[test]
fn possibility_and_necessity_distinguish_open_and_closed_empty_inventories() {
    let mut model = Model::default();
    model.world("urn:w0");
    model.world("urn:w1");
    let box_formula = necessity(
        Term::Iri("urn:w0".into()),
        "u",
        0,
        atom(Term::Var("u".into())),
    );
    let diamond_formula = possibility(
        Term::Iri("urn:w0".into()),
        "u",
        0,
        atom(Term::Var("u".into())),
    );
    for formula in [&box_formula, &diamond_formula] {
        assert_eq!(run(formula, &model, "urn:w0").evidence, Evidence::default());
    }
    model.edges.insert(
        ("urn:w0".into(), Accessibility(0)),
        Successors {
            transitions: vec![],
            closure_witness: Some("urn:closed".into()),
        },
    );
    let universal = run(&box_formula, &model, "urn:w0");
    let existential = run(&diamond_formula, &model, "urn:w0");
    assert!(universal.evidence.support.is_some() && universal.evidence.opposition.is_none());
    assert!(existential.evidence.support.is_none() && existential.evidence.opposition.is_some());
    assert!(universal.evidence.complete && existential.evidence.complete);
    model.edge("urn:w0", 0, "urn:w1", false);
    model.evidence("urn:w1", true, true);
    let open = run(&diamond_formula, &model, "urn:w0");
    assert!(open.evidence.support.is_some() && open.evidence.opposition.is_none());
    assert!(!open.evidence.complete);
}

#[test]
fn shared_dag_work_is_charged_once_and_uncommitted_parent_proofs_are_removed() {
    let mut model = Model::default();
    model.world("urn:w0");
    model.evidence("urn:w0", true, false);
    let atomic = atom(Term::Iri("urn:w0".into()));
    let formula = Formula::And(vec![atomic.clone(), atomic]);
    let program = Program::lower(&formula, "urn:w0").unwrap();
    assert_eq!(program.instructions.len(), 2);
    let exact = program.evaluate(&model, "urn:w0", Some(2), None).unwrap();
    assert!(exact.interrupted.is_none() && exact.evidence.support.is_some());
    assert_eq!(exact.consumed, 2);
    let cut = program.evaluate(&model, "urn:w0", Some(1), None).unwrap();
    assert_eq!(cut.interrupted, Some(IncompleteCause::StepBudget));
    assert_eq!(cut.consumed, 1);
    assert_eq!(cut.evidence, Evidence::default());
    assert_eq!(cut.inferences.len(), 1);
    assert!(cut.inferences[0].rule.ends_with("atomic-support"));
    assert_eq!(cut.anchors.len(), 1);
    assert_ne!(cut.anchors[0].instruction, program.root.0);
}

#[test]
fn equivalent_operand_orders_and_world_binders_have_identical_proofs_and_budgets() {
    let mut model = Model::default();
    for world in ["urn:w0", "urn:w1", "urn:w2"] {
        model.world(world);
        model.evidence(world, true, false);
    }
    model.edge("urn:w0", 0, "urn:w1", true);
    model.edge("urn:w0", 1, "urn:w2", true);
    let make = |variable: &str, axis| {
        necessity(
            Term::Iri("urn:w0".into()),
            variable,
            axis,
            atom(Term::Var(variable.into())),
        )
    };
    let first = Formula::And(vec![make("a", 0), make("b", 1)]);
    let second = Formula::And(vec![make("renamed_second", 1), make("renamed_first", 0)]);
    let left = Program::lower(&first, "urn:w0").unwrap();
    let right = Program::lower(&second, "urn:w0").unwrap();
    assert_eq!(left.formula_key, right.formula_key);
    assert_eq!(left.instructions, right.instructions);
    for budget in 0..=5 {
        let a = left.evaluate(&model, "urn:w0", Some(budget), None).unwrap();
        let b = right
            .evaluate(&model, "urn:w0", Some(budget), None)
            .unwrap();
        assert_eq!(a.evidence, b.evidence);
        assert_eq!(a.consumed, b.consumed);
        assert_eq!(a.interrupted, b.interrupted);
        assert_eq!(a.inferences, b.inferences);
        assert_eq!(a.anchors, b.anchors);
    }
}
