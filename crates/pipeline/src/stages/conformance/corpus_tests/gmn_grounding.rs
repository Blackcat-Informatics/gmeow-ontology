// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected source judgments from the authenticated optimized producer. No codec,
//! dictionary compiler, source parser, or corpus construction runs in these tests.

use std::sync::OnceLock;

use gmeow_lang_bridge::{ConstructCoverageTally, Gmn1ConstructCategory, Gmn1Error};

use super::super::gmn_grounding::{CHANNEL, Observations};

pub(super) fn observations() -> &'static Observations {
    static OBSERVATIONS: OnceLock<
        Result<super::source_artifact::Selected<Observations>, gmeow_errors::Diag>,
    > = OnceLock::new();
    super::source_artifact::get(&OBSERVATIONS, CHANNEL)
}

fn module_roundtrips(slice: &str, minimum: usize) {
    let path = format!("slices/grounding/{slice}/module.ttl");
    let source = observations()
        .sources
        .get(&path)
        .expect("required grounding module");
    assert!(
        source.quads > minimum,
        "{path}: nontrivial authored content required"
    );
    assert!(source.roundtrip.is_ok(), "{path}: {:?}", source.roundtrip);
}

fn examples_roundtrip(slice: &str) {
    let prefix = format!("slices/grounding/{slice}/examples/");
    let mut checked = 0;
    for (path, source) in &observations().sources {
        if path.starts_with(&prefix) {
            assert!(source.roundtrip.is_ok(), "{path}: {:?}", source.roundtrip);
            checked += 1;
        }
    }
    assert!(checked > 0, "required grounding examples for {slice}");
}

#[test]
fn real_lang_module_round_trips() {
    module_roundtrips("lang", 1000);
}

#[test]
fn real_logic_module_round_trips() {
    module_roundtrips("logic", 1000);
}

#[test]
fn real_math_module_round_trips() {
    module_roundtrips("math", 500);
}

#[test]
fn real_lang_examples_round_trip() {
    examples_roundtrip("lang");
}

#[test]
fn real_logic_examples_round_trip() {
    examples_roundtrip("logic");
}

#[test]
fn real_math_examples_round_trip() {
    examples_roundtrip("math");
}

#[test]
fn gate_is_clean_over_the_real_grounding_slices() {
    let report = observations().reports().roundtrip;
    assert!(
        report.is_clean(),
        "GMN roundtrip failures: {:#?}",
        report.failures
    );
}

#[test]
fn construct_coverage_is_complete_over_the_real_grounding_slices() {
    let report = observations().reports().coverage;
    assert!(
        report.is_complete(),
        "unexercised GMN categories: {:?}",
        report.unexercised
    );
    assert_eq!(report.uncovered_quad_count, 0);
}

#[test]
fn construct_coverage_agrees_with_the_roundtrip_gate_on_uncovered_count() {
    let reports = observations().reports();
    let roundtrip_uncovered = reports
        .roundtrip
        .failures
        .iter()
        .filter(|failure| failure.failure_class() == Gmn1Error::CLASS_UNCOVERED_TERM)
        .count();
    assert_eq!(roundtrip_uncovered, 0);
    assert_eq!(reports.coverage.uncovered_quad_count, 0);
}

#[test]
fn construct_coverage_audit_is_falsifiable_when_a_real_category_is_removed() {
    let mut full = ConstructCoverageTally::default();
    let mut filtered = ConstructCoverageTally::default();
    let mut retained_quads = 0;
    for source in observations().sources.values() {
        full.merge(&source.coverage);
        filtered.merge(&source.without_decimal);
        retained_quads += source.retained_quads;
    }
    assert!(
        full.count(Gmn1ConstructCategory::LiteralDecimal) > 0,
        "negative control requires a real decimal occurrence"
    );
    assert!(
        retained_quads > 0,
        "filtering must retain non-decimal content"
    );
    assert_eq!(filtered.count(Gmn1ConstructCategory::LiteralDecimal), 0);
    assert!(
        filtered
            .unexercised_categories()
            .contains(&Gmn1ConstructCategory::LiteralDecimal)
    );
}
