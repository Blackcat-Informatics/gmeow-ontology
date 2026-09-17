// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit class-family observations using the same native ingress and governor.

use super::native::NativeClosureStatus;
use super::{ClassAdmissionObservation, ClassExecutionOutcome, NativeFamilyLedger};
use crate::physical::{LogicalGraph, SelectedDomains, WitnessDerivation};
use crate::query_ir::CompletionFrontier;
use crate::reason::{ChaseCertificate, PreparedReasoningInput};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Original-source refusal is a pre-execution observation. An executed class
/// diagnostic retains only its selected family evidence, never a whole-program
/// satisfiability or consistency claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassDiagnosticOutcome {
    /// No writer ran because the selected source grammar or capability refused.
    SourceRefused {
        /// Every selected source owner, exact context and refusal.
        admission: ClassAdmissionObservation,
    },
    /// Intrinsic domains and the class producer used the shared native governor.
    Executed {
        /// Complete evidence for that explicit operation.
        execution: ClassDiagnosticObservation,
    },
}

/// Compact retained class execution; graph-shaped output is not a transport.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassDiagnosticObservation {
    /// Exact admitted source, operation and world-selection contract.
    pub input_contract: [u8; 32],
    /// Every executing context, with original graph identity.
    pub worlds: BTreeMap<String, LogicalGraph>,
    /// Explicit nonempty-domain authority for every diagnostic context.
    pub selected_domains: SelectedDomains,
    /// Actual native completed producer frontier.
    pub frontier: CompletionFrontier,
    /// Actual governor termination, independently of positive conflict support.
    pub status: NativeClosureStatus,
    /// Exact selected governor allowance; None is explicitly unbounded.
    pub allowance: Option<u64>,
    /// Original-source admission from the same retained preparation.
    pub admission: ClassAdmissionObservation,
    /// One current class outcome for each executing context.
    pub classes: Vec<ClassExecutionOutcome>,
    /// Shared proof DAGs; unselected native family outcomes are absent.
    pub proofs: Vec<NativeFamilyLedger>,
    /// Actual native program admission for each context.
    pub chase_certificates: Vec<ChaseCertificate>,
    /// Actual intrinsic domain witnesses, with full minting receipts.
    pub witnesses: Vec<WitnessDerivation>,
}

fn fail(detail: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.to_owned(),
    })
}

impl ClassDiagnosticOutcome {
    /// Validate selected-operation framing of already authenticated native output.
    pub fn validate(&self) -> gmeow_errors::Result<()> {
        match self {
            Self::SourceRefused { admission } => {
                admission.validate()?;
                if !admission
                    .selected_worlds
                    .values()
                    .any(|world| world.refusal.is_some())
                {
                    return Err(fail("class source refusal omits its actual selected cause"));
                }
                Ok(())
            }
            Self::Executed { execution } => execution.validate(),
        }
    }
}

