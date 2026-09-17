// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Retained contextual class results over the shared native world and proof DAG.

use super::super::{
    ContextualConflict, NativeFamilyCompletion, NativeFamilyLedger, NativeFamilyObstruction,
};
use purrdf::TermValue;
use serde::{Deserialize, Serialize};

/// One world's complete current class-family evidence, independent of local heads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassExecutionOutcome {
    /// Exact execution world selected by native ingress.
    pub world: String,
    /// Original asserting graph, never inferred by reversing a skolem IRI.
    #[serde(with = "crate::term_serde::optional")]
    pub graph: Option<TermValue>,
    /// Exact native source/selection admission commitment.
    pub input_contract: [u8; 32],
    /// Completion of the selected class obligations, independent of positive proofs.
    pub completion: NativeFamilyCompletion,
    /// Context-wide closing arguments, including those with no shared local subject.
    pub contextual_conflicts: Vec<ContextualConflict>,
    /// Every runtime capability obstruction, with its actual native support.
    /// Original request grammar refusals remain in ClassAdmissionObservation.
    pub obstructions: Vec<NativeFamilyObstruction>,
}

impl ClassExecutionOutcome {
    /// Validate intrinsic scope, proof references and exhaustive branch framing of
    /// an authenticated engine result. A digest alone is not a theorem authority.
    pub fn validate(&self, ledger: &NativeFamilyLedger) -> gmeow_errors::Result<()> {
        let fail = |detail: &str| {
            gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: detail.to_owned(),
            })
        };
        if self.world != ledger.world
            || self.graph != ledger.graph
            || self.input_contract != ledger.input_contract
            || super::admission::graph_world(self.graph.as_ref())? != self.world
        {
            return Err(fail(
                "class result and native proof ledger have different scopes",
            ));
        }
        if matches!(
            self.completion,
            NativeFamilyCompletion::Complete | NativeFamilyCompletion::NotEngaged
        ) && !self.obstructions.is_empty()
        {
            return Err(fail("obstructed class result cannot claim completion"));
        }
        if matches!(&self.completion, NativeFamilyCompletion::Awaiting { reads } | NativeFamilyCompletion::Blocked { reads } if reads.is_empty())
        {
            return Err(fail(
                "pending class result has no unfinished read dependency",
            ));
        }
        if matches!(self.completion, NativeFamilyCompletion::NotEngaged)
            && !self.contextual_conflicts.is_empty()
        {
            return Err(fail("an unselected class family cannot publish a conflict"));
        }
        for conflict in &self.contextual_conflicts {
            conflict.validate(ledger)?;
        }
        for obstruction in &self.obstructions {
            ledger.source_leaves(&obstruction.support)?;
        }
        Ok(())
    }
}

use super::super::native::NativeFamilyInput;
use super::super::{NativeObstructionKind, NativeRead, NativeReadKind, NativeSupportedClash};
use super::{PreparedClassAnalysis, Support, WorldOutcome};
use crate::rule_ir::Fact;
use std::collections::BTreeSet;

/// Fixed class schema reads are declared before the joint effect graph is prepared.
pub(crate) fn preparation_reads() -> Vec<NativeRead> {
    let mut reads: BTreeSet<_> = super::EXPRESSION_DEFINITION_PREDICATES
        .iter()
        .chain(super::CONSISTENT_BLOCKING_PREDICATES)
        .copied()
        .chain([
            super::RDF_FIRST,
            super::RDF_REST,
            super::RDFS_SUBCLASSOF,
            super::OWL_EQUIVALENT_CLASS,
            super::OWL_DISJOINT_WITH,
            super::OWL_MEMBERS,
            super::OWL_DISTINCT_MEMBERS,
        ])
        .map(|predicate| NativeRead {
            predicate: Some(predicate.to_owned()),
            marker: None,
            kind: NativeReadKind::Completed,
        })
        .collect();
    for marker in super::DECLARATION_TYPE_OBJECTS
        .iter()
        .chain(super::CONSISTENT_BLOCKING_TYPE_OBJECTS)
    {
        reads.insert(NativeRead {
            predicate: Some(super::RDF_TYPE.to_owned()),
            marker: Some((*marker).to_owned()),
            kind: NativeReadKind::Completed,
        });
    }
    reads.into_iter().collect()
}

