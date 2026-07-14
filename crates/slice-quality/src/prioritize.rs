// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Repo-wide slice-quality prioritization: the Pareto frontier over the per-axis
//! profile vectors, plus each slice's capping axis (the meet witness).
//!
//! The per-axis profile vector is the primary object; the roll-up tier is only its
//! lossy meet projection. `--all` is a maintainer-facing question — "which slices,
//! and which axes, to uplift first" — so this module answers it objectively:
//!
//! * **Pareto frontier**: a slice is *dominated* when another slice grades `>=` on
//!   every axis and `>` on at least one (pure vector dominance — no weighting, no
//!   tuning). The non-dominated slices are the *frontier*: already ahead. The
//!   dominated ones are strictly behind a peer and are the first place to look.
//! * **Capping axis (meet witness)**: the least-rank axis of a slice — the single
//!   axis that caps its roll-up meet, hence its highest-leverage uplift target.
//!   `axisWeight` breaks ties among equally-weak axes ONLY; it never changes a
//!   rank or a score (that would corrupt the unweighted-meet law).
//!
//! Everything here is deterministic: stable sorts, explicit tie-breaks on IRIs.

use std::cmp::Ordering;

use crate::model::{Rubric, SliceAssessment};

/// One axis of a slice's profile: the axis identity, the earned tier rank, and the
/// advice-ranking weight (used to order ties only).
#[derive(Debug, Clone)]
pub struct AxisRank {
    /// The axis IRI.
    pub axis_iri: String,
    /// The axis local name (for compact display).
    pub axis_label: String,
    /// The earned tier's ladder rank — the coordinate of the profile vector.
    pub rank: i64,
    /// The `gmeow:axisWeight` — orders ties among equal-rank axes, never the rank.
    pub weight: f64,
}

/// A slice's full prioritization row: its per-axis profile vector, its roll-up
/// meet, its capping axis, and whether it sits on the Pareto frontier.
#[derive(Debug, Clone)]
pub struct SliceProfile {
    /// The slice IRI (or path key) under assessment.
    pub slice: String,
    /// The roll-up tier rank (the unweighted meet).
    pub rollup_rank: i64,
    /// The roll-up tier label.
    pub rollup_label: String,
    /// The per-axis profile vector, in canonical rubric-axis order.
    pub axes: Vec<AxisRank>,
    /// The capping axis (meet witness): least-rank axis, ties on highest weight
    /// then axis IRI. `None` only when the rubric has no axes.
    pub capping_axis: Option<AxisRank>,
    /// The count of ranked uplift advisories the slice surfaced.
    pub advice_count: usize,
    /// `true` when no other slice Pareto-dominates this one (frontier), `false`
    /// when at least one peer is `>=` everywhere and `>` somewhere (dominated).
    pub on_frontier: bool,
}

/// The input for one slice: its assessment plus the count of ranked advisories.
pub struct SliceInput<'a> {
    /// The per-axis grade vector + roll-up tier.
    pub assessment: &'a SliceAssessment,
    /// The number of ranked uplift advisories (for display only).
    pub advice_count: usize,
}

/// Does profile vector `a` Pareto-dominate `b`? `a` dominates iff `a[i] >= b[i]`
/// for **every** axis and `a[i] > b[i]` for **at least one**. Two equal vectors
/// therefore do not dominate each other (both stay on the frontier).
///
/// The two vectors MUST be axis-aligned (same rubric-axis order and length); the
/// builder guarantees this by projecting every slice through the same axis order.
#[must_use]
pub fn dominates(a: &[i64], b: &[i64]) -> bool {
    debug_assert_eq!(a.len(), b.len(), "profile vectors must be axis-aligned");
    let mut strictly_ahead_somewhere = false;
    for (x, y) in a.iter().zip(b.iter()) {
        if x < y {
            return false; // behind on this axis → cannot dominate
        }
        if x > y {
            strictly_ahead_somewhere = true;
        }
    }
    strictly_ahead_somewhere
}

/// The capping axis of a profile: the least-rank axis, ties broken by highest
/// weight, then by axis IRI. This is the meet witness — the axis whose rank equals
/// the roll-up meet, the single highest-leverage uplift target.
#[must_use]
fn capping_axis(axes: &[AxisRank]) -> Option<AxisRank> {
    axes.iter()
        .min_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                // Heavier axis wins the tie → it must compare as "less" for min_by.
                .then_with(|| b.weight.partial_cmp(&a.weight).unwrap_or(Ordering::Equal))
                .then_with(|| a.axis_iri.cmp(&b.axis_iri))
        })
        .cloned()
}

