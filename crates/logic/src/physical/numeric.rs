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
    fn prepare(term: &RcTerm) -> gmeow_errors::Result<Self> {
        match term {
            RcTerm::Var(name) => Ok(Self::Variable(name.clone())),
            RcTerm::Literal(value) => decode_literal(value).map(Self::Constant),
            _ => Err(failure(
                "operand preparation",
                format!("{term:?}"),
                "",
                "numeric operand must be a variable or a finite numeric literal",
            )),
        }
    }

    fn resolve<'a>(
        &'a self,
        solution: &Solution,
        values: &mut BTreeMap<&'a str, XsdValue>,
    ) -> gmeow_errors::Result<XsdValue> {
        match self {
            Self::Constant(value) => Ok(value.clone()),
            Self::Variable(name) => {
                if let Some(value) = values.get(name.as_str()) {
                    return Ok(value.clone());
                }
                let term = solution.get(name).ok_or_else(|| {
                    failure(
                        "operand resolution",
                        name,
                        "",
                        format!("unbound numeric input {name}"),
                    )
                })?;
                let TermValue::Literal {
                    lexical_form,
                    datatype,
                    language: None,
                    direction: None,
                } = term
                else {
                    return Err(failure(
                        "operand resolution",
                        format!("{name}={term:?}"),
                        "",
                        format!("nonnumeric binding for {name}"),
                    ));
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
    ) -> gmeow_errors::Result<Self> {
        if !calls.is_empty() {
            let bound = input_variables(body);
            let scheduled =
                gmeow_logic_compile::relational_core::numeric::schedule(calls.to_vec(), bound)
                    .map_err(|detail| {
                        failure("binding schedule", format!("{calls:?}"), "", detail)
                    })?;
            if scheduled != calls {
                return Err(failure(
                    "binding schedule",
                    format!("{calls:?}"),
                    "",
                    "noncanonical numeric binding schedule",
                ));
            }
        }
        Self::prepare(calls)
    }

    pub(super) fn prepare(calls: &[RcNumeric]) -> gmeow_errors::Result<Self> {
        let calls = calls
            .iter()
            .map(Instruction::prepare)
            .collect::<gmeow_errors::Result<_>>()?;
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
                .map_err(|diagnostic| with_rule(rule, with_operation(call.operator, diagnostic)))?;
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
    fn prepare(call: &RcNumeric) -> gmeow_errors::Result<Self> {
        let prepare = || {
            call.validate()
                .map_err(|detail| failure(call.operator.iri(), format!("{call:?}"), "", detail))?;
            Ok(Self {
                operator: call.operator,
                left: Operand::prepare(&call.left)?,
                right: Operand::prepare(&call.right)?,
                result: call.result.as_ref().map(Operand::prepare).transpose()?,
            })
        };
        prepare().map_err(|diagnostic| with_operation(call.operator, diagnostic))
    }

    fn apply<'a>(
        &'a self,
        solution: &mut Solution,
        values: &mut BTreeMap<&'a str, XsdValue>,
    ) -> gmeow_errors::Result<bool> {
        use NumericOperator as Op;
        let left = self.left.resolve(solution, values)?;
        let right = self.right.resolve(solution, values)?;
        let operation = match self.operator {
            Op::Add => xsd::numeric_add,
            Op::Subtract => xsd::numeric_sub,
            Op::Multiply => xsd::numeric_mul,
            Op::Divide => xsd::numeric_div,
            comparison => {
                let ordering = xsd::numeric_cmp(&left, &right).ok_or_else(|| {
                    failure(
                        self.operator.iri(),
                        format!("{left:?}, {right:?}"),
                        "",
                        "unordered numeric comparison",
                    )
                })?;
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
        let result = operation(&left, &right).map_err(|error| {
            failure(
                self.operator.iri(),
                format!("{left:?}, {right:?}"),
                format!("{}, {}", left.datatype().iri(), right.datatype().iri()),
                error.to_string(),
            )
        })?;
        finite(&result, &format!("{result:?}"), result.datatype().iri())?;
        let target = self.result.as_ref().ok_or_else(|| {
            failure(
                self.operator.iri(),
                format!("{left:?}, {right:?}"),
                "",
                "missing numeric result operand",
            )
        })?;
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
            .ok_or_else(|| {
                failure(
                    self.operator.iri(),
                    format!("{bound:?}, {result:?}"),
                    "",
                    "unordered numeric result equality",
                )
            })
    }
}

fn decode_literal(value: &RdfLiteral) -> gmeow_errors::Result<XsdValue> {
    if value.language.as_ref().is_some() || value.direction.as_ref().is_some() {
        return Err(failure(
            "literal decoding",
            format!("{value:?}"),
            value.datatype_iri(),
            "numeric literal cannot carry language or direction",
        ));
    }
    decode(&value.lexical_form, value.datatype_iri())
}

fn decode(lexical: &str, datatype: &str) -> gmeow_errors::Result<XsdValue> {
    let value = xsd::parse_by_iri(lexical, datatype)
        .map_err(|error| failure("literal decoding", lexical, datatype, error.to_string()))?
        .ok_or_else(|| {
            failure(
                "literal decoding",
                lexical,
                datatype,
                format!("unsupported numeric datatype {datatype}"),
            )
        })?;
    finite(&value, lexical, datatype)?;
    Ok(value)
}

fn finite(value: &XsdValue, operand: &str, datatype: &str) -> gmeow_errors::Result<()> {
    match value {
        XsdValue::Integer { .. } | XsdValue::Decimal(_) => Ok(()),
        XsdValue::Float(value) if value.is_finite() => Ok(()),
        XsdValue::Double(value) if value.is_finite() => Ok(()),
        _ => Err(failure(
            "finite numeric value",
            operand,
            datatype,
            "selected numeric relation requires a finite RDF numeric value",
        )),
    }
}

/// Mint the diagnostic at the failing numeric boundary, before propagation.
#[track_caller]
fn failure(
    operation: impl Into<String>,
    operands: impl Into<String>,
    datatype: impl Into<String>,
    detail: impl Into<String>,
) -> gmeow_errors::Diag {
    diagnostic(crate::error::Numeric {
        rule: String::new(),
        operation: operation.into(),
        operands: operands.into(),
        datatype: datatype.into(),
        detail: detail.into(),
    })
}

#[track_caller]
fn diagnostic(payload: crate::error::Numeric) -> gmeow_errors::Diag {
    let detail = payload.to_string();
    let mut diagnostic = gmeow_errors::Diag::of_kind(crate::error::Physical { detail });
    diagnostic.inner_mut().source = Some(Box::new(payload));
    diagnostic
}

fn with_operation(
    operator: NumericOperator,
    mut diagnostic: gmeow_errors::Diag,
) -> gmeow_errors::Diag {
    let mut payload = numeric_payload(&diagnostic);
    payload.operation = operator.iri().to_owned();
    diagnostic.inner_mut().message = payload.to_string();
    diagnostic.inner_mut().source = Some(Box::new(payload));
    diagnostic
}

/// Attach the owning rule without reducing a diagnostic to its rendered message.
pub(super) fn with_rule(rule: &str, mut diagnostic: gmeow_errors::Diag) -> gmeow_errors::Diag {
    let mut payload = numeric_payload(&diagnostic);
    payload.rule = rule.to_owned();
    diagnostic.inner_mut().message = payload.to_string();
    diagnostic.inner_mut().source = Some(Box::new(payload));
    diagnostic
}

fn numeric_payload(diagnostic: &gmeow_errors::Diag) -> crate::error::Numeric {
    diagnostic
        .downcast_ref::<crate::error::Numeric>()
        .expect("physical numeric failures retain the typed Numeric source payload")
        .clone()
}

/// A cached preparation verdict, never a function's error type. Only the typed
/// failure evidence is retained; each retrieval mints an independent diagnostic.
#[derive(Debug)]
pub(super) enum Prepared {
    Ready(Plan),
    Rejected(crate::error::Numeric),
}

impl Prepared {
    pub(super) fn for_body(calls: &[RcNumeric], body: &[crate::rule_ir::EvalAtom]) -> Self {
        match Plan::for_body(calls, body) {
            Ok(plan) => Self::Ready(plan),
            Err(diagnostic) => Self::Rejected(numeric_payload(&diagnostic)),
        }
    }

    #[track_caller]
    pub(super) fn get(&self, rule: &str) -> gmeow_errors::Result<&Plan> {
        match self {
            Self::Ready(plan) => Ok(plan),
            Self::Rejected(payload) => {
                let mut payload = payload.clone();
                payload.rule = rule.to_owned();
                Err(diagnostic(payload))
            }
        }
    }
}

#[cfg(test)]
mod tests;