/// Positive membership and equality can grow while class conflict heads feed back.
pub(crate) fn positive_reads() -> Vec<NativeRead> {
    [
        super::RDF_TYPE,
        super::OWL_SAME_AS,
        super::OWL_DIFFERENT_FROM,
    ]
    .into_iter()
    .map(|predicate| NativeRead {
        predicate: Some(predicate.to_owned()),
        marker: None,
        kind: NativeReadKind::Positive,
    })
    .collect()
}

/// Non-writing model finalization requires the completed shared current extension.
pub(crate) fn completion_reads() -> Vec<NativeRead> {
    vec![NativeRead {
        predicate: None,
        marker: None,
        kind: NativeReadKind::Completed,
    }]
}

pub(super) struct NativeClassState {
    world: String,
    graph: Option<TermValue>,
    contract: [u8; 32],
    observed_rows: usize,
    last: Option<ClassExecutionOutcome>,
}

fn failure(detail: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.to_owned(),
    })
}

fn source_fact(source: &super::RefutationPremise) -> Fact {
    Fact {
        subject: crate::facts::skolemize(&source.subject).into_owned(),
        predicate: source.predicate.clone(),
        object: crate::facts::skolemize(&source.object).into_owned(),
    }
}

fn native_support(
    input: &NativeFamilyInput<'_>,
    ledger: &mut NativeFamilyLedger,
    facts: &[Fact],
) -> gmeow_errors::Result<Support> {
    let ids = input.support(facts, ledger)?;
    Ok(Support::from_native(ledger.source_leaves(&ids)?, ids))
}

fn relevant(predicate: &str) -> bool {
    super::EXPRESSION_DEFINITION_PREDICATES.contains(&predicate)
        || super::CONSISTENT_BLOCKING_PREDICATES.contains(&predicate)
        || matches!(
            predicate,
            super::RDF_TYPE
                | super::RDF_FIRST
                | super::RDF_REST
                | super::RDFS_SUBCLASSOF
                | super::OWL_DISJOINT_WITH
                | super::OWL_SAME_AS
                | super::OWL_DIFFERENT_FROM
                | super::OWL_MEMBERS
                | super::OWL_DISTINCT_MEMBERS
        )
}

impl PreparedClassAnalysis {
    /// Freeze schema only after its declared writers finish; bind the existing
    /// source analysis to the shared proof DAG and admit actual derived schema once.
    pub(crate) fn prepare(
        &mut self,
        input: &NativeFamilyInput<'_>,
        ledger: &mut NativeFamilyLedger,
    ) -> gmeow_errors::Result<NativeFamilyCompletion> {
        if let Some(native) = &self.native {
            if native.world != input.world()
                || native.graph.as_ref() != input.graph()
                || &native.contract != input.input_contract()
            {
                return Err(failure(
                    "prepared class schema cannot move between native input contracts",
                ));
            }
            return Ok(NativeFamilyCompletion::Complete);
        }
        let reads: Vec<_> = preparation_reads()
            .into_iter()
            .filter(|read| !input.completed(read))
            .collect();
        if !reads.is_empty() {
            return Ok(NativeFamilyCompletion::Awaiting { reads });
        }
        if self.scan.worlds.len() != 1 || !self.scan.worlds.contains_key(input.world()) {
            return Err(failure(
                "joint class preparation requires its exact retained per-world source analysis",
            ));
        }
        // Source admission is decided over the original occurrences before native
        // execution.  A refused grammar must not populate the execution proof DAG:
        // those asserted nodes would look like examined semantic evidence even though
        // the selected theory never crossed its source boundary.  The authenticated
        // ClassAdmissionObservation already retains the complete typed refusal and its
        // original premises.
        if self
            .scan
            .worlds
            .get(input.world())
            .is_some_and(|world| world.source_boundary.is_some())
        {
            return Ok(NativeFamilyCompletion::Obstructed);
        }
        let world = self
            .scan
            .worlds
            .get_mut(input.world())
            .expect("checked native world");
        for support in world
            .source_premises
            .values_mut()
            .chain(world.list_fields.values_mut())
            .chain(world.list_premises.values_mut())
            .chain(world.admission_issues.values_mut())
        {
            let facts: Vec<_> = support.rows().iter().map(source_fact).collect();
            *support = native_support(input, ledger, &facts)?;
        }
        let mut schema_changed = false;
        for fact in input.store.facts().iter().skip(input.original_row_count()) {
            if !relevant(&super::semantic_predicate(&fact.predicate)) {
                continue;
            }
            let support = native_support(input, ledger, std::slice::from_ref(fact))?;
            self.scan.ingest(
                super::supported_quad(&fact.subject, &fact.predicate, &fact.object, input.graph())?,
                support,
            );
            let predicate = super::semantic_predicate(&fact.predicate);
            schema_changed |= !matches!(
                predicate.as_str(),
                super::RDF_TYPE | super::OWL_SAME_AS | super::OWL_DIFFERENT_FROM
            ) || (predicate == super::RDF_TYPE
                && super::semantic_value(super::RDF_TYPE, fact.object.clone())
                    .as_iri()
                    .is_some_and(|marker| {
                        super::DECLARATION_TYPE_OBJECTS.contains(&marker)
                            || super::CONSISTENT_BLOCKING_TYPE_OBJECTS.contains(&marker)
                    }));
        }
        if schema_changed {
            self.scan.prepare_schema();
        }
        if self
            .scan
            .worlds
            .get(input.world())
            .is_some_and(|world| world.source_boundary.is_some())
        {
            return Ok(NativeFamilyCompletion::Obstructed);
        }
        self.native = Some(NativeClassState {
            world: input.world().to_owned(),
            graph: input.graph().cloned(),
            contract: *input.input_contract(),
            observed_rows: input.store.row_count(),
            last: None,
        });
        Ok(NativeFamilyCompletion::Complete)
    }

