// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::rule_ir::EvalTerm;
use purrdf::TermValue;

fn atom(s: &str, p: &str, o: &str) -> EvalAtom {
    EvalAtom {
        subject: EvalTerm::var(s),
        predicate: p.into(),
        object: EvalTerm::var(o),
        negated: false,
    }
}

fn seed() -> Solution {
    Solution {
        bindings: Vec::new(),
        source_facts: Vec::new(),
    }
}

fn policy(limit: usize) -> Policy<'static> {
    Policy {
        max_matches: limit,
        distinct: &[],
        retain_sources: true,
    }
}

#[test]
fn late_match_retains_its_actual_body_provenance_and_exact_cap_completes() {
    let mut store = RelationStore::new();
    for index in 0..128 {
        store.insert(
            "urn:p",
            &TermValue::iri(format!("urn:x:{index}")),
            &TermValue::iri(format!("urn:y:{index}")),
        );
    }
    store.insert(
        "urn:q",
        &TermValue::iri("urn:y:127"),
        &TermValue::iri("urn:z"),
    );
    let atoms = [atom("?x", "urn:p", "?y"), atom("?y", "urn:q", "?z")];
    let mut found = Vec::new();
    assert_eq!(
        walk(&atoms, &store, &seed(), policy(128), |solution| {
            found.push(solution);
            Ok(true)
        })
        .unwrap(),
        Outcome::Complete
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].get("?x"), Some(&TermValue::iri("urn:x:127")));
    assert_eq!(found[0].source_facts.len(), 2);
    assert_eq!(
        found[0].source_facts[0].subject,
        TermValue::iri("urn:x:127")
    );
    assert_eq!(found[0].source_facts[1].predicate, "urn:q");
    assert_eq!(
        walk(&atoms, &store, &seed(), policy(1), |_| Ok(true)).unwrap(),
        Outcome::Exhausted
    );
}

#[test]
fn distinct_existential_probe_stops_at_one_witness_without_cross_product() {
    let mut store = RelationStore::new();
    for index in 0..24 {
        store.insert(
            "urn:p",
            &TermValue::iri("urn:s"),
            &TermValue::iri(format!("urn:value:{index}")),
        );
    }
    let atoms: Vec<_> = (0..6)
        .map(|i| atom("?s", "urn:p", &format!("?v{i}")))
        .collect();
    let distinct: Vec<_> = (0..6)
        .flat_map(|left| {
            (left + 1..6).map(move |right| (format!("?v{left}"), format!("?v{right}")))
        })
        .collect();
    let mut visits = 0;
    let outcome = walk(
        &atoms,
        &store,
        &seed(),
        Policy {
            max_matches: 32,
            distinct: &distinct,
            retain_sources: false,
        },
        |solution| {
            visits += 1;
            assert!(crate::rule_ir::distinct_pairs_satisfied(
                &distinct, &solution
            )?);
            assert!(solution.source_facts.is_empty());
            Ok(false)
        },
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Stopped);
    assert_eq!(visits, 1);
}

#[test]
fn repeated_variable_and_empty_body_keep_their_native_join_meaning() {
    let mut store = RelationStore::new();
    store.insert("urn:p", &TermValue::iri("urn:a"), &TermValue::iri("urn:b"));
    store.insert("urn:p", &TermValue::iri("urn:c"), &TermValue::iri("urn:c"));
    let mut visits = 0;
    assert_eq!(
        walk(
            &[atom("?x", "urn:p", "?x")],
            &store,
            &seed(),
            policy(1),
            |solution| {
                visits += 1;
                assert_eq!(solution.get("?x"), Some(&TermValue::iri("urn:c")));
                Ok(true)
            }
        )
        .unwrap(),
        Outcome::Complete
    );
    assert_eq!(visits, 1);
    assert_eq!(
        walk(&[], &store, &seed(), policy(1), |solution| {
            assert!(solution.bindings.is_empty());
            Ok(false)
        })
        .unwrap(),
        Outcome::Stopped
    );
    assert_eq!(
        walk(&[], &store, &seed(), policy(0), |_| panic!(
            "zero budget cannot visit"
        ))
        .unwrap(),
        Outcome::Exhausted
    );
}
