// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Evidence retained from the single native producer execution. Result consumers
//! validate these records; they never re-run source admission or a family scanner.

use std::collections::{BTreeMap, BTreeSet};

use crate::physical::{LogicalGraph, SelectedDomains, WitnessDerivation};
use crate::query_ir::CompletionFrontier;
use crate::reason::refute::{
    ClassAdmissionObservation, ClassExecutionOutcome, NativeFamilyCompletion, NativeFamilyLedger,
    NativeObligationScope, NativeRefutationFamily,
};
use crate::reason::{ChaseCertificate, InferredAxiom};
use crate::result::{BudgetUsage, result_err};

/// Complete native execution scope and retained obligations. This record is
/// mandatory whenever a result was produced by the forward native engine. An
/// explicit query operation that did not execute that engine has no such record.
/// A missing record never authorizes a weaker consistency or completion claim.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeExecutionEvidence {
    /// Exact prepared native input and source-definition contract.
    pub input_contract: [u8; 32],
    /// Original native graph for each execution key, including selected empty worlds.
    pub worlds: BTreeMap<String, LogicalGraph>,
    /// Caller-admitted intrinsic domains; graph presence never supplies this selection.
    pub selected_domains: SelectedDomains,
    /// Actual shared governor frontier, including only completed producer strata.
    pub frontier: CompletionFrontier,
    /// Actual driver termination, including exact dependency roles withheld by
    /// an unsupported selected producer. A frontier gap never invents a cause.
    pub status: crate::reason::refute::native::NativeClosureStatus,
    /// Complete current counting/datatype outcomes for every execution world.
    pub families: Vec<NativeFamilyLedger>,
    /// Original source-owner admission, retained before inference begins.
    pub class_admission: ClassAdmissionObservation,
    /// Role-selected source and derived constructs, bound to actual native proofs.
    pub source_coverage: crate::reason::dl::SourceCoverageObservation,
    /// Every world's current class-model outcome and supported contextual proofs.
    pub classes: Vec<ClassExecutionOutcome>,
    /// The actual world-scoped termination admissions used by this run.
    pub chase_certificates: Vec<ChaseCertificate>,
    /// Committed, decomposable native witness evidence from the same registry.
    pub witness_derivations: Vec<WitnessDerivation>,
}

impl NativeExecutionEvidence {
    /// Validate the intrinsic framing of an authenticated execution record. Exact
    /// proof identities do not authenticate external source bytes or authorize
    /// unrestricted program rewrites; those remain the owning admission's claims.
    ///
    /// # Errors
    /// Rejects conflicting worlds, missing family observations, foreign evidence,
    /// invented domain authority, or a frontier outside the recorded allowance.
    pub fn validate(&self, budget: &BudgetUsage) -> gmeow_errors::Result<()> {
        self.validate_structure()?;
        if self.frontier.consumed_steps != budget.consumed
            || budget
                .allowance
                .is_some_and(|limit| self.frontier.consumed_steps > limit)
        {
            return Err(result_err(
                "native result has an invalid governed frontier".into(),
            ));
        }
        for receipt in self
            .families
            .iter()
            .flat_map(|ledger| &ledger.contextual_receipts)
        {
            receipt.judgment.validate_upstream_allowance(budget)?;
        }
        Ok(())
    }

