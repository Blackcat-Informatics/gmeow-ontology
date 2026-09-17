// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Prepared canonical finite RDF numeric relations. PurRDF owns scalar semantics;
//! this adapter owns binding modes, failure publication and GMEOW source support.

use std::collections::BTreeMap;

use gmeow_logic_compile::relational_core::{NumericOperator, RcNumeric, RcTerm};
use purrdf::{
    RdfLiteral, TermValue,
    xsd::{self, XsdValue},
};

use crate::rule_ir::Solution;

#[derive(Debug)]
enum Operand {
    Variable(String),
    Constant(XsdValue),
}

impl Operand {
    fn prepare(term: &RcTerm) -> Result<Self, String> {
        match term {
            RcTerm::Var(name) => Ok(Self::Variable(name.clone())),
            RcTerm::Literal(value) => decode_literal(value).map(Self::Constant),
            _ => Err("numeric operand must be a variable or a finite numeric literal".to_owned()),
        }
    }

    fn resolve<'a>(
        &'a self,
        solution: &Solution,
        values: &mut BTreeMap<&'a str, XsdValue>,
    ) -> Result<XsdValue, String> {
        match self {
            Self::Constant(value) => Ok(value.clone()),
            Self::Variable(name) => {
                if let Some(value) = values.get(name.as_str()) {
                    return Ok(value.clone());
                }
                let term = solution
                    .get(name)
                    .ok_or_else(|| format!("unbound numeric input {name}"))?;
                let TermValue::Literal {
                    lexical_form,
                    datatype,
                    language: None,
                    direction: None,
                } = term
                else {
                    return Err(format!("nonnumeric binding for {name}"));
                };
                let value = decode(lexical_form, datatype)?;
                values.insert(name, value.clone());
                Ok(value)
            }
        }
    }
}

#[derive(Debug)]
struct Instruction {
    operator: NumericOperator,
    left: Operand,
    right: Operand,
    result: Option<Operand>,
}

/// Immutable operator selection and decoded constants, retained by the shared plan.
#[derive(Debug)]
pub(super) struct Plan {
    calls: Vec<Instruction>,
}

