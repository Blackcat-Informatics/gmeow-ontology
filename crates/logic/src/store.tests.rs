// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const WORLD_A: &str = "http://world/A";
const WORLD_B: &str = "http://world/B";

const S_A: &str = "http://example.org/s/a";
const P_A: &str = "http://example.org/p/a";
const O_A: &str = "http://example.org/o/a";

const S_B: &str = "http://example.org/s/b";
const P_B: &str = "http://example.org/p/b";
const O_B: &str = "http://example.org/o/b";

fn populated_store() -> WorldStore {
    let store = WorldStore::new();
    store.insert_quad(WORLD_A, S_A, P_A, O_A);
    store.insert_quad(WORLD_B, S_B, P_B, O_B);
    store
}

#[test]
fn native_world_ingress_preserves_standpoint_claim_evidence() {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let world = builder.intern_iri(WORLD_A);
    let reifier = builder.intern_iri("https://example.org/claim");
    let speaker = builder.intern_iri("https://example.org/speaker");
    let says = builder.intern_iri("https://blackcatinformatics.ca/gmeow/fullName");
    let literal = builder.intern_literal(purrdf::RdfLiteral {
        lexical_form: "مرحبا".to_owned(),
        datatype: None,
        language: Some("ar".to_owned()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    });
    let statement = builder.intern_triple(speaker, says, literal);
    builder.push_reifier_in_graph(reifier, statement, Some(world));
    let according_to = builder.intern_iri("https://blackcatinformatics.ca/gmeow/accordingTo");
    builder.push_annotation_in_graph(reifier, according_to, speaker, Some(world));
    let source = builder.freeze().expect("attributed claim");
    let store = WorldStore::from_dataset(&source).expect("native world ingress");
    let quads = store.quads_for_pattern_in_world(WORLD_A, None, None, None);
    assert_eq!(
        quads.len(),
        2,
        "both the claim and its attribution reach the world"
    );
    assert!(quads.iter().any(|quad| quad.p
        == TermValue::iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")
        && quad.o == source.term_value(statement)));
    assert!(
        quads
            .iter()
            .any(|quad| quad.p == source.term_value(according_to)
                && quad.o == source.term_value(speaker))
    );
    assert!(
        store
            .quads_for_pattern_in_world(WORLD_B, None, None, None)
            .is_empty(),
        "another world cannot see the claim or attribution"
    );
}

#[test]
fn load_dataset_preserves_named_graph_worlds() {
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    let quad =
        RdfQuad::new(RdfTerm::iri(S_A), P_A, RdfTerm::iri(O_A)).in_graph(RdfTerm::iri(WORLD_A));
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&quad);
    let source = builder.freeze().expect("valid dataset");
    let store = WorldStore::new();
    store
        .load_dataset(source.as_ref())
        .expect("RDF dataset should load into LOGIC");

    assert_eq!(store.quads_in_world(WORLD_A).len(), 1);
    assert!(store.quads_in_world(WORLD_B).is_empty());
}

/// Build a single-quad frozen dataset placing `(s, p, o)` in graph `world`.
fn single_quad_dataset(world: &str, s: &str, p: &str, o: &str) -> std::sync::Arc<RdfDataset> {
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};
    let quad = RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(world));
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&quad);
    builder.freeze().expect("valid dataset")
}

#[test]
fn from_dataset_folds_arc_dataset_repo_root_free() {
    // The Arc-ergonomic constructor a runtime consumer uses on its own dataset.
    let arc = single_quad_dataset(WORLD_A, S_A, P_A, O_A);
    let store = WorldStore::from_dataset(&arc).expect("Arc<RdfDataset> should fold");
    assert_eq!(store.worlds(), vec![WORLD_A]);
    assert_eq!(store.quads_in_world(WORLD_A).len(), 1);
}

#[test]
fn from_dataset_then_insert_quad_is_additive() {
    // Refresh shape 1: additive append on top of the folded base.
    let arc = single_quad_dataset(WORLD_A, S_A, P_A, O_A);
    let store = WorldStore::from_dataset(&arc).expect("fold base");
    store.insert_quad(WORLD_B, S_B, P_B, O_B);
    let mut worlds = store.worlds();
    worlds.sort();
    assert_eq!(worlds, vec![WORLD_A, WORLD_B], "append must not reset");
    assert_eq!(store.quads_in_world(WORLD_A).len(), 1);
    assert_eq!(store.quads_in_world(WORLD_B).len(), 1);
}

#[test]
fn from_dataset_fresh_construct_is_wholesale_replace() {
    // Refresh shape 2: a fresh store from a re-folded dataset carries ONLY the
    // new fold's worlds — no in-place mutation, no double-count.
    let first = single_quad_dataset(WORLD_A, S_A, P_A, O_A);
    let store = WorldStore::from_dataset(&first).expect("fold first");
    assert_eq!(store.worlds(), vec![WORLD_A]);

    let second = single_quad_dataset(WORLD_B, S_B, P_B, O_B);
    let replaced = WorldStore::from_dataset(&second).expect("fold second");
    assert_eq!(
        replaced.worlds(),
        vec![WORLD_B],
        "wholesale replace carries only the re-folded world"
    );
    assert!(replaced.quads_in_world(WORLD_A).is_empty());
}

#[test]
fn world_a_contains_its_own_quad() {
    let store = populated_store();
    let quads = store.quads_in_world(WORLD_A);
    assert_eq!(quads.len(), 1, "world A should have exactly 1 quad");
    let q = &quads[0];
    assert!(
        q[0].contains("s/a"),
        "subject should be A's subject, got {q:?}"
    );
}

