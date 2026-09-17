// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Validated canonical reduce operators. RDF value arithmetic stays in PurRDF;
//! variable scope, complete grouping and rule provenance belong to this engine.

use std::collections::BTreeSet;

use gmeow_logic_compile::ir::{AtomicTerm, LogicRule};
use purrdf::sparql::ValueAggregate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Function {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

#[cfg(test)]
mod tests;

/// A reduce specification whose input and output variable scopes are disjoint.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Reduction {
    function: Function,
    pub(crate) input: String,
    pub(crate) result: String,
    pub(crate) groups: Vec<String>,
}

impl Reduction {
    /// Admit the authored operator before it reaches an executable plan.
    pub(crate) fn lower(rule: &LogicRule) -> gmeow_errors::Result<Option<Self>> {
        let Some(spec) = &rule.aggregation else {
            return Ok(None);
        };
        let refuse = |detail: &str| {
            gmeow_errors::Diag::of_kind(crate::error::Lower {
                detail: format!(
                    "aggregation in rule {:?} deriving <{}>: {detail}",
                    rule.scope.provenance, rule.head.predicate,
                ),
            })
        };
        let function = match spec.function.as_str() {
            "COUNT" => Function::Count,
            "SUM" => Function::Sum,
            "AVG" => Function::Avg,
            "MIN" => Function::Min,
            "MAX" => Function::Max,
            _ => return Err(refuse("unsupported aggregate function")),
        };
        let variables = |atom: &gmeow_logic_compile::ir::LogicAxiom| {
            let subject = atom
                .subject
                .starts_with('?')
                .then_some(atom.subject.clone());
            let object = match &atom.obj {
                AtomicTerm::Var(name) => Some(name.clone()),
                _ => None,
            };
            subject.into_iter().chain(object)
        };
        let bound: BTreeSet<_> = rule
            .body
            .iter()
            .filter(|atom| !atom.negated)
            .flat_map(variables)
            .collect();
        if !spec.aggregate_var.starts_with('?') || !bound.contains(&spec.aggregate_var) {
            return Err(refuse("input variable must be positively body-bound"));
        }
        if spec
            .group_keys
            .iter()
            .any(|key| !key.starts_with('?') || !bound.contains(key))
        {
            return Err(refuse("every group key must be positively body-bound"));
        }
        if !spec.result_var.starts_with('?')
            || rule
                .body
                .iter()
                .flat_map(variables)
                .any(|name| name == spec.result_var)
        {
            return Err(refuse(
                "result variable must be fresh outside the entire body",
            ));
        }
        if rule.head.obj != AtomicTerm::Var(spec.result_var.clone()) {
            return Err(refuse("head object must be the declared result variable"));
        }
        if rule.head.subject.starts_with('?') && !spec.group_keys.contains(&rule.head.subject) {
            return Err(refuse("head subject variable must be a group key"));
        }
        if rule
            .distinct_pairs
            .iter()
            .flat_map(|(a, b)| [a, b])
            .any(|var| !bound.contains(var))
        {
            return Err(refuse(
                "inequality guards must use positively body-bound variables",
            ));
        }
        if rule
            .body
            .iter()
            .filter(|atom| atom.negated)
            .flat_map(variables)
            .any(|variable| !bound.contains(&variable))
        {
            return Err(refuse("negated variables must be positively body-bound"));
        }
        let mut groups = spec.group_keys.clone();
        groups.sort();
        groups.dedup();
        Ok(Some(Self {
            function,
            input: spec.aggregate_var.clone(),
            result: spec.result_var.clone(),
            groups,
        }))
    }

    /// Select the upstream value-level accumulator without query construction.
    pub(crate) const fn function(&self) -> ValueAggregate {
        match self.function {
            Function::Count => ValueAggregate::Count,
            Function::Sum => ValueAggregate::Sum,
            Function::Avg => ValueAggregate::Avg,
            Function::Min => ValueAggregate::Min,
            Function::Max => ValueAggregate::Max,
        }
    }

    /// Stable logical spelling for executable identity and diagnostics.
    pub(crate) const fn name(&self) -> &'static str {
        match self.function {
            Function::Count => "COUNT",
            Function::Sum => "SUM",
            Function::Avg => "AVG",
            Function::Min => "MIN",
            Function::Max => "MAX",
        }
    }
}
