// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Live execution façade over the Transaction-Logic engine.
//!
//! The public entry the MCP memory triad (`crates/pipeline`) drives: execute ONE
//! transaction program over caller-assembled facts and return its executional-entailment
//! verdict plus substrate. It drives the SAME engine the authored cases and the read-only
//! trajectory audit use ([`super::emit_program_outcome`]) — it mints no vocabulary and
//! encodes NO action theory of its own. The precondition gate IS [`super::plan_path`]'s
//! executional entailment over the supplied facts (the real start state), never a synthetic
//! boolean: hand it a start state that omits the precondition and the run fails.
//!
//! A child module of `transaction` so it reaches the engine's `pub(crate)` emission helpers
//! through that one shared path — no duplicated branch, no second authority.

use std::collections::BTreeSet;

use super::{
    EXECUTED_HYPOTHETICALLY_AS, ExecutionMode, TEMPORALLY_SUCCEEDS, TRANSACTION_SUCCEEDS,
    TRANSITION_FROM_STATE, TransactionProgram, emit_program_outcome, logic, parse_program,
    root_start, xsd_bool,
};
use crate::teleology::{TeleologyQuad, WorldFacts, triple_reifier};

/// Whether an executed transaction commits its effects or runs as the hypothetical (sandbox)
/// operator — the caller's choice (a memory tool's `dry_run` selects [`CommitMode::Hypothetical`]).
///
/// A public mirror of the engine's `pub(crate)` `ExecutionMode`, so the façade can take the
/// mode explicitly while the engine internals stay crate-private. It is the commit-vs-discard
/// `ExecutionMode` facet, NOT modal possibility — see `slices/grounding/logic/design/LOGIC-TRANSACTION.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitMode {
    /// Materialize the effects (the default for a write that is finalized).
    Committed,
    /// Run the sandbox operator: decide the verdict, discard the effects, emit only a witness.
    Hypothetical,
}

/// The outcome of one executed transaction.
///
/// A SUM over the four legal shapes so an illegal state — a committed run carrying a
/// hypothetical witness, or a hypothetical run carrying a committed path — is unrepresentable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxReceipt {
    /// Committed run, executional entailment held: effects are materialized. `outcome_nquads`
    /// is the full `logic:TransactionOutcome` substrate (verdict + executed path + per-step
    /// supersession) as N-Quads; `path_len` is the number of states on the executed path.
    CommittedSuccess {
        outcome_nquads: String,
        path_len: usize,
    },
    /// Committed run, executional entailment failed: the start state is untouched, no substrate.
    CommittedFailure { reason: String },
    /// Hypothetical (sandbox) run that WOULD succeed: a content-addressed witness, no effects
    /// asserted (suppression-never-erasure holds for free — nothing is committed, so nothing
    /// is erased).
    HypotheticalSuccess { witness: String },
    /// Hypothetical run that would fail.
    HypotheticalFailure { reason: String },
}

impl TxReceipt {
    /// Did executional entailment hold (a path exists from the start)? Mode-invariant — the
    /// verdict is identical committed vs hypothetical; only the emitted substrate differs.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        matches!(
            self,
            TxReceipt::CommittedSuccess { .. } | TxReceipt::HypotheticalSuccess { .. }
        )
    }
}

/// Execute the one transaction program rooted at `root` in graph `world`, parsed from
/// `nquads`, under `mode`, and return its executional-entailment outcome.
///
/// `nquads` is the caller-assembled transaction world: the program root (a primitive bearing
/// `logic:instantiatesSchema` + `logic:transitionFromState`, or a combinator), the action
/// schema(s) (`logic:precondition` / `logic:effect` with `logic:ins` / `logic:del`), and the
/// start state's `logic:situationObtains` facts. The precondition is decided by the engine's
/// executional entailment over THESE facts — the schema facts are the single authority; this
/// façade encodes none of them.
///
/// # Errors
///
/// Propagates any STRUCTURAL fault from the engine ([`root_start`], [`parse_program`],
/// [`emit_program_outcome`]) — a missing or multi-valued start state, a malformed program, a
/// primitive schema with no effect, or a non-terminating program — as a hard error (no
/// optionality, no degraded fallback).
pub fn execute_transaction(
    nquads: &str,
    world: &str,
    root: &str,
    mode: CommitMode,
) -> gmeow_errors::Result<TxReceipt> {
    let dataset =
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Store {
                detail: format!("N-Quads parse error: {error}"),
            })
        })?;
    execute_transaction_dataset(&dataset, world, root, mode)
}

/// Execute the same transaction authority over a native input carrier.
/// This adapter preserves the selected world and avoids RDF serialization/parsing.
/// It returns the same committed or hypothetical receipt as [`execute_transaction`].
pub fn execute_transaction_dataset(
    dataset: &purrdf::RdfDataset,
    world: &str,
    root: &str,
    mode: CommitMode,
) -> gmeow_errors::Result<TxReceipt> {
    PreparedTransaction::new(dataset, world, root)?.execute(mode)
}

