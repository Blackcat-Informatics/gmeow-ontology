// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ns::math;

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
    assert_eq!(format_decimal(2.0), "2.0");
    assert_eq!(format_decimal(0.25), "0.25");
    assert_eq!(format_decimal(-3.5), "-3.5");
}

#[test]
fn duplicate_triples_collapse() {
    let mut sink = Sink::new();
    sink.typed("http://example.org/x", &math("Proof")).unwrap();
    sink.typed("http://example.org/x", &math("Proof")).unwrap();
    let ttl = sink.serialize().unwrap();
    assert_eq!(ttl.matches("Proof").count(), 1, "C0.5 duplicate collapse");
}
