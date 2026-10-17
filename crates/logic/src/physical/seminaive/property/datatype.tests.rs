// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// A datatype enumeration cannot admit malformed decimals through rational equality.
#[test]
fn malformed_decimal_does_not_satisfy_a_rational_enumeration() {
    let mut rel = RelationStore::new();
    edge(&mut rel, "urn:range", ONE_OF, "urn:list");
    rel.insert(
        super::super::list::FIRST,
        &iri("urn:list"),
        &literal("1/2", "http://www.w3.org/2002/07/owl#rational"),
    );
    edge(
        &mut rel,
        "urn:list",
        super::super::list::REST,
        super::super::list::NIL,
    );
    let mut values = NativeValues::default();
    let plan = DatatypeCache::default()
        .prepare(
            &rel,
            &iri("urn:range"),
            &mut ListCache::default(),
            &mut values,
        )
        .unwrap();
    const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
    assert_eq!(plan.contains(&literal("1/2", DECIMAL), &mut values), None);
    assert_eq!(
        plan.contains(&literal("0.5", DECIMAL), &mut values),
        Some(true)
    );
    assert_eq!(
        plan.contains(&literal("0.25", DECIMAL), &mut values),
        Some(false)
    );
    for lexical in [
        "\u{a0}0.5\u{a0}",
        "\u{2003}0.5\u{2003}",
        "\u{202f}0.5\u{202f}",
    ] {
        assert_eq!(plan.contains(&literal(lexical, DECIMAL), &mut values), None);
    }
    for (lexical, datatype) in [
        (" 1/2 ", "http://www.w3.org/2002/07/owl#rational"),
        ("0.5", "http://www.w3.org/2002/07/owl#rational"),
        ("1/2", "http://www.w3.org/2002/07/owl#real"),
    ] {
        assert_eq!(
            plan.contains(&literal(lexical, datatype), &mut values),
            None
        );
    }
}

fn iri(value: &str) -> TermValue {
    TermValue::Iri(value.to_owned())
}

fn literal(value: &str, datatype: &str) -> TermValue {
    TermValue::Literal {
        lexical_form: value.to_owned(),
        datatype: datatype.to_owned(),
        language: None,
        direction: None,
    }
}

fn edge(rel: &mut RelationStore, subject: &str, predicate: &str, object: &str) {
    rel.insert(predicate, &iri(subject), &iri(object));
}

#[test]
fn shared_datatype_subexpressions_do_not_expand_and_completed_plans_survive_rounds() {
    let mut rel = RelationStore::new();
    let mut child = INTEGER.to_owned();
    for index in 0..96 {
        let root = format!("urn:node:{index}");
        let first = format!("urn:list:{index}:first");
        let second = format!("urn:list:{index}:second");
        edge(&mut rel, &root, INTERSECTION, &first);
        edge(&mut rel, &first, super::super::list::FIRST, &child);
        edge(&mut rel, &first, super::super::list::REST, &second);
        edge(&mut rel, &second, super::super::list::FIRST, &child);
        edge(
            &mut rel,
            &second,
            super::super::list::REST,
            super::super::list::NIL,
        );
        child = root;
    }
    let mut cache = DatatypeCache::default();
    let mut values = NativeValues::default();
    let plan = cache
        .prepare(&rel, &iri(&child), &mut ListCache::default(), &mut values)
        .unwrap();
    assert_eq!(plan.nodes.len(), 97);
    assert_eq!(
        plan.contains(&literal("2", INTEGER), &mut values),
        Some(true)
    );
    // Only value extensions grow after the completed definition boundary.
    rel.insert("urn:values", &iri("urn:subject"), &literal("3", INTEGER));
    let reused = cache
        .prepare(&rel, &iri(&child), &mut ListCache::default(), &mut values)
        .unwrap();
    assert!(Arc::ptr_eq(&plan, &reused));
    assert_eq!(
        reused.contains(&literal("3", INTEGER), &mut values),
        Some(true)
    );
}

#[test]
fn datatype_cache_capacity_and_payload_misses_preserve_execution() {
    let mut rel = RelationStore::new();
    for index in 0..65 {
        edge(&mut rel, &format!("urn:node:{index}"), COMPLEMENT, INTEGER);
    }
    let mut cache = DatatypeCache::default();
    let mut lists = ListCache::default();
    let mut values = NativeValues::default();
    for index in 0..65 {
        let plan = cache
            .prepare(
                &rel,
                &iri(&format!("urn:node:{index}")),
                &mut lists,
                &mut values,
            )
            .unwrap();
        assert_eq!(
            plan.contains(&literal("2", INTEGER), &mut values),
            Some(false)
        );
    }
    assert_eq!(cache.entries.len(), 64);
    assert!(cache.bytes <= 512 * 1024);

    let large = literal(
        &"x".repeat(512 * 1024),
        "http://www.w3.org/2001/XMLSchema#string",
    );
    let mut rel = RelationStore::new();
    edge(&mut rel, "urn:large", ONE_OF, "urn:list");
    rel.insert(super::super::list::FIRST, &iri("urn:list"), &large);
    edge(
        &mut rel,
        "urn:list",
        super::super::list::REST,
        super::super::list::NIL,
    );
    let mut cache = DatatypeCache::default();
    let plan = cache
        .prepare(
            &rel,
            &iri("urn:large"),
            &mut ListCache::default(),
            &mut values,
        )
        .unwrap();
    assert!(cache.entries.is_empty());
    assert_eq!(cache.bytes, 0);
    assert_eq!(plan.contains(&large, &mut values), Some(true));
}

