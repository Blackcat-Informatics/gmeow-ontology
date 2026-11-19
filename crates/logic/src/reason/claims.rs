// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Claim ingress for the execution of one selected theory.
//!
//! A scoped claim is a reifier or `rdf:Statement` node that carries a scope
//! coordinate. The dataset holds only its carrier, so the bare-program entries never
//! read the claimed statement. This module is the one place a scoped claim becomes a
//! fact. The evidence-only route and the distinct source-resolved module route
//! each enforce their closed carrier contract against the complete original source.
//! Descriptive graph-box annotations supply neither evidence nor assertion authority.
//! Read claims retain their carried native RDF terms. Excluded modules contribute
//! no premise; other unread scoped claims retain their boundaries. A quotation
//! carries no scope or evidence coordinate and is never read as an assertion.

use std::collections::BTreeSet;

use gmeow_logic_compile::frontend::{ClaimCarrier, ClaimStatement, CompiledTheory, SourceNode};
use gmeow_logic_compile::ir::{
    AtomicTerm, ContextualScope, EvidenceCoordinate, LogicAxiom, LogicProgram, ScopeMembership,
    TheorySelection,
};
use gmeow_logic_compile::source_inventory::{
    ResolvedTheorySelection, SourceInventory, TheoryUnitSelection,
};
use purrdf::TermValue;

use super::program::prepare_default_reasoning_input;
use super::{PreparedReasoningInput, reason_err};
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
    /// Only the original carriers whose own closed assertion contracts admitted
    /// this statement. Equal canonical content grants no sibling admission.
    pub carriers: Vec<ClaimCarrier>,
}

/// Why a scoped claim was not read as a fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnreadReason {
    /// The scope selects a context outside the selected theory. The `logic:` names
    /// of the selecting coordinates are listed.
    Context(Vec<&'static str>),
    /// A statement outside the admitted contract describes the claim's carrier, so the
    /// claim may be placed, denied or qualified. `carrier` is the reifier or
    /// statement node concerned, `predicate` the first such predicate IRI in sorted
    /// order, and `statements` counts all of them.
    Qualified {
        carrier: String,
        predicate: String,
        /// The named graph of that statement; none for the default graph.
        graph: Option<String>,
        statements: usize,
    },
    /// Another statement mentions the claim's carrier: inside a triple term at any
    /// depth, as its predicate, or as a term of a classic statement node. `by` is the first
    /// mentioning node in sorted order and `statements` counts the mentions. It
    /// counts whether that node is itself read, unread or a bare quotation.
    Mentioned {
        carrier: String,
        by: String,
        /// The named graph of that statement; none for the default graph.
        graph: Option<String>,
        statements: usize,
    },
    /// The claim's carrier names a graph, so the node also stands for a context.
    GraphName { carrier: String },
    /// The claim has a variable in a term position.
    NotGround,
    /// The carried statement could not be identified in the dataset.
    SourceUnresolved,
    /// The claim states a term other than the RDF term its carrier holds.
    SourceMismatch,
}

/// The place of a statement in a drop: nothing for the default graph.
fn in_graph(graph: Option<&str>) -> String {
    graph.map_or_else(String::new, |graph| format!(" in graph {graph}"))
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
                graph,
                statements,
            } => write!(
                f,
                "{statements} statement(s) about its carrier {carrier} are not evidence, first <{predicate}>{}",
                in_graph(graph.as_deref())
            ),
            Self::Mentioned {
                carrier,
                by,
                graph,
                statements,
            } => write!(
                f,
                "its carrier {carrier} is mentioned by {statements} statement(s), first that of {by}{}",
                in_graph(graph.as_deref())
            ),
            Self::GraphName { carrier } => write!(f, "its carrier {carrier} names a graph"),
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
    /// The original carrier, when the scoped axiom has a reification contributor.
    pub carrier: Option<ClaimCarrier>,
    /// What kept the claim out of the selected world.
    pub reason: UnreadReason,
}

impl UnreadClaim {
    /// Deterministic source detail retaining distinct carriers of one axiom.
    #[must_use]
    pub fn source_detail(&self) -> String {
        self.carrier.as_ref().map_or_else(
            || format!("program:axiom:{}", self.axiom),
            |carrier| format!("program:axiom:{} carrier {}", self.axiom, carrier.name),
        )
    }
}

/// The account of every scoped axiom of one compiled theory under one selection.
///
/// It is bound to the input the read claims were added to, to the selection, and to
/// the scoped axioms it classified, and the selected-theory entry refuses any other
/// combination.
#[derive(Debug, Clone)]
pub struct ClaimIngress {
    resolved: ResolvedTheorySelection,
    read: Vec<ReadClaim>,
    unread: Vec<UnreadClaim>,
    /// Compact sorted offsets into `unread`, never cloned source obligations.
    unread_carriers: Vec<(SourceNode, usize)>,
    excluded: Vec<usize>,
    input_contract: [u8; 32],
    /// The scoped axioms the ledger classified, to recognise their program.
    claims: Vec<(usize, LogicAxiom)>,
}

impl ClaimIngress {
    pub(super) fn authorize_source(&self, theory: &CompiledTheory) -> gmeow_errors::Result<()> {
        self.resolved.authorize_source(theory)
    }
    /// The selection the claims were classified under.
    #[must_use]
    pub fn selection(&self) -> &TheorySelection {
        self.resolved.request()
    }

