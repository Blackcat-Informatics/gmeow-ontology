// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::store::WorldStore;

// ── BudgetStatus canonical spellings ──────────────────────────────────────────────────────

#[test]
fn budget_status_ok_spells_ok() {
    assert_eq!(BudgetStatus::Ok.as_str(), "ok");
    assert_eq!(BudgetStatus::Ok.to_string(), "ok");
}

#[test]
fn budget_status_partial_spells_partial() {
    assert_eq!(BudgetStatus::Partial.as_str(), "partial");
    assert_eq!(BudgetStatus::Partial.to_string(), "partial");
}

#[test]
fn budget_status_exhausted_spells_exhausted() {
    assert_eq!(BudgetStatus::Exhausted.as_str(), "exhausted");
    assert_eq!(BudgetStatus::Exhausted.to_string(), "exhausted");
}

// ── DerivedQuad construction and field access ─────────────────────────────────────────────

fn make_derived_quad() -> DerivedQuad {
    let world = "http://logic.gmeow.example/world/alpha".to_owned();
    DerivedQuad {
        graph: world.clone(),
        subject: TermValue::iri("http://example.org/subject/1"),
        predicate: "http://example.org/predicate/type".to_owned(),
        object: TermValue::iri("http://example.org/object/Thing"),
        graph_component: world.clone(),
        derivation_id: DerivationId("http://logic.gmeow.example/derivation/d001".to_owned()),
        rule_iri: "http://logic.gmeow.example/rule/r001".to_owned(),
        source_quad_ids: vec![
            "http://logic.gmeow.example/quad/q001".to_owned(),
            "http://logic.gmeow.example/quad/q002".to_owned(),
        ],
        profile: "http://logic.gmeow.example/profile/MonotonicDatalog".to_owned(),
        budget_status: BudgetStatus::Ok,
    }
}

#[test]
fn derived_quad_graph_field_accessible() {
    let dq = make_derived_quad();
    assert_eq!(dq.graph, "http://logic.gmeow.example/world/alpha");
}

#[test]
fn derived_quad_graph_equals_graph_component() {
    let dq = make_derived_quad();
    assert_eq!(
        dq.graph, dq.graph_component,
        "graph and graph_component must be equal"
    );
}

#[test]
fn derived_quad_derivation_id_round_trips() {
    let dq = make_derived_quad();
    assert_eq!(
        dq.derivation_id.as_str(),
        "http://logic.gmeow.example/derivation/d001"
    );
    assert_eq!(
        dq.derivation_id.to_string(),
        "http://logic.gmeow.example/derivation/d001"
    );
}

#[test]
fn derived_quad_rule_iri_round_trips() {
    let dq = make_derived_quad();
    assert_eq!(dq.rule_iri, "http://logic.gmeow.example/rule/r001");
}

#[test]
fn derived_quad_source_quad_ids_populated() {
    let dq = make_derived_quad();
    assert_eq!(dq.source_quad_ids.len(), 2);
    assert_eq!(
        dq.source_quad_ids[0],
        "http://logic.gmeow.example/quad/q001"
    );
    assert_eq!(
        dq.source_quad_ids[1],
        "http://logic.gmeow.example/quad/q002"
    );
}

#[test]
fn derived_quad_profile_round_trips() {
    let dq = make_derived_quad();
    assert_eq!(
        dq.profile,
        "http://logic.gmeow.example/profile/MonotonicDatalog"
    );
}

#[test]
fn derived_quad_budget_status_ok() {
    let dq = make_derived_quad();
    assert_eq!(dq.budget_status, BudgetStatus::Ok);
    assert_eq!(dq.budget_status.as_str(), "ok");
}

#[test]
fn derived_quad_clone_is_equal() {
    let dq = make_derived_quad();
    let cloned = dq.clone();
    assert_eq!(dq, cloned);
}

// ── DerivationId display ──────────────────────────────────────────────────────────────────

#[test]
fn derivation_id_display_matches_as_str() {
    let id = DerivationId("http://example.org/d/42".to_owned());
    assert_eq!(id.as_str(), id.to_string().as_str());
}

// ── WorldFactSnapshot ─────────────────────────────────────────────────────────────────────

