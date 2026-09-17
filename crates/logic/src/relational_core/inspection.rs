// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Data-only observations of the native formula adapter. These are diagnostic
//! products, never executable plans, termination proofs or rewrite certificates.

use gmeow_logic_compile::ir::LogicProgram;
use gmeow_logic_compile::relational_core::{RcAtom, RcNumeric, RcTerm};
use serde::{Deserialize, Serialize};

use crate::result::PreservationClaim;
use crate::rule_ir::{EvalAtom, EvalTerm};

/// The actual native rule shapes and lowering residue of a selected formula set.
#[derive(Debug, Serialize, Deserialize)]
pub struct FormulaLoweringInspection {
    /// Ordinary Horn rules after the native adapter accepted their terms.
    pub rules: Vec<RuleInspection>,
    /// Conjunctive dependencies after the native adapter accepted their terms.
    pub existential_rules: Vec<RuleInspection>,
    /// Syntactic preservation only; execution admission is a separate obligation.
    pub preservation: PreservationClaim,
}

/// A diagnostic rule shape. Head order and common witness variables are retained.
#[derive(Debug, Serialize, Deserialize)]
pub struct RuleInspection {
    /// Canonical numeric operator identities, operand metadata and binding schedule.
    pub numeric: Vec<RcNumeric>,
    /// The native adapter's deterministic firing identity.
    pub rule_iri: String,
    /// All head atoms, in their actual native order.
    pub head: Vec<RcAtom>,
    /// All body atoms, including negation polarity.
    pub body: Vec<RcAtom>,
    /// The native inequality guards.
    pub distinct_pairs: Vec<(String, String)>,
}

/// Inspect the actual formula-to-native adapter once, without RDF text conversion.
/// Compact rules and axioms are outside this formula-specific diagnostic scope.
/// The returned observation cannot authorize execution or strengthen preservation.
///
/// # Errors
/// Refuses an unexpected nonliteral value in a native literal slot rather than
/// rendering it to text and changing its kind.
pub fn inspect_formula_lowering(
    program: &LogicProgram,
) -> gmeow_errors::Result<FormulaLoweringInspection> {
    let lowered = super::lower_formulas(program);
    Ok(FormulaLoweringInspection {
        rules: lowered
            .rules
            .iter()
            .map(|rule| {
                inspect_rule(
                    &rule.rule_iri,
                    std::slice::from_ref(&rule.head),
                    &rule.body,
                    &rule.distinct_pairs,
                    &rule.numeric,
                )
            })
            .collect::<gmeow_errors::Result<_>>()?,
        existential_rules: lowered
            .existential_rules
            .iter()
            .map(|rule| {
                inspect_rule(
                    &rule.rule_iri,
                    &rule.head,
                    &rule.body,
                    &rule.distinct,
                    &rule.numeric,
                )
            })
            .collect::<gmeow_errors::Result<_>>()?,
        preservation: lowered.preservation,
    })
}

fn inspect_rule(
    rule_iri: &str,
    head: &[EvalAtom],
    body: &[EvalAtom],
    distinct_pairs: &[(String, String)],
    numeric: &[RcNumeric],
) -> gmeow_errors::Result<RuleInspection> {
    Ok(RuleInspection {
        numeric: numeric.to_vec(),
        rule_iri: rule_iri.to_owned(),
        head: head
            .iter()
            .map(inspect_atom)
            .collect::<gmeow_errors::Result<_>>()?,
        body: body
            .iter()
            .map(inspect_atom)
            .collect::<gmeow_errors::Result<_>>()?,
        distinct_pairs: distinct_pairs.to_vec(),
    })
}

fn inspect_atom(atom: &EvalAtom) -> gmeow_errors::Result<RcAtom> {
    Ok(RcAtom {
        subject: inspect_term(&atom.subject)?,
        predicate: atom.predicate.clone(),
        object: inspect_term(&atom.object)?,
        negated: atom.negated,
    })
}

fn inspect_term(term: &EvalTerm) -> gmeow_errors::Result<RcTerm> {
    match term {
        EvalTerm::Var(name) => Ok(RcTerm::Var(name.clone())),
        EvalTerm::ConstNamed(iri) => Ok(RcTerm::Iri(iri.clone())),
        EvalTerm::ConstLit(purrdf::TermValue::Literal {
            lexical_form,
            datatype,
            language,
            direction,
        }) => Ok(RcTerm::Literal(purrdf::RdfLiteral {
            lexical_form: lexical_form.clone(),
            datatype: Some(datatype.clone()),
            language: language.clone(),
            direction: *direction,
        })),
        EvalTerm::ConstLit(value) => Err(super::rc_err(format!(
            "nonliteral native literal slot: {value:?}"
        ))),
    }
}

#[path = "inspection.tests.rs"]
#[cfg(test)]
mod tests;