    /// Observe current positive facts in the same store and reuse the frozen schema.
    /// The one current result is bounded by this prepared input's world lifetime.
    pub(crate) fn evaluate(
        &mut self,
        input: &NativeFamilyInput<'_>,
        ledger: &mut NativeFamilyLedger,
    ) -> gmeow_errors::Result<ClassExecutionOutcome> {
        let prepared = self.prepare(input, ledger)?;
        if !matches!(prepared, NativeFamilyCompletion::Complete) {
            return Ok(ClassExecutionOutcome {
                world: input.world().to_owned(),
                graph: input.graph().cloned(),
                input_contract: *input.input_contract(),
                completion: prepared,
                contextual_conflicts: Vec::new(),
                obstructions: Vec::new(),
            });
        }
        let mut native = self.native.take().expect("completed class preparation");
        if input.store.row_count() < native.observed_rows {
            return Err(failure(
                "class analysis moved behind its committed native prefix",
            ));
        }
        let mut changed = false;
        for fact in &input.store.facts()[native.observed_rows..] {
            let predicate = super::semantic_predicate(&fact.predicate);
            if !relevant(&predicate) {
                continue;
            }
            let declaration = predicate == super::RDF_TYPE
                && super::semantic_value(super::RDF_TYPE, fact.object.clone())
                    .as_iri()
                    .is_some_and(|marker| {
                        super::DECLARATION_TYPE_OBJECTS.contains(&marker)
                            || super::CONSISTENT_BLOCKING_TYPE_OBJECTS.contains(&marker)
                    });
            if declaration
                || !matches!(
                    predicate.as_str(),
                    super::RDF_TYPE | super::OWL_SAME_AS | super::OWL_DIFFERENT_FROM
                )
            {
                return Err(failure(
                    "a class schema writer committed after its completed preparation frontier",
                ));
            }
            let support = native_support(input, ledger, std::slice::from_ref(fact))?;
            self.scan.ingest(
                super::supported_quad(&fact.subject, &fact.predicate, &fact.object, input.graph())?,
                support,
            );
            changed = true;
        }
        native.observed_rows = input.store.row_count();
        let prior = native.last.take();
        let mut result = if !changed { prior.clone() } else { None };
        if result.is_none() {
            result = Some(self.observe_world(input, ledger)?);
        }
        let mut result = result.expect("class current result");
        if let Some(prior) = prior {
            for conflict in prior.contextual_conflicts {
                if !result.contextual_conflicts.contains(&conflict) {
                    result.contextual_conflicts.push(conflict);
                }
            }
        }
        if matches!(
            result.completion,
            NativeFamilyCompletion::Complete
                | NativeFamilyCompletion::NotEngaged
                | NativeFamilyCompletion::Awaiting { .. }
        ) {
            let reads: Vec<_> = completion_reads()
                .into_iter()
                .filter(|read| !input.completed(read))
                .collect();
            result.completion = if reads.is_empty() {
                if self.scan.engages() {
                    NativeFamilyCompletion::Complete
                } else {
                    NativeFamilyCompletion::NotEngaged
                }
            } else {
                NativeFamilyCompletion::Awaiting { reads }
            };
        }
        result.validate(ledger)?;
        native.last = Some(result.clone());
        self.native = Some(native);
        Ok(result)
    }