    #[must_use]
    pub fn resolved_selection(&self) -> &ResolvedTheorySelection {
        &self.resolved
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

    pub(super) fn unread_carrier(&self, carrier: SourceNode) -> Option<&UnreadClaim> {
        self.unread_carriers
            .binary_search_by_key(&carrier, |(source, _)| *source)
            .ok()
            .map(|position| &self.unread[self.unread_carriers[position].1])
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
                && self.excluded.binary_search(index).is_err()
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
/// Filter physical source streams through the compiler-issued selection before
/// native ingestion, existential collection or contextual preparation. Ground
/// evidence-only claims and source-resolved module assertions use separate closed
/// carrier contracts. Every envelope check reads the complete original source,
/// including excluded modules and named graphs. A read claim preserves native
/// blank identity, datatype, language and direction. Other selected claims are
/// named as unread; authorized module exclusions remain outside this theory.
///
/// # Errors
/// Returns a foreign source/selection, ingestion failure or unsupported target.
pub fn prepare_selected_theory_input(
    inventory: &SourceInventory<'_>,
    world: &LogicalGraph,
    selection: &ResolvedTheorySelection,
) -> gmeow_errors::Result<(PreparedReasoningInput, ClaimIngress)> {
    if *world != LogicalGraph::Default {
        return Err(reason_err(format!(
            "selected-theory claim ingress supports only the default graph, not {world:?}"
        )));
    }
    selection.authorize_inventory(inventory)?;
    let theory = inventory.theory();
    let dataset = theory.source().dataset();
    let program = theory.program();
    let mut facts = Vec::new();
    let mut read = Vec::new();
    let mut unread = Vec::new();
    let mut excluded = Vec::new();
    let resolver = theory.claim_resolver();
    let view = selection.program_view(inventory)?;
    for (index, axiom) in scoped(program) {
        let membership = view.axiom_selection(index)?;
        if matches!(membership, TheoryUnitSelection::OutsideRequestedTheory) {
            excluded.push(index);
            continue;
        }
        let mut scope = axiom.scope.clone();
        if selection.request().is_composed() && matches!(membership, TheoryUnitSelection::Included)
        {
            scope.module = None;
        }
        let scope_membership = selection.request().membership(&scope);
        let contributions = resolver.selected_statements(index, inventory, selection)?;
        let no_carriers = contributions.is_empty();
        let mut carriers = Vec::new();
        for (carrier, statement) in contributions
            .into_iter()
            .map(|(carrier, statement)| (Some(carrier), statement))
            .chain(no_carriers.then_some((None, ClaimStatement::Unresolved)))
        {
            let reason = match &scope_membership {
                ScopeMembership::UntranslatedContext { coordinates } => {
                    UnreadReason::Context(coordinates.clone())
                }
                ScopeMembership::InTheory { .. } => match claimed_fact(statement, dataset, axiom) {
                    Ok(fact) => {
                        facts.push(fact);
                        carriers
                            .push(carrier.expect("a resolved statement has its original carrier"));
                        continue;
                    }
                    Err(reason) => reason,
                },
            };
            unread.push(UnreadClaim {
                axiom: index,
                carrier,
                reason,
            });
        }
        if let ScopeMembership::InTheory { evidence } = scope_membership
            && !carriers.is_empty()
        {
            read.push(ReadClaim {
                axiom: index,
                evidence,
                carriers,
            });
        }
    }
    let source_view = selection.source_view(inventory)?;
    let mut input = prepare_default_reasoning_input(&source_view, selection)?;
    input.assert_facts(world, facts)?;
    let mut unread_carriers: Vec<_> = unread
        .iter()
        .enumerate()
        .filter_map(|(index, claim)| {
            claim
                .carrier
                .as_ref()
                .map(|carrier| (carrier.source, index))
        })
        .collect();
    unread_carriers.sort_unstable();
    let ingress = ClaimIngress {
        resolved: selection.clone(),
        read,
        unread,
        unread_carriers,
        excluded,
        input_contract: *input.ingress_contract(),
        claims: scoped(program)
            .map(|(index, axiom)| (index, axiom.clone()))
            .collect(),
    };
    Ok((input, ingress))
}

/// The fact a claim inside the selected theory states, or why it states none.
fn claimed_fact(
    statement: ClaimStatement,
    dataset: &purrdf::RdfDataset,
    axiom: &LogicAxiom,
) -> Result<Fact, UnreadReason> {
    if axiom.subject.starts_with('?')
        || axiom.predicate.starts_with('?')
        || matches!(axiom.obj, AtomicTerm::Var(_))
    {
        return Err(UnreadReason::NotGround);
    }
    let (subject, predicate, object) = match statement {
        ClaimStatement::Resolved {
            subject,
            predicate,
            object,
        } => (subject, predicate, object),
        ClaimStatement::Qualified {
            carrier,
            predicate,
            graph,
            statements,
        } => {
            return Err(UnreadReason::Qualified {
                carrier,
                predicate,
                graph,
                statements,
            });
        }
        ClaimStatement::Mentioned {
            carrier,
            by,
            graph,
            statements,
        } => {
            return Err(UnreadReason::Mentioned {
                carrier,
                by,
                graph,
                statements,
            });
        }
        ClaimStatement::GraphName { carrier } => {
            return Err(UnreadReason::GraphName { carrier });
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