/// Build the prioritization rows: one [`SliceProfile`] per input, with the Pareto
/// frontier computed across the whole set and each slice's capping axis named.
///
/// The canonical axis order is the rubric's axis order (already IRI-sorted), so
/// every slice's profile vector is aligned coordinate-for-coordinate. A slice
/// missing a grade for a rubric axis (an abnormal, mis-scored slice) coordinates
/// at the ladder-bottom rank so the vectors stay aligned rather than ragged.
///
/// The returned rows are sorted for the caller — highest-leverage work first:
///   1. roll-up tier ascending (the weakest slices, the biggest wins, first),
///   2. dominated before frontier (strictly-behind-a-peer slices act first),
///   3. capping axis by weight descending (the heaviest leverage axis first),
///   4. capping axis IRI ascending, then slice IRI ascending (determinism).
#[must_use]
pub fn prioritize(inputs: &[SliceInput], rubric: &Rubric) -> Vec<SliceProfile> {
    let bottom_rank = rubric.standard.bottom_tier().map_or(0, |t| t.rank);

    // Project every slice onto the canonical rubric-axis order.
    let mut rows: Vec<SliceProfile> = inputs
        .iter()
        .map(|input| {
            let rank_of: std::collections::HashMap<&str, i64> = input
                .assessment
                .grades
                .iter()
                .map(|g| (g.axis_iri.as_str(), g.tier.rank))
                .collect();
            let axes: Vec<AxisRank> = rubric
                .standard
                .axes
                .iter()
                .map(|axis| AxisRank {
                    axis_iri: axis.iri.clone(),
                    axis_label: local_name(&axis.iri),
                    rank: rank_of
                        .get(axis.iri.as_str())
                        .copied()
                        .unwrap_or(bottom_rank),
                    weight: axis.weight,
                })
                .collect();
            let capping_axis = capping_axis(&axes);
            SliceProfile {
                slice: input.assessment.slice.clone(),
                rollup_rank: input.assessment.rollup.rank,
                rollup_label: input.assessment.rollup.label.clone(),
                axes,
                capping_axis,
                advice_count: input.advice_count,
                on_frontier: true, // provisional; the dominance sweep sets it below
            }
        })
        .collect();

    // Pareto sweep: a slice is dominated iff some OTHER slice dominates its vector.
    let vectors: Vec<Vec<i64>> = rows
        .iter()
        .map(|r| r.axes.iter().map(|a| a.rank).collect())
        .collect();
    for i in 0..rows.len() {
        let dominated = vectors
            .iter()
            .enumerate()
            .any(|(j, other)| j != i && dominates(other, &vectors[i]));
        rows[i].on_frontier = !dominated;
    }

    // Order: highest-leverage work first (see the doc comment for the full key).
    rows.sort_by(|a, b| {
        a.rollup_rank
            .cmp(&b.rollup_rank)
            .then_with(|| a.on_frontier.cmp(&b.on_frontier)) // false(dominated) < true
            .then_with(|| {
                cap_weight(b)
                    .partial_cmp(&cap_weight(a))
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| cap_iri(a).cmp(cap_iri(b)))
            .then_with(|| a.slice.cmp(&b.slice))
    });
    rows
}

/// The capping-axis weight of a row (0.0 when the rubric has no axes).
fn cap_weight(row: &SliceProfile) -> f64 {
    row.capping_axis.as_ref().map_or(0.0, |a| a.weight)
}

/// The capping-axis IRI of a row ("" when the rubric has no axes).
fn cap_iri(row: &SliceProfile) -> &str {
    row.capping_axis
        .as_ref()
        .map_or("", |a| a.axis_iri.as_str())
}

/// Render the deterministic text prioritization view for `--all`.
///
/// A header documents the ordering; a legend explains the frontier/dominated
/// marks; then one row per slice, weakest-and-most-dominated first, each naming
/// its capping axis (the single highest-leverage uplift target).
#[must_use]
pub fn render_text(rows: &[SliceProfile]) -> String {
    let total = rows.len();
    let frontier = rows.iter().filter(|r| r.on_frontier).count();
    let mut out = String::new();
    out.push_str(&format!(
        "slice-quality: repo-wide prioritization ({total} slice(s))\n"
    ));
    out.push_str(
        "  ordering: roll-up tier ascending, then dominated-before-frontier, then capping axis by weight — highest-leverage uplift first\n",
    );
    out.push_str(
        "  legend: [!] Pareto-dominated (a peer slice grades >= on every axis and > on one)  [=] on the Pareto frontier\n",
    );
    out.push_str(&format!(
        "  Pareto frontier: {frontier} of {total} slice(s) non-dominated\n",
    ));
    out.push_str(
        "  columns: <mark> <slice>  roll-up=<tier>  cap=<axis>(<tier-rank>)  advice=<n>\n",
    );
    for row in rows {
        let mark = if row.on_frontier { "[=]" } else { "[!]" };
        let cap = row.capping_axis.as_ref().map_or_else(
            || "none".to_owned(),
            |a| format!("{}(rank {})", a.axis_label, a.rank),
        );
        out.push_str(&format!(
            "  {mark} {}  roll-up={}  cap={cap}  advice={}\n",
            row.slice, row.rollup_label, row.advice_count
        ));
    }
    out
}

/// The local name of an IRI (tail after the last `/` or `#`).
fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Axis, AxisGrade, ContextScope, GovernanceFloors, MeasurementStandard, Tier,
    };

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
}
