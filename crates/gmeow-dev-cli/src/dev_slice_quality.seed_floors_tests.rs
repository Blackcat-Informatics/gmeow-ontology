// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_slice_quality::model::{AxisGrade, GMEOW};
use std::collections::BTreeMap;

/// A throwaway bottom tier for the grade/roll-up fields the seeder never reads.
fn tier0() -> Tier {
    Tier {
        iri: format!("{GMEOW}tierRegistered"),
        label: "Registered".to_owned(),
        rank: 0,
    }
}

fn grade(axis_local: &str, score: f64) -> AxisGrade {
    AxisGrade {
        axis_iri: format!("{GMEOW}{axis_local}"),
        score,
        tier: tier0(),
    }
}

/// A slice assessment keyed by the on-disk slice IRI shape
/// (`…/gmeow/slices/<local>`), so the emitted subject/`floorSlice` match module.ttl.
fn assessment(slice_local: &str, grades: Vec<AxisGrade>) -> SliceAssessment {
    SliceAssessment {
        slice: format!("{GMEOW}slices/{slice_local}"),
        grades,
        rollup: tier0(),
    }
}

/// Extract the `gmeow:floorValue` decimal lexical from an emitted floor line.
fn floor_value_of(line: &str) -> &str {
    line.rsplit("gmeow:floorValue ")
        .next()
        .unwrap()
        .trim_end_matches(" .")
}

#[test]
fn emitted_floor_value_equals_live_measured_score() {
    // (a) The seeded floorValue is EXACTLY the live measured AxisGrade.score —
    // what the seeder emits is what the gate reads.
    let score = 0.571_428_571_428_571_4;
    let a = assessment("diagnostics", vec![grade("axisShapeMigration", score)]);
    let committed = BTreeMap::new();
    let lines =
        collect_seed_lines(&[&a], &committed, SeedSelector::One("axisShapeMigration")).unwrap();
    assert_eq!(lines.len(), 1, "one target → one line");
    let line = &lines[0];
    assert!(
        line.starts_with("gmeow:afc-diagnostics-axisShapeMigration a gmeow:AxisFloorCommitment"),
        "subject/type: {line}"
    );
    assert!(
        line.contains("gmeow:floorSlice <https://blackcatinformatics.ca/gmeow/slices/diagnostics>"),
        "full slice IRI: {line}"
    );
    assert!(
        line.contains("gmeow:floorAxis gmeow:axisShapeMigration"),
        "prefixed axis local: {line}"
    );
    let parsed: f64 = floor_value_of(line).parse().unwrap();
    assert_eq!(parsed, score, "emitted value == live measured score");
}

#[test]
fn seeded_value_round_trips_and_satisfies_the_gate() {
    // (b) parse(Display(score)) == score for every score shape, so the seeded
    // floor satisfies the gate's `measured + f64::EPSILON >= floor` at the same
    // live measurement (floor == parsed == score).
    for &score in &[
        0.0_f64,
        1.0,
        0.571_428_571_428_571_4,
        0.995_433_789_954_337_8,
        0.123_456_789,
        1e-9,
    ] {
        let rendered = format_floor_value(score);
        let parsed: f64 = rendered.parse().unwrap();
        assert_eq!(parsed, score, "round trip for {score} rendered {rendered}");
        assert!(
            score + f64::EPSILON >= parsed,
            "gate holds at the seeded floor for {score}"
        );
    }
    // Integer-valued floats gain a `.0` so they parse as xsd:decimal and match the
    // on-disk `1.0`/`0.0` convention (Display would print `1`/`0`).
    assert_eq!(format_floor_value(1.0), "1.0");
    assert_eq!(format_floor_value(0.0), "0.0");
    // A fractional value is rendered at full precision, untouched.
    assert_eq!(format_floor_value(0.5), "0.5");
}

#[test]
fn output_is_deterministically_ordered_by_slice_then_axis() {
    // (c) Assessments and grades fed OUT of order still emit sorted by
    // (slice IRI, axis local).
    let a1 = assessment("zebra", vec![grade("axisB", 0.2), grade("axisA", 0.3)]);
    let a2 = assessment("alpha", vec![grade("axisA", 0.4)]);
    let committed = BTreeMap::new();
    let lines = collect_seed_lines(&[&a1, &a2], &committed, SeedSelector::All).unwrap();
    let subjects: Vec<&str> = lines.iter().map(|l| l.split(' ').next().unwrap()).collect();
    assert_eq!(
        subjects,
        vec![
            "gmeow:afc-alpha-axisA",
            "gmeow:afc-zebra-axisA",
            "gmeow:afc-zebra-axisB",
        ]
    );
}

#[test]
fn already_floored_pair_at_or_above_is_not_re_emitted() {
    // A committed floor the live score still holds is never re-emitted (no
    // overwrite, no silent ratchet) — the seeder is emit-only for UNfloored pairs.
    let a = assessment("diagnostics", vec![grade("axisShapeMigration", 0.90)]);
    let mut committed = BTreeMap::new();
    committed.insert((a.slice.clone(), "axisShapeMigration".to_owned()), 0.80);
    let lines =
        collect_seed_lines(&[&a], &committed, SeedSelector::One("axisShapeMigration")).unwrap();
    assert!(
        lines.is_empty(),
        "already-floored pair is skipped: {lines:?}"
    );
}

#[test]
fn seeding_below_an_already_committed_floor_hard_fails() {
    // (d) A live score BELOW an already-committed floor is a regression the gate
    // reds — the seeder hard-fails and emits nothing, never lowering the floor.
    let a = assessment("diagnostics", vec![grade("axisShapeMigration", 0.40)]);
    let mut committed = BTreeMap::new();
    committed.insert((a.slice.clone(), "axisShapeMigration".to_owned()), 0.80);
    let err = collect_seed_lines(&[&a], &committed, SeedSelector::One("axisShapeMigration"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("regression"), "names the regression: {err}");
    assert!(err.contains("axisShapeMigration"), "names the axis: {err}");
    assert!(err.contains("0.8"), "names the committed floor: {err}");
}

#[test]
fn unknown_axis_name_hard_fails() {
    // (e) `--axis` naming no rubric axis is a hard fail (nonzero exit), never a
    // silent empty emission. The unknown-axis guard fires right after the rubric
    // load, before any slice is scored.
    assert_ne!(
        slice_quality_seed_floors(Some("axisDefinitelyNotAReal_Axis"), false),
        0
    );
}

#[test]
fn neither_selector_hard_fails() {
    // (f) Neither --axis nor --all-axes → hard fail, no silent default.
    assert_ne!(slice_quality_seed_floors(None, false), 0);
}

#[test]
fn both_selectors_hard_fail() {
    // (f) Both --axis and --all-axes → hard fail (mutually exclusive).
    assert_ne!(slice_quality_seed_floors(Some("axisGmn1Coverage"), true), 0);
}
