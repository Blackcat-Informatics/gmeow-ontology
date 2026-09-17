// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::model::{ContextScope, Threshold};

fn tier(local: &str, rank: i64) -> Tier {
    Tier {
        iri: format!("{}{local}", crate::model::GMEOW),
        label: local.to_owned(),
        rank,
    }
}

fn ladder() -> Vec<Tier> {
    vec![
        tier("tierRegistered", 0),
        tier("tierGrounded", 1),
        tier("tierLinked", 2),
        tier("tierExemplified", 3),
        tier("tierMaximal", 4),
    ]
}

fn axis(iri: &str, weight: f64) -> Axis {
    let thresholds = vec![
        Threshold {
            tier_iri: format!("{}tierGrounded", crate::model::GMEOW),
            floor: 0.60,
        },
        Threshold {
            tier_iri: format!("{}tierLinked", crate::model::GMEOW),
            floor: 0.75,
        },
        Threshold {
            tier_iri: format!("{}tierExemplified", crate::model::GMEOW),
            floor: 0.85,
        },
        Threshold {
            tier_iri: format!("{}tierMaximal", crate::model::GMEOW),
            floor: 0.95,
        },
    ];
    Axis {
        iri: iri.to_owned(),
        label: iri.to_owned(),
        producer: "test".to_owned(),
        dimension_iri: String::new(),
        thresholds,
        weight,
        scope: ContextScope::SliceLocal,
        advice: String::new(),
    }
}

fn standard() -> MeasurementStandard {
    MeasurementStandard {
        tiers: ladder(),
        axes: vec![],
    }
}

#[test]
fn score_below_all_floors_is_bottom_tier() {
    let r = standard();
    let a = axis("ex:a", 1.0);
    let g = grade_axis(&a, 0.10, &r);
    assert_eq!(g.tier.rank, 0, "0.10 meets no floor → Registered");
}

#[test]
fn score_earns_the_strongest_met_tier() {
    let r = standard();
    let a = axis("ex:a", 1.0);
    assert_eq!(grade_axis(&a, 0.60, &r).tier.rank, 1, "0.60 → Grounded");
    assert_eq!(grade_axis(&a, 0.80, &r).tier.rank, 2, "0.80 → Linked");
    assert_eq!(grade_axis(&a, 0.96, &r).tier.rank, 4, "0.96 → Maximal");
}

#[test]
fn meet_caps_at_the_weakest_axis_and_weight_never_leaks() {
    let r = standard();
    // Eight Maximal axes with tiny weight, one Registered axis with huge weight.
    let strong = axis("ex:strong", 0.01);
    let weak = axis("ex:weak", 1000.0);
    let mut scores: Vec<(&Axis, f64)> = (0..8).map(|_| (&strong, 0.99)).collect();
    scores.push((&weak, 0.05));
    let assessment = assess("ex:slice", &scores, &r);
    assert_eq!(
        assessment.rollup.rank, 0,
        "one Registered axis caps the slice at Registered regardless of weight"
    );
}

#[test]
fn empty_grades_meet_to_bottom_not_maximal() {
    let r = standard();
    let assessment = assess("ex:empty", &[], &r);
    assert_eq!(
        assessment.rollup.rank, 0,
        "no axes → floor, never silently maximal"
    );
}
