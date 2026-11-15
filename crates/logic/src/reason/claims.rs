// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Claim ingress for the execution of one selected theory.
//!
//! A scoped claim is a reifier or `rdf:Statement` node that carries a scope
//! coordinate. The dataset holds only its carrier, so the bare-program entries never
//! read the claimed statement. This module is the one place a scoped claim becomes a
//! fact: a claim whose scope is inside the selected theory, and whose carrier says
//! nothing but evidence, is added to the selected world with the exact RDF terms of
//! its carrier, and every other scoped claim is recorded with the reason it was left
//! out. The compiler owns the evidence allow-list and the resolution. A quotation, which carries no scope
//! coordinate, is not a scoped axiom of the program and is never read.

use std::collections::BTreeSet;

use gmeow_logic_compile::frontend::{
    ClaimResolver, ClaimStatement, CompiledTheory, PreparedLogicSource,
};
use gmeow_logic_compile::ir::{
    AtomicTerm, ContextualScope, EvidenceCoordinate, LogicAxiom, LogicProgram, ScopeMembership,
    TheorySelection,
};
use purrdf::TermValue;

use super::{PreparedReasoningInput, prepare_reasoning_input, reason_err};
use crate::physical::LogicalGraph;
use crate::rule_ir::Fact;

/// A scoped claim that was read as a fact of the selected world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadClaim {
    /// Index of the claim in the compiled program's axioms.
    pub axiom: usize,
    /// The evidence coordinates of its scope. They are metadata of the report and
    /// stay on the carrier; they are not part of the fact.
    pub evidence: BTreeSet<EvidenceCoordinate>,
}

/// Why a scoped claim was not read as a fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnreadReason {
    /// The scope selects a context outside the selected theory. The `logic:` names
    /// of the selecting coordinates are listed.
    Context(Vec<&'static str>),
    /// Something other than evidence is said about the claim's carrier, so the
    /// claim may be placed, denied or qualified. `carrier` is the reifier or
    /// statement node concerned, `predicate` the first such predicate IRI in sorted
    /// order, and `statements` counts all of them.
    Qualified {
        carrier: String,
        predicate: String,
        statements: usize,
    },
    /// Another carried statement mentions the claim's carrier: inside a triple term
    /// at any depth, or as a term of a classic statement node. `by` is the first
    /// mentioning node in sorted order and `statements` counts the mentions. It
    /// counts whether that node is itself read, unread or a bare quotation.
    Mentioned {
        carrier: String,
        by: String,
        statements: usize,
    },
    /// The claim has a variable in a term position.
    NotGround,
    /// The carried statement could not be identified in the dataset.
    SourceUnresolved,
    /// The claim states a term other than the RDF term its carrier holds.
    SourceMismatch,
}

impl std::fmt::Display for UnreadReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Context(coordinates) => write!(
                f,
                "its scope selects a context outside the selected theory ({})",
                coordinates.join(", ")
            ),
            Self::Qualified {
                carrier,
                predicate,
                statements,
            } => write!(
                f,
                "{statements} statement(s) about its carrier {carrier} are not evidence, first <{predicate}>"
            ),
            Self::Mentioned {
                carrier,
                by,
                statements,
            } => write!(
                f,
                "its carrier {carrier} is mentioned by {statements} carried statement(s), first that of {by}"
            ),
            Self::NotGround => f.write_str("the claim has a variable in a term position"),
            Self::SourceUnresolved => {
                f.write_str("its carrier does not identify exactly one statement of the source")
            }
            Self::SourceMismatch => f.write_str(
                "its carrier rewrites a term, so the claim does not state the carried RDF terms",
            ),
        }
    }
}

/// A scoped claim that was not read, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadClaim {
    /// Index of the claim in the compiled program's axioms.
    pub axiom: usize,
    /// What kept the claim out of the selected world.
    pub reason: UnreadReason,
}

/// The account of every scoped axiom of one compiled theory under one selection.
///
/// It is bound to the input the read claims were added to, to the selection, and to
/// the scoped axioms it classified, and the selected-theory entry refuses any other
/// combination.
#[derive(Debug, Clone)]
pub struct ClaimIngress {
    selection: TheorySelection,
    read: Vec<ReadClaim>,
    unread: Vec<UnreadClaim>,
    input_contract: [u8; 32],
    /// The scoped axioms the ledger classified, to recognise their program.
    claims: Vec<(usize, LogicAxiom)>,
}

impl ClaimIngress {
    /// The selection the claims were classified under.
    #[must_use]
    pub fn selection(&self) -> &TheorySelection {
        &self.selection
    }

    /// Claims read as facts, in axiom order.
    #[must_use]
    pub fn read(&self) -> &[ReadClaim] {
        &self.read
    }

    /// Claims not read, in axiom order.
    #[must_use]
    pub fn unread(&self) -> &[UnreadClaim] {
        &self.unread
    }

    /// The first scoped axiom of `program` this ledger does not account for.
    fn unaccounted(&self, program: &LogicProgram) -> Option<usize> {
        scoped(program).map(|(index, _)| index).find(|index| {
            self.read
                .binary_search_by_key(index, |claim| claim.axiom)
                .is_err()
                && self
                    .unread
                    .binary_search_by_key(index, |claim| claim.axiom)
                    .is_err()
        })
    }

