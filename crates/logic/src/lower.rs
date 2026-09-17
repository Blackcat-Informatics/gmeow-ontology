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
use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, LogicAxiom, LogicProgram, LogicRule, NodeKind};

/// Lower one canonical source atom to an [`EvalAtom`] (arity-3 world slot is not
/// part of the source IR, so nothing is dropped — subject/predicate/object map
/// 1:1).  `?`-prefixed terms become variables, a literal object becomes a plain
/// `xsd:string` `ConstLit`, every other term an IRI constant.
fn lower_atom(atom: &LogicAxiom, negated: bool) -> gmeow_errors::Result<EvalAtom> {
    let predicate = atom.predicate.clone();
    let subject = lower_term(&atom.subject, false, "subject")?;
    let object = match &atom.obj {
        gmeow_logic_compile::ir::AtomicTerm::Var(value) => EvalTerm::Var(value.clone()),
        gmeow_logic_compile::ir::AtomicTerm::Iri(value) => EvalTerm::ConstNamed(value.clone()),
        gmeow_logic_compile::ir::AtomicTerm::Literal(value) => {
            EvalTerm::ConstLit(crate::rule_ir::literal_value(value))
        }
        gmeow_logic_compile::ir::AtomicTerm::Blank(value) => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Lower {
                detail: format!("blank object {value:?} requires an admitted existential rule"),
            }));
        }
    };
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
    admit_rule_structure(rule)?;
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
        numeric: Vec::new(),
        head,
        body,
        rule_iri,
        distinct_pairs: rule.distinct_pairs.clone(),
        // This lowering carries no arithmetic builtins.
        builtins: Vec::new(),
        reduction: crate::rule_ir::Reduction::lower(rule)?,
        constraint_tag: None,
    })
}

/// Refuse source operators that this evaluation IR cannot represent. A negative
/// conclusion is not positive evidence. These checks precede every direct, cached
/// and certification lowering; reductions receive their own typed admission.
fn admit_rule_structure(rule: &LogicRule) -> gmeow_errors::Result<()> {
    let refuse = |detail: String| {
        gmeow_errors::Diag::of_kind(crate::error::Lower {
            detail: format!(
                "native rule template deriving <{}> from {:?}: {detail}",
                rule.head.predicate, rule.scope.provenance
            ),
        })
    };
    if std::iter::once(&rule.head).chain(&rule.body).any(|atom| {
        gmeow_logic_compile::relational_core::NumericOperator::from_iri(&atom.predicate).is_some()
    }) {
        return Err(refuse("interpreted numeric operands require the canonical Formula AST, not a compact binary rule".to_owned()));
    }
    if rule.head.negated {
        return Err(refuse(
            "a negative head requires signed-conclusion execution; this positive-head template cannot reverse its sign".into(),
        ));
    }
    if rule.node_kind != NodeKind::DerivationRule {
        return Err(refuse(format!(
            "rule kind {:?} is not an ordinary derivation template",
            rule.node_kind,
        )));
    }
    if rule.head.node_kind != NodeKind::ObjectLevelFormula {
        return Err(refuse(format!(
            "head kind {:?} requires its own semantic admission; it cannot become an object-level conclusion",
            rule.head.node_kind,
        )));
    }
    for (index, atom) in rule.body.iter().enumerate() {
        if atom.node_kind != NodeKind::ObjectLevelFormula {
            return Err(refuse(format!(
                "body/{index} kind {:?} requires its own semantic admission; it cannot become an object-level premise",
                atom.node_kind,
            )));
        }
    }
    Ok(())
}

/// Lower rules for immediate execution or certification under the declared native
/// world-local template profile. Every source context must first be admitted.
pub(crate) fn lower_eval_rules(program: &LogicProgram) -> gmeow_errors::Result<Vec<EvalRule>> {
    crate::native_semantics::ProgramAdmission::capture(program).admit_world_local_template()?;
    prepare_rule_templates(program)
}

/// Prepare structural templates while the owning PreparedProgram retains the source
/// admission obligation. Preparation alone never authorizes their execution in a world.
pub(crate) fn prepare_rule_templates(
    program: &LogicProgram,
) -> gmeow_errors::Result<Vec<EvalRule>> {
    program.rules.iter().map(lower_rule).collect()
}

#[path = "lower.tests.rs"]
#[cfg(test)]
mod tests;