/// One transaction root prepared against an immutable, world-scoped input.
///
/// Fact indexing, operand lowering and the start-state provenance anchor are
/// computed once. Each execution uses the same native transaction authority with
/// fresh work counters and effects; committed and hypothetical results cannot
/// contaminate later runs. A changed source requires a new preparation. This is
/// situation-level execution, not an RDF view-update or a lens-law certificate.
pub struct PreparedTransaction {
    facts: WorldFacts,
    program: TransactionProgram,
    world: String,
    root: String,
    start: String,
    sits: BTreeSet<String>,
    source: String,
}

impl PreparedTransaction {
    /// Prepare the selected root from all three native RDF tables in `world`.
    ///
    /// # Errors
    /// Refuses a missing or ambiguous start state and malformed program structure.
    /// Runtime preconditions and termination remain checked on every execution.
    pub fn new(
        dataset: &purrdf::RdfDataset,
        world: &str,
        root: &str,
    ) -> gmeow_errors::Result<Self> {
        let facts = WorldFacts::read_dataset(dataset, world);
        let (start, sits) = root_start(&facts, root)?;
        let program = parse_program(&facts, root, 0)?;
        // Ground the receipt on the real input anchor, including primitive roots
        // that have no rdf:type assertion to reify.
        let source = triple_reifier(root, &logic(TRANSITION_FROM_STATE), &start)?;
        Ok(Self {
            facts,
            program,
            world: world.to_owned(),
            root: root.to_owned(),
            start,
            sits,
            source,
        })
    }

    /// Execute with fresh runtime state against the exact prepared input.
    ///
    /// # Errors
    /// Propagates structural, termination and receipt-emission failures from the
    /// shared engine without returning a partial receipt.
    pub fn execute(&self, mode: CommitMode) -> gmeow_errors::Result<TxReceipt> {
        let Self {
            facts,
            program,
            world,
            root,
            start,
            sits,
            source,
        } = self;

        let exec_mode = match mode {
            CommitMode::Committed => ExecutionMode::Committed,
            CommitMode::Hypothetical => ExecutionMode::Hypothetical,
        };
        let quads =
            emit_program_outcome(facts, world, root, program, exec_mode, start, sits, source)?;

        let succeeds_pred = logic(TRANSACTION_SUCCEEDS);
        let succeeded_true = xsd_bool(true);
        let succeeded = quads
            .iter()
            .any(|q| q.predicate == succeeds_pred && q.object == succeeded_true);

        Ok(match (mode, succeeded) {
            (CommitMode::Committed, true) => TxReceipt::CommittedSuccess {
                path_len: path_len(&quads),
                outcome_nquads: render_nquads(&quads),
            },
            (CommitMode::Committed, false) => TxReceipt::CommittedFailure {
                reason: format!("executional entailment failed from start state <{start}>"),
            },
            (CommitMode::Hypothetical, true) => TxReceipt::HypotheticalSuccess {
                witness: witness(&quads).ok_or_else(|| {
                    gmeow_errors::Diag::of_kind(crate::error::Transaction {
                        detail: "hypothetical success emitted no logic:executedHypotheticallyAs \
                             witness"
                            .to_owned(),
                    })
                })?,
            },
            (CommitMode::Hypothetical, false) => TxReceipt::HypotheticalFailure {
                reason: format!("executional entailment failed from start state <{start}>"),
            },
        })
    }
}

/// States on the executed path = `logic:temporallySucceeds` edges + 1 (a one-step run walks
/// start → end: one edge, two states). Zero when no path was materialized.
fn path_len(quads: &[TeleologyQuad]) -> usize {
    let temporally_succeeds = logic(TEMPORALLY_SUCCEEDS);
    let edges = quads
        .iter()
        .filter(|q| q.predicate == temporally_succeeds)
        .count();
    if edges == 0 { 0 } else { edges + 1 }
}

/// The content-addressed `logic:executedHypotheticallyAs` witness (a quoted N3 string
/// literal) if the run emitted one.
fn witness(quads: &[TeleologyQuad]) -> Option<String> {
    let executed_hypothetically_as = logic(EXECUTED_HYPOTHETICALLY_AS);
    quads
        .iter()
        .find(|q| q.predicate == executed_hypothetically_as)
        .map(|q| q.object.trim_matches('"').to_owned())
}

/// Render the outcome substrate as N-Quads. Each [`TeleologyQuad`]'s `object` is already in
/// canonical N3 form (an `<iri>` or a `"lit"^^<dt>`); subject / predicate / graph are IRIs.
/// Lines are sorted so the rendering is deterministic regardless of emission order.
fn render_nquads(quads: &[TeleologyQuad]) -> String {
    let mut lines: Vec<String> = quads
        .iter()
        .map(|q| {
            format!(
                "<{}> <{}> {} <{}> .",
                q.subject, q.predicate, q.object, q.graph
            )
        })
        .collect();
    lines.sort();
    lines.join("\n")
}

#[path = "execute.tests.rs"]
#[cfg(test)]
mod tests;
