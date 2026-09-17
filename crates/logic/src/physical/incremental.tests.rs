// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::query_ir::{ArithOp, QBuiltin, QTerm};
use crate::rule_ir::{EvalAtom, EvalTerm, least_model_of_reduct};
use purrdf::TermValue;

const NS: &str = "https://example.org/incremental/";

fn iri(local: &str) -> TermValue {
    TermValue::iri(format!("{NS}{local}"))
}

fn fact(predicate: &str, subject: &str, object: &str) -> Fact {
    Fact {
        subject: iri(subject),
        predicate: format!("{NS}{predicate}"),
        object: iri(object),
    }
}

fn var(name: &str) -> EvalTerm {
    EvalTerm::Var(format!("?{name}"))
}

fn constant(local: &str) -> EvalTerm {
    EvalTerm::ConstNamed(format!("{NS}{local}"))
}

fn atom(predicate: &str, subject: EvalTerm, object: EvalTerm) -> EvalAtom {
    EvalAtom {
        subject,
        predicate: format!("{NS}{predicate}"),
        object,
        negated: false,
    }
}

fn rule(name: &str, head: EvalAtom, body: Vec<EvalAtom>) -> EvalRule {
    EvalRule {
        numeric: Vec::new(),
        head,
        body,
        rule_iri: format!("{NS}rule/{name}"),
        distinct_pairs: Vec::new(),
        builtins: Vec::new(),
        reduction: None,
        constraint_tag: None,
    }
}

fn closure_keys(facts: &[Fact]) -> BTreeSet<FactKey> {
    facts.iter().map(Fact::key).collect()
}

fn scratch(edb: &[Fact], rules: &[EvalRule]) -> BTreeSet<FactKey> {
    let mut edb_store = FactStore::new();
    let mut sorted = edb.to_vec();
    sorted.sort_by_key(Fact::key);
    for fact in sorted {
        edb_store.insert(fact);
    }
    let result = least_model_of_reduct(&edb_store, rules, &edb_store)
        .expect("positive scratch materialization");
    closure_keys(result.store.facts())
}

fn assert_scratch_parity(session: &IncrementalSession, edb: &[Fact], rules: &[EvalRule]) {
    assert_eq!(closure_keys(&session.closure()), scratch(edb, rules));
}

fn transitive_rules() -> Vec<EvalRule> {
    vec![
        rule(
            "base",
            atom("path", var("X"), var("Y")),
            vec![atom("edge", var("X"), var("Y"))],
        ),
        rule(
            "step",
            atom("path", var("X"), var("Z")),
            vec![
                atom("path", var("X"), var("Y")),
                atom("edge", var("Y"), var("Z")),
            ],
        ),
    ]
}

#[test]
fn recursive_insert_and_retract_match_clean_rebuild() {
    let rules = transitive_rules();
    let mut edb = vec![
        fact("edge", "a", "b"),
        fact("edge", "b", "c"),
        fact("edge", "c", "d"),
    ];
    let mut session = IncrementalSession::new("contract", edb.clone(), &rules).unwrap();
    assert_scratch_parity(&session, &edb, &rules);

    let inserted = fact("edge", "d", "e");
    let delta = session
        .apply([SignedFact {
            fact: inserted.clone(),
            weight: 1,
        }])
        .unwrap();
    edb.push(inserted);
    assert_scratch_parity(&session, &edb, &rules);
    assert!(
        delta.changes.iter().any(|change| {
            change.weight == 1 && change.fact.key() == fact("path", "a", "e").key()
        })
    );
    assert!(delta.inner_iterations > 0);
    assert!(delta.joined_rows > 0);

    let removed = fact("edge", "b", "c");
    let delta = session
        .apply([SignedFact {
            fact: removed.clone(),
            weight: -1,
        }])
        .unwrap();
    edb.retain(|candidate| candidate.key() != removed.key());
    assert_scratch_parity(&session, &edb, &rules);
    assert!(delta.changes.iter().any(|change| {
        change.weight == -1 && change.fact.key() == fact("path", "a", "d").key()
    }));
}