const TEST_WORLD: &str = "http://world/TestForeign";
const TEST_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const S1: &str = "http://example.org/s1";
const P1: &str = "http://example.org/p1";
const O1: &str = "http://example.org/o1";
const S2: &str = "http://example.org/s2";
const P2: &str = "http://example.org/p2";
const O2: &str = "http://example.org/o2";

fn small_store() -> WorldStore {
    let store = WorldStore::new();
    store.insert_quad(TEST_WORLD, S1, P1, O1);
    store.insert_quad(TEST_WORLD, S2, P2, O2);
    store
}

fn small_foreign() -> WorldFactSnapshot {
    let store = small_store();
    WorldFactSnapshot::from_world(&store, TEST_WORLD, TEST_PROFILE)
        .expect("from_world on a valid store must succeed")
}

#[test]
fn foreign_in_world_all_none_returns_all_asserted_quads() {
    let foreign = small_foreign();
    let quads = foreign
        .in_world(TEST_WORLD, None, None, None)
        .expect("snapshot scan");
    assert_eq!(quads.len(), 2, "should return both asserted quads");
}

#[test]
fn foreign_in_world_predicate_filter() {
    let foreign = small_foreign();
    let quads = foreign
        .in_world(TEST_WORLD, None, Some(P1), None)
        .expect("snapshot scan");
    assert_eq!(quads.len(), 1, "P1 filter should return exactly 1 quad");
    assert_eq!(quads[0].predicate, P1);
}

#[test]
fn foreign_in_world_subject_filter() {
    let foreign = small_foreign();
    let subj_term = TermValue::iri(S2);
    let quads = foreign
        .in_world(TEST_WORLD, Some(&subj_term), None, None)
        .expect("snapshot scan");
    assert_eq!(quads.len(), 1, "S2 filter should return exactly 1 quad");
    assert_eq!(quads[0].subject, subj_term);
}

#[test]
fn foreign_in_world_wrong_world_returns_empty() {
    let foreign = small_foreign();
    let quads = foreign
        .in_world("http://world/Other", None, None, None)
        .expect("snapshot scan");
    assert!(quads.is_empty(), "wrong world must return no quads");
}

#[test]
fn foreign_derived_by_enumerates_with_assert_rule() {
    let foreign = small_foreign();
    let triples = foreign
        .derived_by(None, None, None)
        .expect("provenance scan");
    assert_eq!(triples.len(), 2, "should enumerate 2 asserted derivations");
    for (_, rule, _) in &triples {
        assert_eq!(
            rule,
            crate::provenance::ASSERT_RULE_IRI,
            "rule_iri must be ASSERT_RULE_IRI for asserted facts"
        );
    }
}

#[test]
fn foreign_derived_by_rule_filter() {
    let foreign = small_foreign();
    // Filter by ASSERT_RULE_IRI — should return both.
    let triples = foreign
        .derived_by(None, Some(crate::provenance::ASSERT_RULE_IRI), None)
        .expect("provenance scan");
    assert_eq!(triples.len(), 2);

    // Filter by a different rule IRI — should return none.
    let triples_none = foreign
        .derived_by(None, Some("http://example.org/someOtherRule"), None)
        .expect("provenance scan");
    assert!(triples_none.is_empty());
}

#[test]
fn foreign_derived_by_derivation_id_filter() {
    let foreign = small_foreign();
    // Get the derivation_id of the first quad.
    let first_id = foreign.quads[0].derivation_id.clone();
    let triples = foreign
        .derived_by(Some(&first_id), None, None)
        .expect("provenance scan");
    assert_eq!(
        triples.len(),
        1,
        "derivation_id filter must return exactly 1"
    );
    assert_eq!(triples[0].0, first_id);
}

#[test]
fn foreign_contradiction_witness_is_empty() {
    let foreign = small_foreign();
    let witnesses: Vec<_> = foreign.contradiction_witness(TEST_WORLD).collect();
    assert!(
        witnesses.is_empty(),
        "monotonic fragment: contradiction_witness must always be empty"
    );
}

#[test]
fn foreign_derivation_ids_are_well_formed_iris() {
    let foreign = small_foreign();
    for dq in &foreign.quads {
        let id = dq.derivation_id.as_str();
        assert!(
            id.starts_with("https://blackcatinformatics.ca/gmeow/derivation/"),
            "derivation_id must use derivation prefix: {id:?}"
        );
    }
}
