// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ns::math;

/// Language identity survives emission and invalid IRIs are refused at native freeze.
#[test]
fn explicit_language_literals_keep_their_identity_and_invalid_iris_fail() {
    let mut sink = Sink::new();
    sink.lit(
        "urn:subject",
        "urn:label",
        RdfLiteral::language_tagged("émotion", "x-gmeow-english"),
    )
    .unwrap();
    sink.string("urn:subject", "urn:label", "émotion").unwrap();
    let turtle = sink.serialize().unwrap();
    let dataset = purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None).unwrap();
    assert_eq!(dataset.quad_count(), 2);
    assert!(turtle.contains("@x-gmeow-english"));
    let mut invalid = Sink::new();
    invalid.string("not an IRI", "urn:label", "x").unwrap();
    assert!(invalid.serialize().is_err());
}

#[test]
fn emission_order_does_not_change_the_bytes() {
    let a = {
        let mut sink = Sink::new();
        sink.typed("http://example.org/x", &math("FittedModel"))
            .unwrap();
        sink.integer("http://example.org/x", &math("slotIndex"), 0)
            .unwrap();
        sink.serialize().unwrap()
    };
    let b = {
        let mut sink = Sink::new();
        sink.integer("http://example.org/x", &math("slotIndex"), 0)
            .unwrap();
        sink.typed("http://example.org/x", &math("FittedModel"))
            .unwrap();
        sink.serialize().unwrap()
    };
    assert_eq!(a, b, "the codec canonicalizes; emission order is free");
}

#[test]
fn a_decimal_never_serializes_in_exponent_form() {
    assert_eq!(f64_decimal_lexical(2.0), "2.0");
    assert_eq!(f64_decimal_lexical(0.25), "0.25");
    assert_eq!(f64_decimal_lexical(-3.5), "-3.5");
}

#[test]
fn duplicate_triples_collapse() {
    let mut sink = Sink::new();
    sink.typed("http://example.org/x", &math("Proof")).unwrap();
    sink.typed("http://example.org/x", &math("Proof")).unwrap();
    let ttl = sink.serialize().unwrap();
    assert_eq!(ttl.matches("Proof").count(), 1, "C0.5 duplicate collapse");
}
