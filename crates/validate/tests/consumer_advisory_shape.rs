// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Verifies the declarative SHACL twin of the native consumer-advisory range
//! gate (`slices/core/kernel/shapes.ttl`): every `gmeow:useForConsumer` /
//! `gmeow:avoidForConsumer` value must be a declared `gmeow:ProjectionContext`
//! individual. These exercise the shape over the merged ontology corpus the same
//! way `make validate` does (disk-sourced slice `shapes.ttl`), so no regenerate
//! is required to prove the shape both fires on a bad value and does not
//! over-flag the ontology's own (valid) advisory annotations.

#![cfg(not(target_arch = "wasm32"))]

mod conformance_support;
use conformance_support::*;

const USE_MSG: &str = "gmeow:useForConsumer must point at a declared gmeow:ProjectionContext";
const AVOID_MSG: &str = "gmeow:avoidForConsumer must point at a declared gmeow:ProjectionContext";

#[test]
fn use_for_consumer_pointing_at_non_projection_context_is_flagged() {
    let bad = "<https://example.org/badterm> \
        <https://blackcatinformatics.ca/gmeow/useForConsumer> \
        <https://example.org/notacontext> .\n";
    let report = validate_with_ontology(bad);
    assert!(
        violations(&report).iter().any(|v| v.contains(USE_MSG)),
        "shape must flag a non-ProjectionContext useForConsumer value: {:?}",
        violations(&report)
    );
}

#[test]
fn avoid_for_consumer_pointing_at_non_projection_context_is_flagged() {
    let bad = "<https://example.org/badterm> \
        <https://blackcatinformatics.ca/gmeow/avoidForConsumer> \
        <https://example.org/notacontext> .\n";
    let report = validate_with_ontology(bad);
    assert!(
        violations(&report).iter().any(|v| v.contains(AVOID_MSG)),
        "shape must flag a non-ProjectionContext avoidForConsumer value: {:?}",
        violations(&report)
    );
}

#[test]
fn ontology_advisory_annotations_do_not_over_flag() {
    // The merged ontology's own useForConsumer / avoidForConsumer assertions all
    // point at declared ProjectionContext individuals, so the new shapes must add
    // no violation of their own over the real corpus (no over-flagging).
    let report = validate_with_ontology("");
    let v = violations(&report);
    assert!(
        !v.iter()
            .any(|m| m.contains(USE_MSG) || m.contains(AVOID_MSG)),
        "consumer-advisory shapes must not over-flag the real ontology: {:?}",
        v.iter()
            .filter(|m| m.contains(USE_MSG) || m.contains(AVOID_MSG))
            .collect::<Vec<_>>()
    );
}