#[test]
fn loop_forks_share_cached_histories_until_the_branch_updates() {
    let rules = transitive_rules();
    let base = fact("edge", "a", "b");
    let session = IncrementalSession::new("contract", [base], &rules).unwrap();
    let mut branch = session.clone();

    assert!(Arc::ptr_eq(&session.rules, &branch.rules));
    assert!(Arc::ptr_eq(&session.arena, &branch.arena));
    assert!(Arc::ptr_eq(&session.edb, &branch.edb));
    assert!(Arc::ptr_eq(&session.snapshots, &branch.snapshots));
    assert!(Arc::ptr_eq(&session.raw, &branch.raw));

    branch
        .apply([SignedFact {
            fact: fact("edge", "b", "c"),
            weight: 1,
        }])
        .unwrap();

    assert!(Arc::ptr_eq(&session.rules, &branch.rules));
    assert!(!Arc::ptr_eq(&session.arena, &branch.arena));
    assert!(!Arc::ptr_eq(&session.edb, &branch.edb));
    assert!(!Arc::ptr_eq(&session.snapshots, &branch.snapshots));
    assert!(!Arc::ptr_eq(&session.raw, &branch.raw));
    assert_eq!(
        closure_keys(&session.closure()),
        scratch(&[fact("edge", "a", "b")], &rules)
    );
    assert_eq!(
        closure_keys(&branch.closure()),
        scratch(&[fact("edge", "a", "b"), fact("edge", "b", "c")], &rules)
    );
}

#[test]
fn retracting_last_ground_removes_a_mutual_support_cycle() {
    let rules = vec![
        rule(
            "seed-q",
            atom("q", var("X"), var("Y")),
            vec![atom("seed", var("X"), var("Y"))],
        ),
        rule(
            "q-p",
            atom("p", var("X"), var("Y")),
            vec![atom("q", var("X"), var("Y"))],
        ),
        rule(
            "p-q",
            atom("q", var("X"), var("Y")),
            vec![atom("p", var("X"), var("Y"))],
        ),
    ];
    let seed = fact("seed", "a", "b");
    let mut session = IncrementalSession::new("contract", [seed.clone()], &rules).unwrap();
    assert!(closure_keys(&session.closure()).contains(&fact("p", "a", "b").key()));

    let delta = session
        .apply([SignedFact {
            fact: seed,
            weight: -1,
        }])
        .unwrap();
    assert_scratch_parity(&session, &[], &rules);
    let removed: BTreeSet<FactKey> = delta
        .changes
        .iter()
        .filter(|change| change.weight == -1)
        .map(|change| change.fact.key())
        .collect();
    assert!(removed.contains(&fact("p", "a", "b").key()));
    assert!(removed.contains(&fact("q", "a", "b").key()));
}

#[test]
fn alternative_proof_survives_one_retraction() {
    let rules = vec![
        rule(
            "left",
            atom("answer", var("X"), var("Y")),
            vec![atom("left", var("X"), var("Y"))],
        ),
        rule(
            "right",
            atom("answer", var("X"), var("Y")),
            vec![atom("right", var("X"), var("Y"))],
        ),
    ];
    let left = fact("left", "a", "b");
    let right = fact("right", "a", "b");
    let mut session =
        IncrementalSession::new("contract", [left.clone(), right.clone()], &rules).unwrap();
    let delta = session
        .apply([SignedFact {
            fact: left,
            weight: -1,
        }])
        .unwrap();
    assert_scratch_parity(&session, &[right], &rules);
    assert!(
        !delta
            .changes
            .iter()
            .any(|change| { change.fact.key() == fact("answer", "a", "b").key() })
    );
}

#[test]
fn newly_derived_fact_selects_a_witness_that_survives_signed_cancellation() {
    let rules = vec![
        rule(
            "a-left",
            atom("answer", var("X"), var("Y")),
            vec![
                atom("left", var("X"), var("Y")),
                atom("gate", var("X"), var("Y")),
            ],
        ),
        rule(
            "b-right",
            atom("answer", var("X"), var("Y")),
            vec![
                atom("right", var("X"), var("Y")),
                atom("gate", var("X"), var("Y")),
            ],
        ),
    ];
    let left = fact("left", "a", "b");
    let right = fact("right", "a", "b");
    let gate = fact("gate", "a", "b");
    let answer = fact("answer", "a", "b");
    let mut session = IncrementalSession::new("contract", [left.clone()], &rules).unwrap();

    // The left rule contributes +answer when gate arrives and -answer when its
    // old support retracts; the right rule contributes the surviving +answer.
    let delta = session
        .apply([
            SignedFact {
                fact: gate.clone(),
                weight: 1,
            },
            SignedFact {
                fact: right.clone(),
                weight: 1,
            },
            SignedFact {
                fact: left.clone(),
                weight: -1,
            },
        ])
        .unwrap();

    let witness = delta
        .derivations
        .get(&answer.key())
        .expect("new answer carries a surviving proof witness");
    let premise_keys: BTreeSet<_> = witness.premises.iter().map(Fact::key).collect();
    assert_eq!(witness.rule_iri, format!("{NS}rule/b-right"));
    assert!(premise_keys.contains(&right.key()));
    assert!(premise_keys.contains(&gate.key()));
    assert!(!premise_keys.contains(&left.key()));
    assert_scratch_parity(&session, &[right, gate], &rules);
}