#[test]
fn any_uri_is_an_intrinsic_primitive_datatype() {
    // `xsd:anyURI` is an OWL 2 datatype-map primitive. A data range naming it compiles
    // without a constructor, admits valid anyURI literals, excludes every interpreted
    // value of another primitive, and leaves an uninterpreted foreign datatype unknown.
    const ANY_URI: &str = "http://www.w3.org/2001/XMLSchema#anyURI";
    let mut rel = RelationStore::new();
    rel.insert(
        "urn:values",
        &iri("urn:subject"),
        &literal("urn:x", ANY_URI),
    );
    edge(&mut rel, "urn:p", "urn:range", ANY_URI);
    let mut values = NativeValues::default();
    let plan = DatatypeCache::default()
        .prepare(&rel, &iri(ANY_URI), &mut ListCache::default(), &mut values)
        .unwrap();
    assert_eq!(
        plan.contains(&literal("urn:x", ANY_URI), &mut values),
        Some(true)
    );
    assert_eq!(
        plan.contains(&literal("not a uri at all", ANY_URI), &mut values),
        Some(true)
    );
    assert_eq!(
        plan.contains(
            &literal("urn:x", "http://www.w3.org/2001/XMLSchema#string"),
            &mut values
        ),
        Some(false)
    );
    assert_eq!(
        plan.contains(&literal("2", INTEGER), &mut values),
        Some(false)
    );
    assert_eq!(
        plan.contains(&literal("x", "urn:custom"), &mut values),
        None
    );
    // Infinite, not unknown: any finite count of distinct anyURI values exists.
    assert_eq!(extent::named(ANY_URI).admits(1_000_000), Some(true));
}

#[test]
fn any_uri_enumeration_compares_collapsed_values() {
    const ANY_URI: &str = "http://www.w3.org/2001/XMLSchema#anyURI";
    let mut rel = RelationStore::new();
    edge(&mut rel, "urn:range", ONE_OF, "urn:list");
    rel.insert(
        super::super::list::FIRST,
        &iri("urn:list"),
        &literal("a b", ANY_URI),
    );
    edge(
        &mut rel,
        "urn:list",
        super::super::list::REST,
        super::super::list::NIL,
    );
    let mut values = NativeValues::default();
    let plan = DatatypeCache::default()
        .prepare(
            &rel,
            &iri("urn:range"),
            &mut ListCache::default(),
            &mut values,
        )
        .unwrap();
    for value in ["a b", "a\tb", "\r\n a \t b \r"] {
        assert_eq!(
            plan.contains(&literal(value, ANY_URI), &mut values),
            Some(true)
        );
    }
    for value in ["a%20b", "a\u{a0}b"] {
        assert_eq!(
            plan.contains(&literal(value, ANY_URI), &mut values),
            Some(false)
        );
    }
    assert_eq!(
        plan.contains(&literal("a b", "urn:foreign"), &mut values),
        None
    );
}

#[test]
fn any_uri_length_facets_measure_collapsed_characters() {
    const ANY_URI: &str = "http://www.w3.org/2001/XMLSchema#anyURI";
    for (facet, kind) in [("length", 4), ("minLength", 5), ("maxLength", 6)] {
        let mut rel = RelationStore::new();
        edge(&mut rel, "urn:range", ON_DATATYPE, ANY_URI);
        edge(&mut rel, "urn:range", RESTRICTIONS, "urn:facets");
        edge(
            &mut rel,
            "urn:facets",
            super::super::list::FIRST,
            "urn:facet",
        );
        edge(
            &mut rel,
            "urn:facets",
            super::super::list::REST,
            super::super::list::NIL,
        );
        rel.insert(
            &format!("http://www.w3.org/2001/XMLSchema#{facet}"),
            &iri("urn:facet"),
            &literal("2", INTEGER),
        );
        let mut values = NativeValues::default();
        let plan = DatatypeCache::default()
            .prepare(
                &rel,
                &iri("urn:range"),
                &mut ListCache::default(),
                &mut values,
            )
            .unwrap();
        for (lexical, length) in [
            ("", 0),
            ("a", 1),
            ("é𐐀", 2),
            ("\t é𐐀 \r\n", 2),
            ("é\t\r\n𐐀", 3),
            ("a\u{a0}", 2),
            ("a%20b", 5),
        ] {
            let expected = match kind {
                4 => length == 2,
                5 => length >= 2,
                6 => length <= 2,
                _ => unreachable!(),
            };
            assert_eq!(
                plan.contains(&literal(lexical, ANY_URI), &mut values),
                Some(expected),
                "{facet} for {lexical:?}",
            );
        }
        assert_eq!(
            plan.contains(&literal("é𐐀", "urn:foreign"), &mut values),
            None
        );
    }
}
