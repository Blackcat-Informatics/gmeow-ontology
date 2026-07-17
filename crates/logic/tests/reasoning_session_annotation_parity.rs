// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC2 — the session's derived-tuple proof provenance matches the full-recompute oracle.
//!
//! For each certified fragment, [`ReasoningSession::provenance`] carries one canonical
//! witness per derived fact in the maintained closure. This suite asserts, per derived
//! `(subject, predicate, object)`:
//!
//! * `rule_iri` ↔ the oracle's `rule_name` (the firing rule identity), and
//! * premise-SET equality against the oracle's immediate antecedents, and
//! * `weight == 1` (the Z-set set-membership multiplicity of a present derived fact),
//!
//! with the witness key set in exact bijection with the oracle's derived closure. The EDBs
//! are simple chains so every derived fact has a UNIQUE derivation — the two independent
//! engines then have no witness-choice freedom, making premise-set equality falsifiable.

use std::collections::BTreeSet;

use gmeow_logic::runtime::{OperationOutcome, ReasoningSession, SessionDelta};
use gmeow_logic_compile::ir::LogicProgram;

mod session_common;
use session_common::*;

/// Assert the session's full-closure provenance matches the oracle over `edb`.
fn assert_provenance_parity(
    program: &LogicProgram,
    idb: &[String],
    session: &ReasoningSession,
    edb: &purrdf::RdfDataset,
) {
    let oracle = oracle_witnesses(program, edb, idb);
    let session_map = session_witnesses(session);

    // Bijection: one session witness per derived fact the oracle recomputes.
    assert_eq!(
        session_map.keys().cloned().collect::<BTreeSet<_>>(),
        oracle.keys().cloned().collect::<BTreeSet<_>>(),
        "session provenance covers exactly the oracle's derived facts"
    );
    assert!(!session_map.is_empty(), "the derived closure is non-empty");

    for (key, witness) in &session_map {
        let (oracle_rule, oracle_premises) = oracle.get(key).expect("oracle witness for key");
        assert_eq!(
            Some(witness.rule.clone()),
            *oracle_rule,
            "rule_iri matches the oracle rule_name for {key:?}"
        );
        assert_eq!(
            witness.premises, *oracle_premises,
            "premise SET matches the oracle antecedents for {key:?}"
        );
        assert_eq!(
            witness.weight, 1,
            "a present derived fact has Z-weight +1 for {key:?}"
        );
    }
}

fn drive_provenance(
    program: &LogicProgram,
    idb: &[String],
    base: &[(&str, &str)],
    new_edge: (&str, &str),
) {
    let (contract, annotation) = baseline_contracts();
    let edb0 = edge_dataset(base);
    let mut session = ReasoningSession::open(&edb0, program, &contract, &annotation).expect("open");

    // Provenance parity at the initial settle (base closure, before any delta).
    assert_provenance_parity(program, idb, &session, &edb0);

    // Apply a delta; provenance() then covers base + delta-derived facts.
    let base_commit = session.identity().data_generation.clone();
    let head = session.head().to_owned();
    let delta = SessionDelta::new(base_commit, head, edge_dataset(&[new_edge]), vec![], None)
        .expect("delta");
    match session.apply(&delta) {
        OperationOutcome::Applied { .. } => {}
        other => panic!("expected Applied, got {other:?}"),
    }

    let mut grown: Vec<(&str, &str)> = base.to_vec();
    grown.push(new_edge);
    let edb1 = edge_dataset(&grown);
    assert_provenance_parity(program, idb, &session, &edb1);
}

#[test]
fn ac2_projection_provenance_parity() {
    drive_provenance(
        &projection_program(),
        &idb_reach(),
        &[("a", "b"), ("b", "c")],
        ("c", "d"),
    );
}

#[test]
fn ac2_transitive_closure_provenance_parity() {
    // A linear chain a→b→c→d → every reach fact has a single derivation path.
    drive_provenance(
        &transitive_program(),
        &idb_reach(),
        &[("a", "b"), ("b", "c")],
        ("c", "d"),
    );
}

#[test]
fn ac2_mutual_recursion_provenance_parity() {
    drive_provenance(
        &mutual_program(),
        &idb_pq(),
        &[("a", "b"), ("b", "c"), ("c", "d")],
        ("d", "e"),
    );
}
