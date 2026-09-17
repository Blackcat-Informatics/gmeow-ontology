// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_slice_quality::gate::{
    AxisRatchetVerdict, axis_floor_monotonicity, evaluate_axis_floor, tier_floor_monotonicity,
};

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// A structurally-complete minimal rubric TTL (one two-rung ladder, one axis with
/// a threshold) with `body` appended — the same scaffolding the rubric loader
/// needs, used to exercise the floor projections through `load_rubric_from_ttl`
/// exactly as the base-`module.ttl` monotonicity path does.
fn mini_rubric(body: &str) -> String {
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
{body}
"#
    )
}

#[test]
fn axis_floors_project_from_commitments() {
    // The (slice, axis-local) → floor projection carries the committed value.
    let rubric = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:afc a gmeow:AxisFloorCommitment ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis gmeow:axisGmn1Coverage ; gmeow:floorValue 0.9954337899543378 ."#,
            ),
            "test",
        )
        .unwrap();
    let map = axis_floors_from_rubric(&rubric).unwrap();
    assert_eq!(
        map.get(&(format!("{NS}sliceX"), "axisGmn1Coverage".to_owned()))
            .copied(),
        Some(0.9954337899543378)
    );
}

#[test]
fn axis_floors_from_rubric_hard_fails_on_global_axis_local_name_collision() {
    // `axis_floors_from_rubric` first enforces that every rubric axis's local name
    // is GLOBALLY UNIQUE across `rubric.axes` — the floor gate keys every lookup
    // by local name, so two DISTINCT axis IRIs sharing a local name would let a
    // commitment against one axis silently apply to the other's grade. Positive
    // control first: two DISTINCT local names for the same slice map to two
    // entries with no collision at all.
    let clean = load_rubric_from_ttl(
            &mini_rubric(
                r#"<https://a.example/ns#axisFoo> a gmeow:QualityAxis ;
    gmeow:axisProducer "syn_foo_a" ; gmeow:axisDimension gmeow:dimSyn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thrSynFooA .
gmeow:thrSynFooA a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
<https://a.example/ns#axisBar> a gmeow:QualityAxis ;
    gmeow:axisProducer "syn_bar_a" ; gmeow:axisDimension gmeow:dimSyn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thrSynBarA .
gmeow:thrSynBarA a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
gmeow:afc1 a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis <https://a.example/ns#axisFoo> ;
    gmeow:floorValue 0.9 .
gmeow:afc2 a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis <https://a.example/ns#axisBar> ;
    gmeow:floorValue 0.5 ."#,
            ),
            "test",
        )
        .unwrap();
    let clean_map = axis_floors_from_rubric(&clean).unwrap();
    assert_eq!(
        clean_map
            .get(&(format!("{NS}sliceX"), "axisFoo".to_owned()))
            .copied(),
        Some(0.9)
    );
    assert_eq!(
        clean_map
            .get(&(format!("{NS}sliceX"), "axisBar".to_owned()))
            .copied(),
        Some(0.5)
    );

    // Now the collision: two DISTINCT rubric AXES (not merely commitments) share
    // the SAME local name `axisFoo` across two different namespaces. This must
    // hard-fail at the axis level, independent of any commitment against either
    // axis, because the floor gate would otherwise key lookups on the shared
    // local name and could apply a commitment to the wrong axis's grade.
    let colliding = load_rubric_from_ttl(
            &mini_rubric(
                r#"<https://a.example/ns#axisFoo> a gmeow:QualityAxis ;
    gmeow:axisProducer "syn_foo_a" ; gmeow:axisDimension gmeow:dimSyn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thrSynFooA .
gmeow:thrSynFooA a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
<https://b.example/other#axisFoo> a gmeow:QualityAxis ;
    gmeow:axisProducer "syn_foo_b" ; gmeow:axisDimension gmeow:dimSyn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thrSynFooB .
gmeow:thrSynFooB a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
gmeow:afc1 a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis <https://a.example/ns#axisFoo> ;
    gmeow:floorValue 0.9 .
gmeow:afc2 a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis <https://b.example/other#axisFoo> ;
    gmeow:floorValue 0.5 ."#,
            ),
            "test",
        )
        .unwrap();
    let err = axis_floors_from_rubric(&colliding).unwrap_err();
    assert!(
        err.message().contains("https://a.example/ns#axisFoo")
            && err.message().contains("https://b.example/other#axisFoo"),
        "names both colliding full axis IRIs: {err}"
    );
    assert!(
        err.message().contains("axisFoo"),
        "names the shared local name: {err}"
    );
}

