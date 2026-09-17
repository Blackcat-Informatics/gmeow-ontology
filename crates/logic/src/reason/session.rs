// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Prepared native theory and isolated proof-retaining transactions. Ingress and
//! program lowering have one owner; forks apply native source deltas and reuse
//! only proofs admitted by the joint producer dependency graph.

use std::sync::Arc;

use super::{IncrementalReasoningResult, PreparedReasoningInput, ReasoningResult};
use crate::physical::{LogicalGraph, RetainedJoint, SelectedDomains};
use crate::rule_ir::Fact;

pub(crate) struct NativeReasoningSession {
    input: PreparedReasoningInput,
    domains: SelectedDomains,
    prepared: Arc<crate::program_analysis::PreparedProgram>,
    potential: Vec<(String, Fact)>,
    retained: RetainedJoint,
    base: ReasoningResult,
}

impl NativeReasoningSession {
    pub(crate) fn new(
        input: PreparedReasoningInput,
        domains: &SelectedDomains,
        potential: Vec<(String, Fact)>,
    ) -> gmeow_errors::Result<Self> {
        let program = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None);
        let prepared = crate::program_analysis::prepare_program(&program)?;
        let mut retained = RetainedJoint::default();
        let closure = super::program::execute_transaction(
            &prepared,
            input.clone(),
            domains,
            None,
            &potential,
            Some(&mut retained),
        )?;
        let (base, _, _) = super::result_from_closure(&prepared, closure, None)?;
        Ok(Self {
            input,
            domains: domains.clone(),
            prepared,
            potential,
            retained,
            base,
        })
    }

    pub(crate) fn base(&self) -> &ReasoningResult {
        &self.base
    }
    pub(crate) fn input(&self) -> PreparedReasoningInput {
        self.input.clone()
    }
    pub(crate) fn domains(&self) -> &SelectedDomains {
        &self.domains
    }

    fn transaction(
        &self,
        input: PreparedReasoningInput,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<IncrementalReasoningResult> {
        let mut retained = self.retained.clone();
        let closure = super::program::execute_transaction(
            &self.prepared,
            input,
            &self.domains,
            max_steps,
            &self.potential,
            Some(&mut retained),
        )?;
        let (result, status, consumed_steps) =
            super::result_from_closure(&self.prepared, closure, max_steps)?;
        Ok(IncrementalReasoningResult {
            result,
            status,
            consumed_steps,
        })
    }

    pub(crate) fn insert(
        &self,
        graph: LogicalGraph,
        fact: Fact,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<IncrementalReasoningResult> {
        let mut input = self.input.clone();
        // Even an already asserted candidate is a new selected operation. Its
        // retained proofs must be admitted under this operation's allowance;
        // returning the base result would carry the base run's budget receipt.
        input.assert_fact(graph, fact)?;
        self.transaction(input, max_steps)
    }

    pub(crate) fn retract(
        &self,
        axiom: &super::LeaveOneOutAxiom,
    ) -> gmeow_errors::Result<ReasoningResult> {
        let mut input = self.input.clone();
        if !input.retract_axiom(axiom)? {
            return Ok(self.base.clone());
        }
        Ok(self.transaction(input, None)?.result)
    }
}

#[cfg(test)]
mod tests;
