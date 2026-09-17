// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::model::{Axis, AxisGrade, ContextScope, GovernanceFloors, MeasurementStandard, Tier};

fn tier(rank: i64) -> Tier {
    Tier {
        iri: format!("{}tier{rank}", crate::model::GMEOW),
        label: format!("T{rank}"),
        rank,
    }
}

fn axis(local: &str, weight: f64) -> Axis {
    Axis {
        iri: format!("{}{local}", crate::model::GMEOW),
        label: local.to_owned(),
        producer: "test".to_owned(),
        dimension_iri: String::new(),
        thresholds: vec![],
        weight,
        scope: ContextScope::SliceLocal,
        advice: String::new(),
    }
}

/// A rubric with three axes a/b/c and a five-rung ladder.
fn rubric(axes: Vec<Axis>) -> Rubric {
    Rubric {
        standard: MeasurementStandard {
            tiers: (0..5).map(tier).collect(),
            axes,
        },
        floors: GovernanceFloors::default(),
    }
}

fn assessment(slice: &str, axes: &[&Axis], ranks: &[i64]) -> SliceAssessment {
    let grades: Vec<AxisGrade> = axes
        .iter()
        .zip(ranks.iter())
        .map(|(ax, &r)| AxisGrade {
            axis_iri: ax.iri.clone(),
            score: 0.5,
            tier: tier(r),
        })
        .collect();
    let rollup_rank = *ranks.iter().min().unwrap();
    SliceAssessment {
        slice: slice.to_owned(),
        grades,
        rollup: tier(rollup_rank),
    }
}

#[test]
fn dominance_is_ge_everywhere_and_gt_somewhere() {
    // a is >= b everywhere and > on axis 0 → a dominates b.
    assert!(dominates(&[2, 2, 2], &[1, 2, 2]));
    // Equal vectors do not dominate (no strict >).
    assert!(!dominates(&[2, 2, 2], &[2, 2, 2]));
    // Behind on one axis → cannot dominate even if ahead elsewhere.
    assert!(!dominates(&[3, 0, 3], &[2, 1, 3]));
    // Strictly worse everywhere → does not dominate (the OTHER one does).
    assert!(!dominates(&[0, 0, 0], &[1, 1, 1]));
}

#[test]
fn frontier_and_dominated_split_is_identified() {
    // Three axes a/b/c; equal weights so the split is pure vector dominance.
    let a = axis("axisA", 1.0);
    let b = axis("axisB", 1.0);
    let c = axis("axisC", 1.0);
    let r = rubric(vec![a.clone(), b.clone(), c.clone()]);
    let axes = [&a, &b, &c];

    // `ahead`=(3,3,1) dominates `behind`=(2,2,1) (>= all, > on axes 0/1).
    // `tradeoff`=(1,4,4) is a genuine trade-off: it beats both peers on axes
    // 1/2 but loses on axis 0, so nothing dominates it and it dominates nothing.
    let ahead = assessment("s:ahead", &axes, &[3, 3, 1]);
    let behind = assessment("s:behind", &axes, &[2, 2, 1]);
    let tradeoff = assessment("s:tradeoff", &axes, &[1, 4, 4]);

    let inputs = vec![
        SliceInput {
            assessment: &ahead,
            advice_count: 0,
        },
        SliceInput {
            assessment: &behind,
            advice_count: 2,
        },
        SliceInput {
            assessment: &tradeoff,
            advice_count: 1,
        },
    ];
    let rows = prioritize(&inputs, &r);
    let by = |name: &str| rows.iter().find(|x| x.slice == name).unwrap();

    // `behind` is dominated by `ahead` (>= everywhere, > on two axes).
    assert!(!by("s:behind").on_frontier, "behind must be dominated");
    // `ahead` is a frontier slice — nothing dominates it.
    assert!(by("s:ahead").on_frontier, "ahead must be on the frontier");
    // `tradeoff` is on the frontier — it beats every peer on some axis.
    assert!(
        by("s:tradeoff").on_frontier,
        "tradeoff must be on the frontier"
    );
}

#[test]
fn capping_axis_is_min_rank_ties_broken_by_weight() {
    // Two axes tie at the min rank (0); the heavier one is the capping witness.
    let light = axis("axisLight", 1.0);
    let heavy = axis("axisHeavy", 5.0);
    let ok = axis("axisOk", 3.0);
    let r = rubric(vec![heavy.clone(), light.clone(), ok.clone()]);
    // heavy=0, light=0, ok=4 → min rank 0 shared by heavy & light.
    let a = assessment("s:x", &[&heavy, &light, &ok], &[0, 0, 4]);
    let inputs = vec![SliceInput {
        assessment: &a,
        advice_count: 0,
    }];
    let rows = prioritize(&inputs, &r);
    let cap = rows[0].capping_axis.as_ref().unwrap();
    assert_eq!(cap.rank, 0, "capping axis is the least-rank axis");
    assert_eq!(
        cap.axis_iri, heavy.iri,
        "the heavier of the tied weakest axes is the leverage target"
    );
}

#[test]
fn ordering_puts_weakest_and_dominated_first() {
    let a = axis("axisA", 1.0);
    let b = axis("axisB", 1.0);
    let r = rubric(vec![a.clone(), b.clone()]);
    let axes = [&a, &b];
    // low-dominated: rollup 0, dominated. mid-frontier: rollup 1. high: rollup 3.
    let dominator = assessment("s:dominator", &axes, &[1, 3]);
    let low_dominated = assessment("s:low", &axes, &[0, 3]);
    let high = assessment("s:high", &axes, &[3, 3]);
    let inputs = vec![
        SliceInput {
            assessment: &high,
            advice_count: 0,
        },
        SliceInput {
            assessment: &dominator,
            advice_count: 0,
        },
        SliceInput {
            assessment: &low_dominated,
            advice_count: 0,
        },
    ];
    let rows = prioritize(&inputs, &r);
    // Lowest roll-up first.
    assert_eq!(rows[0].slice, "s:low", "weakest roll-up sorts first");
    assert_eq!(rows.last().unwrap().slice, "s:high", "strongest sorts last");
}