#[test]
fn world_b_contains_its_own_quad() {
    let store = populated_store();
    let quads = store.quads_in_world(WORLD_B);
    assert_eq!(quads.len(), 1, "world B should have exactly 1 quad");
    let q = &quads[0];
    assert!(
        q[0].contains("s/b"),
        "subject should be B's subject, got {q:?}"
    );
}

#[test]
fn no_cross_world_leakage_a_to_b() {
    let store = populated_store();
    let a_quads = store.quads_in_world(WORLD_A);
    // none of world A's quads should appear in world B
    for q in &a_quads {
        assert!(
            !q[0].contains("s/b"),
            "world A contains B's subject — cross-world leak: {q:?}"
        );
    }
    // world B should not see A's triple
    let b_quads = store.quads_in_world(WORLD_B);
    for q in &b_quads {
        assert!(
            !q[0].contains("s/a"),
            "world B contains A's subject — cross-world leak: {q:?}"
        );
    }
}

#[test]
fn worlds_lists_both_world_iris() {
    let store = populated_store();
    let mut worlds = store.worlds();
    worlds.sort();
    assert_eq!(worlds, vec![WORLD_A, WORLD_B]);
}

#[test]
fn empty_store_has_no_worlds() {
    let store = WorldStore::new();
    assert!(store.worlds().is_empty());
}

#[test]
fn quads_in_nonexistent_world_returns_empty() {
    let store = populated_store();
    let quads = store.quads_in_world("http://world/doesNotExist");
    assert!(quads.is_empty());
}

#[test]
fn quad_world_column_matches_world_iri() {
    let store = populated_store();
    for q in store.quads_in_world(WORLD_A) {
        assert_eq!(q[3], WORLD_A, "fourth column must be the world IRI");
    }
    for q in store.quads_in_world(WORLD_B) {
        assert_eq!(q[3], WORLD_B, "fourth column must be the world IRI");
    }
}

// ── quads_for_pattern_in_world ────────────────────────────────────────────

#[test]
fn pattern_all_none_returns_all_quads_in_world() {
    let store = populated_store();
    let quads = store.quads_for_pattern_in_world(WORLD_A, None, None, None);
    assert_eq!(quads.len(), 1, "world A has exactly 1 quad");
    assert_eq!(
        quads[0].s.as_iri(),
        Some(S_A),
        "subject must be world A's subject"
    );
}

#[test]
fn pattern_subject_filter_returns_match() {
    let store = populated_store();
    // Filter by the correct subject — should return the one quad.
    let quads = store.quads_for_pattern_in_world(WORLD_A, Some(S_A), None, None);
    assert_eq!(quads.len(), 1);
    // Filter by a wrong subject — should return empty.
    let quads_miss = store.quads_for_pattern_in_world(WORLD_A, Some(S_B), None, None);
    assert!(
        quads_miss.is_empty(),
        "wrong subject should return no results"
    );
}

#[test]
fn pattern_predicate_filter_returns_match() {
    let store = populated_store();
    let quads = store.quads_for_pattern_in_world(WORLD_A, None, Some(P_A), None);
    assert_eq!(quads.len(), 1);
    let quads_miss = store.quads_for_pattern_in_world(WORLD_A, None, Some(P_B), None);
    assert!(quads_miss.is_empty());
}

#[test]
fn pattern_nonexistent_world_returns_empty() {
    let store = populated_store();
    let quads = store.quads_for_pattern_in_world("http://world/doesNotExist", None, None, None);
    assert!(quads.is_empty());
}

#[test]
fn pattern_invalid_world_iri_returns_empty() {
    let store = populated_store();
    let quads = store.quads_for_pattern_in_world("not a valid IRI", None, None, None);
    assert!(quads.is_empty());
}

#[test]
fn pattern_no_cross_world_leak() {
    let store = populated_store();
    // World A's pattern should NOT see world B's quads.
    let quads_a = store.quads_for_pattern_in_world(WORLD_A, None, None, None);
    for q in &quads_a {
        assert!(
            !q.s.as_iri().unwrap_or_default().contains("s/b"),
            "world A pattern returned world B's quad: {q:?}"
        );
    }
    // World B's pattern should NOT see world A's quads.
    let quads_b = store.quads_for_pattern_in_world(WORLD_B, None, None, None);
    for q in &quads_b {
        assert!(
            !q.s.as_iri().unwrap_or_default().contains("s/a"),
            "world B pattern returned world A's quad: {q:?}"
        );
    }
}

// ── select (SPARQL SELECT helper) ─────────────────────────────────────────

#[test]
fn select_returns_canonical_bindings() {
    let store = populated_store();
    // Query only world A's objects for a known subject and predicate.
    let sparql = format!("SELECT ?o WHERE {{ GRAPH <{WORLD_A}> {{ <{S_A}> <{P_A}> ?o }} }}");
    let rows = store.select(&sparql).expect("select must succeed");
    assert_eq!(rows.len(), 1, "exactly one match expected: {rows:?}");
    let canonical_o = &rows[0]["o"];
    assert_eq!(
        canonical_o,
        &format!("<{O_A}>"),
        "canonical form must be <iri>: {canonical_o:?}"
    );
}

#[test]
fn select_no_cross_world_results() {
    let store = populated_store();
    // Query world A but for world B's triple — should return nothing.
    let sparql = format!("SELECT ?o WHERE {{ GRAPH <{WORLD_A}> {{ <{S_B}> <{P_B}> ?o }} }}");
    let rows = store.select(&sparql).expect("select must succeed");
    assert!(rows.is_empty(), "no cross-world results expected: {rows:?}");
}

#[test]
fn select_parse_error_returns_err() {
    let store = WorldStore::new();
    let result = store.select("NOT VALID SPARQL AT ALL");
    assert!(result.is_err(), "invalid SPARQL must return Err");
}
