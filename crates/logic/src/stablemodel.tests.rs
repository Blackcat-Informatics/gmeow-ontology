// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::provenance::ASSERT_RULE_IRI;
use crate::rule_ir::{EvalAtom, EvalTerm};
use crate::store::WorldStore;
use purrdf::TermValue;

const SM: &str = "https://example.org/profiles/stable-model/";

fn sm_rules() -> Vec<EvalRule> {
    let atom = |predicate: &str, negated| EvalAtom {
        subject: EvalTerm::var("?X"),
        predicate: format!("{SM}{predicate}"),
        object: EvalTerm::var("?X"),
        negated,
    };
    let rule = |head: &str, blocked: &str, name: &str| EvalRule {
        numeric: Vec::new(),
        head: atom(head, false),
        body: vec![atom("candidate", false), atom(blocked, true)],
        rule_iri: format!("{SM}{name}"),
        distinct_pairs: Vec::new(),
        builtins: Vec::new(),
        reduction: None,
        constraint_tag: None,
    };
    vec![
        rule("inSet", "outSet", "ruleInSet"),
        rule("outSet", "inSet", "ruleOutSet"),
    ]
}

fn sm_store() -> WorldStore {
    let store = WorldStore::new();
    store.insert_quad(
        &format!("{SM}world-choice"),
        &format!("{SM}x"),
        &format!("{SM}candidate"),
        &format!("{SM}x"),
    );
    store
}

fn sm_fact(name: &str) -> Fact {
    Fact {
        subject: TermValue::iri(format!("{SM}{name}")),
        predicate: format!("{SM}candidate"),
        object: TermValue::iri(format!("{SM}{name}")),
    }
}

fn row_key(row: &DerivedRow) -> (String, String, String, String, String) {
    (
        row.graph.clone(),
        crate::provenance::term_display(&row.subject),
        row.predicate.clone(),
        crate::provenance::term_display(&row.object),
        row.rule_iri.clone(),
    )
}

#[test]
fn exactly_two_stable_models() {
    let rules = sm_rules();
    let store = sm_store();
    let per_world = stable_models(&store, &rules).expect("stable_models");
    assert_eq!(per_world.len(), 1, "one world");
    let (world, models) = &per_world[0];
    assert_eq!(world, &format!("{SM}world-choice"));
    assert_eq!(models.len(), 2, "exactly two stable models: {models:#?}");

    // Model 1 = {candidate, inSet}; Model 2 = {candidate, outSet} (canonical
    // order: inSet < outSet lexicographically).
    let predicates: Vec<Vec<String>> = models
        .iter()
        .map(|m| {
            m.atoms
                .iter()
                .map(|f| f.predicate.as_str().to_owned())
                .collect()
        })
        .collect();
    assert!(
        predicates
            .iter()
            .any(|ps| ps.contains(&format!("{SM}inSet"))),
        "an inSet model exists: {predicates:?}"
    );
    assert!(
        predicates
            .iter()
            .any(|ps| ps.contains(&format!("{SM}outSet"))),
        "an outSet model exists: {predicates:?}"
    );
    // Neither model contains BOTH inSet and outSet.
    for ps in &predicates {
        let has_in = ps.contains(&format!("{SM}inSet"));
        let has_out = ps.contains(&format!("{SM}outSet"));
        assert!(!(has_in && has_out), "no model has both: {ps:?}");
    }
}

#[test]
fn cautious_emits_only_asserted_candidate() {
    let rules = sm_rules();
    let store = sm_store();
    let rows = cautious_materialize(&store, &rules).expect("cautious");

    // Cautious intersection is empty → only the asserted candidate(x,x) quad.
    assert_eq!(rows.len(), 1, "exactly one (asserted) row: {rows:#?}");
    let row = &rows[0];
    assert_eq!(row.rule_iri, ASSERT_RULE_IRI);
    assert_eq!(row.predicate.as_str(), format!("{SM}candidate"));
    assert_eq!(
        crate::provenance::term_display(&row.subject),
        format!("<{SM}x>")
    );
    assert_eq!(
        crate::provenance::term_display(&row.object),
        format!("<{SM}x>")
    );
    // No derived (non-asserted) rows.
    assert!(
        !rows.iter().any(|r| r.rule_iri != ASSERT_RULE_IRI),
        "no derived rows in the cautious materialization"
    );
}

#[test]
fn incremental_grounding_reruns_stable_solver_only_for_changed_slice() {
    let world = format!("{SM}world-choice");
    let rules = sm_rules();
    let mut session =
        IncrementalStableModelSession::new("contract", &world, [sm_fact("x")], &rules)
            .expect("initial incremental stable-model session");
    let direct = cautious_materialize(&sm_store(), &rules).expect("direct cautious solve");
    assert_eq!(
        session.rows().iter().map(row_key).collect::<Vec<_>>(),
        direct.iter().map(row_key).collect::<Vec<_>>(),
        "ground-program solve preserves direct cautious rows"
    );

    let initial_rows = session.rows.clone();
    let cancelled = session
        .apply([
            SignedFact {
                fact: sm_fact("y"),
                weight: 1,
            },
            SignedFact {
                fact: sm_fact("y"),
                weight: -1,
            },
        ])
        .expect("cancelled stable-model shot");
    assert!(!cancelled.grounding.slice_changed);
    assert!(!cancelled.solve.solver_reran());
    assert!(Arc::ptr_eq(&initial_rows, &cancelled.rows));
    assert!(Arc::ptr_eq(&cancelled.rows, &session.rows));

    let changed = session
        .apply([SignedFact {
            fact: sm_fact("y"),
            weight: 1,
        }])
        .expect("changed stable-model shot");
    assert!(changed.grounding.slice_changed);
    assert!(changed.solve.solver_reran());
    assert!(!Arc::ptr_eq(&cancelled.rows, &changed.rows));
    assert_eq!(
        changed.solve.solver.as_str(),
        "stable-model cautious enumeration"
    );
    assert_eq!(
        changed
            .rows
            .iter()
            .filter(|row| row.rule_iri == ASSERT_RULE_IRI)
            .count(),
        2,
        "both candidates are asserted while the choice atoms remain non-cautious"
    );
    session
        .check_grounding_scratch_parity()
        .expect("changed stable-model grounding matches scratch");
}
