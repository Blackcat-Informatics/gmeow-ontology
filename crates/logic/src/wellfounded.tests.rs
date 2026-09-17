// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::provenance::{mint_derivation_id, mint_reifier};
use crate::rule_ir::{EvalAtom, EvalTerm};
use crate::store::WorldStore;
use purrdf::TermValue;

const WF: &str = "https://example.org/profiles/well-founded/";

fn wf_rules() -> Vec<EvalRule> {
    let atom = |subject: &str, predicate: &str, object: &str, negated| EvalAtom {
        subject: EvalTerm::var(subject),
        predicate: format!("{WF}{predicate}"),
        object: EvalTerm::var(object),
        negated,
    };
    vec![EvalRule {
        numeric: Vec::new(),
        head: atom("?X", "win", "?X", false),
        body: vec![
            atom("?X", "move", "?Y", false),
            atom("?Y", "win", "?Y", true),
        ],
        rule_iri: format!("{WF}ruleWin"),
        distinct_pairs: Vec::new(),
        builtins: Vec::new(),
        reduction: None,
        constraint_tag: None,
    }]
}

fn wf_store() -> WorldStore {
    let store = WorldStore::new();
    store.insert_quad(
        &format!("{WF}world-game"),
        &format!("{WF}p1"),
        &format!("{WF}move"),
        &format!("{WF}p2"),
    );
    store
}

fn wf_fact(subject: &str, predicate: &str, object: &str) -> Fact {
    Fact {
        subject: TermValue::iri(format!("{WF}{subject}")),
        predicate: format!("{WF}{predicate}"),
        object: TermValue::iri(format!("{WF}{object}")),
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
fn well_founded_derives_exactly_win_p1_p1() {
    let rules = wf_rules();
    let store = wf_store();
    let rows = materialize(&store, &rules).expect("materialize");

    // Partition asserted vs derived.
    let world = format!("{WF}world-game");
    let derived: Vec<&DerivedRow> = rows
        .iter()
        .filter(|r| r.rule_iri != crate::provenance::ASSERT_RULE_IRI)
        .collect();
    assert_eq!(derived.len(), 1, "exactly one derived quad: {rows:#?}");
    let row = derived[0];

    // win(p1, p1) in world-game.
    assert_eq!(row.graph, world);
    assert_eq!(row.subject, TermValue::iri(format!("{WF}p1")));
    assert_eq!(row.predicate.as_str(), format!("{WF}win"));
    assert_eq!(row.object, TermValue::iri(format!("{WF}p1")));

    // Provenance: rule_iri = …/ruleWin, source = reifier(move(p1,p2)).
    assert_eq!(row.rule_iri, format!("{WF}ruleWin"));
    let move_reifier = mint_reifier(
        &TermValue::iri(format!("{WF}p1")),
        &format!("{WF}move"),
        &TermValue::iri(format!("{WF}p2")),
    )
    .unwrap();
    assert_eq!(row.source_quad_ids, vec![move_reifier.clone()]);
    assert_eq!(
        row.derivation_id,
        mint_derivation_id(&format!("{WF}ruleWin"), &[move_reifier.as_str()])
    );

    // The asserted move(p1,p2) row is also present.
    assert!(
        rows.iter()
            .any(|r| r.rule_iri == crate::provenance::ASSERT_RULE_IRI
                && r.predicate.as_str() == format!("{WF}move")),
        "asserted move row present"
    );
}

#[test]
fn incremental_grounding_reuses_only_an_unchanged_complete_wfs_slice() {
    let world = format!("{WF}world-game");
    let rules = wf_rules();
    let mut session = IncrementalWellFoundedSession::new(
        "contract",
        &world,
        [wf_fact("p1", "move", "p2")],
        &rules,
    )
    .expect("initial incremental WFS session");
    let direct = materialize(&wf_store(), &rules).expect("direct WFS");
    assert_eq!(
        session.rows().iter().map(row_key).collect::<Vec<_>>(),
        direct.iter().map(row_key).collect::<Vec<_>>(),
        "ground-program solve preserves direct WFS rows"
    );

    let initial_rows = session.rows.clone();
    let cancelled = session
        .apply([
            SignedFact {
                fact: wf_fact("p2", "move", "p3"),
                weight: 1,
            },
            SignedFact {
                fact: wf_fact("p2", "move", "p3"),
                weight: -1,
            },
        ])
        .expect("cancelled shot");
    assert!(!cancelled.grounding.slice_changed);
    assert!(!cancelled.solve.solver_reran());
    assert_eq!(cancelled.solve.edb_changes, 0);
    assert_eq!(cancelled.solve.ground_rule_changes, 0);
    assert!(Arc::ptr_eq(&initial_rows, &cancelled.rows));
    assert!(Arc::ptr_eq(&cancelled.rows, &session.rows));

    let changed = session
        .apply([SignedFact {
            fact: wf_fact("p2", "move", "p3"),
            weight: 1,
        }])
        .expect("changed shot");
    assert!(changed.grounding.slice_changed);
    assert!(changed.solve.solver_reran());
    assert!(!Arc::ptr_eq(&cancelled.rows, &changed.rows));
    assert_eq!(
        changed.solve.solver.as_str(),
        "well-founded alternating fixpoint"
    );
    assert!(changed.rows.iter().any(|row| {
        row.predicate == format!("{WF}win")
            && crate::provenance::term_display(&row.subject) == format!("<{WF}p2>")
    }));
    session
        .check_grounding_scratch_parity()
        .expect("changed WFS grounding matches scratch");
}