    /// Validate the shared scope and proof framing independently of an external
    /// budget record. Diagnostic folds use this exact validator; they do not
    /// invent a governor allowance or repeat source admission.
    ///
    /// # Errors
    /// Rejects foreign or missing scope, malformed proof records, and an invalid
    /// completion frontier. Authorship is established by the producer receipt.
    pub fn validate_structure(&self) -> gmeow_errors::Result<()> {
        self.selected_domains.validate()?;
        self.class_admission.validate()?;
        if self.frontier.completed > self.frontier.total {
            return Err(result_err(
                "native result has an invalid completion frontier".into(),
            ));
        }
        match &self.status {
            crate::reason::refute::native::NativeClosureStatus::Completed
                if self.frontier.completed != self.frontier.total =>
            {
                return Err(result_err(
                    "completed native result has unfinished producer strata".into(),
                ));
            }
            crate::reason::refute::native::NativeClosureStatus::Blocked { reads }
                if reads.is_empty() =>
            {
                return Err(result_err(
                    "blocked native result omits its withheld dependency roles".into(),
                ));
            }
            _ => {}
        }
        for (world, graph) in &self.worlds {
            if graph.world()? != *world {
                return Err(result_err(
                    "native result changes an original graph's execution world".into(),
                ));
            }
        }
        for domain in self.selected_domains.worlds() {
            if self.worlds.get(&domain.world()?) != Some(domain.graph()) {
                return Err(result_err(
                    "native result domain selection is outside its admitted worlds".into(),
                ));
            }
        }
        for (world, source) in &self.class_admission.source_worlds {
            if self.worlds.get(world).map(LogicalGraph::graph) != Some(source.graph.as_ref()) {
                return Err(result_err(
                    "native class admission changes original source ownership".into(),
                ));
            }
        }
        let mut observed = BTreeSet::new();
        for ledger in &self.families {
            ledger.validate()?;
            if ledger.input_contract != self.input_contract
                || self.worlds.get(&ledger.world).map(LogicalGraph::graph)
                    != Some(ledger.graph.as_ref())
                || !observed.insert(ledger.world.as_str())
            {
                return Err(result_err(
                    "native family evidence has a duplicate or foreign run scope".into(),
                ));
            }
            let families: BTreeSet<_> = ledger
                .outcomes
                .iter()
                .filter(|outcome| outcome.obligation == NativeObligationScope::World)
                .map(|outcome| outcome.family)
                .collect();
            let required = BTreeSet::from([
                NativeRefutationFamily::Cardinality,
                NativeRefutationFamily::Identity,
                NativeRefutationFamily::HasSelf,
                NativeRefutationFamily::Datatype,
            ]);
            if families != required {
                return Err(result_err(
                    "native result omits a required family admission observation".into(),
                ));
            }
        }
        if observed != self.worlds.keys().map(String::as_str).collect() {
            return Err(result_err(
                "native result omits an admitted world's family evidence".into(),
            ));
        }
        for receipt in self
            .families
            .iter()
            .flat_map(|ledger| &ledger.contextual_receipts)
        {
            receipt
                .judgment
                .validate_upstream(&self.status, &self.families, &self.classes)?;
        }
        validate_modal_proofs(self.input_contract, &self.families)?;
        self.source_coverage
            .validate(self.input_contract, &self.worlds, &self.families)?;
        let families: BTreeMap<_, _> = self
            .families
            .iter()
            .map(|ledger| (ledger.world.as_str(), ledger))
            .collect();
        let mut class_worlds = BTreeSet::new();
        for class in &self.classes {
            let ledger = families.get(class.world.as_str()).ok_or_else(|| {
                result_err("class outcome has no owning native world evidence".into())
            })?;
            class.validate(ledger)?;
            if !class_worlds.insert(class.world.as_str()) {
                return Err(result_err(
                    "native result repeats a class-world outcome".into(),
                ));
            }
        }
        if class_worlds != observed {
            return Err(result_err(
                "native result omits an admitted world's class outcome".into(),
            ));
        }
        let mut certificates = BTreeSet::new();
        for certificate in &self.chase_certificates {
            if !self.worlds.contains_key(&certificate.world)
                || certificate.input_contract != self.input_contract
                || !certificates.insert(certificate.world.as_str())
            {
                return Err(result_err(
                    "native chase certificate has a duplicate or foreign run scope".into(),
                ));
            }
        }
        if certificates != observed {
            return Err(result_err(
                "native result omits an admitted world's program certificate".into(),
            ));
        }
        let mut witnesses = BTreeSet::new();
        for witness in &self.witness_derivations {
            witness.validate()?;
            if !self.worlds.contains_key(&witness.scope.world)
                || !witnesses.insert(witness.witness.as_str())
            {
                return Err(result_err(
                    "native witness belongs to an absent world or repeats an identity".into(),
                ));
            }
            if let crate::physical::WitnessOrigin::NonemptyDomain(domain) = &witness.scope.origin
                && !self.selected_domains.worlds().contains(domain)
            {
                return Err(result_err(
                    "native witness invents an unselected intrinsic domain".into(),
                ));
            }
        }
        Ok(())
    }

