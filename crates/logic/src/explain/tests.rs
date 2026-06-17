// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the native explanation engine.
//!
//! The `transitive_derivation_*` tests reconstruct the committed conformance case
//! `conformance/logic/cases/explanation/transitive-derivation/` from inline rows and
//! assert the derived quad's `cited_iris` equals the golden `cited-iri-skeleton`
//! block (the parity anchor).  The remaining tests cover the reifier recipe, cycle
//! detection, the trivial asserted-fact explanation, and the faithfulness gate.

use super::*;
use std::collections::BTreeSet;

const BASE: &str = "https://example.org/explanation/transitive-derivation/";

/// The committed golden skeleton file for the derived `A subOf C` quad.
const GOLDEN_MD: &str = include_str!(
    "../../../../conformance/logic/cases/explanation/transitive-derivation/expected/explanation/b2fb756dbd211fce14c818c1fdd042e04d46f4e0.md"
);

/// Parse the `<!-- cited-iri-skeleton ... -->` block from a golden `.md`.
/// Mirrors the Python `_parse_cited_iri_skeleton` in `logic_runner.py`.
fn parse_cited_iri_skeleton(text: &str) -> BTreeSet<String> {
    let mut in_block = false;
    let mut iris: BTreeSet<String> = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "<!-- cited-iri-skeleton" {
            in_block = true;
            continue;
        }
        if in_block {
            if trimmed == "-->" {
                break;
            }
            if !trimmed.is_empty() {
                iris.insert(trimmed.to_owned());
            }
        }
    }
    iris
}

/// Build the three rows for the transitive-derivation case (two asserted facts +
/// one derived quad), in the same input order the runner produces them.
fn transitive_rows() -> Vec<Row> {
    let world = format!("{BASE}world-main");
    let a = format!("{BASE}A");
    let b = format!("{BASE}B");
    let c = format!("{BASE}C");
    let sub_of = format!("{BASE}subOf");
    let rule = format!("{BASE}ruleTransitiveSubOf");
    let assert_rule = ASSERT_RULE_IRI.to_owned();

    // Reifiers (golden-pinned values from the committed .md).
    let r_ab =
        "https://blackcatinformatics.ca/gmeow/reifier/2a1d02ad634a4477d506abf9016855e8254c0cdc";
    let r_bc =
        "https://blackcatinformatics.ca/gmeow/reifier/a04391d8bb175265efcb8bbc9bcd0611e03dcf0d";

    let d_ab =
        "https://blackcatinformatics.ca/gmeow/derivation/3979c926fb3ed2fc3308b1056e6fa759cee8bf74";
    let d_bc =
        "https://blackcatinformatics.ca/gmeow/derivation/f403cceecb4702c0ef59c169674eeb05b0aa7d6d";
    let d_ac =
        "https://blackcatinformatics.ca/gmeow/derivation/23b05cdc161584b4de12f42c4a840d71f4bf0149";

    vec![
        // A subOf B (asserted)
        Row {
            graph: world.clone(),
            subject: a.clone(),
            predicate: sub_of.clone(),
            obj: format!("<{b}>"),
            derivation_id: d_ab.to_owned(),
            rule_iri: assert_rule.clone(),
            source_quad_ids: vec![r_ab.to_owned()],
        },
        // B subOf C (asserted)
        Row {
            graph: world.clone(),
            subject: b.clone(),
            predicate: sub_of.clone(),
            obj: format!("<{c}>"),
            derivation_id: d_bc.to_owned(),
            rule_iri: assert_rule.clone(),
            source_quad_ids: vec![r_bc.to_owned()],
        },
        // A subOf C (derived by ruleTransitiveSubOf from A subOf B + B subOf C)
        Row {
            graph: world.clone(),
            subject: a.clone(),
            predicate: sub_of.clone(),
            obj: format!("<{c}>"),
            derivation_id: d_ac.to_owned(),
            rule_iri: rule.clone(),
            source_quad_ids: vec![r_ab.to_owned(), r_bc.to_owned()],
        },
    ]
}