    fn observe_world(
        &self,
        input: &NativeFamilyInput<'_>,
        ledger: &mut NativeFamilyLedger,
    ) -> gmeow_errors::Result<ClassExecutionOutcome> {
        let mut result = ClassExecutionOutcome {
            world: input.world().to_owned(),
            graph: input.graph().cloned(),
            input_contract: *input.input_contract(),
            completion: NativeFamilyCompletion::NotEngaged,
            contextual_conflicts: Vec::new(),
            obstructions: Vec::new(),
        };
        if !self.scan.engages() {
            return Ok(result);
        }
        let world = &self.scan.worlds[input.world()];
        for (boundary, support) in &world.admission_issues {
            let super::FragmentBoundary::SourceAdmission { issue, .. } = boundary else {
                return Err(failure("prepared class issue lost its typed grammar cause"));
            };
            result.obstructions.push(NativeFamilyObstruction {
                kind: NativeObstructionKind::UnsupportedCombination,
                detail: format!("derived class schema is not admitted: {}", issue.detail()),
                support: support.native(),
            });
        }
        if ledger.work.exhausted {
            result.completion = NativeFamilyCompletion::Exhausted;
            return Ok(result);
        }
        let available = ledger
            .work
            .allowance
            .map(|limit| limit.saturating_sub(ledger.work.consumed))
            .unwrap_or(u64::MAX);
        let mut remaining = available;
        let (outcome, obstructions) = self.scan.run_world_budget(input.world(), &mut remaining);
        result
            .obstructions
            .extend(
                obstructions
                    .into_iter()
                    .map(|(detail, support)| NativeFamilyObstruction {
                        kind: NativeObstructionKind::UnsupportedCombination,
                        detail,
                        support: support.native(),
                    }),
            );
        if !ledger.charge(available - remaining) {
            return Err(failure(
                "class analysis exceeded its supplied shared allowance",
            ));
        }
        match outcome {
            WorldOutcome::Inconsistent(proof) => {
                result.contextual_conflicts.push(ContextualConflict {
                    world: input.world().to_owned(),
                    proof,
                })
            }
            WorldOutcome::Consistent => {}
            WorldOutcome::SourceBoundary(boundary) => {
                if world.source_boundary.as_ref() != Some(&boundary)
                    || result.obstructions.is_empty()
                {
                    return Err(failure(
                        "class source boundary lost its prepared obstruction",
                    ));
                }
            }
            WorldOutcome::OutOfFragment(detail) => {
                if result.obstructions.is_empty() {
                    return Err(failure(&format!(
                        "class model boundary lost its exact prepared support: {detail}"
                    )));
                }
            }
            WorldOutcome::SearchBoundary(bound) => {
                match bound {
                    super::SearchBound::Steps => {
                        ledger.work.exhausted = true;
                        result.completion = NativeFamilyCompletion::Exhausted;
                    }
                    super::SearchBound::Depth => {
                        result.completion = NativeFamilyCompletion::Exhausted;
                    }
                    super::SearchBound::NonProgress => {}
                }
                result.obstructions.push(NativeFamilyObstruction {
                    kind: if matches!(bound, super::SearchBound::NonProgress) {
                        NativeObstructionKind::UnsupportedCombination
                    } else {
                        NativeObstructionKind::ResourceLimit
                    },
                    detail: bound.detail(),
                    support: world
                        .source_premises
                        .values()
                        .flat_map(Support::native)
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                });
            }
        }
        if !matches!(result.completion, NativeFamilyCompletion::Exhausted) {
            result.completion = if result.obstructions.is_empty() {
                NativeFamilyCompletion::Complete
            } else {
                NativeFamilyCompletion::Obstructed
            };
        }
        Ok(result)
    }
}

impl ClassExecutionOutcome {
    /// Only subjects contradicted in every branch receive a local native head.
    /// A contextual closing proof may legitimately produce no local conclusion.
    pub fn local_conclusions(&self) -> Vec<NativeSupportedClash> {
        self.contextual_conflicts
            .iter()
            .flat_map(|conflict| {
                let support = conflict.proof.native_support();
                conflict
                    .proof
                    .local_subjects()
                    .into_iter()
                    .map(move |subject| NativeSupportedClash {
                        subject: TermValue::iri(subject),
                        rule: super::RULE_CASESPLIT.to_owned(),
                        support: support.clone(),
                        committed: None,
                    })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
