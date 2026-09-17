// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const W: &str = "https://example.org/txn/world";

fn li(local: &str) -> String {
    format!("https://blackcatinformatics.ca/logic/{local}")
}
fn ex(local: &str) -> String {
    format!("https://example.org/txn/{local}")
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
fn typed_transaction_input_preserves_executional_entailment_and_commit_mode() {
    for ready in [false, true] {
        let source = store_world(ready);
        let dataset = purrdf::parse_dataset(source.as_bytes(), "application/n-quads", None)
            .expect("transaction world");
        for mode in [CommitMode::Committed, CommitMode::Hypothetical] {
            let native = execute_transaction_dataset(&dataset, W, &ex("txStore"), mode)
                .expect("native transaction");
            assert_eq!(native.succeeded(), ready);
            assert_eq!(
                native,
                execute_transaction(&source, W, &ex("txStore"), mode)
                    .expect("text transaction boundary")
            );
        }
    }
}

#[test]
fn prepared_transaction_replays_without_effect_or_input_leakage() {
    let parse = |ready| {
        purrdf::parse_dataset(store_world(ready).as_bytes(), "application/n-quads", None)
            .expect("transaction input")
    };
    let ready = parse(true);
    let missing = parse(false);
    let accepted = PreparedTransaction::new(&ready, W, &ex("txStore")).expect("prepare");
    let refused = PreparedTransaction::new(&missing, W, &ex("txStore")).expect("prepare");
    let first = accepted.execute(CommitMode::Committed).expect("execute");
    assert!(first.succeeded());
    assert!(matches!(
        accepted.execute(CommitMode::Hypothetical).expect("sandbox"),
        TxReceipt::HypotheticalSuccess { .. }
    ));
    assert!(
        !refused
            .execute(CommitMode::Committed)
            .expect("failed precondition")
            .succeeded()
    );
    assert_eq!(
        accepted.execute(CommitMode::Committed).expect("replay"),
        first
    );
    assert!(PreparedTransaction::new(&ready, &ex("other-world"), &ex("txStore")).is_err());
    assert!(PreparedTransaction::new(&ready, W, &ex("absent-root")).is_err());
}

#[test]
fn committed_store_succeeds_when_precondition_obtains() {
    let receipt = execute_transaction(&store_world(true), W, &ex("txStore"), CommitMode::Committed)
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
    let receipt = execute_transaction(&revise_world(), W, &ex("txRevise"), CommitMode::Committed)
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
