// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lowering the **one canonical AST** ([`gmeow_logic_compile::ir`]) directly into the
//! evaluable rule IR ([`crate::rule_ir::EvalRule`]) — the AST-unification keystone.
//!
//! [`lower_eval_rules`]
//! derives the evaluable rules **straight from the canonical source AST**, so the
//! canonical IR is the single definitional source and the runtime forms are mere
//! views of it.
#![allow(dead_code)]

use purrdf::TermValue;

use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};
use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, LogicAxiom, LogicProgram, LogicRule};

/// Lower one canonical source atom to an [`EvalAtom`] (arity-3 world slot is not
/// part of the source IR, so nothing is dropped — subject/predicate/object map
/// 1:1).  `?`-prefixed terms become variables, a literal object becomes a plain
/// `xsd:string` `ConstLit`, every other term an IRI constant.
fn lower_atom(atom: &LogicAxiom, negated: bool) -> gmeow_errors::Result<EvalAtom> {
    let predicate = atom.predicate.clone();
    let subject = lower_term(&atom.subject, false, "subject")?;
    let object = lower_term(&atom.obj, atom.obj_is_literal, "object")?;
    Ok(EvalAtom {
        subject,
        predicate,
        object,
        negated,
    })
}

fn lower_term(value: &str, is_literal: bool, slot: &str) -> gmeow_errors::Result<EvalTerm> {
    if value.starts_with('?') {
        return Ok(EvalTerm::Var(value.to_owned()));
    }
    if is_literal {
        if slot != "object" {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Lower {
                detail: format!(
                    "compile::lower: literal in {slot} position {value:?} — only an object may be \
                     a literal"
                ),
            }));
        }
        return Ok(EvalTerm::ConstLit(TermValue::simple_literal(value)));
    }
    Ok(EvalTerm::ConstNamed(value.to_owned()))
}

/// Lower a single canonical [`LogicRule`] to an [`EvalRule`], with the same body
/// ordering the reparse yields (positive atoms first, then negated), the same
/// `rule_iri` (`scope.provenance` or the synthesized anonymous IRI), and — unlike
/// the reparse — the rule's `distinct_pairs` preserved.
pub(crate) fn lower_rule(rule: &LogicRule) -> gmeow_errors::Result<EvalRule> {
    let head = lower_atom(&rule.head, false)?;
    let mut body: Vec<EvalAtom> = Vec::new();
    for atom in rule.body.iter().filter(|a| !a.negated) {
        body.push(lower_atom(atom, false)?);
    }
    for atom in rule.body.iter().filter(|a| a.negated) {
        body.push(lower_atom(atom, true)?);
    }
    let rule_iri = rule
        .scope
        .provenance
        .clone()
        .unwrap_or_else(|| format!("{LOGIC_NAMESPACE}rule/anonymous"));
    Ok(EvalRule {
        head,
        body,
        rule_iri,
        distinct_pairs: rule.distinct_pairs.clone(),
        // This lowering carries no arithmetic builtins.
        builtins: Vec::new(),
        constraint_tag: None,
    })
}

/// Lower every rule in a canonical [`LogicProgram`] to the evaluable IR — the
/// canonical-AST-authoritative production lowering.
pub(crate) fn lower_eval_rules(program: &LogicProgram) -> gmeow_errors::Result<Vec<EvalRule>> {
    program.rules.iter().map(lower_rule).collect()
}
