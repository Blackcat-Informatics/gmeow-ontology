// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Recognize the finite-order standard translation inside the shared formula IR.

use std::collections::BTreeMap;

use gmeow_logic_compile::frontend::{FINITE_AT_OR_AFTER, FINITE_NEXT, FINITE_STRICTLY_BEFORE};
use gmeow_logic_compile::ir::{Formula, Term};

use super::{AdmissionError, Instruction, NodeId, Quantifier, TemporalOperator, lower};

pub(super) fn is_guard(relation: &str) -> bool {
    matches!(relation, FINITE_NEXT | FINITE_AT_OR_AFTER)
}

fn atom_is(formula: &Formula, relation: &str, left: &Term, right: &Term) -> bool {
    matches!(formula, Formula::Atom { relation: Term::Iri(iri), args }
        if iri == relation && args.as_slice() == [left.clone(), right.clone()])
}

fn interval_body<'a>(
    formula: &'a Formula,
    current: &Term,
    future: &Term,
) -> Option<(&'a Formula, Term)> {
    let Formula::Forall { vars, body } = formula else {
        return None;
    };
    if vars.len() != 1 {
        return None;
    }
    let intermediate = Term::Var(vars[0].clone());
    if &intermediate == current || &intermediate == future {
        return None;
    }
    let Formula::Implies(guard, body) = body.as_ref() else {
        return None;
    };
    let Formula::And(bounds) = guard.as_ref() else {
        return None;
    };
    if bounds.len() != 2 {
        return None;
    }
    let starts = |bound: &Formula| atom_is(bound, FINITE_AT_OR_AFTER, current, &intermediate);
    let ends = |bound: &Formula| atom_is(bound, FINITE_STRICTLY_BEFORE, &intermediate, future);
    if (starts(&bounds[0]) && ends(&bounds[1])) || (starts(&bounds[1]) && ends(&bounds[0])) {
        Some((body, intermediate))
    } else {
        None
    }
}

pub(super) fn lower_temporal(
    quantifier: Quantifier,
    relation: &str,
    body: &Formula,
    current: &Term,
    future: &Term,
    instructions: &mut Vec<Instruction>,
    intern: &mut BTreeMap<String, NodeId>,
) -> Result<Instruction, AdmissionError> {
    if quantifier == Quantifier::Some
        && relation == FINITE_AT_OR_AFTER
        && let Formula::And(children) = body
        && children.len() == 2
    {
        let first = interval_body(&children[0], current, future);
        let second = interval_body(&children[1], current, future);
        let operands = match (first, second) {
            (Some((left, intermediate)), None) => Some((left, intermediate, &children[1])),
            (None, Some((left, intermediate))) => Some((left, intermediate, &children[0])),
            (Some(_), Some(_)) => {
                return Err(AdmissionError::OutsideFragment(
                    "ambiguous finite-until interval guards".into(),
                ));
            }
            (None, None) => None,
        };
        if let Some((left, intermediate, right)) = operands {
            return Ok(Instruction::Until {
                left: lower(left, &intermediate, instructions, intern)?,
                right: lower(right, future, instructions, intern)?,
            });
        }
    }
    let operator = match (quantifier, relation) {
        (Quantifier::Some, FINITE_NEXT) => TemporalOperator::Next,
        (Quantifier::Some, FINITE_AT_OR_AFTER) => TemporalOperator::Eventually,
        (Quantifier::Every, FINITE_AT_OR_AFTER) => TemporalOperator::Globally,
        _ => {
            return Err(AdmissionError::OutsideFragment(
                "the journal guard is outside the declared finite temporal operators".into(),
            ));
        }
    };
    Ok(Instruction::Temporal {
        operator,
        body: lower(body, future, instructions, intern)?,
    })
}