#[test]
fn closure_reconstruct_selects_the_minimal_height_witness() {
    let rules = transitive_rules();
    // a→b, b→c, and a direct a→c edge: path(a,c) has a height-1 (direct `base`) and a
    // height-2 (via b, `step`) derivation. The reconstructed canonical witness is the
    // minimal-height one, and every derived fact appears exactly once.
    let edb = vec![
        fact("edge", "a", "b"),
        fact("edge", "b", "c"),
        fact("edge", "a", "c"),
    ];
    let session = IncrementalSession::new("contract", edb, &rules).unwrap();
    let derivations = session.closure_derivations().unwrap();

    // path(a,b), path(b,c), path(a,c) — one canonical witness each.
    assert_eq!(derivations.len(), 3);

    let witness = |s: &str, o: &str| {
        derivations
            .iter()
            .find(|(f, _)| f.key() == fact("path", s, o).key())
            .map(|(_, w)| w.clone())
            .unwrap_or_else(|| panic!("path {s} {o} is derived"))
    };

    let ac = witness("a", "c");
    assert_eq!(ac.proof_height.get(), 1, "the minimal proof height wins");
    assert_eq!(ac.rule_iri, format!("{NS}rule/base"));
    assert_eq!(ac.premises.len(), 1);
    assert_eq!(ac.premises[0].key(), fact("edge", "a", "c").key());

    assert_eq!(witness("a", "b").proof_height.get(), 1);
    assert_eq!(witness("b", "c").proof_height.get(), 1);
}

#[test]
fn reconstructed_height_rises_when_a_short_proof_is_retracted() {
    let rules = transitive_rules();
    // path(a,c) starts with a height-1 direct proof; retracting edge(a,c) leaves only
    // the height-2 path via b, so the maintained proof height must RISE from 1 to 2.
    let mut session = IncrementalSession::new(
        "contract",
        vec![
            fact("edge", "a", "b"),
            fact("edge", "b", "c"),
            fact("edge", "a", "c"),
        ],
        &rules,
    )
    .unwrap();
    let height_ac = |session: &IncrementalSession| {
        session
            .closure_derivations()
            .unwrap()
            .into_iter()
            .find(|(f, _)| f.key() == fact("path", "a", "c").key())
            .map(|(_, w)| w.proof_height.get())
            .expect("path a c is derived")
    };
    assert_eq!(height_ac(&session), 1);

    session
        .apply([SignedFact {
            fact: fact("edge", "a", "c"),
            weight: -1,
        }])
        .unwrap();
    assert_eq!(
        height_ac(&session),
        2,
        "the surviving proof is one hop longer"
    );
}

#[test]
fn constants_repeated_variables_and_inequality_are_differential() {
    let mut guarded = rule(
        "guarded",
        atom("out", var("X"), constant("fixed")),
        vec![
            atom("pair", var("X"), var("Y")),
            atom("pair", var("Y"), var("Y")),
        ],
    );
    guarded
        .distinct_pairs
        .push(("?X".to_owned(), "?Y".to_owned()));
    let rules = vec![guarded];
    let loop_row = fact("pair", "b", "b");
    let edge = fact("pair", "a", "b");
    let mut session =
        IncrementalSession::new("contract", [loop_row.clone(), edge.clone()], &rules).unwrap();
    assert_scratch_parity(&session, &[loop_row.clone(), edge.clone()], &rules);
    assert!(closure_keys(&session.closure()).contains(&fact("out", "a", "fixed").key()));

    session
        .apply([SignedFact {
            fact: loop_row,
            weight: -1,
        }])
        .unwrap();
    assert_scratch_parity(&session, &[edge], &rules);
    assert!(!closure_keys(&session.closure()).contains(&fact("out", "a", "fixed").key()));
}

