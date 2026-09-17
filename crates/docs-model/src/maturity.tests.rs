// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn earned_is_largest_intent_subset() {
    let table = anchor_table();

    // Covers exactly the six-dimension core coat → earns Basic, not Full.
    let basic_cover = MaturityAnchor::Basic.intent();
    assert_eq!(
        earned_maturity(&basic_cover, &table),
        Some(MaturityAnchor::Basic)
    );

    // Covers exactly Minimal → earns Minimal.
    let minimal_cover = MaturityAnchor::Minimal.intent();
    assert_eq!(
        earned_maturity(&minimal_cover, &table),
        Some(MaturityAnchor::Minimal)
    );

    // Covers all nineteen → earns Maximal.
    let all: DimSet = Dimension::ALL.iter().copied().collect();
    assert_eq!(earned_maturity(&all, &table), Some(MaturityAnchor::Maximal));

    // Covers nothing → earns nothing (not even Minimal).
    assert_eq!(earned_maturity(&DimSet::new(), &table), None);

    // Missing a single Minimal dimension (Label) → earns nothing, even though
    // many higher dimensions are present: the floor is a subset test, not a count.
    let mut holey = all.clone();
    holey.remove(&Dimension::Label);
    assert_eq!(earned_maturity(&holey, &table), None);
}

#[test]
fn asserting_maximal_on_full_coverage_trips_the_gate() {
    let table = anchor_table();
    let full_cover = MaturityAnchor::Full.intent();
    let earned = earned_maturity(&full_cover, &table);
    assert_eq!(earned, Some(MaturityAnchor::Full));

    // Asserting Maximal while only earning Full is a violation.
    assert!(asserted_exceeds_earned(MaturityAnchor::Maximal, earned));
    // Asserting Full (== earned) is NOT a violation.
    assert!(!asserted_exceeds_earned(MaturityAnchor::Full, earned));
    // Asserting Basic (below earned) is NOT a violation.
    assert!(!asserted_exceeds_earned(MaturityAnchor::Basic, earned));
    // Asserting anything above an unearned (None) floor is a violation.
    assert!(asserted_exceeds_earned(MaturityAnchor::Minimal, None));
}

#[test]
fn coverage_fraction_is_bounded_and_correct() {
    let full_intent = MaturityAnchor::Full.intent(); // |intent| = 12

    // Empty coverage → 0.0.
    assert_eq!(coverage_fraction(&DimSet::new(), &full_intent), 0.0);

    // Full coverage of the intent → 1.0.
    assert_eq!(coverage_fraction(&full_intent, &full_intent), 1.0);

    // The six-dimension Basic coat covers 6 of Full's 12 → 0.5.
    let basic_cover = MaturityAnchor::Basic.intent();
    assert_eq!(coverage_fraction(&basic_cover, &full_intent), 0.5);

    // Empty intent → 1.0 (vacuously covered), never a divide-by-zero.
    assert_eq!(coverage_fraction(&DimSet::new(), &DimSet::new()), 1.0);

    // Superset coverage still bounded at 1.0 (intersection capped by intent).
    let all: DimSet = Dimension::ALL.iter().copied().collect();
    let frac = coverage_fraction(&all, &full_intent);
    assert!((0.0..=1.0).contains(&frac));
    assert_eq!(frac, 1.0);
}

#[test]
fn from_local_and_next_round_trip_the_ladder() {
    // from_local is the inverse of local_name for every anchor and dimension.
    for a in MaturityAnchor::ALL {
        assert_eq!(MaturityAnchor::from_local(a.local_name()), Some(a));
    }
    for d in Dimension::ALL {
        assert_eq!(Dimension::from_local(d.local_name()), Some(d));
    }
    assert_eq!(MaturityAnchor::from_local("docMaturityNope"), None);
    assert_eq!(Dimension::from_local("dimNope"), None);

    // next climbs the derived ladder and stops at the ceiling.
    assert_eq!(MaturityAnchor::Minimal.next(), Some(MaturityAnchor::Basic));
    assert_eq!(MaturityAnchor::Basic.next(), Some(MaturityAnchor::Full));
    assert_eq!(MaturityAnchor::Full.next(), Some(MaturityAnchor::Maximal));
    assert_eq!(MaturityAnchor::Maximal.next(), None);
}

#[test]
fn anchor_intents_nest() {
    // The Rust mirror of the structural cell saAnchorIntentsNest:
    // Minimal ⊆ Basic ⊆ Full ⊆ Maximal.
    let minimal = MaturityAnchor::Minimal.intent();
    let basic = MaturityAnchor::Basic.intent();
    let full = MaturityAnchor::Full.intent();
    let maximal = MaturityAnchor::Maximal.intent();

    assert!(minimal.is_subset(&basic));
    assert!(basic.is_subset(&full));
    assert!(full.is_subset(&maximal));

    // Proper containment at each step — no degenerate collapse.
    assert!(minimal.len() < basic.len());
    assert!(basic.len() < full.len());
    assert!(full.len() < maximal.len());

    // Maximal is exactly the full dimension set.
    assert_eq!(maximal.len(), Dimension::ALL.len());

    // Every dimension is required by some anchor (no orphan) — the Rust mirror
    // of saNoOrphanDimension. Since Maximal requires all, this is total.
    for dim in Dimension::ALL {
        assert!(
            MaturityAnchor::ALL
                .iter()
                .any(|a| a.intent().contains(&dim)),
            "dimension {dim:?} is required by no anchor"
        );
    }
}