    /// Check actual committed witness heads against the result's native closure.
    /// Merely computing a candidate or a witness address is not publication.
    ///
    /// # Errors
    /// Rejects a receipt whose exact world and statement are absent from the
    /// committed closure. Its own minting-rule receipt survives proof tie-breaks.
    pub fn validate_committed(&self, inferred: &[InferredAxiom]) -> gmeow_errors::Result<()> {
        validate_committed_native(
            &self.worlds,
            &self.families,
            &self.witness_derivations,
            inferred,
        )
    }

    /// Every retained refutation obligation has a conclusive observation.
    /// This does not certify unmodeled source constructs or an unrestricted rewrite.
    #[must_use]
    pub fn refutation_complete(&self) -> bool {
        self.families.iter().all(NativeFamilyLedger::complete)
            && self.classes.iter().all(|class| {
                matches!(
                    class.completion,
                    NativeFamilyCompletion::Complete | NativeFamilyCompletion::NotEngaged
                )
            })
    }

    /// Positive native support remains meaningful even if another obligation was
    /// obstructed or exhausted. Completion must be inspected independently.
    #[must_use]
    pub fn has_conflict(&self) -> bool {
        self.families.iter().any(NativeFamilyLedger::has_conflict)
            || self
                .classes
                .iter()
                .any(|class| !class.contextual_conflicts.is_empty())
    }
}

/// Verify actual native commits for a selected operation without manufacturing a
/// full consistency claim. Class diagnostics and full closure use the same test.
pub(crate) fn validate_committed_native(
    worlds: &BTreeMap<String, LogicalGraph>,
    families: &[NativeFamilyLedger],
    witnesses: &[WitnessDerivation],
    inferred: &[InferredAxiom],
) -> gmeow_errors::Result<()> {
    let mut required = BTreeMap::<(&str, &str, &str), BTreeSet<&purrdf::TermValue>>::new();
    let mut minted = BTreeMap::<(&str, &str, &str), BTreeSet<&purrdf::TermValue>>::new();
    for ledger in families {
        for proof in &ledger.proofs {
            let subject = proof.statement.subject.as_iri().ok_or_else(|| {
                result_err("native proof requires a resource execution subject".into())
            })?;
            required
                .entry((&ledger.world, subject, &proof.statement.predicate))
                .or_default()
                .insert(&proof.statement.object);
        }
    }
    for witness in witnesses {
        for head in &witness.heads {
            for statement in std::iter::once(&head.statement).chain(&head.premises) {
                let subject = statement.subject.as_iri().ok_or_else(|| {
                    result_err("native witness requires a resource execution subject".into())
                })?;
                required
                    .entry((&witness.scope.world, subject, &statement.predicate))
                    .or_default()
                    .insert(&statement.object);
            }
            let subject = head
                .statement
                .subject
                .as_iri()
                .expect("head subject admitted");
            minted
                .entry((&witness.scope.world, subject, &head.statement.predicate))
                .or_default()
                .insert(&head.statement.object);
        }
    }
    // Retained minting receipts own their actual firing even if another proof
    // wins the closure row tie-break. Compare committed statement membership;
    // witness.validate() checks the minting rule and ordered derivation itself.
    for row in inferred {
        if !worlds.contains_key(&row.world) {
            return Err(result_err(
                "native closure contains an unadmitted execution world".into(),
            ));
        }
        let key = (
            row.world.as_str(),
            row.subject.as_str(),
            row.predicate.as_str(),
        );
        if let Some(objects) = required.get_mut(&key) {
            objects.remove(&row.object);
        }
        if !row.is_edb
            && let Some(objects) = minted.get_mut(&key)
        {
            objects.remove(&row.object);
        }
    }
    if required
        .values()
        .chain(minted.values())
        .any(|objects| !objects.is_empty())
    {
        return Err(result_err(
            "native witness head or premise is absent from the committed closure".into(),
        ));
    }
    Ok(())
}