impl ClassDiagnosticObservation {
    /// Validate exact world/domain scope and native proof framing. The producer
    /// separately validates actual committed rows before this compact publication.
    pub fn validate(&self) -> gmeow_errors::Result<()> {
        self.selected_domains.validate()?;
        self.admission.validate()?;
        if self
            .admission
            .selected_worlds
            .values()
            .any(|world| world.refusal.is_some())
        {
            return Err(fail("class diagnostic executed a refused original source"));
        }
        if self.frontier.completed > self.frontier.total
            || self
                .allowance
                .is_some_and(|limit| self.frontier.consumed_steps > limit)
        {
            return Err(fail("class diagnostic has an invalid governed frontier"));
        }
        if matches!(self.status, NativeClosureStatus::Completed)
            && self.frontier.completed != self.frontier.total
        {
            return Err(fail(
                "completed class diagnostic has unfinished producer strata",
            ));
        }
        if matches!(&self.status, NativeClosureStatus::Blocked { reads } if reads.is_empty()) {
            return Err(fail(
                "blocked class diagnostic omits its exact dependency reads",
            ));
        }
        for (world, graph) in &self.worlds {
            if graph.world()? != *world {
                return Err(fail("class diagnostic graph changes its execution world"));
            }
        }
        let selected: BTreeMap<_, _> = self
            .selected_domains
            .worlds()
            .iter()
            .map(|domain| Ok((domain.world()?, domain.graph().clone())))
            .collect::<gmeow_errors::Result<_>>()?;
        if selected != self.worlds {
            return Err(fail(
                "every class diagnostic context requires its exact selected domain",
            ));
        }
        if self.admission.source_worlds.keys().ne(self.worlds.keys()) {
            return Err(fail(
                "class diagnostic loses its exact original/selected context inventory",
            ));
        }
        for (world, source) in &self.admission.source_worlds {
            if self.worlds[world].graph() != source.graph.as_ref() {
                return Err(fail("class diagnostic changes source graph ownership"));
            }
        }
        let mut ledgers = BTreeMap::new();
        for ledger in &self.proofs {
            ledger.validate()?;
            if ledger.input_contract != self.input_contract
                || self.worlds.get(&ledger.world).map(LogicalGraph::graph)
                    != Some(ledger.graph.as_ref())
                || !ledger.outcomes.is_empty()
                || ledgers.insert(ledger.world.as_str(), ledger).is_some()
            {
                return Err(fail(
                    "class diagnostic proof ledger has a foreign, repeated or unselected family scope",
                ));
            }
        }
        if ledgers
            .keys()
            .copied()
            .ne(self.worlds.keys().map(String::as_str))
        {
            return Err(fail("class diagnostic omits a world proof ledger"));
        }
        let mut classes = BTreeSet::new();
        for class in &self.classes {
            let ledger = ledgers
                .get(class.world.as_str())
                .ok_or_else(|| fail("class outcome has no owning native ledger"))?;
            class.validate(ledger)?;
            if !classes.insert(class.world.as_str()) {
                return Err(fail("class diagnostic repeats a world outcome"));
            }
        }
        if classes.iter().copied().ne(ledgers.keys().copied()) {
            return Err(fail("class diagnostic omits a world outcome"));
        }
        let mut certificates = BTreeSet::new();
        for certificate in &self.chase_certificates {
            if certificate.input_contract != self.input_contract
                || !self.worlds.contains_key(&certificate.world)
                || !certificates.insert(certificate.world.as_str())
            {
                return Err(fail(
                    "class diagnostic has foreign or duplicate program admission",
                ));
            }
        }
        if certificates != classes {
            return Err(fail("class diagnostic omits a world program admission"));
        }
        let mut witnesses = BTreeSet::new();
        for witness in &self.witnesses {
            witness.validate()?;
            let crate::physical::WitnessOrigin::NonemptyDomain(domain) = &witness.scope.origin
            else {
                return Err(fail(
                    "class diagnostic publishes an unselected existential producer",
                ));
            };
            if domain.world()? != witness.scope.world
                || !self.selected_domains.worlds().contains(domain)
                || !witnesses.insert(witness.witness.as_str())
            {
                return Err(fail(
                    "class diagnostic witness has foreign or repeated domain authority",
                ));
            }
        }
        Ok(())
    }

    /// Positive contextual refutation remains supported even if another selected
    /// class obligation is obstructed. This does not assert whole-program closure.
    pub fn has_conflict(&self) -> bool {
        self.classes
            .iter()
            .any(|class| !class.contextual_conflicts.is_empty())
    }
}

/// Execute the explicit class diagnostic against one prepared native ingress.
/// Original source refusal is returned before any writer; an executed result
/// shares the actual intrinsic domain producer, governor, store and proof DAG.
pub fn class_diagnostic(
    input: PreparedReasoningInput,
    domains: &SelectedDomains,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<ClassDiagnosticOutcome> {
    match crate::reason::program::class_diagnostic(input, domains, max_steps)? {
        crate::reason::program::ClassDiagnosticRun::SourceRefused { admission } => {
            let outcome = ClassDiagnosticOutcome::SourceRefused { admission };
            outcome.validate()?;
            Ok(outcome)
        }
        crate::reason::program::ClassDiagnosticRun::Executed { closure } => {
            let worlds: BTreeMap<_, _> = closure
                .graphs
                .iter()
                .map(|(world, graph)| (world.clone(), LogicalGraph::from_graph(graph.clone())))
                .collect();
            crate::result::validate_committed_native(
                &worlds,
                &closure.native_families,
                &closure.witnesses,
                &closure.inferred,
            )?;
            let execution = ClassDiagnosticObservation {
                input_contract: closure.input_contract,
                worlds,
                selected_domains: closure.selected_domains,
                frontier: closure.frontier,
                status: closure.native_status,
                allowance: max_steps,
                admission: closure.class_admission,
                classes: closure.classes,
                proofs: closure.native_families,
                chase_certificates: closure.certificates,
                witnesses: closure.witnesses,
            };
            execution.validate()?;
            Ok(ClassDiagnosticOutcome::Executed { execution })
        }
    }
}

#[cfg(test)]
mod tests;
