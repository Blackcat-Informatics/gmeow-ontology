// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC1 — insert / suppress-retract / checkpoint→restore / restart parity against the
//! full-recompute oracle, for every certified fragment (finite positive binary Datalog).
//!
//! Each `apply` MUST return [`OperationOutcome::Applied`] (a genuine incremental
//! maintenance, asserted so parity is not tautological), and after EVERY operation the
//! session's maintained derived closure equals the from-scratch
//! [`gmeow_logic::reason::reason_program`] oracle over the current EDB.
//!
//! Also the remaining production-reachable non-refusal outcomes: budget-bounded
//! [`OperationOutcome::Incomplete`] (state unchanged) and a genuine
//! [`OperationOutcome::EngineFailure`]. A proptest arm generalizes the closure parity over
//! arbitrary bounded insert/retract sequences.

use gmeow_logic::runtime::{
    FragmentDisposition, IncompleteCause, OperationOutcome, ReasoningSession, SessionDelta,
    Suppression,
};
use gmeow_logic::seam::BudgetStatus;
use gmeow_logic_compile::ir::LogicProgram;
use proptest::prelude::*;

mod session_common;
use session_common::*;

/// Build a delta from the session's current authorization + transition anchors and apply
/// it, asserting the genuine-incremental `Applied` variant. Returns the committed
/// `consumed_steps` so a crash/replay caller can compare it.
fn apply_expecting_applied(
    session: &mut ReasoningSession,
    additions: purrdf::RdfDataset,
    retirements: Vec<Suppression>,
) -> u64 {
    let base = session.identity().data_generation.clone();
    let head = session.head().to_owned();
    let delta = SessionDelta::new(base, head, additions, retirements, None).expect("valid delta");
    match session.apply(&delta) {
        OperationOutcome::Applied { run, .. } => run.consumed_steps,
        other => panic!("expected Applied, got {other:?}"),
    }
}

/// The AC1 op-sequence driver over one certified program.
fn drive(program: &LogicProgram, idb: &[String], base: &[(&str, &str)], new_edge: (&str, &str)) {
    let (contract, annotation) = baseline_contracts();
    let edb0 = edge_dataset(base);

    // 1. open — must be a genuine incremental maintainer, parity at the initial settle.
    let mut session =
        ReasoningSession::open(&edb0, program, &contract, &annotation).expect("open certified");
    assert_eq!(
        session.fragment_disposition(),
        &FragmentDisposition::Incremental,
        "program must be incrementally certified for AC1"
    );
    assert_eq!(
        session_derived(&session, idb),
        oracle_derived(program, &edb0, idb),
        "open closure matches full recompute"
    );

    // 2. insert — Applied, and parity against the oracle over the grown EDB.
    let mut grown: Vec<(&str, &str)> = base.to_vec();
    grown.push(new_edge);
    let edb1 = edge_dataset(&grown);
    apply_expecting_applied(&mut session, edge_dataset(&[new_edge]), vec![]);
    let closure_after_insert = session_derived(&session, idb);
    assert_eq!(
        closure_after_insert,
        oracle_derived(program, &edb1, idb),
        "post-insert closure matches full recompute"
    );
    assert_ne!(
        closure_after_insert,
        oracle_derived(program, &edb0, idb),
        "the inserted edge genuinely grew the derived closure"
    );

    // 3. suppress-retract then re-insert — non-erasure round trip.
    apply_expecting_applied(
        &mut session,
        empty_dataset(),
        vec![Suppression::new(edge_dataset(&[new_edge]))],
    );
    assert_eq!(
        session_derived(&session, idb),
        oracle_derived(program, &edb0, idb),
        "retracting the edge returns to the base closure (full recompute)"
    );
    apply_expecting_applied(&mut session, edge_dataset(&[new_edge]), vec![]);
    assert_eq!(
        session_derived(&session, idb),
        closure_after_insert,
        "re-inserting the retracted edge restores the pre-retract closure (non-erasure)"
    );

    // 4. checkpoint (at the base generation) → insert → restore returns to base.
    let mut session2 =
        ReasoningSession::open(&edb0, program, &contract, &annotation).expect("second open");
    let base_closure = session_derived(&session2, idb);
    let checkpoint = session2.checkpoint();
    apply_expecting_applied(&mut session2, edge_dataset(&[new_edge]), vec![]);
    assert_ne!(
        session_derived(&session2, idb),
        base_closure,
        "the post-checkpoint insert changed the live closure"
    );
    let restored = ReasoningSession::restore(&checkpoint, &edb0, program, &contract, &annotation)
        .expect("untampered checkpoint restores");
    assert_eq!(
        session_derived(&restored, idb),
        base_closure,
        "restore re-materializes the pre-checkpoint (base data-generation) closure"
    );

    // 5. restart equals a fresh open over the same authorized EDB.
    let restarted = ReasoningSession::restart(&checkpoint, &edb0, program, &contract, &annotation)
        .expect("restart from checkpoint");
    let fresh = ReasoningSession::open(&edb0, program, &contract, &annotation).expect("fresh open");
    assert_eq!(
        session_derived(&restarted, idb),
        session_derived(&fresh, idb),
        "restart equals a fresh open"
    );
}

#[test]
fn ac1_projection_parity() {
    drive(
        &projection_program(),
        &idb_reach(),
        &[("a", "b"), ("b", "c")],
        ("c", "d"),
    );
}

#[test]
fn ac1_transitive_closure_parity() {
    drive(
        &transitive_program(),
        &idb_reach(),
        &[("a", "b"), ("b", "c")],
        ("c", "d"),
    );
}