/// Local native ledgers own ordinary derivation ordering. Modal references can
/// cross those ledgers, so their exact support and acyclicity need one global
/// proof-index fold. This never reconstructs or scans source datasets.
fn validate_modal_proofs(
    input_contract: [u8; 32],
    families: &[NativeFamilyLedger],
) -> gmeow_errors::Result<()> {
    use crate::reason::refute::{NativeProofId, NativeProofOrigin};
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Node<'a> {
        Proof(&'a str, NativeProofId),
        Contextual(&'a str, [u8; 32]),
    }
    let proofs: BTreeMap<_, _> = families
        .iter()
        .flat_map(|ledger| {
            ledger
                .proofs
                .iter()
                .map(move |proof| ((ledger.world.as_str(), proof.id), proof))
        })
        .collect();
    let mut unresolved = BTreeMap::<Node<'_>, usize>::new();
    let mut consumers = BTreeMap::<Node<'_>, Vec<Node<'_>>>::new();
    let mut contextual = BTreeMap::new();
    for ledger in families {
        for receipt in &ledger.contextual_receipts {
            receipt.validate_supports(&input_contract, &proofs)?;
            contextual.insert((ledger.world.as_str(), receipt.id), receipt);
            let key = Node::Contextual(ledger.world.as_str(), receipt.id);
            unresolved.insert(key, receipt.supports.len());
            for support in &receipt.supports {
                consumers
                    .entry(Node::Proof(support.world.as_str(), support.proof))
                    .or_default()
                    .push(key);
            }
        }
    }
    for (key, proof) in &proofs {
        let premises: Vec<_> = match &proof.origin {
            NativeProofOrigin::Derived { premises, .. } => {
                premises.iter().map(|id| Node::Proof(key.0, *id)).collect()
            }
            NativeProofOrigin::Modal { evidence } => {
                evidence.validate_structure(key.0, &proof.statement)?;
                evidence.validate_supports(&input_contract, &proofs)?;
                evidence
                    .supports
                    .iter()
                    .map(|support| Node::Proof(support.world.as_str(), support.proof))
                    .collect()
            }
            NativeProofOrigin::Contextual { receipt } => {
                if !contextual.contains_key(&(key.0, *receipt)) {
                    return Err(result_err(
                        "contextual proof names an absent shared assessment".into(),
                    ));
                }
                vec![Node::Contextual(key.0, *receipt)]
            }
            NativeProofOrigin::Asserted { .. } | NativeProofOrigin::Intrinsic { .. } => Vec::new(),
        };
        let key = Node::Proof(key.0, key.1);
        unresolved.insert(key, premises.len());
        for premise in premises {
            if let Node::Proof(world, id) = premise
                && !proofs.contains_key(&(world, id))
            {
                return Err(result_err(
                    "native derivation cites a missing world-qualified premise".into(),
                ));
            }
            consumers.entry(premise).or_default().push(key);
        }
    }
    let mut ready: std::collections::VecDeque<_> = unresolved
        .iter()
        .filter_map(|(key, count)| (*count == 0).then_some(*key))
        .collect();
    let mut visited = 0;
    while let Some(key) = ready.pop_front() {
        visited += 1;
        if let Some(next) = consumers.get(&key) {
            for dependent in next {
                let count = unresolved
                    .get_mut(dependent)
                    .expect("every consumer is indexed");
                *count -= 1;
                if *count == 0 {
                    ready.push_back(*dependent);
                }
            }
        }
    }
    if visited != unresolved.len() {
        return Err(result_err(
            "native derivations contain a cross-world proof cycle".into(),
        ));
    }
    Ok(())
}