    /// Refuse an input, program or selection other than the one this ledger accounts
    /// for. Execution under the selected-theory profile ignores axiom scopes, so it
    /// may only start from the exact input whose claims were admitted.
    pub(super) fn authorize(
        &self,
        program: &LogicProgram,
        input: &PreparedReasoningInput,
    ) -> gmeow_errors::Result<()> {
        if input.ingress_contract() != &self.input_contract {
            return Err(reason_err(
                "the claim ingress ledger does not belong to this reasoning input".to_owned(),
            ));
        }
        if let Some(index) = self.unaccounted(program) {
            return Err(reason_err(format!(
                "scoped axiom {index} of the program is absent from the claim ingress ledger"
            )));
        }
        if !scoped(program).eq(self.claims.iter().map(|(index, axiom)| (*index, axiom))) {
            return Err(reason_err(
                "the claim ingress ledger was computed for a different program".to_owned(),
            ));
        }
        Ok(())
    }
}

fn scoped(program: &LogicProgram) -> impl Iterator<Item = (usize, &LogicAxiom)> {
    let unscoped = ContextualScope::default();
    program
        .axioms
        .iter()
        .enumerate()
        .filter(move |(_, axiom)| axiom.scope != unscoped)
}

/// Prepare the native input of one selected theory.
///
/// The source statements are ingested exactly as [`prepare_reasoning_input`] ingests
/// them. Then every scoped axiom of the compiled program is classified by
/// [`TheorySelection::membership`]. A ground claim inside the theory, whose carrier
/// the compiler resolves as evidence only (nothing is said about the carrier except
/// its reification structure, `logic:provenance` and `logic:confidence`, and no other
/// carried statement mentions it), is added to
/// `world` as one fact whose subject, predicate and object are the
/// native terms of the statement its carrier holds, so blank identity, datatype,
/// language and direction are those of the source. Every other scoped axiom is
/// recorded as unread. A claim that is also asserted plainly adds no second fact.
///
/// # Errors
/// Returns ingestion failures and a world that aliases another source graph.
pub fn prepare_selected_theory_input(
    theory: &CompiledTheory,
    world: &LogicalGraph,
    selection: &TheorySelection,
) -> gmeow_errors::Result<(PreparedReasoningInput, ClaimIngress)> {
    let dataset = theory.source().dataset();
    let program = theory.program();
    let mut facts = Vec::new();
    let mut read = Vec::new();
    let mut unread = Vec::new();
    let resolver = theory.claim_resolver();
    for (index, axiom) in scoped(program) {
        let reason = match selection.membership(&axiom.scope) {
            ScopeMembership::UntranslatedContext { coordinates } => {
                UnreadReason::Context(coordinates)
            }
            ScopeMembership::InTheory { evidence } => {
                match claimed_fact(&resolver, dataset, index, axiom) {
                    Ok(fact) => {
                        facts.push(fact);
                        read.push(ReadClaim {
                            axiom: index,
                            evidence,
                        });
                        continue;
                    }
                    Err(reason) => reason,
                }
            }
        };
        unread.push(UnreadClaim {
            axiom: index,
            reason,
        });
    }
    let mut input = prepare_reasoning_input(dataset)?;
    input.assert_facts(world, facts)?;
    let ingress = ClaimIngress {
        selection: selection.clone(),
        read,
        unread,
        input_contract: *input.ingress_contract(),
        claims: scoped(program)
            .map(|(index, axiom)| (index, axiom.clone()))
            .collect(),
    };
    Ok((input, ingress))
}

/// The fact a claim inside the selected theory states, or why it states none.
fn claimed_fact(
    resolver: &ClaimResolver<'_, PreparedLogicSource>,
    dataset: &purrdf::RdfDataset,
    index: usize,
    axiom: &LogicAxiom,
) -> Result<Fact, UnreadReason> {
    if axiom.subject.starts_with('?')
        || axiom.predicate.starts_with('?')
        || matches!(axiom.obj, AtomicTerm::Var(_))
    {
        return Err(UnreadReason::NotGround);
    }
    let (subject, predicate, object) = match resolver.statement(index) {
        ClaimStatement::Resolved {
            subject,
            predicate,
            object,
        } => (subject, predicate, object),
        ClaimStatement::Qualified {
            carrier,
            predicate,
            statements,
        } => {
            return Err(UnreadReason::Qualified {
                carrier,
                predicate,
                statements,
            });
        }
        ClaimStatement::Mentioned {
            carrier,
            by,
            statements,
        } => {
            return Err(UnreadReason::Mentioned {
                carrier,
                by,
                statements,
            });
        }
        ClaimStatement::Unresolved => return Err(UnreadReason::SourceUnresolved),
        ClaimStatement::Mismatch => return Err(UnreadReason::SourceMismatch),
    };
    let native = |term| super::dataset::native(dataset, term);
    let subject @ (TermValue::Iri(_) | TermValue::Blank { .. }) = native(subject) else {
        return Err(UnreadReason::SourceUnresolved);
    };
    let TermValue::Iri(predicate) = native(predicate) else {
        return Err(UnreadReason::SourceUnresolved);
    };
    let object = native(object);
    if matches!(object, TermValue::Triple { .. }) {
        return Err(UnreadReason::SourceUnresolved);
    }
    Ok(Fact {
        subject,
        predicate,
        object,
    })
}

#[cfg(test)]
#[path = "claims.tests.rs"]
mod tests;
