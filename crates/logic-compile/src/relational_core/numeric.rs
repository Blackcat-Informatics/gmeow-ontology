// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit finite RDF numeric relations, distinct from mathematical function terms.

use super::{Formula, RcTerm, Term, formula_term_to_rc, identity};
use std::collections::BTreeSet;

/// Interpreted value-space relation selected by its canonical predicate identity.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum NumericOperator {
    /// Sum, with numeric promotion.
    Add,
    /// Difference, with numeric promotion.
    Subtract,
    /// Product, with numeric promotion.
    Multiply,
    /// Quotient, with numeric promotion.
    Divide,
    /// Numeric value equality, distinct from RDF term identity.
    Equal,
    /// Numeric value inequality.
    NotEqual,
    /// Strict ascending comparison.
    Less,
    /// Non-strict ascending comparison.
    LessOrEqual,
    /// Strict descending comparison.
    Greater,
    /// Non-strict descending comparison.
    GreaterOrEqual,
}

impl NumericOperator {
    /// The closed relation inventory, in stable order.
    pub const ALL: [Self; 10] = [
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Divide,
        Self::Equal,
        Self::NotEqual,
        Self::Less,
        Self::LessOrEqual,
        Self::Greater,
        Self::GreaterOrEqual,
    ];

    /// Canonical relation identity; no general math symbol is implicitly interpreted.
    pub fn iri(self) -> &'static str {
        match self {
            Self::Add => "https://blackcatinformatics.ca/logic/rdfNumericAdd",
            Self::Subtract => "https://blackcatinformatics.ca/logic/rdfNumericSubtract",
            Self::Multiply => "https://blackcatinformatics.ca/logic/rdfNumericMultiply",
            Self::Divide => "https://blackcatinformatics.ca/logic/rdfNumericDivide",
            Self::Equal => "https://blackcatinformatics.ca/logic/rdfNumericEqual",
            Self::NotEqual => "https://blackcatinformatics.ca/logic/rdfNumericNotEqual",
            Self::Less => "https://blackcatinformatics.ca/logic/rdfNumericLess",
            Self::LessOrEqual => "https://blackcatinformatics.ca/logic/rdfNumericLessOrEqual",
            Self::Greater => "https://blackcatinformatics.ca/logic/rdfNumericGreater",
            Self::GreaterOrEqual => "https://blackcatinformatics.ca/logic/rdfNumericGreaterOrEqual",
        }
    }

    /// Resolve only an explicitly declared interpreted relation.
    pub fn from_iri(iri: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|operator| operator.iri() == iri)
    }

    /// Arithmetic relations take `(left, right, result)`; comparisons take two inputs.
    pub fn arity(self) -> usize {
        if matches!(
            self,
            Self::Add | Self::Subtract | Self::Multiply | Self::Divide
        ) {
            3
        } else {
            2
        }
    }
}

/// Typed operands retained from the canonical Formula AST, without RDF text conversion.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct RcNumeric {
    /// The selected finite numeric relation.
    pub operator: NumericOperator,
    /// Bound left input.
    pub left: RcTerm,
    /// Bound right input.
    pub right: RcTerm,
    /// Arithmetic output, either a fresh binding or a bound value equality filter.
    /// Absent exactly for comparison relations.
    pub result: Option<RcTerm>,
}

impl RcNumeric {
    /// Complete, length-framed identity including literal metadata and operand order.
    pub fn key(&self) -> String {
        identity::frame(
            "numeric",
            [
                self.operator.iri().to_owned(),
                self.left.key(),
                self.right.key(),
                self.result.as_ref().map_or_else(String::new, RcTerm::key),
            ],
        )
    }

    /// Output binding, if arithmetic names a variable.
    pub fn output_var(&self) -> Option<&str> {
        match &self.result {
            Some(RcTerm::Var(name)) => Some(name),
            _ => None,
        }
    }

    /// Check the selected relation's arity and typed atomic numeric operands.
    pub fn validate(&self) -> Result<(), &'static str> {
        if (self.operator.arity() == 3) != self.result.is_some() {
            return Err("numeric relation has the wrong result arity");
        }
        for term in [&self.left, &self.right].into_iter().chain(&self.result) {
            if !matches!(term, RcTerm::Var(_) | RcTerm::Literal(_)) {
                return Err("numeric relation requires literal or variable operands");
            }
        }
        Ok(())
    }
}

pub(super) fn lower(formula: &Formula) -> Result<Option<RcNumeric>, &'static str> {
    let Formula::Atom {
        relation: Term::Iri(iri),
        args,
    } = formula
    else {
        return Ok(None);
    };
    let Some(operator) = NumericOperator::from_iri(iri) else {
        return Ok(None);
    };
    if args.len() != operator.arity() {
        return Err("numeric relation has the wrong operand arity");
    }
    let call = RcNumeric {
        operator,
        left: formula_term_to_rc(&args[0], true)?,
        right: formula_term_to_rc(&args[1], true)?,
        result: args
            .get(2)
            .map(|term| formula_term_to_rc(term, true))
            .transpose()?,
    };
    call.validate()?;
    Ok(Some(call))
}

/// Canonical binding schedule for a commutative conjunction. An output becomes
/// available only after both inputs are bound. No inverse arithmetic is invented.
pub fn schedule(
    mut calls: Vec<RcNumeric>,
    mut bound: BTreeSet<String>,
) -> Result<Vec<RcNumeric>, &'static str> {
    for call in &calls {
        call.validate()?;
    }
    calls.sort_unstable();
    calls.dedup();
    let mut ordered = Vec::with_capacity(calls.len());
    while !calls.is_empty() {
        let next = calls
            .iter()
            .position(|call| {
                [&call.left, &call.right]
                    .into_iter()
                    .all(|term| !matches!(term, RcTerm::Var(name) if !bound.contains(name)))
            })
            .ok_or("numeric relation has an unbound or cyclic input dependency")?;
        let call = calls.remove(next);
        if let Some(name) = call.output_var() {
            bound.insert(name.to_owned());
        }
        ordered.push(call);
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests;
