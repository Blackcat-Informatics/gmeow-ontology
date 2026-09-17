// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn c(local: &str, value: &str, is_literal: bool) -> Constraint {
    Constraint {
        local: local.to_owned(),
        value: if is_literal {
            AtomicTerm::Literal(purrdf::RdfLiteral::simple(value))
        } else {
            AtomicTerm::resource(value)
        },
        is_blank: false,
    }
}

#[test]
fn content_key_is_order_independent() {
    // The mint key sorts constraints, so declaration order cannot change the id.
    let a = content_key(
        "P",
        &[c("someValuesFrom", "C", false), c("hasValue", "V", false)],
    );
    let b = content_key(
        "P",
        &[c("hasValue", "V", false), c("someValuesFrom", "C", false)],
    );
    assert_eq!(a, b);
    assert_eq!(skolem_iri(&a), skolem_iri(&b));
}

#[test]
fn content_key_distinguishes_literal_from_iri_filler() {
    // An IRI filler and a literal filler with the same lexical form must NOT collide.
    let iri = content_key("P", &[c("hasValue", "Red", false)]);
    let lit = content_key("P", &[c("hasValue", "Red", true)]);
    assert_ne!(iri, lit);
    assert_ne!(skolem_iri(&iri), skolem_iri(&lit));
}

#[test]
fn content_key_excludes_subject_class() {
    // The key is a function of onProperty + constraints only — no subject — so two
    // classes bearing the same restriction share one skolem node.
    let k = content_key("P", &[c("someValuesFrom", "C", false)]);
    assert!(k.starts_with("onProperty=P"));
    assert!(!k.contains("subClassOf"));
}

fn f(facet_local: &str, value: &str) -> Facet {
    Facet {
        iri: format!("{XSD_NS}{facet_local}"),
        value: AtomicTerm::Literal(purrdf::RdfLiteral::simple(value)),
    }
}

#[test]
fn datarange_content_key_is_order_independent() {
    // The mint key sorts facets, so authored facet order cannot change the id.
    let a = datarange_content_key(
        "http://www.w3.org/2001/XMLSchema#decimal",
        &[f("minInclusive", "0.0"), f("maxInclusive", "1.0")],
    );
    let b = datarange_content_key(
        "http://www.w3.org/2001/XMLSchema#decimal",
        &[f("maxInclusive", "1.0"), f("minInclusive", "0.0")],
    );
    assert_eq!(a, b);
    assert_eq!(datarange_skolem_iri(&a), datarange_skolem_iri(&b));
}

#[test]
fn datarange_content_key_distinguishes_datatype_and_facets() {
    // A different base datatype, a different facet IRI, and a different facet value
    // must each mint a distinct node.
    let base = datarange_content_key(
        "http://www.w3.org/2001/XMLSchema#decimal",
        &[f("minInclusive", "0.0")],
    );
    let other_dt = datarange_content_key(
        "http://www.w3.org/2001/XMLSchema#integer",
        &[f("minInclusive", "0.0")],
    );
    let other_facet = datarange_content_key(
        "http://www.w3.org/2001/XMLSchema#decimal",
        &[f("minExclusive", "0.0")],
    );
    let other_value = datarange_content_key(
        "http://www.w3.org/2001/XMLSchema#decimal",
        &[f("minInclusive", "0.5")],
    );
    assert!(base.starts_with("onDatatype=http://www.w3.org/2001/XMLSchema#decimal"));
    assert_ne!(base, other_dt);
    assert_ne!(base, other_facet);
    assert_ne!(base, other_value);
    assert_ne!(datarange_skolem_iri(&base), datarange_skolem_iri(&other_dt));
}
