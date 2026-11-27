// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-owned positive denials for native consistency proofs.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::frontend::FormulaDisposition;
use gmeow_logic_compile::ir::{ConstraintIr, Formula, LOGIC_NAMESPACE, Term};
use gmeow_logic_compile::source_inventory::SelectedProgramView;
use purrdf::{RdfDataset, TermRef};

use super::accounting::{AtomPolarity, account_formula};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, literal_value};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const DENIAL_SHAPE: &str = "expected a class guard and one forbidden binary atom on its focus";

/// Compile a positive denial to a rule whose conclusion witnesses a contradiction.
pub(crate) fn lower_forbidden_pattern(constraint: &ConstraintIr) -> gmeow_errors::Result<EvalRule> {
    let refusal = |detail| gmeow_errors::Diag::of_kind(crate::error::Lower { detail });
    let (class, predicate, value) =
        denial_atoms(&constraint.integrity).ok_or_else(|| refusal(DENIAL_SHAPE.to_owned()))?;
    let mut refusals = Vec::new();
    account_formula(&constraint.integrity, AtomPolarity::Positive, &mut refusals);
    if !refusals.is_empty() {
        return Err(refusal(refusals.join("; ")));
    }

    let focus = EvalTerm::var("?focus");
    Ok(denial_rule(
        "forbidden-pattern",
        &constraint.iri,
        focus.clone(),
        vec![
            EvalAtom::positive(focus.clone(), RDF_TYPE, EvalTerm::named(class)),
            EvalAtom::positive(focus, predicate, value),
        ],
    ))
}

pub(crate) struct SelectedDenial {
    pub(crate) owner: String,
    pub(crate) rule: EvalRule,
    pub(crate) formula_index: Option<usize>,
}

/// Lower exactly the selected denial owners. A shared or anonymous formula
/// root remains in the general lowering so its source boundary is not lost.
pub(crate) fn selected_denials(
    view: SelectedProgramView<'_, '_>,
    dataset: &RdfDataset,
) -> Vec<SelectedDenial> {
    let mut formula_owners = BTreeMap::new();
    for (_, occurrence) in view.formula_lowerings() {
        let FormulaDisposition::Formula(index) = occurrence.disposition else {
            continue;
        };
        let owner = match dataset.resolve(occurrence.source.term) {
            TermRef::Iri(iri) if occurrence.source.graph.is_none() => Some(iri),
            _ => None,
        };
        formula_owners
            .entry(index)
            .and_modify(|prior| *prior = None)
            .or_insert(owner);
    }
    view.forbidden_patterns()
        .filter_map(|(_, constraint)| {
            Some(SelectedDenial {
                owner: constraint.iri.clone(),
                rule: lower_forbidden_pattern(constraint).ok()?,
                formula_index: None,
            })
        })
        .chain(view.formulas().filter_map(|(index, formula)| {
            let owner = formula_owners.get(&index).copied().flatten()?;
            Some(SelectedDenial {
                owner: owner.to_owned(),
                rule: lower_formula_denial(owner, formula)?,
                formula_index: Some(index),
            })
        }))
        .collect()
}

/// `forall x. (A(x) -> not B(x))` denies the positive join of A and B.
/// Only a bound, binary, positive body enters this source-proof route.
fn lower_formula_denial(owner: &str, formula: &Formula) -> Option<EvalRule> {
    let Formula::Forall { vars, body } = formula else {
        return None;
    };
    let bound: BTreeSet<_> = vars.iter().map(String::as_str).collect();
    if bound.is_empty() || bound.len() != vars.len() {
        return None;
    }
    let Formula::Implies(guard, consequent) = body.as_ref() else {
        return None;
    };
    let Formula::Not(prohibited) = consequent.as_ref() else {
        return None;
    };
    let prohibited = positive_formula_atom(prohibited, &bound)?;
    let focus = prohibited.subject.clone();
    let guards = match guard.as_ref() {
        Formula::And(parts) if !parts.is_empty() => parts.as_slice(),
        atom => std::slice::from_ref(atom),
    };
    let mut body = guards
        .iter()
        .map(|atom| positive_formula_atom(atom, &bound))
        .collect::<Option<Vec<_>>>()?;
    body.push(prohibited);

    let mut refusals = Vec::new();
    account_formula(formula, AtomPolarity::Positive, &mut refusals);
    refusals
        .is_empty()
        .then(|| denial_rule("formula-denial", owner, focus, body))
}

fn positive_formula_atom(formula: &Formula, bound: &BTreeSet<&str>) -> Option<EvalAtom> {
    let Formula::Atom {
        relation: Term::Iri(predicate),
        args,
    } = formula
    else {
        return None;
    };
    let [subject, object] = args.as_slice() else {
        return None;
    };
    let object = match object {
        Term::Literal(value) => EvalTerm::ConstLit(literal_value(value)),
        term => resource_or_bound(term, bound)?,
    };
    Some(EvalAtom::positive(
        resource_or_bound(subject, bound)?,
        predicate,
        object,
    ))
}

fn resource_or_bound(term: &Term, bound: &BTreeSet<&str>) -> Option<EvalTerm> {
    match term {
        Term::Iri(iri) => Some(EvalTerm::named(iri)),
        Term::Var(name) if bound.contains(name.as_str()) => Some(EvalTerm::Var(format!("?{name}"))),
        _ => None,
    }
}

fn denial_rule(family: &str, owner: &str, focus: EvalTerm, body: Vec<EvalAtom>) -> EvalRule {
    let digest = purrdf_hash::blake3::hash(owner.as_bytes());
    let rule_iri = format!(
        "{LOGIC_NAMESPACE}{family}/{}",
        purrdf_hash::hex::encode(digest.as_bytes())
    );
    EvalRule::positive(
        &rule_iri,
        EvalAtom::positive(
            focus,
            RDF_TYPE,
            EvalTerm::named("http://www.w3.org/2002/07/owl#Nothing"),
        ),
        body,
    )
}

fn denial_atoms(formula: &Formula) -> Option<(&str, &str, EvalTerm)> {
    let Formula::Forall { vars, body } = formula else {
        return None;
    };
    let [focus] = vars.as_slice() else {
        return None;
    };
    let Formula::Implies(guard, consequent) = body.as_ref() else {
        return None;
    };
    let (RDF_TYPE, Term::Iri(class)) = focus_atom(guard, focus)? else {
        return None;
    };
    let Formula::Not(prohibited) = consequent.as_ref() else {
        return None;
    };

    let (existential, prohibited) = match prohibited.as_ref() {
        Formula::Exists { vars, body } => {
            let [witness] = vars.as_slice() else {
                return None;
            };
            if witness == focus {
                return None;
            }
            (Some(witness), body.as_ref())
        }
        atom => (None, atom),
    };
    let (predicate, object) = focus_atom(prohibited, focus)?;
    let value = match (existential, object) {
        (Some(witness), Term::Var(variable)) if variable == witness => EvalTerm::var("?value"),
        (None, Term::Iri(iri)) => EvalTerm::named(iri),
        (None, Term::Literal(literal)) => EvalTerm::ConstLit(literal_value(literal)),
        _ => return None,
    };
    Some((class, predicate, value))
}

fn focus_atom<'a>(formula: &'a Formula, focus: &str) -> Option<(&'a str, &'a Term)> {
    let Formula::Atom {
        relation: Term::Iri(predicate),
        args,
    } = formula
    else {
        return None;
    };
    let [Term::Var(subject), object] = args.as_slice() else {
        return None;
    };
    (subject == focus).then_some((predicate, object))
}
