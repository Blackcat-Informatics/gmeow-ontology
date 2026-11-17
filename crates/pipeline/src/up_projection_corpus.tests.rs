// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Imported rows use SKOS object direction while preserving diagnostic relation buckets.
#[test]
fn sssom_relation_buckets_follow_skos_direction() {
    assert_eq!(
        classify_sssom("gmeow:Person", "skos:exactMatch", "foaf:Person").bucket,
        "clean-reversible"
    );
    assert_eq!(
        classify_sssom("foaf:Agent", "skos:exactMatch", "gmeow:Agent").bucket,
        "clean-reversible"
    );
    assert_eq!(
        classify_sssom("gmeow:noteContent", "skos:closeMatch", "schema:text").bucket,
        "liftable-with-claim"
    );
    assert_eq!(
        classify_sssom("gmeow:Appellation", "skos:broadMatch", "schema:name").bucket,
        "down-only-narrowing"
    );
    assert_eq!(
        classify_sssom("gmeow:X", "skos:narrowMatch", "schema:Y").bucket,
        "liftable-generalizing"
    );
    assert_eq!(
        classify_sssom(
            &format!("{GM}Person"),
            SKOS_EXACT_MATCH,
            "http://xmlns.com/foaf/0.1/Person"
        )
        .bucket,
        "clean-reversible"
    );
    assert_eq!(
        classify_sssom(
            "https://schema.org/text",
            SKOS_CLOSE_MATCH,
            &format!("{GM}noteContent")
        )
        .bucket,
        "liftable-with-claim"
    );
    for (predicate, gmeow_subject, gmeow_object) in [
        (
            "skos:broadMatch",
            "down-only-narrowing",
            "liftable-generalizing",
        ),
        (
            "skos:narrowMatch",
            "liftable-generalizing",
            "down-only-narrowing",
        ),
        (
            "rdfs:subClassOf",
            "down-only-narrowing",
            "liftable-generalizing",
        ),
        (
            "rdfs:subPropertyOf",
            "down-only-narrowing",
            "liftable-generalizing",
        ),
        (
            "skos:relatedMatch",
            "down-only-related",
            "down-only-related",
        ),
        ("custom:unknown", "other:unknown", "other:unknown"),
    ] {
        assert_eq!(
            classify_sssom("gmeow:X", predicate, "schema:Y").bucket,
            gmeow_subject,
            "{predicate}, GMEOW subject"
        );
        assert_eq!(
            classify_sssom("schema:Y", predicate, "gmeow:X").bucket,
            gmeow_object,
            "{predicate}, GMEOW object"
        );
    }
    assert_eq!(
        classify_sssom("gmeow:X", "skos:broadMatch", "schema:Y").bucket,
        classify_sssom("schema:Y", "skos:narrowMatch", "gmeow:X").bucket,
        "inverse encodings agree"
    );
    assert_eq!(
        classify_sssom(
            &format!("{GM}X"),
            &format!("{SKOS}narrowMatch"),
            "https://schema.org/Y"
        )
        .bucket,
        "liftable-generalizing"
    );
}

#[test]
fn combined_class_prefers_best_layer() {
    assert_eq!(
        combined_class(
            "x",
            &BTreeMap::from([("x".into(), "clean-reversible".into())]),
            &BTreeMap::new()
        ),
        "clean"
    );
    assert_eq!(
        combined_class(
            "x",
            &BTreeMap::new(),
            &BTreeMap::from([("x".into(), "structural-mint".into())])
        ),
        "hard-mint"
    );
}

#[test]
fn decimal_confidence_rejects_exponents_and_out_of_range_values() {
    assert_eq!(decimal_confidence("0.9"), Some(0.9));
    for bad in ["1e-1", "NaN", "Infinity", "-0.1", "1.5", "abc", ""] {
        assert!(decimal_confidence(bad).is_none(), "{bad}");
    }
}
