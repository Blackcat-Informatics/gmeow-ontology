// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Verifies the declarative SHACL twin of the native consumer-advisory range
//! gate (`slices/core/kernel/shapes.ttl`): every `gmeow:useForConsumer` /
//! `gmeow:avoidForConsumer` value must be a declared `gmeow:ProjectionContext`
//! individual. These exercise the two kernel shapes from their canonical file
//! over the merged ontology corpus. Selecting the shape family under test avoids
//! executing every unrelated repository shape while preserving the production
//! data graph and both positive and negative assertions.

#![cfg(not(target_arch = "wasm32"))]

mod conformance_support;
use conformance_support::*;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};
use purrdf::shapes::report::ValidationReport;
use purrdf::shapes::shapes::Shapes;
use std::sync::OnceLock;

const USE_MSG: &str = "gmeow:useForConsumer must point at a declared gmeow:ProjectionContext";
const AVOID_MSG: &str = "gmeow:avoidForConsumer must point at a declared gmeow:ProjectionContext";

fn consumer_advisory_shapes() -> &'static Shapes {
    static CACHE: OnceLock<Shapes> = OnceLock::new();
    CACHE.get_or_init(|| {
        let path = repo_root().join("slices/core/kernel/shapes.ttl");
        parse_shapes(&read_ttl(&path), None).expect("consumer-advisory SHACL shapes must parse")
    })
}

fn validate_consumer_advisory(fixture_nt: &str) -> ValidationReport {
    let dataset = ontology_with_fixture_dataset(fixture_nt);
    validate_dataset(&dataset, consumer_advisory_shapes())
        .expect("consumer-advisory SHACL validation must succeed")
}

#[test]
fn use_for_consumer_pointing_at_non_projection_context_is_flagged() {
    let bad = "<https://example.org/badterm> \
        <https://blackcatinformatics.ca/gmeow/useForConsumer> \
        <https://example.org/notacontext> .\n";
    let report = validate_consumer_advisory(bad);
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
    let report = validate_consumer_advisory(bad);
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
    let report = validate_consumer_advisory("");
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