#[test]
fn tier_floors_project_and_resolve_rank() {
    let rubric = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:stf a gmeow:SliceTierFloor ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorTier gmeow:tierGrounded ."#,
            ),
            "test",
        )
        .unwrap();
    let map = tier_floors_from_rubric(&rubric).unwrap();
    let f = map.get(&format!("{NS}sliceX")).unwrap();
    assert_eq!(f.rank, 1, "tierGrounded resolves to rank 1");
    assert_eq!(f.local, "tierGrounded");
}

#[test]
fn tier_floor_naming_unknown_tier_hard_fails() {
    // A gmeow:floorTier that resolves to no ladder rung is a hard fail — the gate
    // never silently drops a floor it cannot rank. The rubric LOADER
    // already rejects an unknown gmeow:floorTier at load time, so this case can no
    // longer be reached through `load_rubric_from_ttl`. The `tier_floors_from_rubric`
    // guard is now defense-in-depth behind that load-time validation, so this test
    // builds a `Rubric` struct literal directly (bypassing the loader) to still
    // exercise the guard itself.
    use gmeow_slice_quality::model::{GovernanceFloors, SliceTierFloorCommitment};

    let rubric = Rubric {
        standard: MeasurementStandard {
            tiers: vec![Tier {
                iri: format!("{NS}tierRegistered"),
                label: "Registered".to_owned(),
                rank: 0,
            }],
            axes: Vec::new(),
        },
        floors: GovernanceFloors {
            exemptions: Vec::new(),
            commitments: Vec::new(),
            tier_floors: vec![SliceTierFloorCommitment {
                slice: format!("{NS}sliceX"),
                tier: format!("{NS}tierBogus"),
            }],
            ..Default::default()
        },
    };
    let err = tier_floors_from_rubric(&rubric).unwrap_err();
    assert!(err.message().contains("tierBogus"), "names the tier: {err}");
    assert!(
        err.message().contains("resolves to no gmeow:QualityTier"),
        "{err}"
    );
}

#[test]
fn non_gmn1_axis_floor_is_enforced() {
    // (a) A committed floor on an axis OTHER than axisGmn1Coverage is resolved
    // and enforced: an explicit floor is found regardless of grounding, and a
    // measured score below it fails.
    let mut map = std::collections::BTreeMap::new();
    map.insert(
        ("ex:slice".to_owned(), "axisProseQuality".to_owned()),
        0.80_f64,
    );
    assert_eq!(
        axis_floor_for(&map, "ex:slice", "axisProseQuality", false).unwrap(),
        Some(0.80),
        "an explicit non-GMN1 floor is resolved even off a grounding slice"
    );
    assert_eq!(
        evaluate_axis_floor(0.50, 0.80),
        AxisRatchetVerdict::MeasuredBelowFloor,
        "measured below the non-GMN1 floor fails"
    );
}

#[test]
fn gmn1_grounding_default_holds() {
    // (b) With no explicit commitment, axisGmn1Coverage on a grounding slice is
    // floored at 1.0; on a non-grounding slice it is unfloored; and the 1.0
    // default is applied to NO other axis, even on a grounding slice.
    let empty = std::collections::BTreeMap::new();
    assert_eq!(
        axis_floor_for(&empty, "ex:slice", AXIS_GMN1_COVERAGE, true).unwrap(),
        Some(1.0),
        "grounding GMN1 defaults to 1.0"
    );
    assert_eq!(
        axis_floor_for(&empty, "ex:slice", AXIS_GMN1_COVERAGE, false).unwrap(),
        None,
        "non-grounding GMN1 with no commitment is unfloored"
    );
    assert_eq!(
        axis_floor_for(&empty, "ex:slice", "axisProseQuality", true).unwrap(),
        None,
        "the 1.0 default is GMN1-only, never any other axis"
    );
}

