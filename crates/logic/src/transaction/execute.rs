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

use super::{
    emit_program_outcome, logic, parse_program, root_start, xsd_bool, ExecutionMode,
    EXECUTED_HYPOTHETICALLY_AS, TEMPORALLY_SUCCEEDS, TRANSACTION_SUCCEEDS, TRANSITION_FROM_STATE,
};
use crate::store::WorldStore;
use crate::teleology::{triple_reifier, TeleologyQuad, WorldFacts};

/// Whether an executed transaction commits its effects or runs as the hypothetical (sandbox)
/// operator — the caller's choice (a memory tool's `dry_run` selects [`CommitMode::Hypothetical`]).
///
/// A public mirror of the engine's `pub(crate)` `ExecutionMode`, so the façade can take the
/// mode explicitly while the engine internals stay crate-private. It is the commit-vs-discard
/// `ExecutionMode` facet, NOT modal possibility — see `slices/core/logic/design/LOGIC-TRANSACTION.md`.
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
) -> Result<TxReceipt, String> {
    let store = WorldStore::new();
    store.load_nquads(nquads)?;
    let facts = WorldFacts::read(&store, world);

    let (start, sits) = root_start(&facts, root)?;
    let program = parse_program(&facts, root, 0)?;
    // Ground the outcome on the root's `logic:transitionFromState` quad — a REAL input quad.
    // A primitive root has no `rdf:type` to reify, and the explain engine refuses a dangling
    // reifier; mirror `trajectory::emit_trajectory_audits`, which grounds on the same anchor.
    let source = triple_reifier(root, &logic(TRANSITION_FROM_STATE), &start)?;

    let exec_mode = match mode {
        CommitMode::Committed => ExecutionMode::Committed,
        CommitMode::Hypothetical => ExecutionMode::Hypothetical,
    };
    let quads = emit_program_outcome(
        &facts, world, root, &program, exec_mode, &start, &sits, &source,
    )?;

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
                "hypothetical success emitted no logic:executedHypotheticallyAs witness".to_owned()
            })?,
        },
        (CommitMode::Hypothetical, false) => TxReceipt::HypotheticalFailure {
            reason: format!("executional entailment failed from start state <{start}>"),
        },
    })
}

