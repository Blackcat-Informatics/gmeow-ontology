// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native proof rules for the compact constraint reader's positive denials.

use gmeow_logic_compile::ir::{ConstraintIr, Formula, LOGIC_NAMESPACE, Term};

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
    let owner_digest = purrdf_hash::blake3::hash(constraint.iri.as_bytes());
    let rule_iri = format!(
        "{LOGIC_NAMESPACE}forbidden-pattern/{}",
        purrdf_hash::hex::encode(owner_digest.as_bytes())
    );
    Ok(EvalRule {
        head: EvalAtom::positive(
            focus.clone(),
            RDF_TYPE,
            EvalTerm::named("http://www.w3.org/2002/07/owl#Nothing"),
        ),
        body: vec![
            EvalAtom::positive(focus.clone(), RDF_TYPE, EvalTerm::named(class)),
            EvalAtom::positive(focus, predicate, value),
        ],
        rule_iri,
        numeric: Vec::new(),
        distinct_pairs: Vec::new(),
        builtins: Vec::new(),
        reduction: None,
        constraint_tag: None,
    })
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