#[test]
fn signed_input_consolidates_and_invalid_membership_hard_fails() {
    let rules = transitive_rules();
    let edge = fact("edge", "a", "b");
    let mut session = IncrementalSession::new("contract", [], &rules).unwrap();
    let noop = session
        .apply([
            SignedFact {
                fact: edge.clone(),
                weight: 1,
            },
            SignedFact {
                fact: edge.clone(),
                weight: -1,
            },
        ])
        .unwrap();
    assert!(noop.changes.is_empty());

    let error = session
        .apply([SignedFact {
            fact: edge,
            weight: -1,
        }])
        .expect_err("an absent-row retraction must hard-fail");
    assert!(error.message().contains("expected exactly 0 or 1"));
}

#[test]
fn insert_budget_cuts_at_sorted_new_fact_commit_without_recharging_cache() {
    let rules = transitive_rules();
    let edb = vec![
        fact("edge", "a", "b"),
        fact("edge", "b", "c"),
        fact("edge", "c", "d"),
    ];
    let mut session = IncrementalSession::new("contract", edb.clone(), &rules).unwrap();
    let old = closure_keys(&session.closure());
    let inserted = fact("edge", "d", "e");
    let cut = session
        .apply_insert_budgeted(
            [SignedFact {
                fact: inserted.clone(),
                weight: 1,
            }],
            Some(1),
        )
        .unwrap();

    assert_eq!(cut.status, BudgetStatus::Exhausted);
    assert_eq!(cut.consumed_steps, 1);
    let cut_keys = closure_keys(&cut.closure);
    assert!(
        old.is_subset(&cut_keys),
        "stable cached closure is never recharged"
    );
    assert!(
        cut_keys.contains(&inserted.key()),
        "the asserted delta is free"
    );
    let mut full_edb = edb;
    full_edb.push(inserted);
    assert!(
        cut_keys.is_subset(&scratch(&full_edb, &rules)),
        "the cut closure is sound under the updated EDB"
    );
    assert_eq!(
        cut_keys.len(),
        old.len() + 2,
        "one asserted fact plus exactly one governed derivation"
    );

    // An exhausted attempt is atomic: the reusable base session remains unchanged.
    assert_eq!(closure_keys(&session.closure()), old);
}

#[test]
fn governed_retraction_is_refused_instead_of_returning_stale_facts() {
    let rules = transitive_rules();
    let edge = fact("edge", "a", "b");
    let mut session = IncrementalSession::new("contract", [edge.clone()], &rules).unwrap();
    let error = session
        .apply_insert_budgeted(
            [SignedFact {
                fact: edge,
                weight: -1,
            }],
            Some(0),
        )
        .expect_err("a bounded retraction has no sound partial-prefix contract yet");
    assert!(error.message().contains("must be insert-only"));
}

#[test]
fn uncovered_fragments_are_refused_at_construction() {
    let mut negated = rule(
        "negated",
        atom("p", var("X"), var("Y")),
        vec![atom("q", var("X"), var("Y"))],
    );
    negated.body[0].negated = true;
    assert!(
        IncrementalSession::new("contract", [], &[negated])
            .unwrap_err()
            .message()
            .contains("negation-as-failure")
    );

    let mut arithmetic = rule(
        "arithmetic",
        atom("p", var("X"), var("Y")),
        vec![atom("q", var("X"), var("Y"))],
    );
    arithmetic.builtins.push(QBuiltin::Is {
        target: QTerm::Var("?N".to_owned()),
        lhs: QTerm::Num(1),
        op: ArithOp::Add,
        rhs: QTerm::Num(1),
    });
    assert!(
        IncrementalSession::new("contract", [], &[arithmetic])
            .unwrap_err()
            .message()
            .contains("builtins")
    );

    let head_unbound = rule(
        "head-unbound",
        atom("p", var("X"), var("Z")),
        vec![atom("q", var("X"), var("Y"))],
    );
    let error = IncrementalSession::new("contract", [], &[head_unbound])
        .expect_err("a head-only variable must be refused at construction");
    assert!(error.message().contains("head variable ?Z"), "{error}");

    let mut inequality_unbound = rule(
        "inequality-unbound",
        atom("p", var("X"), var("Y")),
        vec![atom("q", var("X"), var("Y"))],
    );
    inequality_unbound
        .distinct_pairs
        .push(("?X".to_owned(), "?Z".to_owned()));
    let error = IncrementalSession::new("contract", [], &[inequality_unbound])
        .expect_err("an inequality-only variable must be refused at construction");
    assert!(
        error.message().contains("inequality variable ?Z"),
        "{error}"
    );
}

