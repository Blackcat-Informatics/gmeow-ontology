// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC5 — a delta can never be applied twice, and the guard survives a crash/restart
//! boundary because it is anchored on the durable journal head (not an in-memory seen-set).
//!
//! * A delta lost BEFORE its journal entry was durable is safely re-applied from the last
//!   pre-delta checkpoint, yielding the identical closure and step count (idempotent
//!   recovery — recovered == applied-once == full-recompute).
//! * A delta whose commit WAS durable (its head folded into a checkpoint) is refused on
//!   re-submission after a restart: its stale `expected_head` fails the transition
//!   precondition → `Invalid{PreconditionMismatch}`, closure unchanged.
//! * Double-apply in a single process is refused by the same structural guard.
//! * The session is never an authority writer: `apply` never mints a new authorized
//!   data-generation.

use gmeow_logic::runtime::{
    IntegrityFault, OperationOutcome, ReasoningSession, SessionDelta, Suppression,
};

mod session_common;
use session_common::*;

fn make_delta(session: &ReasoningSession, additions: purrdf::RdfDataset) -> SessionDelta {
    SessionDelta::new(
        session.identity().data_generation.clone(),
        session.head(),
        additions,
        Vec::<Suppression>::new(),
        None,
    )
    .expect("valid delta")
}

#[test]
fn ac5_crash_before_durable_journal_replays_idempotently() {
    let program = transitive_program();
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let idb = idb_reach();

    // Path A — apply the delta once.
    let mut session_a =
        ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open A");
    let delta_a = make_delta(&session_a, edge_dataset(&[("c", "d")]));
    let steps_a = match session_a.apply(&delta_a) {
        OperationOutcome::Applied { run, .. } => run.consumed_steps,
        other => panic!("expected Applied, got {other:?}"),
    };
    let closure_a = session_derived(&session_a, &idb);

    // Path B — take a durable checkpoint BEFORE the delta, then simulate a crash that lost
    // the in-memory (never-durably-journaled) delta: recover from the pre-delta checkpoint
    // and re-apply the same delta.
    let session_b = ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open B");
    let checkpoint = session_b.checkpoint();
    drop(session_b); // the crash — in-memory state (incl. the applied delta) is gone.

    let mut recovered =
        ReasoningSession::restore(&checkpoint, &edb, &program, &contract, &annotation)
            .expect("recover from the durable pre-delta checkpoint");
    // The recovered head is the durable journal head (genesis), so the lost delta's
    // precondition still holds and it re-applies cleanly.
    let delta_b = make_delta(&recovered, edge_dataset(&[("c", "d")]));
    let steps_b = match recovered.apply(&delta_b) {
        OperationOutcome::Applied { run, .. } => run.consumed_steps,
        other => panic!("expected Applied on replay, got {other:?}"),
    };
    let closure_b = session_derived(&recovered, &idb);

    let edb1 = edge_dataset(&[("a", "b"), ("b", "c"), ("c", "d")]);
    assert_eq!(
        closure_b, closure_a,
        "recovered closure equals the applied-once closure"
    );
    assert_eq!(
        closure_b,
        oracle_derived(&program, &edb1, &idb),
        "recovered closure equals full recompute"
    );
    assert_eq!(
        steps_a, steps_b,
        "consumed steps are identical across single-apply and recovery"
    );
}

#[test]
fn ac5_committed_delta_is_refused_after_restart() {
    let program = transitive_program();
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let idb = idb_reach();

    let mut session = ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open");
    let genesis = session.head().to_owned();

    // Apply the delta and fold its committed head into a durable checkpoint.
    let delta = make_delta(&session, edge_dataset(&[("c", "d")]));
    match session.apply(&delta) {
        OperationOutcome::Applied { .. } => {}
        other => panic!("expected Applied, got {other:?}"),
    }
    let committed_closure = session_derived(&session, &idb);
    let committed_checkpoint = session.checkpoint();
    assert_ne!(
        committed_checkpoint.journal_head, genesis,
        "the checkpoint pins the advanced head"
    );
    assert!(
        !committed_checkpoint.deltas.is_empty(),
        "a post-apply checkpoint durably carries the committed delta"
    );

    // Restart from the durable checkpoint (its journal_head is the post-commit head).
    let mut restarted = ReasoningSession::restore(
        &committed_checkpoint,
        &edb,
        &program,
        &contract,
        &annotation,
    )
    .expect("restart from the committed checkpoint");
    let before = session_derived(&restarted, &idb);

    // POSITIVE assertion: the delta survived the crash — restarting from a
    // post-apply checkpoint reproduces the committed post-delta closure (the 9-fact state
    // incl. c→d), NOT the base. Before the fix the restart reverted to the 5-fact base while
    // reporting the 9-fact head; this asserts the faithful round-trip.
    let edb_committed = edge_dataset(&[("a", "b"), ("b", "c"), ("c", "d")]);
    assert_eq!(
        before, committed_closure,
        "restart reproduces the committed post-delta closure (the delta survived the crash)"
    );
    assert_eq!(
        before,
        oracle_derived(&program, &edb_committed, &idb),
        "the restarted committed closure equals the full recompute over the committed EDB"
    );
    assert_eq!(
        restarted.head(),
        committed_checkpoint.journal_head,
        "the restarted head is the durable committed head, reproduced by replay"
    );

    // Re-submit the ALREADY-COMMITTED delta: its expected_head is the stale pre-commit
    // (genesis) hash, so the transition precondition fails against the restored head.
    let stale_delta = SessionDelta::new(
        restarted.identity().data_generation.clone(),
        genesis.clone(),
        edge_dataset(&[("c", "d")]),
        Vec::<Suppression>::new(),
        None,
    )
    .expect("valid delta shape");
    match restarted.apply(&stale_delta) {
        OperationOutcome::Invalid {
            fault: IntegrityFault::PreconditionMismatch { .. },
        } => {}
        other => panic!("expected Invalid{{PreconditionMismatch}} on re-submit, got {other:?}"),
    }
    assert_eq!(
        session_derived(&restarted, &idb),
        before,
        "a refused re-submit leaves the closure unchanged"
    );
}

#[test]
fn ac5_double_apply_in_process_is_structurally_refused() {
    let program = transitive_program();
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let idb = idb_reach();

    let mut session = ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open");
    let delta = make_delta(&session, edge_dataset(&[("c", "d")]));

    match session.apply(&delta) {
        OperationOutcome::Applied { .. } => {}
        other => panic!("expected first Applied, got {other:?}"),
    }
    let after_first = session_derived(&session, &idb);

    // Re-apply the SAME delta: its expected_head is now stale vs the advanced head.
    match session.apply(&delta) {
        OperationOutcome::Invalid {
            fault: IntegrityFault::PreconditionMismatch { .. },
        } => {}
        other => panic!("expected Invalid{{PreconditionMismatch}} on double-apply, got {other:?}"),
    }
    assert_eq!(
        session_derived(&session, &idb),
        after_first,
        "a refused double-apply leaves the closure unchanged"
    );
}

#[test]
fn ac5_apply_never_mints_a_new_authorized_generation() {
    let program = transitive_program();
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);

    let mut session = ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open");
    let generation_before = session.identity().data_generation.clone();

    let delta = make_delta(&session, edge_dataset(&[("c", "d")]));
    match session.apply(&delta) {
        OperationOutcome::Applied { .. } => {}
        other => panic!("expected Applied, got {other:?}"),
    }
    assert_eq!(
        session.identity().data_generation,
        generation_before,
        "the session references an authorized commit but never mints a new authorized generation"
    );
}