/// States on the executed path = `logic:temporallySucceeds` edges + 1 (a one-step run walks
/// start → end: one edge, two states). Zero when no path was materialized.
fn path_len(quads: &[TeleologyQuad]) -> usize {
    let temporally_succeeds = logic(TEMPORALLY_SUCCEEDS);
    let edges = quads
        .iter()
        .filter(|q| q.predicate == temporally_succeeds)
        .count();
    if edges == 0 {
        0
    } else {
        edges + 1
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const W: &str = "https://example.org/t7/world";

    fn li(local: &str) -> String {
        format!("https://blackcatinformatics.ca/logic/{local}")
    }
    fn ex(local: &str) -> String {
        format!("https://example.org/t7/{local}")
    }
    fn q(s: &str, p: &str, o: &str) -> String {
        format!("<{s}> <{p}> <{o}> <{W}> .\n")
    }

    /// A one-step `store_claim` transaction world. The precondition (`wellFormedClaim`)
    /// obtains at the start state iff `ready`.
    fn store_world(ready: bool) -> String {
        let mut s = String::new();
        s += &q(
            &ex("txStore"),
            &li("instantiatesSchema"),
            &ex("storeSchema"),
        );
        s += &q(&ex("txStore"), &li("transitionFromState"), &ex("start"));
        if ready {
            s += &q(
                &ex("start"),
                &li("situationObtains"),
                &ex("wellFormedClaim"),
            );
        }
        s += &q(
            &ex("storeSchema"),
            &li("precondition"),
            &ex("wellFormedClaim"),
        );
        s += &q(&ex("storeSchema"), &li("effect"), &ex("storeEffect"));
        s += &q(&ex("storeEffect"), &li("ins"), &ex("claimInMemory"));
        s += &q(&ex("storeEffect"), &li("ins"), &ex("targetClaimExists"));
        s
    }

    /// A one-step `revise_belief` transaction world — `revise_belief` IS `store_claim`'s
    /// compensation. The del targets (`claimInMemory`, `targetClaimExists`) obtain at the
    /// start so the committed run retires them via the supersession quartet (P10).
    fn revise_world() -> String {
        let mut s = String::new();
        s += &q(
            &ex("txRevise"),
            &li("instantiatesSchema"),
            &ex("reviseSchema"),
        );
        s += &q(&ex("txRevise"), &li("transitionFromState"), &ex("start"));
        s += &q(
            &ex("start"),
            &li("situationObtains"),
            &ex("targetClaimExists"),
        );
        s += &q(&ex("start"), &li("situationObtains"), &ex("claimInMemory"));
        s += &q(
            &ex("reviseSchema"),
            &li("precondition"),
            &ex("targetClaimExists"),
        );
        s += &q(&ex("reviseSchema"), &li("effect"), &ex("reviseEffect"));
        s += &q(&ex("reviseEffect"), &li("ins"), &ex("claimSuppressed"));
        s += &q(&ex("reviseEffect"), &li("del"), &ex("claimInMemory"));
        s += &q(&ex("reviseEffect"), &li("del"), &ex("targetClaimExists"));
        s
    }

    #[test]
    fn committed_store_succeeds_when_precondition_obtains() {
        let receipt =
            execute_transaction(&store_world(true), W, &ex("txStore"), CommitMode::Committed)
                .expect("execute");
        match receipt {
            TxReceipt::CommittedSuccess {
                outcome_nquads,
                path_len,
            } => {
                assert!(path_len >= 2, "one-step run walks start → end: {path_len}");
                assert!(
                    outcome_nquads.contains(&li("TransactionOutcome")),
                    "committed substrate carries the outcome node"
                );
                assert!(
                    outcome_nquads.contains(&li("transactionSucceeds")),
                    "committed substrate carries the verdict"
                );
            }
            other => panic!("expected CommittedSuccess, got {other:?}"),
        }
    }

    #[test]
    fn committed_store_fails_when_precondition_absent_leaves_start_untouched() {
        let receipt = execute_transaction(
            &store_world(false),
            W,
            &ex("txStore"),
            CommitMode::Committed,
        )
        .expect("execute");
        match receipt {
            TxReceipt::CommittedFailure { .. } => {}
            other => panic!("expected CommittedFailure, got {other:?}"),
        }
        assert!(!receipt_succeeded(
            &store_world(false),
            CommitMode::Committed
        ));
    }

    #[test]
    fn hypothetical_success_emits_witness_and_no_committed_substrate() {
        let receipt = execute_transaction(
            &store_world(true),
            W,
            &ex("txStore"),
            CommitMode::Hypothetical,
        )
        .expect("execute");
        match receipt {
            TxReceipt::HypotheticalSuccess { witness } => {
                assert!(!witness.is_empty(), "a sandbox run leaves a witness trace");
            }
            other => panic!("expected HypotheticalSuccess, got {other:?}"),
        }
    }

    #[test]
    fn revise_is_compensation_supersession_quartet_present() {
        let receipt =
            execute_transaction(&revise_world(), W, &ex("txRevise"), CommitMode::Committed)
                .expect("execute");
        match receipt {
            TxReceipt::CommittedSuccess { outcome_nquads, .. } => {
                // The supersession quartet (P10 — superseded, never erased).
                for pred in [
                    "activeInState",
                    "validUntilState",
                    "retiredByTransaction",
                    "supersededBy",
                ] {
                    assert!(
                        outcome_nquads.contains(&li(pred)),
                        "committed revise emits logic:{pred}"
                    );
                }
            }
            other => panic!("expected CommittedSuccess, got {other:?}"),
        }
    }

    #[test]
    fn verdict_is_mode_invariant_only_substrate_differs() {
        // Same world, both modes succeed; same world (no precondition) both fail.
        assert!(receipt_succeeded(&store_world(true), CommitMode::Committed));
        assert!(receipt_succeeded(
            &store_world(true),
            CommitMode::Hypothetical
        ));
        assert!(!receipt_succeeded(
            &store_world(false),
            CommitMode::Committed
        ));
        assert!(!receipt_succeeded(
            &store_world(false),
            CommitMode::Hypothetical
        ));
    }

    #[test]
    fn execution_is_deterministic() {
        let a = execute_transaction(&store_world(true), W, &ex("txStore"), CommitMode::Committed)
            .expect("execute");
        let b = execute_transaction(&store_world(true), W, &ex("txStore"), CommitMode::Committed)
            .expect("execute");
        assert_eq!(a, b, "same world → byte-identical receipt");
    }

    fn receipt_succeeded(nquads: &str, mode: CommitMode) -> bool {
        let root = if nquads.contains(&ex("txRevise")) {
            ex("txRevise")
        } else {
            ex("txStore")
        };
        execute_transaction(nquads, W, &root, mode)
            .expect("execute")
            .succeeded()
    }
}