#[test]
fn validate_fragment_subsumes_the_typed_classifier() {
    // The typed classifier and the diagnostic projection must never disagree:
    // whenever `classify_incremental_fragment` returns a typed refusal,
    // `validate_fragment` errors with the SAME message that refusal renders, and
    // whenever the classifier accepts, `validate_fragment` accepts.
    let ok_rules = transitive_rules();
    assert!(classify_incremental_fragment(&ok_rules).is_ok());
    assert!(validate_fragment(&ok_rules).is_ok());

    let mut negated = rule(
        "negated",
        atom("p", var("X"), var("Y")),
        vec![atom("q", var("X"), var("Y"))],
    );
    negated.body[0].negated = true;

    let mut arithmetic = rule(
        "arithmetic",
        atom("p", var("X"), var("Y")),
        vec![atom("q", var("X"), var("Y"))],
    );
    arithmetic.builtins.push(QBuiltin::Is {
        target: QTerm::Var("?N".to_owned()),
        lhs: QTerm::Num(1),
        op: ArithOp::Add,
        rhs: QTerm::Num(1),
    });

    let head_unbound = rule(
        "head-unbound",
        atom("p", var("X"), var("Z")),
        vec![atom("q", var("X"), var("Y"))],
    );

    let bodyless = EvalRule {
        numeric: Vec::new(),
        head: atom("p", var("X"), var("Y")),
        body: Vec::new(),
        rule_iri: format!("{NS}rule/bodyless"),
        distinct_pairs: Vec::new(),
        builtins: Vec::new(),
        reduction: None,
        constraint_tag: None,
    };

    for rules in [
        vec![negated],
        vec![arithmetic],
        vec![head_unbound],
        vec![bodyless],
    ] {
        let refusal = classify_incremental_fragment(&rules).expect_err("must be a typed refusal");
        let diag = validate_fragment(&rules).expect_err("must project to the same Diag");
        assert_eq!(
            diag.message(),
            refusal.message(),
            "validate_fragment must reproduce the typed refusal's message verbatim"
        );
    }
}

#[test]
fn identity_pins_contract_rules_and_solver() {
    let rules = transitive_rules();
    let a = IncrementalSession::new("contract-a", [], &rules).unwrap();
    let b = IncrementalSession::new("contract-b", [], &rules).unwrap();
    assert_ne!(a.identity(), b.identity());
    assert_eq!(a.identity().rule_hash, b.identity().rule_hash);
    assert_eq!(a.identity().solver_version, INCREMENTAL_SOLVER_VERSION);
}

/// Exhaust every directed graph on three nodes, then apply every valid single-edge
/// insert/retract.  This covers cycles, self-loops, redundant paths, and alternative
/// proofs without relying on random seeds; every adjusted closure must equal a clean
/// semi-naive rebuild.
#[test]
fn exhaustive_three_node_transitive_updates_match_scratch() {
    let rules = transitive_rules();
    let nodes = ["a", "b", "c"];
    let universe: Vec<Fact> = nodes
        .iter()
        .flat_map(|from| nodes.iter().map(move |to| fact("edge", from, to)))
        .collect();

    for mask in 0usize..(1usize << universe.len()) {
        let edb: Vec<Fact> = universe
            .iter()
            .enumerate()
            .filter(|(bit, _)| mask & (1usize << bit) != 0)
            .map(|(_, edge)| edge.clone())
            .collect();
        let base = IncrementalSession::new("contract", edb.clone(), &rules).unwrap();
        assert_scratch_parity(&base, &edb, &rules);

        for (bit, edge) in universe.iter().enumerate() {
            let present = mask & (1usize << bit) != 0;
            let mut expected_edb = edb.clone();
            let mut adjusted = base.clone();
            if present {
                adjusted
                    .apply([SignedFact {
                        fact: edge.clone(),
                        weight: -1,
                    }])
                    .unwrap();
                expected_edb.retain(|candidate| candidate.key() != edge.key());
            } else {
                adjusted
                    .apply([SignedFact {
                        fact: edge.clone(),
                        weight: 1,
                    }])
                    .unwrap();
                expected_edb.push(edge.clone());
            }
            assert_scratch_parity(&adjusted, &expected_edb, &rules);
        }
    }
}