/// Every positive relational input variable. Numeric value creation can depend on
/// inputs absent from the head; its termination frontier must retain them too.
pub(super) fn input_variables(
    body: &[crate::rule_ir::EvalAtom],
) -> std::collections::BTreeSet<String> {
    body.iter()
        .filter(|atom| !atom.negated)
        .flat_map(|atom| [&atom.subject, &atom.object])
        .filter_map(|term| match term {
            crate::rule_ir::EvalTerm::Var(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

impl Plan {
    pub(super) fn for_body(
        calls: &[RcNumeric],
        body: &[crate::rule_ir::EvalAtom],
    ) -> Result<Self, String> {
        if !calls.is_empty() {
            let bound = input_variables(body);
            let scheduled =
                gmeow_logic_compile::relational_core::numeric::schedule(calls.to_vec(), bound)
                    .map_err(str::to_owned)?;
            if scheduled != calls {
                return Err("noncanonical numeric binding schedule".to_owned());
            }
        }
        Self::prepare(calls)
    }

    pub(super) fn prepare(calls: &[RcNumeric]) -> Result<Self, String> {
        let calls = calls
            .iter()
            .map(|call| {
                call.validate().map_err(str::to_owned)?;
                Ok(Instruction {
                    operator: call.operator,
                    left: Operand::prepare(&call.left)?,
                    right: Operand::prepare(&call.right)?,
                    result: call.result.as_ref().map(Operand::prepare).transpose()?,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self { calls })
    }

    /// Decode each variable at most once per solution. Computed values enter the
    /// same bounded local map directly; only the final RDF binding gets a lexical form.
    /// The solution retains its complete, unchanged source-fact support.
    pub(super) fn apply(&self, rule: &str, solution: &mut Solution) -> gmeow_errors::Result<bool> {
        let mut values = BTreeMap::new();
        for call in &self.calls {
            let keep = call
                .apply(solution, &mut values)
                .map_err(|detail| error(rule, &format!("{}: {detail}", call.operator.iri())))?;
            if !keep {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn apply_all(
        &self,
        rule: &str,
        mut solutions: Vec<Solution>,
    ) -> gmeow_errors::Result<Vec<Solution>> {
        if self.calls.is_empty() {
            return Ok(solutions);
        }
        let mut failure = None;
        solutions.retain_mut(|solution| {
            if failure.is_some() {
                return false;
            }
            match self.apply(rule, solution) {
                Ok(keep) => keep,
                Err(error) => {
                    failure = Some(error);
                    false
                }
            }
        });
        match failure {
            Some(error) => Err(error),
            None => Ok(solutions),
        }
    }
}

impl Instruction {
    fn apply<'a>(
        &'a self,
        solution: &mut Solution,
        values: &mut BTreeMap<&'a str, XsdValue>,
    ) -> Result<bool, String> {
        use NumericOperator as Op;
        let left = self.left.resolve(solution, values)?;
        let right = self.right.resolve(solution, values)?;
        let operation = match self.operator {
            Op::Add => xsd::numeric_add,
            Op::Subtract => xsd::numeric_sub,
            Op::Multiply => xsd::numeric_mul,
            Op::Divide => xsd::numeric_div,
            comparison => {
                let ordering =
                    xsd::numeric_cmp(&left, &right).ok_or("unordered numeric comparison")?;
                return Ok(match comparison {
                    Op::Equal => ordering.is_eq(),
                    Op::NotEqual => !ordering.is_eq(),
                    Op::Less => ordering.is_lt(),
                    Op::LessOrEqual => !ordering.is_gt(),
                    Op::Greater => ordering.is_gt(),
                    Op::GreaterOrEqual => !ordering.is_lt(),
                    _ => unreachable!("arithmetic dispatched above"),
                });
            }
        };
        let result = operation(&left, &right).map_err(|error| error.to_string())?;
        finite(&result)?;
        let target = self
            .result
            .as_ref()
            .ok_or("missing numeric result operand")?;
        if let Operand::Variable(name) = target
            && solution.get(name).is_none()
        {
            solution.bindings.push((
                name.clone(),
                TermValue::Literal {
                    lexical_form: result.canonical_lexical(),
                    datatype: result.datatype().iri().to_owned(),
                    language: None,
                    direction: None,
                },
            ));
            values.insert(name, result);
            return Ok(true);
        }
        let bound = target.resolve(solution, values)?;
        xsd::numeric_cmp(&bound, &result)
            .map(|order| order.is_eq())
            .ok_or_else(|| "unordered numeric result equality".to_owned())
    }
}

fn decode_literal(value: &RdfLiteral) -> Result<XsdValue, String> {
    if value.language.as_ref().is_some() || value.direction.as_ref().is_some() {
        return Err("numeric literal cannot carry language or direction".to_owned());
    }
    decode(&value.lexical_form, value.datatype_iri())
}

fn decode(lexical: &str, datatype: &str) -> Result<XsdValue, String> {
    let value = xsd::parse_by_iri(lexical, datatype)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("unsupported numeric datatype {datatype}"))?;
    finite(&value)?;
    Ok(value)
}

fn finite(value: &XsdValue) -> Result<(), String> {
    match value {
        XsdValue::Integer { .. } | XsdValue::Decimal(_) => Ok(()),
        XsdValue::Float(value) if value.is_finite() => Ok(()),
        XsdValue::Double(value) if value.is_finite() => Ok(()),
        _ => Err("selected numeric relation requires a finite RDF numeric value".to_owned()),
    }
}

pub(super) fn error(rule: &str, detail: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::RelationalCore {
        detail: format!("numeric rule {rule}: {detail}; selected operation is incomplete"),
    })
}

#[cfg(test)]
mod tests;
