// SPDX-License-Identifier: AGPL-3.0-only
//! First-class native coverage of the SPARQL features the migration relies on.
//!
//! Each `#[test]` here drives one [`QueryCase`] over a tiny inline fixture,
//! exercising exactly one [`Feature`]. Together they seed
//! [`MIGRATION_FEATURE_REGISTRY`] so the tag-union covers [`Feature::ALL`] from the
//! first commit (see [`feature_registry_covers_all_features`]); later cluster tasks
//! register their migrated cases and the union only grows.
//!
//! The COALESCE case is the honest first-class coverage the issue's feature list
//! requires: no source `.rq` uses `COALESCE`, so it is exercised directly here.

use crate::conformance_support::*;

/// A minimal numeric fixture shared by the arithmetic/UNION/OPTIONAL cases.
const NUM_TTL: &str = "\
@prefix ex: <https://example.org/> .
ex:a ex:p 1 .
ex:a ex:q 2 .
ex:b ex:p 3 .
";

const EX: &str = "https://example.org/";

fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

#[gmeow_test_batch_macros::batch_test]
fn union_merges_two_patterns() {
    QueryCase::new("sparql-features/union", &[Feature::Union])
        .over_raw_ttl(NUM_TTL)
        .query(
            "PREFIX ex: <https://example.org/>\n\
             SELECT ?v WHERE { { ex:a ex:p ?v } UNION { ex:a ex:q ?v } }",
        )
        .select_row_set(vec![vec![int_lit(1)], vec![int_lit(2)]])
        .run();
}

#[gmeow_test_batch_macros::batch_test]
fn optional_keeps_rows_with_unbound_variable() {
    // ex:a has ex:q (bound), ex:b does not (unbound) — OPTIONAL must keep BOTH.
    QueryCase::new("sparql-features/optional", &[Feature::Optional])
        .over_raw_ttl(NUM_TTL)
        .query(
            "PREFIX ex: <https://example.org/>\n\
             SELECT ?s ?q WHERE { ?s ex:p ?v OPTIONAL { ?s ex:q ?q } }",
        )
        .select_count_at_least(2)
        .column_superset("s", vec![iri(&ex("a")), iri(&ex("b"))])
        .run();
}

#[gmeow_test_batch_macros::batch_test]
fn filter_not_exists_excludes_matching_subjects() {
    // Only ex:b has ex:p WITHOUT any ex:q.
    QueryCase::new(
        "sparql-features/filter-not-exists",
        &[Feature::FilterNotExists],
    )
    .over_raw_ttl(NUM_TTL)
    .query(
        "PREFIX ex: <https://example.org/>\n\
         SELECT ?s WHERE { ?s ex:p ?v FILTER NOT EXISTS { ?s ex:q ?any } }",
    )
    .select_row_set(vec![vec![iri(&ex("b"))]])
    .run();
}

#[gmeow_test_batch_macros::batch_test]
fn bind_computes_a_derived_value() {
    QueryCase::new("sparql-features/bind", &[Feature::Bind])
        .over_raw_ttl(NUM_TTL)
        .query(
            "PREFIX ex: <https://example.org/>\n\
             SELECT ?two WHERE { ex:a ex:p ?v BIND(?v + 1 AS ?two) }",
        )
        .select_row_set(vec![vec![int_lit(2)]])
        .run();
}

/// COALESCE fallback semantics — no source `.rq` uses COALESCE, so this is the
/// issue-mandated first-class coverage.
#[gmeow_test_batch_macros::batch_test]
fn coalesce_falls_back_to_the_next_bound_argument() {
    // row1 has a primary (COALESCE picks it); row2 has only a fallback (COALESCE
    // must skip the unbound primary and pick the fallback).
    let ttl = "\
@prefix ex: <https://example.org/> .
ex:row1 ex:primary \"P1\" ; ex:fallback \"F1\" .
ex:row2 ex:fallback \"F2\" .
";
    QueryCase::new("sparql-features/coalesce", &[Feature::Coalesce])
        .over_raw_ttl(ttl)
        .query(
            "PREFIX ex: <https://example.org/>\n\
             SELECT ?s ?v WHERE {\n\
               ?s ex:fallback ?f .\n\
               OPTIONAL { ?s ex:primary ?p }\n\
               BIND(COALESCE(?p, ?f) AS ?v)\n\
             }",
        )
        .select_row_set(vec![
            vec![iri(&ex("row1")), lit("P1")],
            vec![iri(&ex("row2")), lit("F2")],
        ])
        .run();
}

#[gmeow_test_batch_macros::batch_test]
fn construct_projects_a_graph() {
    QueryCase::new(
        "sparql-features/construct-graph",
        &[Feature::ConstructGraph],
    )
    .over_raw_ttl(NUM_TTL)
    .query(
        "PREFIX ex: <https://example.org/>\n\
         CONSTRUCT { ?s ex:derived ?v } WHERE { ?s ex:p ?v }",
    )
    .construct_has(vec![
        (iri(&ex("a")), iri(&ex("derived")), int_lit(1)),
        (iri(&ex("b")), iri(&ex("derived")), int_lit(3)),
    ])
    .construct_len(2)
    .run();
}

#[gmeow_test_batch_macros::batch_test]
fn init_bindings_prebinds_a_variable() {
    // Pre-binding ?s = ex:a restricts the otherwise-open pattern to ex:a's ex:p.
    QueryCase::new("sparql-features/init-bindings", &[Feature::InitBindings])
        .over_raw_ttl(NUM_TTL)
        .query("SELECT ?v WHERE { ?s <https://example.org/p> ?v }")
        .bind("s", iri(&ex("a")))
        .select_row_set(vec![vec![int_lit(1)]])
        .run();
}

/// The migration coverage invariant: the registry's feature-tag union must cover
/// EVERY [`Feature`]. Seeded green by the cases above; later tasks only extend it.
#[gmeow_test_batch_macros::batch_test]
fn feature_registry_covers_all_features() {
    // The registry itself must be well-formed: non-empty id + at least one tag.
    for (cq_id, tags) in MIGRATION_FEATURE_REGISTRY {
        assert!(!cq_id.is_empty(), "registry row has an empty cq_id");
        assert!(
            !tags.is_empty(),
            "registry row {cq_id:?} has no feature tags"
        );
    }
    let union = registry_feature_union();
    for feature in Feature::ALL {
        assert!(
            union.contains(feature),
            "MIGRATION_FEATURE_REGISTRY tag-union does not cover {feature:?}; \
             union = {union:?}"
        );
    }
}