// ── Reifier recipe golden ──────────────────────────────────────────────────────

#[test]
fn reifier_recipe_golden() {
    // The derived quad A subOf C must mint the golden target reifier.
    let row = &transitive_rows()[2];
    assert_eq!(
        reifier_from_row(row),
        "https://blackcatinformatics.ca/gmeow/reifier/b2fb756dbd211fce14c818c1fdd042e04d46f4e0",
        "reifier recipe must be byte-identical to the Python oracle golden"
    );
}

// ── Transitive-derivation parity ───────────────────────────────────────────────

#[test]
fn transitive_derivation_cited_iris_match_golden() {
    let rows = transitive_rows();
    // The derived quad is the third row (input order).
    let exp = explain_one(&rows, 2).expect("explanation must reconstruct");

    assert_eq!(
        exp.target_quad_reifier,
        "https://blackcatinformatics.ca/gmeow/reifier/b2fb756dbd211fce14c818c1fdd042e04d46f4e0"
    );

    let golden = parse_cited_iri_skeleton(GOLDEN_MD);
    assert!(
        !golden.is_empty(),
        "golden cited-iri-skeleton block must parse"
    );
    assert_eq!(
        exp.cited_iris, golden,
        "cited_iris must equal the committed golden skeleton set"
    );
}

#[test]
fn transitive_derivation_step_order_and_rules() {
    let rows = transitive_rows();
    let exp = explain_one(&rows, 2).expect("explanation must reconstruct");

    // DFS order: derived step first (depth 0), then the two asserted antecedents.
    assert_eq!(exp.step_skeleton.len(), 3);
    assert_eq!(exp.step_skeleton[0].depth, 0);
    assert!(!exp.step_skeleton[0].is_asserted);
    assert_eq!(
        exp.step_skeleton[0].rule_iri,
        format!("{BASE}ruleTransitiveSubOf")
    );
    // Antecedents are sorted by reifier: r_ab (2a1d02...) < r_bc (a04391...),
    // so A subOf B precedes B subOf C.
    assert!(exp.step_skeleton[1].is_asserted);
    assert!(exp.step_skeleton[2].is_asserted);
    assert_eq!(exp.step_skeleton[1].subject_iri, format!("{BASE}A"));
    assert_eq!(exp.step_skeleton[2].subject_iri, format!("{BASE}B"));

    // source_step_ids on the derived step = sorted derivation_ids of the two
    // antecedents' first steps.
    let mut expected_sources = vec![
        "https://blackcatinformatics.ca/gmeow/derivation/3979c926fb3ed2fc3308b1056e6fa759cee8bf74"
            .to_owned(),
        "https://blackcatinformatics.ca/gmeow/derivation/f403cceecb4702c0ef59c169674eeb05b0aa7d6d"
            .to_owned(),
    ];
    expected_sources.sort();
    assert_eq!(exp.step_skeleton[0].source_step_ids, expected_sources);
}

#[test]
fn explain_all_is_input_order() {
    let rows = transitive_rows();
    let all = explain_all(&rows).expect("all explanations must reconstruct");
    assert_eq!(all.len(), 3);
    // One explanation per row, same order; targets match the row reifiers.
    for (i, exp) in all.iter().enumerate() {
        assert_eq!(exp.target_quad_reifier, reifier_from_row(&rows[i]));
        assert_eq!(exp.target_derivation_id, rows[i].derivation_id);
    }
}

// ── Asserted-fact trivial explanation ──────────────────────────────────────────

#[test]
fn asserted_fact_is_single_depth_zero_step() {
    let rows = transitive_rows();
    // Row 0 is the asserted A subOf B.
    let exp = explain_one(&rows, 0).expect("asserted-fact explanation must reconstruct");
    assert_eq!(
        exp.step_skeleton.len(),
        1,
        "asserted fact has a single step"
    );
    let step = &exp.step_skeleton[0];
    assert_eq!(step.depth, 0);
    assert!(step.is_asserted);
    assert_eq!(step.rule_iri, ASSERT_RULE_IRI);
    assert!(
        step.source_step_ids.is_empty(),
        "asserted fact has no antecedent steps (self-reifier is filtered out)"
    );
}

