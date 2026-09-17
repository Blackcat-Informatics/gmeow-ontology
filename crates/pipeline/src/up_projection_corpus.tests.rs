// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn sssom_relation_buckets_match_python_contract() {
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
        "liftable-generalizing"
    );
    assert_eq!(
        classify_sssom("gmeow:X", "skos:narrowMatch", "schema:Y").bucket,
        "down-only-narrowing"
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