#[test]
fn ac1_mutual_recursion_parity() {
    drive(
        &mutual_program(),
        &idb_pq(),
        &[("a", "b"), ("b", "c"), ("c", "d")],
        ("d", "e"),
    );
}

#[test]
fn ac1_budget_bounded_apply_is_incomplete_and_leaves_state_unchanged() {
    // A recursive program with a small step budget: the insert cannot reach fixpoint, so
    // it is reported Incomplete with the closure UNCHANGED (insert commits only on Ok).
    let program = transitive_program();
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c"), ("c", "d")]);
    let mut session = ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open");
    let before = session_derived(&session, &idb_reach());

    // Insert a long tail that forces several new transitive derivations under a 1-step
    // budget.
    let delta = SessionDelta::new(
        session.identity().data_generation.clone(),
        session.head(),
        edge_dataset(&[("d", "e"), ("e", "f"), ("f", "g")]),
        vec![],
        Some(1),
    )
    .expect("valid budgeted delta");
    let head_before = session.head().to_owned();
    match session.apply(&delta) {
        OperationOutcome::Incomplete { status, cause } => {
            assert!(
                matches!(status, BudgetStatus::Exhausted | BudgetStatus::Partial),
                "budget cut yields Exhausted/Partial, got {status:?}"
            );
            assert_eq!(
                cause,
                IncompleteCause::StepBudget,
                "the governor was the step budget"
            );
        }
        other => panic!("expected Incomplete under a 1-step budget, got {other:?}"),
    }
    assert_eq!(
        session_derived(&session, &idb_reach()),
        before,
        "an Incomplete apply leaves the maintained closure unchanged"
    );
    assert_eq!(
        session.head(),
        head_before,
        "an Incomplete apply does not advance the journal"
    );
}

#[test]
fn ac1_illegal_retraction_underflow_is_engine_failure() {
    // Retracting more facts than the EDB holds is a structurally illegal signed
    // transaction; the maintenance engine reports it as a genuine diagnostic, surfaced as
    // EngineFailure (NOT a silent no-op, NOT an approximate Applied).
    let program = transitive_program();
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b")]);
    let mut session = ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open");
    let before = session_derived(&session, &idb_reach());

    // Suppress two edges when the EDB has only one → an unbounded-retraction row-count
    // underflow inside the maintainer.
    let delta = SessionDelta::new(
        session.identity().data_generation.clone(),
        session.head(),
        empty_dataset(),
        vec![Suppression::new(edge_dataset(&[("a", "b"), ("x", "y")]))],
        None,
    )
    .expect("valid delta shape");
    match session.apply(&delta) {
        OperationOutcome::EngineFailure { .. } => {}
        other => panic!("expected EngineFailure on a retraction underflow, got {other:?}"),
    }
    assert_eq!(
        session_derived(&session, &idb_reach()),
        before,
        "a failed apply leaves the maintained closure unchanged"
    );
}

// ── proptest arm — closure parity over arbitrary bounded op sequences ───────────────

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(48);
    ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// One generated op: `insert` (true) or `retract` (false) of the edge `(s, o)` over a
/// 4-node vertex pool.
fn arb_ops() -> impl Strategy<Value = Vec<(bool, u8, u8)>> {
    proptest::collection::vec((any::<bool>(), 0u8..4, 0u8..4), 0..14)
}

proptest! {
    #![proptest_config(config())]

    /// For any bounded insert/retract sequence over the certified transitive-closure
    /// program, the session's maintained derived closure equals the full-recompute
    /// oracle over the resulting EDB — after every applied op. Only set-changing ops are
    /// emitted (insert an absent edge / retract a present one) so every apply is a genuine
    /// `Applied`, matching the exact EDB the oracle recomputes from.
    #[test]
    fn ac1_proptest_incremental_matches_full_recompute(ops in arb_ops()) {
        let program = transitive_program();
        let idb = idb_reach();
        let (contract, annotation) = baseline_contracts();

        let node = |i: u8| -> &'static str {
            match i { 0 => "n0", 1 => "n1", 2 => "n2", _ => "n3" }
        };

        // Seed with one edge so the base closure is non-trivial and the data-generation is
        // stable across the whole sequence (the session is not an authority writer).
        let base_edges = vec![("n0", "n1")];
        let mut current: std::collections::BTreeSet<(&'static str, &'static str)> =
            base_edges.iter().copied().collect();
        let edb0 = edge_dataset(&base_edges);
        let mut session =
            ReasoningSession::open(&edb0, &program, &contract, &annotation).expect("open");

        prop_assert_eq!(session.fragment_disposition(), &FragmentDisposition::Incremental);

        for (insert, s, o) in ops {
            let edge = (node(s), node(o));
            let present = current.contains(&edge);
            // Only emit a set-changing op; otherwise the apply would be a no-op (empty
            // movement → Invalid) or a retraction underflow.
            let (additions, retirements) = if insert && !present {
                current.insert(edge);
                (edge_dataset(&[edge]), vec![])
            } else if !insert && present {
                current.remove(&edge);
                (empty_dataset(), vec![Suppression::new(edge_dataset(&[edge]))])
            } else {
                continue;
            };

            let delta = SessionDelta::new(
                session.identity().data_generation.clone(),
                session.head(),
                additions,
                retirements,
                None,
            )
            .expect("valid delta");
            match session.apply(&delta) {
                OperationOutcome::Applied { .. } => {}
                other => prop_assert!(false, "expected Applied, got {:?}", other),
            }

            let edges: Vec<(&str, &str)> = current.iter().copied().collect();
            let edb = edge_dataset(&edges);
            prop_assert_eq!(
                session_derived(&session, &idb),
                oracle_derived(&program, &edb, &idb)
            );
        }
    }
}