#[test]
fn grounding_gmn1_sub_one_floor_hard_fails() {
    // A grounding slice's axisGmn1Coverage floor is definitionally 1.0; an
    // explicit commitment BELOW 1.0 contradicts that definition and must
    // hard-fail (.goals no-optionality), never be silently clamped up.
    let mut map = std::collections::BTreeMap::new();
    map.insert(
        ("ex:slice".to_owned(), AXIS_GMN1_COVERAGE.to_owned()),
        0.9_f64,
    );
    let err = axis_floor_for(&map, "ex:slice", AXIS_GMN1_COVERAGE, true).unwrap_err();
    assert!(
        err.message().contains("grounding") && err.message().contains("1.0"),
        "names the grounding contradiction and the definitional 1.0: {err}"
    );

    // Positive control: an explicit commitment that RESTATES 1.0 is accepted,
    // not treated as a contradiction.
    let mut map_one = std::collections::BTreeMap::new();
    map_one.insert(
        ("ex:slice".to_owned(), AXIS_GMN1_COVERAGE.to_owned()),
        1.0_f64,
    );
    assert_eq!(
        axis_floor_for(&map_one, "ex:slice", AXIS_GMN1_COVERAGE, true).unwrap(),
        Some(1.0),
        "an explicit 1.0 grounding commitment restates, not undercuts, the default"
    );

    // The guard is grounding-only: the SAME sub-1.0 commitment on a
    // non-grounding slice loads fine — no hard fail.
    assert_eq!(
        axis_floor_for(&map, "ex:slice", AXIS_GMN1_COVERAGE, false).unwrap(),
        Some(0.9),
        "a sub-1.0 GMN1 floor on a non-grounding slice is not a contradiction"
    );
}

#[test]
fn multiple_axes_are_floored_independently_on_one_slice() {
    // (c) Two committed axis floors on the SAME slice are each resolved and
    // evaluated independently — one can fail while the other passes.
    let s = "ex:slice".to_owned();
    let mut map = std::collections::BTreeMap::new();
    map.insert((s.clone(), "axisProseQuality".to_owned()), 0.80_f64);
    map.insert((s.clone(), "axisLinkageCalculus".to_owned()), 0.60_f64);
    assert_eq!(
        axis_floor_for(&map, &s, "axisProseQuality", false).unwrap(),
        Some(0.80)
    );
    assert_eq!(
        axis_floor_for(&map, &s, "axisLinkageCalculus", false).unwrap(),
        Some(0.60)
    );
    // Independent verdicts: prose below its floor fails, linkage above its passes.
    assert!(evaluate_axis_floor(0.70, 0.80).is_failure());
    assert!(!evaluate_axis_floor(0.70, 0.60).is_failure());
}

#[test]
fn axis_floor_monotonicity_reds_on_lowered_commitment_vs_base_ttl() {
    // (d) A working-tree module.ttl that LOWERS a committed per-axis floor below
    // its base-TTL value is a hard violation (floors are raise-only) — parsed and
    // projected through the SAME loader path the gate uses.
    let base = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:afc a gmeow:AxisFloorCommitment ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis gmeow:axisGmn1Coverage ; gmeow:floorValue 0.98 ."#,
            ),
            "base",
        )
        .unwrap();
    let work = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:afc a gmeow:AxisFloorCommitment ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis gmeow:axisGmn1Coverage ; gmeow:floorValue 0.90 ."#,
            ),
            "work",
        )
        .unwrap();
    let base_map = axis_floors_from_rubric(&base).unwrap();
    let work_map = axis_floors_from_rubric(&work).unwrap();
    let out = axis_floor_monotonicity(RUBRIC_MODULE, &base_map, &work_map, |_, _| true);
    assert_eq!(
        out.violations.len(),
        1,
        "the lowered axis floor reds: {out:#?}"
    );
    assert!(
        out.violations[0].contains("axisGmn1Coverage") && out.violations[0].contains("LOWERED"),
        "names the axis and the lowering: {out:#?}"
    );
    // The reverse direction (holding at base) is clean.
    let up = axis_floor_monotonicity(RUBRIC_MODULE, &base_map, &base_map, |_, _| true);
    assert!(up.violations.is_empty());
}

#[test]
fn tier_floor_monotonicity_reds_on_lowered_tier_vs_base_ttl() {
    // (e) A working-tree module.ttl that LOWERS a committed roll-up tier floor
    // (tierGrounded → tierRegistered) is a hard violation (floors are raise-only).
    let base = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:stf a gmeow:SliceTierFloor ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorTier gmeow:tierGrounded ."#,
            ),
            "base",
        )
        .unwrap();
    let work = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:stf a gmeow:SliceTierFloor ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorTier gmeow:tierRegistered ."#,
            ),
            "work",
        )
        .unwrap();
    let base_map = tier_floors_from_rubric(&base).unwrap();
    let work_map = tier_floors_from_rubric(&work).unwrap();
    let out = tier_floor_monotonicity(RUBRIC_MODULE, &base_map, &work_map, |_| true);
    assert_eq!(
        out.violations.len(),
        1,
        "the lowered tier floor reds: {out:#?}"
    );
    assert!(
        out.violations[0].contains("LOWERED")
            && out.violations[0].contains("tierGrounded")
            && out.violations[0].contains("tierRegistered"),
        "names the lowering old → new: {out:#?}"
    );
}
