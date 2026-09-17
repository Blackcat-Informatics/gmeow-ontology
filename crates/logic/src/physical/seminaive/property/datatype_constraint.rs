// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Positive datatype contradictions over the native world and its shared plans.
//! Definitions are completed dependencies; operands are bound by ordinary joins.
//! No source conversion, second relation store or value-space materialization.

use std::sync::Arc;

use super::{Fact, NativeCaches, RelationStore, Slot, datatype, seminaive_err};
use crate::physical::dependency::ReadDependency;
use crate::rule_ir::EvalTerm;
use purrdf::TermValue;

/// A witnessed contradiction, never an absence-based negation or model claim.
#[derive(Debug, Clone)]
pub(crate) enum DatatypeConstraint {
    /// A declared datatype property carries an object outside the literal domain.
    NonLiteral { value: EvalTerm },
    /// An actual value fails a required datatype membership.
    Outside { datatype: EvalTerm, value: EvalTerm },
    /// A required finite lower count exceeds a proved value-space capacity.
    Insufficient {
        datatype: EvalTerm,
        minimum: EvalTerm,
    },
}

impl DatatypeConstraint {
    pub(super) fn terms(&self) -> Vec<&EvalTerm> {
        match self {
            Self::NonLiteral { value } => vec![value],
            Self::Outside { datatype, value } => vec![datatype, value],
            Self::Insufficient { datatype, minimum } => vec![datatype, minimum],
        }
    }

    pub(super) fn reads(&self) -> impl Iterator<Item = (Option<&str>, ReadDependency)> {
        // A literal-only check needs no datatype definition. Expression membership
        // and capacity require every potential definition writer to finish first.
        (!matches!(self, Self::NonLiteral { .. }))
            .then_some(())
            .into_iter()
            .flat_map(|()| datatype::reads())
            .map(|predicate| (Some(predicate), ReadDependency::Completed))
    }
}

#[derive(Debug)]
pub(super) struct PreparedDatatypeConstraint {
    pub(super) slots: Vec<Slot>,
}

pub(super) enum DatatypeWitness {
    LiteralDomain,
    Definition(Arc<datatype::DatatypePlan>),
}

pub(super) enum DatatypeEvaluation {
    NoConflict,
    Conflict(DatatypeWitness),
    Undecided,
}

impl DatatypeWitness {
    pub(super) fn premises(&self) -> &[Fact] {
        match self {
            Self::LiteralDomain => &[],
            Self::Definition(plan) => &plan.premises,
        }
    }
}

impl PreparedDatatypeConstraint {
    pub(super) fn evaluate(
        &self,
        source: &DatatypeConstraint,
        bindings: &[Option<TermValue>],
        rel: &RelationStore,
        caches: &mut NativeCaches<'_>,
    ) -> gmeow_errors::Result<DatatypeEvaluation> {
        let get = |index: usize| {
            self.slots[index]
                .value(bindings)
                .expect("body-bound datatype constraint input")
        };
        if matches!(source, DatatypeConstraint::NonLiteral { .. }) {
            return Ok(if matches!(get(0), TermValue::Literal { .. }) {
                DatatypeEvaluation::NoConflict
            } else {
                DatatypeEvaluation::Conflict(DatatypeWitness::LiteralDomain)
            });
        }
        let plan = caches
            .datatypes
            .prepare(rel, get(0), caches.lists, caches.values)?;
        let admitted = match source {
            DatatypeConstraint::Outside { .. } => plan.contains(get(1), caches.values),
            DatatypeConstraint::Insufficient { .. } => {
                let count = caches.values.cardinality(get(1)).ok_or_else(|| {
                    seminaive_err("a datatype lower bound requires a non-negative integer field")
                })?;
                plan.admits_count(count)
            }
            DatatypeConstraint::NonLiteral { .. } => unreachable!("handled literal domain"),
        };
        match admitted {
            Some(true) => Ok(DatatypeEvaluation::NoConflict),
            Some(false) => Ok(DatatypeEvaluation::Conflict(DatatypeWitness::Definition(
                plan,
            ))),
            None => Ok(DatatypeEvaluation::Undecided),
        }
    }
}
