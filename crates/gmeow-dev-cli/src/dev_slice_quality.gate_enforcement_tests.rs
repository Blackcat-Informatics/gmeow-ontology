// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_slice_quality::gate::evaluate_axis_floor;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// A fixture repo whose tree lives in an owned temp directory: dropping the fixture
/// drops the [`tempfile::TempDir`], which removes the tree — on success, on early
/// return, and on panic alike.
struct TempFixture {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

/// A minimal on-disk fixture repo (no git): a complete rubric slice plus a
/// non-rubric demo slice authoring one `gmeow:AxisFloorCommitment` at `floor`.
fn fixture_with_non_rubric_floor(floor: &str) -> TempFixture {
    let tmp = tempfile::Builder::new()
        .prefix("gmeow-gate-enforce-")
        .tempdir()
        .expect("create temp dir");
    let root = tmp.path().to_path_buf();
    let rubric_dir = root.join("slices/core/slice-quality-rubric");
    let demo_dir = root.join("slices/demo/demo");
    std::fs::create_dir_all(&rubric_dir).unwrap();
    std::fs::create_dir_all(&demo_dir).unwrap();
    std::fs::write(
        rubric_dir.join("module.ttl"),
        format!(
            r#"@prefix gmeow: <{NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:tierGrounded a gmeow:QualityTier ; gmeow:tierRank 1 .
gmeow:axisGmn1Coverage a gmeow:QualityAxis ;
    gmeow:axisProducer "gmn1_coverage_axis" ;
    gmeow:axisDimension gmeow:dimGmn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrGmn .
gmeow:thrGmn a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
"#
        ),
    )
    .unwrap();
    std::fs::write(rubric_dir.join("manifest.ttl"), "# rubric slice\n").unwrap();
    std::fs::write(
        demo_dir.join("module.ttl"),
        format!(
            r#"@prefix gmeow: <{NS}> .
gmeow:afc-demo a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceDemo ;
    gmeow:floorAxis gmeow:axisGmn1Coverage ;
    gmeow:floorValue {floor} .
"#
        ),
    )
    .unwrap();
    std::fs::write(demo_dir.join("manifest.ttl"), "# demo slice\n").unwrap();
    TempFixture { _tmp: tmp, root }
}

#[test]
fn a_non_rubric_slice_floor_is_enforced_by_the_gate_per_axis_decision() {
    // A floor committed at 1.0 in a NON-rubric slice's module.ttl.
    let fx = fixture_with_non_rubric_floor("1.0");
    let slice = format!("{NS}sliceDemo");

    // The gate reads floors through the same segregated loader; project them into the
    // (slice, axis-local) → floor map the per-axis floor pass consumes.
    let rubric = gmeow_slice_quality::load_repo_rubric(&fx.root).unwrap();
    let axis_floors = axis_floors_from_rubric(&rubric).unwrap();

    // The gate's per-axis floor RESOLUTION (`axis_floor_for`) now RESOLVES the
    // non-rubric floor — before the widening the loader never saw it, so this
    // returned None and the axis went silently unfloored.
    let resolved = axis_floor_for(&axis_floors, &slice, "axisGmn1Coverage", false)
        .unwrap()
        .expect("the non-rubric slice's committed floor is resolved by the gate");
    assert_eq!(resolved, 1.0);

    // The gate's per-axis VERDICT reds a measured score below the committed floor,
    // and passes one that meets it — exactly the decision the gate emits per grade.
    assert!(
        evaluate_axis_floor(0.5, resolved).is_failure(),
        "a measured score below the committed non-rubric floor must red the gate"
    );
    assert!(
        !evaluate_axis_floor(1.0, resolved).is_failure(),
        "a measured score meeting the floor must not red"
    );
}
