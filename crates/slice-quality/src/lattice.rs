// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The lattice scorer: raw axis scores → per-axis tier grades → the roll-up meet.
//!
//! The roll-up tier is the **unweighted** meet (greatest lower bound = least rank)
//! of the per-axis grades. The advice weight never enters here — a weighted meet
//! would be an ad-hoc average and would break the ratchet-is-lattice-order law.

use crate::model::{Axis, AxisGrade, MeasurementStandard, SliceAssessment, Tier};

/// Grade a single axis: the earned tier is the highest tier whose floor the
/// measured `score` meets; if it meets no floor it sits at the ladder bottom.
///
/// `score` is clamped to 0.0–1.0. Thresholds are consulted in descending floor
/// order so the first met floor is the strongest tier earned.
#[must_use]
pub fn grade_axis(axis: &Axis, score: f64, standard: &MeasurementStandard) -> AxisGrade {
    let score = score.clamp(0.0, 1.0);

    // Candidate tiers (threshold.tier resolved against the ladder), sorted by
    // rank descending so we award the strongest tier whose floor is met.
    let mut candidates: Vec<(&Tier, f64)> = axis
        .thresholds
        .iter()
        .filter_map(|t| standard.tier(&t.tier_iri).map(|tier| (tier, t.floor)))
        .collect();
    candidates.sort_by(|a, b| b.0.rank.cmp(&a.0.rank).then(a.0.iri.cmp(&b.0.iri)));

    let earned = candidates
        .iter()
        .find(|(_, floor)| score + f64::EPSILON >= *floor)
        .map(|(tier, _)| (*tier).clone());

    // No floor met → the ladder bottom (the honest floor grade).
    let tier = earned
        .or_else(|| standard.bottom_tier().cloned())
        .unwrap_or_else(|| Tier {
            iri: format!("{}tierRegistered", crate::model::GMEOW),
            label: "Registered".to_owned(),
            rank: 0,
        });

    AxisGrade {
        axis_iri: axis.iri.clone(),
        score,
        tier,
    }
}

/// The unweighted lattice meet of a set of grades: the tier with the least rank.
///
/// Deterministic: ties on rank break on the tier IRI. An empty grade set meets to
/// the ladder bottom (a slice with no measurable axis is at the floor, never
/// silently "maximal").
#[must_use]
pub fn meet(grades: &[AxisGrade], standard: &MeasurementStandard) -> Tier {
    grades
        .iter()
        .map(|g| &g.tier)
        .min()
        .cloned()
        .or_else(|| standard.bottom_tier().cloned())
        .unwrap_or_else(|| Tier {
            iri: format!("{}tierRegistered", crate::model::GMEOW),
            label: "Registered".to_owned(),
            rank: 0,
        })
}

/// Assemble the full slice assessment from raw per-axis scores.
///
/// `scores` pairs each axis with its measured 0.0–1.0 score. The grade vector is
/// the primary object; the roll-up is its meet. Axes are graded in rubric order
/// for determinism.
#[must_use]
pub fn assess(
    slice: &str,
    scores: &[(&Axis, f64)],
    standard: &MeasurementStandard,
) -> SliceAssessment {
    let grades: Vec<AxisGrade> = scores
        .iter()
        .map(|(axis, score)| grade_axis(axis, *score, standard))
        .collect();
    let rollup = meet(&grades, standard);
    SliceAssessment {
        slice: slice.to_owned(),
        grades,
        rollup,
    }
}

#[cfg(test)]
mod tests {
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
}