// ── Cycle detection ────────────────────────────────────────────────────────────

#[test]
fn cycle_is_detected() {
    let world = "https://example.org/w".to_owned();
    let s = "https://example.org/x".to_owned();
    let p = "https://example.org/p".to_owned();
    let obj = "<https://example.org/y>".to_owned();
    let self_reifier = reifier_from_strings(&s, &p, &obj);

    // A quad whose source_quad_ids references ITSELF as a non-self antecedent.
    // We engineer the cycle by pointing source at a *different* reifier that
    // resolves back to this same quad's key.  Simplest: two quads that cite each
    // other.  Quad-1 (x p y) cites quad-2's reifier; quad-2 (a p b) cites quad-1's.
    let s2 = "https://example.org/a".to_owned();
    let obj2 = "<https://example.org/b>".to_owned();
    let r1 = reifier_from_strings(&s, &p, &obj);
    let r2 = reifier_from_strings(&s2, &p, &obj2);

    let rows = vec![
        Row {
            graph: world.clone(),
            subject: s.clone(),
            predicate: p.clone(),
            obj: obj.clone(),
            derivation_id: "https://example.org/d1".to_owned(),
            rule_iri: "https://example.org/rule".to_owned(),
            source_quad_ids: vec![r2.clone()],
        },
        Row {
            graph: world.clone(),
            subject: s2.clone(),
            predicate: p.clone(),
            obj: obj2.clone(),
            derivation_id: "https://example.org/d2".to_owned(),
            rule_iri: "https://example.org/rule".to_owned(),
            source_quad_ids: vec![r1.clone()],
        },
    ];
    let _ = self_reifier; // silence unused on some toolchains
    let err = explain_one(&rows, 0).expect_err("a derivation cycle must be a hard error");
    match err {
        ExplainError::Cycle { .. } => {}
        other => panic!("expected Cycle, got {other:?}"),
    }
}

// ── Unresolved reifier ─────────────────────────────────────────────────────────

#[test]
fn unresolved_antecedent_is_error() {
    let world = "https://example.org/w".to_owned();
    let rows = vec![Row {
        graph: world.clone(),
        subject: "https://example.org/x".to_owned(),
        predicate: "https://example.org/p".to_owned(),
        obj: "<https://example.org/y>".to_owned(),
        derivation_id: "https://example.org/d1".to_owned(),
        rule_iri: "https://example.org/rule".to_owned(),
        source_quad_ids: vec!["https://blackcatinformatics.ca/gmeow/reifier/deadbeef".to_owned()],
    }];
    let err = explain_one(&rows, 0).expect_err("an unresolved antecedent must be a hard error");
    match err {
        ExplainError::UnresolvedReifier { .. } => {}
        other => panic!("expected UnresolvedReifier, got {other:?}"),
    }
}

// ── Faithfulness gate ──────────────────────────────────────────────────────────

#[test]
fn faithfulness_passes_for_valid_explanation() {
    let rows = transitive_rows();
    let exp = explain_one(&rows, 2).expect("explanation must reconstruct");
    assert_faithful(&exp, &rows).expect("a valid explanation is faithful by construction");
}

#[test]
fn faithfulness_rejects_fabricated_cited_iri() {
    let rows = transitive_rows();
    let mut exp = explain_one(&rows, 2).expect("explanation must reconstruct");
    // Inject an IRI that is NOT anywhere in the proof trace.
    let fabricated = "https://example.org/HALLUCINATED".to_owned();
    exp.cited_iris.insert(fabricated.clone());
    let err =
        assert_faithful(&exp, &rows).expect_err("a cited IRI outside the trace must be rejected");
    assert_eq!(err.cited_iri, fabricated);
}
