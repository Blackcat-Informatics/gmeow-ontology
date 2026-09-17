// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::model::{GovernanceFloors, MeasurementStandard};

#[test]
fn undeclared_slice_always_passes() {
    // (c) undeclared → advisory only, never fails — even measured at the floor.
    assert_eq!(evaluate_ratchet(None, 0, None), RatchetVerdict::Pass);
    assert_eq!(evaluate_ratchet(None, 0, Some(4)), RatchetVerdict::Pass);
}

#[test]
fn measured_below_declared_fails() {
    // (b) declared Linked(2) but measured Grounded(1) → fail.
    assert_eq!(
        evaluate_ratchet(Some(2), 1, None),
        RatchetVerdict::MeasuredBelowDeclared
    );
    // Holding exactly at the declared tier passes.
    assert_eq!(evaluate_ratchet(Some(2), 2, None), RatchetVerdict::Pass);
    // Exceeding the declared tier passes.
    assert_eq!(evaluate_ratchet(Some(1), 3, None), RatchetVerdict::Pass);
}

/// An axis binding the given producer with an otherwise-minimal shape.
fn mk_axis(producer: &str) -> crate::model::Axis {
    use crate::model::{Axis, ContextScope};
    Axis {
        iri: format!("ex:{producer}"),
        label: String::new(),
        producer: producer.to_owned(),
        dimension_iri: "ex:d".to_owned(),
        thresholds: vec![],
        weight: 1.0,
        scope: ContextScope::SliceLocal,
        advice: String::new(),
    }
}

#[test]
fn binding_gate_reds_when_producer_resolves_to_no_item() {
    // A rubric in perfect bijection with the kernel's closed IMPLEMENTED set
    // still reds if a producer resolves to no real Rust item — so the gate
    // proves real resolution, not mere list membership. This is the H4 fix:
    // a producer left in IMPLEMENTED but whose backing fn is gone must red.
    let axes: Vec<crate::model::Axis> = axes::IMPLEMENTED.iter().map(|p| mk_axis(p)).collect();
    let rubric = Rubric {
        standard: MeasurementStandard {
            tiers: vec![],
            axes,
        },
        floors: GovernanceFloors::default(),
    };
    // Every producer resolves → green (bijection holds and all resolve).
    assert!(
        binding_gate(&rubric, |_| true).is_empty(),
        "a full, resolving bijection is green"
    );
    // One producer's Rust item is missing → exactly that producer reds, even
    // though it is still present in IMPLEMENTED and the rubric.
    let errs = binding_gate(&rubric, |s| s != "grounding_axis");
    assert_eq!(
        errs.len(),
        1,
        "exactly the unresolved producer reds: {errs:#?}"
    );
    assert!(
        errs[0].contains("resolves to no Rust primitive item")
            && errs[0].contains("grounding_axis"),
        "the red names the unresolved producer: {errs:#?}"
    );
}

#[test]
fn binding_gate_reds_on_prefix_producer() {
    // (a) A producer that is a strict PREFIX of a real item name must red:
    // the resolver here recognises only the full name `grounding_axis`, so the
    // prefix `grounding_ax` does not resolve — proving the substring/prefix
    // false-positive is gone (a naive `contains("fn grounding_ax")` would have
    // matched `fn grounding_axis`).
    let real: BTreeSet<&str> = axes::IMPLEMENTED.iter().copied().collect();
    let rubric = Rubric {
        standard: MeasurementStandard {
            tiers: vec![],
            axes: vec![mk_axis("grounding_ax")],
        },
        floors: GovernanceFloors::default(),
    };
    let errs = binding_gate(&rubric, |s| real.contains(s));
    assert!(
            errs.iter()
                .any(|e| e.contains("grounding_ax")
                    && e.contains("resolves to no Rust primitive item")),
            "a strict-prefix producer must red on real resolution: {errs:#?}"
        );
}

#[test]
fn staleness_reds_when_producer_resolves() {
    use crate::model::Exemption;
    let rubric = Rubric {
        standard: MeasurementStandard {
            tiers: vec![],
            axes: vec![],
        },
        floors: GovernanceFloors {
            exemptions: vec![Exemption {
                iri: "ex:e".to_owned(),
                axis_iri: "ex:a".to_owned(),
                reason: "unlanded".to_owned(),
                date: "2026-07-07".to_owned(),
                producer: "DocMaturity".to_owned(),
            }],
            commitments: vec![],
            tier_floors: vec![],
            ..Default::default()
        },
    };
    // Producer not in-repo → not stale.
    assert!(stale_exemptions(&rubric, |_| false).is_empty());
    // Producer resolves in-repo → stale (the exemption must be retired).
    let stale = stale_exemptions(&rubric, |s| s == "DocMaturity");
    assert_eq!(
        stale.len(),
        1,
        "a resolved producer makes its exemption stale"
    );
}

#[test]
fn completeness_gate_reds_on_empty_exemption_reason() {
    // (d) An exemption whose reason is empty/whitespace must red — a dated
    // exemption cannot pass without a doctrine-anchored justification.
    use crate::model::{Axis, ContextScope, Exemption, Threshold};
    let axis = Axis {
        iri: "ex:a".to_owned(),
        label: String::new(),
        producer: "p".to_owned(),
        dimension_iri: "ex:d".to_owned(),
        thresholds: vec![Threshold {
            tier_iri: "ex:t".to_owned(),
            floor: 0.0,
        }],
        weight: 1.0,
        scope: ContextScope::SliceLocal,
        advice: String::new(),
    };
    let rubric = Rubric {
        standard: MeasurementStandard {
            tiers: vec![],
            axes: vec![axis],
        },
        floors: GovernanceFloors {
            exemptions: vec![Exemption {
                iri: "ex:e".to_owned(),
                axis_iri: "ex:a".to_owned(),
                reason: "   ".to_owned(),
                date: "2026-07-08".to_owned(),
                producer: "DocMaturity".to_owned(),
            }],
            commitments: vec![],
            tier_floors: vec![],
            ..Default::default()
        },
    };
    let errs = completeness_gate(&rubric);
    assert!(
        errs.iter().any(|e| e.contains("empty/whitespace reason")),
        "empty exemption reason must red: {errs:#?}"
    );
}

#[test]
fn axis_floor_pass_and_fail() {
    // Exactly at the floor passes.
    assert_eq!(evaluate_axis_floor(1.0, 1.0), AxisRatchetVerdict::Pass);
    // Above the floor passes.
    assert_eq!(evaluate_axis_floor(0.99, 0.5), AxisRatchetVerdict::Pass);
    // Below the floor fails — a real regression.
    assert_eq!(
        evaluate_axis_floor(0.90, 1.0),
        AxisRatchetVerdict::MeasuredBelowFloor
    );
    assert!(evaluate_axis_floor(0.90, 1.0).is_failure());
    assert!(!evaluate_axis_floor(1.0, 1.0).is_failure());
}

fn tf(rank: i64, local: &str) -> TierFloor {
    TierFloor {
        rank,
        local: local.to_owned(),
    }
}

#[test]
fn tier_floor_lowering_is_a_hard_violation() {
    let mut base = BTreeMap::new();
    base.insert("ex:logic".to_owned(), tf(2, "tierLinked"));
    base.insert("ex:math".to_owned(), tf(1, "tierGrounded"));
    base.insert("ex:gone".to_owned(), tf(1, "tierGrounded"));

    // A lowered floor (logic 2→1) is a HARD VIOLATION — floors are raise-only and
    // only the maintainer re-baselines a floor down, out-of-band. A raised floor
    // (math 1→3), an added slice (tags), and a deletion of a no-longer-live slice
    // (`gone`) are all clean.
    let mut working = BTreeMap::new();
    working.insert("ex:logic".to_owned(), tf(1, "tierGrounded"));
    working.insert("ex:math".to_owned(), tf(3, "tierExemplified"));
    working.insert("ex:tags".to_owned(), tf(0, "tierRegistered"));

    let live = |s: &str| s != "ex:gone"; // every base slice but `gone` still exists
    let out = tier_floor_monotonicity("floors.tsv", &base, &working, live);
    assert_eq!(out.violations.len(), 1, "only the lowering reds: {out:#?}");
    assert!(
        out.violations[0].contains("ex:logic")
            && out.violations[0].contains("LOWERED")
            && out.violations[0].contains("tierLinked")
            && out.violations[0].contains("tierGrounded"),
        "the violation names the slice and old → new: {out:#?}"
    );
}

#[test]
fn tier_floor_monotonicity_reds_on_still_live_deletion() {
    // A floor removed from the working file for a slice that STILL EXISTS is a
    // hard fail — greenfield removal is allowed only when the slice is gone.
    let mut base = BTreeMap::new();
    base.insert("ex:logic".to_owned(), tf(2, "tierLinked"));
    let working = BTreeMap::new();
    // Slice still live → deletion reds.
    let out = tier_floor_monotonicity("floors.tsv", &base, &working, |_| true);
    assert_eq!(
        out.violations.len(),
        1,
        "still-live deletion reds: {out:#?}"
    );
    assert!(out.violations[0].contains("DELETED") && out.violations[0].contains("ex:logic"));
    // Slice no longer exists → deletion allowed (greenfield removal).
    assert!(
        tier_floor_monotonicity("floors.tsv", &base, &working, |_| false)
            .violations
            .is_empty()
    );
}

#[test]
fn tier_floor_monotonicity_passes_on_raise_and_addition() {
    let mut base = BTreeMap::new();
    base.insert("ex:logic".to_owned(), tf(1, "tierGrounded"));
    let mut working = BTreeMap::new();
    working.insert("ex:logic".to_owned(), tf(2, "tierLinked")); // raise — allowed
    working.insert("ex:new".to_owned(), tf(0, "tierRegistered")); // addition — allowed
    let out = tier_floor_monotonicity("floors.tsv", &base, &working, |_| true);
    assert!(
        out.violations.is_empty(),
        "a raise plus an addition is clean: {out:#?}"
    );
    // Holding exactly at the same rank is also clean.
    let mut same = BTreeMap::new();
    same.insert("ex:logic".to_owned(), tf(1, "tierGrounded"));
    let held = tier_floor_monotonicity("floors.tsv", &base, &same, |_| true);
    assert!(held.violations.is_empty());
}

#[test]
fn axis_floor_lowering_is_a_hard_violation() {
    let key = |s: &str| ("ex:logic".to_owned(), s.to_owned());
    let mut base = BTreeMap::new();
    base.insert(key("axisGmn1Coverage"), 0.98_f64);
    let mut working = BTreeMap::new();
    // Lowered below tolerance → HARD VIOLATION (raise-only ratchet).
    working.insert(key("axisGmn1Coverage"), 0.90_f64);
    let out = axis_floor_monotonicity("axis.tsv", &base, &working, |_, _| true);
    assert_eq!(out.violations.len(), 1, "the lowering reds: {out:#?}");
    assert!(
        out.violations[0].contains("ex:logic")
            && out.violations[0].contains("axisGmn1Coverage")
            && out.violations[0].contains("LOWERED"),
        "names the slice, axis, and lowering: {out:#?}"
    );
    // A raise passes silently.
    let mut raised = BTreeMap::new();
    raised.insert(key("axisGmn1Coverage"), 1.0_f64);
    let up = axis_floor_monotonicity("axis.tsv", &base, &raised, |_, _| true);
    assert!(up.violations.is_empty());
    // Holding exactly at the floor passes silently (within EPSILON).
    let mut same = BTreeMap::new();
    same.insert(key("axisGmn1Coverage"), 0.98_f64);
    let held = axis_floor_monotonicity("axis.tsv", &base, &same, |_, _| true);
    assert!(held.violations.is_empty());
}

#[test]
fn axis_floor_monotonicity_deletion_liveness() {
    let key = ("ex:logic".to_owned(), "axisGmn1Coverage".to_owned());
    let mut base = BTreeMap::new();
    base.insert(key, 1.0_f64);
    let working = BTreeMap::new();
    // Slice + axis still live → deletion reds.
    let out = axis_floor_monotonicity("axis.tsv", &base, &working, |_, _| true);
    assert_eq!(
        out.violations.len(),
        1,
        "still-live axis deletion reds: {out:#?}"
    );
    assert!(out.violations[0].contains("DELETED"));
    // Axis (or slice) no longer live → deletion allowed.
    assert!(
        axis_floor_monotonicity("axis.tsv", &base, &working, |_, _| false)
            .violations
            .is_empty()
    );
}

#[test]
fn declared_below_floor_fails() {
    // (a) committed floor Linked(2) but manifest lowered to Grounded(1) → fail,
    // regardless of what is measured (the ratchet forbids the lowering itself).
    assert_eq!(
        evaluate_ratchet(Some(1), 4, Some(2)),
        RatchetVerdict::DeclaredBelowFloor
    );
    // Declaring at or above the floor is allowed (measured then decides).
    assert_eq!(evaluate_ratchet(Some(2), 2, Some(2)), RatchetVerdict::Pass);
    assert_eq!(evaluate_ratchet(Some(3), 3, Some(2)), RatchetVerdict::Pass);
}

// --- Floor-coherence fixtures ---------------------------------------------
// Small synthetic rubrics: a Registered(0)/Grounded(1)/Linked(2) ladder and
// axes whose thresholds put the Grounded floor at 0.60 and the Linked floor at
// 0.75, so a floor of 0.10 grades to Registered, 0.65 to Grounded, 0.80 to
// Linked. Coherence reads BOTH floor levels straight off the rubric.

fn co_tier(local: &str, rank: i64) -> Tier {
    Tier {
        iri: format!("ex:{local}"),
        label: local.to_owned(),
        rank,
    }
}

fn co_ladder() -> Vec<Tier> {
    vec![
        co_tier("tierRegistered", 0),
        co_tier("tierGrounded", 1),
        co_tier("tierLinked", 2),
    ]
}

fn co_axis(iri: &str) -> Axis {
    use crate::model::{ContextScope, Threshold};
    Axis {
        iri: iri.to_owned(),
        label: iri.to_owned(),
        producer: "test".to_owned(),
        dimension_iri: "ex:d".to_owned(),
        thresholds: vec![
            Threshold {
                tier_iri: "ex:tierGrounded".to_owned(),
                floor: 0.60,
            },
            Threshold {
                tier_iri: "ex:tierLinked".to_owned(),
                floor: 0.75,
            },
        ],
        weight: 1.0,
        scope: ContextScope::SliceLocal,
        advice: String::new(),
    }
}

fn afc(slice: &str, axis: &str, floor: f64) -> AxisFloorCommitment {
    AxisFloorCommitment {
        slice: slice.to_owned(),
        axis: axis.to_owned(),
        floor,
    }
}

fn stf(slice: &str, tier: &str) -> crate::model::SliceTierFloorCommitment {
    crate::model::SliceTierFloorCommitment {
        slice: slice.to_owned(),
        tier: tier.to_owned(),
    }
}

fn co_rubric(
    axes: Vec<Axis>,
    commitments: Vec<AxisFloorCommitment>,
    tier_floors: Vec<crate::model::SliceTierFloorCommitment>,
) -> Rubric {
    Rubric {
        standard: MeasurementStandard {
            tiers: co_ladder(),
            axes,
        },
        floors: GovernanceFloors {
            exemptions: vec![],
            commitments,
            tier_floors,
            ..Default::default()
        },
    }
}

#[test]
fn coherence_backing_and_tightness_hold_on_a_coherent_fixture() {
    // (a) A slice with a tier floor Grounded(1) and an axis floor on EVERY axis,
    // each grading to Grounded(1) — the backing invariant holds (1 >= 1) and the
    // tightness check holds (meet == 1 == floor). No violation.
    let rubric = co_rubric(
        vec![co_axis("ex:axisA"), co_axis("ex:axisB")],
        vec![
            afc("ex:s", "ex:axisA", 0.65), // → Grounded(1)
            afc("ex:s", "ex:axisB", 0.70), // → Grounded(1)
        ],
        vec![stf("ex:s", "ex:tierGrounded")],
    );
    assert!(
        evaluate_coherence(&rubric).is_empty(),
        "a coherent floored slice passes: {:#?}",
        evaluate_coherence(&rubric)
    );
}

#[test]
fn coherence_reds_when_an_axis_floor_implies_below_the_tier_floor() {
    // (b) Tier floor Linked(2); axisA floor 0.80 → Linked(2) (backs it) but
    // axisB floor 0.10 → Registered(0), below the tier floor. Only 2 of 3 axes
    // are floored, so tightness is skipped and exactly the backing invariant reds.
    let rubric = co_rubric(
        vec![
            co_axis("ex:axisA"),
            co_axis("ex:axisB"),
            co_axis("ex:axisC"),
        ],
        vec![
            afc("ex:s", "ex:axisA", 0.80), // → Linked(2)
            afc("ex:s", "ex:axisB", 0.10), // → Registered(0) — below Linked(2)
        ],
        vec![stf("ex:s", "ex:tierLinked")],
    );
    let v = evaluate_coherence(&rubric);
    assert_eq!(v.len(), 1, "exactly the backing invariant reds: {v:#?}");
    assert_eq!(v[0].kind, CoherenceKind::BackingInvariant);
    assert!(
        v[0].message.contains("ex:s")
            && v[0].message.contains("axisB")
            && v[0].message.contains("tierRegistered")
            && v[0].message.contains("tierLinked"),
        "names slice, axis, implied tier, and tier floor: {}",
        v[0].message
    );
}

#[test]
fn coherence_reds_on_a_loose_tier_floor_when_floored_on_every_axis() {
    // (c) Floored on EVERY axis (both grade to Grounded(1), so meet == 1) but the
    // committed tier floor is Registered(0) — below the achievable meet. The
    // backing invariant holds (1 >= 0); exactly the tightness check reds.
    let rubric = co_rubric(
        vec![co_axis("ex:axisA"), co_axis("ex:axisB")],
        vec![
            afc("ex:s", "ex:axisA", 0.65), // → Grounded(1)
            afc("ex:s", "ex:axisB", 0.70), // → Grounded(1)
        ],
        vec![stf("ex:s", "ex:tierRegistered")],
    );
    let v = evaluate_coherence(&rubric);
    assert_eq!(v.len(), 1, "exactly the tightness check reds: {v:#?}");
    assert_eq!(v[0].kind, CoherenceKind::Tightness);
    assert!(
        v[0].message.contains("ex:s")
                && v[0].message.contains("tierGrounded") // the meet
                && v[0].message.contains("tierRegistered"), // the loose floor
        "names slice, meet tier, and tier floor: {}",
        v[0].message
    );
}

#[test]
fn coherence_skips_slices_missing_either_floor_level() {
    // (d) sliceA has a tier floor but NO axis floor; sliceB has axis floors but
    // NO tier floor. Neither pairing exists, so both are skipped — no violation.
    let rubric = co_rubric(
        vec![co_axis("ex:axisA")],
        vec![afc("ex:sB", "ex:axisA", 0.10)], // sliceB axis floor, no tier floor
        vec![stf("ex:sA", "ex:tierLinked")],  // sliceA tier floor, no axis floor
    );
    assert!(
        evaluate_coherence(&rubric).is_empty(),
        "a tier-floor-only slice and an axis-floor-only slice are both skipped: {:#?}",
        evaluate_coherence(&rubric)
    );
}

// --- Projection-ceiling ratchet fixtures -----------------------------------

#[test]
fn ceiling_pass_at_or_below_ceiling() {
    assert_eq!(evaluate_projection_ceiling(0, 0), CeilingVerdict::Pass);
    assert_eq!(evaluate_projection_ceiling(3, 3), CeilingVerdict::Pass);
    assert_eq!(evaluate_projection_ceiling(2, 5), CeilingVerdict::Pass);
    assert!(!evaluate_projection_ceiling(3, 3).is_failure());
}

#[test]
fn ceiling_fails_above_ceiling() {
    assert_eq!(
        evaluate_projection_ceiling(4, 3),
        CeilingVerdict::MeasuredAboveCeiling
    );
    assert!(evaluate_projection_ceiling(4, 3).is_failure());
    // Default ceiling 0: any nonzero residue on an absent commitment reds.
    assert_eq!(
        evaluate_projection_ceiling(1, 0),
        CeilingVerdict::MeasuredAboveCeiling
    );
}

fn ck(slice: &str, vocab: &str) -> (String, String) {
    (slice.to_owned(), vocab.to_owned())
}

/// A [`CeilingComparison`] over the two ceiling maps ALONE — an EMPTY declaration
/// set, no witness, no measurement. Under an empty declaration set `inflow` is
/// identically `0`, so the comparator must reproduce the pre-relocation behaviour
/// exactly: `working <= base` on a shared key, and `working <= measured(base)` (here
/// `0`, since `base_measured` is empty) on a new key.
fn plain_cmp<'a>(
    base: &'a BTreeMap<(String, String), u64>,
    working: &'a BTreeMap<(String, String), u64>,
    measured: &'a BTreeMap<(String, String), u64>,
    empty_u64: &'a BTreeMap<(String, String), u64>,
    empty_constructs: &'a BTreeMap<(String, String), Vec<crate::counting::Construct>>,
    defaults: &'a BTreeMap<String, u64>,
    empty_reasons: &'a BTreeMap<
        (String, String, String),
        BTreeMap<String, BTreeSet<crate::counting::RelocationReason>>,
    >,
) -> CeilingComparison<'a> {
    CeilingComparison {
        file_label: "module.ttl",
        base_ceilings: base,
        working_ceilings: working,
        base_measured: empty_u64,
        working_measured: measured,
        base_constructs: empty_constructs,
        working_constructs: empty_constructs,
        default_ceilings: defaults,
        declarations: &[],
        edge_reasons: empty_reasons,
    }
}

#[test]
fn ceiling_monotonicity_reds_on_a_raised_shared_key() {
    let mut base = BTreeMap::new();
    base.insert(ck("ex:logic", "sh"), 5_u64);
    let mut working = BTreeMap::new();
    working.insert(ck("ex:logic", "sh"), 7_u64); // RAISED — hard violation
    // Pin the raised cell to its measured residue so the ONLY thing under test is
    // the raise itself, not the pin rule.
    let mut measured = BTreeMap::new();
    measured.insert(ck("ex:logic", "sh"), 7_u64);
    let (eu, ec, ds, er) = (
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let cmp = plain_cmp(&base, &working, &measured, &eu, &ec, &ds, &er);
    let out = projection_ceiling_monotonicity(&cmp);
    assert_eq!(out.violations.len(), 1, "the raise reds: {out:#?}");
    assert!(
        out.violations[0].contains("ex:logic")
            && out.violations[0].contains("sh")
            && out.violations[0].contains("RAISED")
            && out.violations[0].contains("5")
            && out.violations[0].contains("7"),
        "names the slice, vocab, and old → new: {out:#?}"
    );
    assert!(
        out.accepted.is_empty(),
        "an empty declaration set accepts no transfer: {out:#?}"
    );
}

#[test]
fn ceiling_monotonicity_silent_on_hold_lower_delete_add() {
    let mut base = BTreeMap::new();
    base.insert(ck("ex:logic", "sh"), 5_u64); // held
    base.insert(ck("ex:math", "gufo"), 4_u64); // lowered
    base.insert(ck("ex:gone", "bfo"), 3_u64); // deleted (base-only, always allowed)

    let mut working = BTreeMap::new();
    working.insert(ck("ex:logic", "sh"), 5_u64); // hold — clean
    working.insert(ck("ex:math", "gufo"), 2_u64); // lower — clean
    // The addition is grandfathered against a base measured residue of 1, exactly
    // as ratchet invariant 3 permits.
    working.insert(ck("ex:new", "sssom"), 1_u64);
    let mut base_measured = BTreeMap::new();
    base_measured.insert(ck("ex:new", "sssom"), 1_u64);

    let measured = BTreeMap::new();
    let (ec, ds, er) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let cmp = CeilingComparison {
        base_measured: &base_measured,
        ..plain_cmp(&base, &working, &measured, &base_measured, &ec, &ds, &er)
    };
    let out = projection_ceiling_monotonicity(&cmp);
    assert!(
        out.violations.is_empty(),
        "hold, lower, delete, and a grandfathered add are all clean here: {out:#?}"
    );
}

// --- Relocation-aware rebalance fixtures -----------------------------------

/// A residue construct anchored on `term` — the relocation-invariant identity the
/// rebalance joins base and working on.
fn anchored(term: &str) -> crate::counting::Construct {
    crate::counting::Construct {
        key: term.to_owned(),
        grounded: false,
        is_bridge: false,
        witness: crate::counting::Witness::Anchored(term.to_owned()),
    }
}

/// A residue construct with NO cross-view identity (a blank subject with no named
/// `sh:property`/`sh:node` ancestor).
fn unanchored(key: &str) -> crate::counting::Construct {
    crate::counting::Construct {
        key: key.to_owned(),
        grounded: false,
        is_bridge: false,
        witness: crate::counting::Witness::NonRelocatable,
    }
}

fn constructs(
    cells: &[(&str, &str, &[crate::counting::Construct])],
) -> BTreeMap<(String, String), Vec<crate::counting::Construct>> {
    cells
        .iter()
        .map(|(slice, vocab, cs)| (ck(slice, vocab), cs.to_vec()))
        .collect()
}

fn declaration(iri: &str, terms: &[&str], from: &str, to: &str) -> crate::model::CeilingRelocation {
    crate::model::CeilingRelocation {
        iri: iri.to_owned(),
        terms: terms.iter().map(|t| (*t).to_owned()).collect(),
        from_slice: from.to_owned(),
        to_slice: to.to_owned(),
        vocabulary: None,
        date: "2026-07-28".to_owned(),
    }
}

#[test]
fn a_declared_witnessed_and_paid_transfer_is_accepted_with_its_witnesses() {
    // ex:t1 DEPARTS ex:src (present at base, gone in working) and ARRIVES at
    // ex:dst (absent at base, present in working). The source's committed ceiling
    // falls by exactly one and the destination's new ceiling is pinned to its
    // measured residue, so the re-projected base ceiling holds and the transfer is
    // accepted — carrying the witnessed anchor term as its ledger antecedent.
    let base = BTreeMap::from([(ck("ex:src", "sh"), 2_u64)]);
    let working = BTreeMap::from([(ck("ex:src", "sh"), 1_u64), (ck("ex:dst", "sh"), 1_u64)]);
    let base_measured = BTreeMap::new();
    let working_measured =
        BTreeMap::from([(ck("ex:src", "sh"), 1_u64), (ck("ex:dst", "sh"), 1_u64)]);
    let base_constructs = constructs(&[
        ("ex:src", "sh", &[anchored("ex:t1"), anchored("ex:t2")]),
        ("ex:dst", "sh", &[]),
    ]);
    let working_constructs = constructs(&[
        ("ex:src", "sh", &[anchored("ex:t2")]),
        ("ex:dst", "sh", &[anchored("ex:t1")]),
    ]);
    let defaults = BTreeMap::from([("sh".to_owned(), 0_u64)]);
    let decls = vec![declaration("ex:reloc1", &["ex:t1"], "ex:src", "ex:dst")];
    let reasons = BTreeMap::new();
    let out = projection_ceiling_monotonicity(&CeilingComparison {
        file_label: "module.ttl",
        base_ceilings: &base,
        working_ceilings: &working,
        base_measured: &base_measured,
        working_measured: &working_measured,
        base_constructs: &base_constructs,
        working_constructs: &working_constructs,
        default_ceilings: &defaults,
        declarations: &decls,
        edge_reasons: &reasons,
    });
    assert!(out.violations.is_empty(), "clean transfer: {out:#?}");
    assert_eq!(out.accepted.len(), 1, "one accepted edge: {out:#?}");
    let t = &out.accepted[0];
    assert_eq!((t.vocab.as_str(), t.units), ("sh", 1));
    assert_eq!((t.from.as_str(), t.to.as_str()), ("ex:src", "ex:dst"));
    assert_eq!(t.witnesses, vec!["ex:t1".to_owned()]);
    assert_eq!(t.declarations, vec!["ex:reloc1".to_owned()]);
    // The aggregate budget is unchanged: the only cell committed on BOTH sides
    // went 2 → 1, so conservation is silent.
    assert!(ceiling_conservation("module.ttl", &base, &working).is_empty());
}

#[test]
fn every_refusal_names_its_shortfall() {
    // ONE fixture exercising three shortfall reasons at once: the destination asks
    // for 3 but only ex:t1 is witnessed (ex:t2 never departed the source, and the
    // third arrival ex:t9 is undeclared), and the destination additionally carries
    // a blank-subject construct that can never witness anything.
    let base = BTreeMap::from([(ck("ex:src", "sh"), 5_u64)]);
    let working = BTreeMap::from([(ck("ex:src", "sh"), 2_u64), (ck("ex:dst", "sh"), 3_u64)]);
    let base_measured = BTreeMap::new();
    let working_measured =
        BTreeMap::from([(ck("ex:src", "sh"), 2_u64), (ck("ex:dst", "sh"), 3_u64)]);
    let base_constructs = constructs(&[
        (
            "ex:src",
            "sh",
            &[anchored("ex:t1"), anchored("ex:t2"), anchored("ex:keep")],
        ),
        ("ex:dst", "sh", &[]),
    ]);
    let working_constructs = constructs(&[
        ("ex:src", "sh", &[anchored("ex:keep"), anchored("ex:t2")]),
        (
            "ex:dst",
            "sh",
            &[anchored("ex:t1"), anchored("ex:t9"), unanchored("_:b0#1")],
        ),
    ]);
    let defaults = BTreeMap::from([("sh".to_owned(), 0_u64)]);
    let decls = vec![declaration(
        "ex:reloc1",
        &["ex:t1", "ex:t2"],
        "ex:src",
        "ex:dst",
    )];
    let reasons = BTreeMap::new();
    let out = projection_ceiling_monotonicity(&CeilingComparison {
        file_label: "module.ttl",
        base_ceilings: &base,
        working_ceilings: &working,
        base_measured: &base_measured,
        working_measured: &working_measured,
        base_constructs: &base_constructs,
        working_constructs: &working_constructs,
        default_ceilings: &defaults,
        declarations: &decls,
        edge_reasons: &reasons,
    });
    let all = out.violations.join(" | ");
    assert!(
        all.contains("unwitnessed: 1 of 3"),
        "names the unwitnessed shortfall: {all}"
    );
    assert!(
        all.contains("unpaid: credit 1 < demand 3"),
        "names the delivered credit against the demand: {all}"
    );
    assert!(
        all.contains("undeclared: term ex:t9 moved but no relocation declaration covers it"),
        "names the undeclared arrival: {all}"
    );
    assert!(
        all.contains("non-relocatable: 1 blank-subject construct(s) with no named anchor"),
        "names the construct with no cross-view identity: {all}"
    );
    // ex:t2 was declared but stayed put — the declaration contradicts the witness.
    assert!(
        all.contains("ex:t2") && all.contains("did not depart"),
        "names the declared term that never moved: {all}"
    );
}

#[test]
fn ceiling_conservation_is_scoped_to_base_intersect_working() {
    // A brand-new cell committed at its base measured residue is EXACTLY what
    // invariant 3 permits (a new slice grandfathering pre-existing residue), and it
    // must not red the aggregate check — an unscoped Σ would rise from 5 to 8 here.
    let base = BTreeMap::from([(ck("ex:a", "sh"), 5_u64)]);
    let mut working = BTreeMap::from([(ck("ex:a", "sh"), 5_u64)]);
    working.insert(ck("ex:new", "sh"), 3_u64);
    assert!(
        ceiling_conservation("module.ttl", &base, &working).is_empty(),
        "a grandfathered addition must not red the scoped conservation check"
    );
    // A DELETION only ever lowers the total and is likewise silent.
    let deleted = BTreeMap::from([(ck("ex:a", "sh"), 5_u64)]);
    assert!(ceiling_conservation("module.ttl", &deleted, &BTreeMap::new()).is_empty());
    // A raise on a SHARED cell does red, per vocabulary.
    let raised = BTreeMap::from([(ck("ex:a", "sh"), 6_u64)]);
    let errs = ceiling_conservation("module.ttl", &base, &raised);
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(
        errs[0].contains("vocab sh TOTAL") && errs[0].contains("ROSE 5 → 6"),
        "{errs:#?}"
    );
}

#[test]
fn one_lowering_cannot_fund_two_destinations() {
    // The case a per-destination GREEDY sum gets wrong: the source lowered by 3 and
    // its three departed keys landed in BOTH destinations, so each destination sees
    // `witnessed >= demand`. The transport solution saturates exactly one and
    // refuses the other, naming the blocking edge and the residual demand — instead
    // of accepting both and then contradicting itself at the conservation check.
    let terms = ["ex:t1", "ex:t2", "ex:t3"];
    let base = BTreeMap::from([(ck("ex:src", "sh"), 3_u64)]);
    let working = BTreeMap::from([
        (ck("ex:src", "sh"), 0_u64),
        (ck("ex:d1", "sh"), 3_u64),
        (ck("ex:d2", "sh"), 3_u64),
    ]);
    let base_measured = BTreeMap::new();
    let working_measured = BTreeMap::from([
        (ck("ex:src", "sh"), 0_u64),
        (ck("ex:d1", "sh"), 3_u64),
        (ck("ex:d2", "sh"), 3_u64),
    ]);
    let moved: Vec<crate::counting::Construct> = terms.iter().map(|t| anchored(t)).collect();
    let base_constructs = constructs(&[("ex:src", "sh", &moved)]);
    let working_constructs = constructs(&[("ex:d1", "sh", &moved), ("ex:d2", "sh", &moved)]);
    let defaults = BTreeMap::from([("sh".to_owned(), 0_u64)]);
    let decls = vec![
        declaration("ex:relocD1", &terms, "ex:src", "ex:d1"),
        declaration("ex:relocD2", &terms, "ex:src", "ex:d2"),
    ];
    let reasons = BTreeMap::new();
    let out = projection_ceiling_monotonicity(&CeilingComparison {
        file_label: "module.ttl",
        base_ceilings: &base,
        working_ceilings: &working,
        base_measured: &base_measured,
        working_measured: &working_measured,
        base_constructs: &base_constructs,
        working_constructs: &working_constructs,
        default_ceilings: &defaults,
        declarations: &decls,
        edge_reasons: &reasons,
    });
    assert_eq!(
        out.accepted.len(),
        1,
        "exactly one destination is funded: {out:#?}"
    );
    assert_eq!(out.accepted[0].to, "ex:d1");
    assert_eq!(out.violations.len(), 1, "exactly one refusal: {out:#?}");
    assert!(
        out.violations[0].contains("ex:d2")
            && out.violations[0].contains("3 of 3 unit(s) of this raise are unpaid")
            && out.violations[0].contains("blocking edge ex:src → ex:d2"),
        "the refusal names the blocked destination, its residual demand, and the blocking edge: {out:#?}"
    );
    // And the aggregate conservation check does NOT also fire: the flow already
    // named the culprit, so there is no second, contradictory verdict.
    assert!(
        ceiling_conservation("module.ttl", &base, &working).is_empty(),
        "the refusal is the flow's, not a contradictory Σ red"
    );
}

#[test]
fn two_sources_fund_two_destinations_via_a_residual_path() {
    // The shape a single FORWARD pass over each destination gets wrong: TWO
    // sources and TWO destinations, where `ex:s1` has out-edges to BOTH
    // destinations (capacity 1 each) and `ex:s2` has an out-edge to ONLY
    // `ex:d1` (capacity 1). Supply is `ex:s1 = 1`, `ex:s2 = 1`; demand is
    // `ex:d1 = 1`, `ex:d2 = 1`.
    //
    // `ex:d2` can ONLY ever be paid by `ex:s1` (it is `ex:s1`'s sole other
    // edge), so the unique max flow is `ex:s2 -> ex:d1` and `ex:s1 -> ex:d2`
    // (2 of 2 units, both destinations saturated). A greedy walk that
    // processes `ex:d1` first in `BTreeMap` order and always prefers the
    // alphabetically-first source spends `ex:s1`'s only unit on `ex:d1` — a
    // choice that is REVERSIBLE (a max-flow solver would walk the residual
    // `ex:d1 -> ex:s1` back-edge to undo it once it discovers `ex:d2` is
    // starved) — and then finds `ex:s1` exhausted for `ex:d2`, with `ex:s2`
    // unable to help (it has no edge there). A feasible relocation is FALSELY
    // REFUSED.
    let base = BTreeMap::from([(ck("ex:s1", "sh"), 1_u64), (ck("ex:s2", "sh"), 1_u64)]);
    let working = BTreeMap::from([
        (ck("ex:s1", "sh"), 0_u64),
        (ck("ex:s2", "sh"), 0_u64),
        (ck("ex:d1", "sh"), 1_u64),
        (ck("ex:d2", "sh"), 1_u64),
    ]);
    let base_measured = BTreeMap::new();
    let working_measured = BTreeMap::from([
        (ck("ex:s1", "sh"), 0_u64),
        (ck("ex:s2", "sh"), 0_u64),
        (ck("ex:d1", "sh"), 1_u64),
        (ck("ex:d2", "sh"), 1_u64),
    ]);
    let base_constructs = constructs(&[
        ("ex:s1", "sh", &[anchored("ex:tA"), anchored("ex:tB")]),
        ("ex:s2", "sh", &[anchored("ex:tA")]),
    ]);
    let working_constructs = constructs(&[
        ("ex:d1", "sh", &[anchored("ex:tA")]),
        ("ex:d2", "sh", &[anchored("ex:tB")]),
    ]);
    let defaults = BTreeMap::from([("sh".to_owned(), 0_u64)]);
    let decls = vec![
        declaration("ex:relocA1", &["ex:tA"], "ex:s1", "ex:d1"),
        declaration("ex:relocA2", &["ex:tA"], "ex:s2", "ex:d1"),
        declaration("ex:relocB1", &["ex:tB"], "ex:s1", "ex:d2"),
    ];
    let reasons = BTreeMap::new();
    let out = projection_ceiling_monotonicity(&CeilingComparison {
        file_label: "module.ttl",
        base_ceilings: &base,
        working_ceilings: &working,
        base_measured: &base_measured,
        working_measured: &working_measured,
        base_constructs: &base_constructs,
        working_constructs: &working_constructs,
        default_ceilings: &defaults,
        declarations: &decls,
        edge_reasons: &reasons,
    });
    assert!(
        out.violations.is_empty(),
        "the real max flow saturates both destinations: {out:#?}"
    );
    assert_eq!(
        out.accepted.len(),
        2,
        "both destinations are funded: {out:#?}"
    );
    let by_to: BTreeMap<&str, &AcceptedTransfer> =
        out.accepted.iter().map(|t| (t.to.as_str(), t)).collect();
    let d1 = by_to["ex:d1"];
    assert_eq!((d1.from.as_str(), d1.units), ("ex:s2", 1));
    let d2 = by_to["ex:d2"];
    assert_eq!((d2.from.as_str(), d2.units), ("ex:s1", 1));
}

#[test]
fn solve_transport_at_u64_max_produces_the_exact_flow_not_a_clamped_one() {
    // Boundary regression for the `i64` clamp this solver used to route residual
    // capacities through: a supply/demand/capacity at `u64::MAX` (the top of the
    // representable range for `Transport`'s own field type) must flow EXACTLY —
    // never a silently-clamped `i64::MAX`, which is barely half of `u64::MAX` and
    // would falsely leave ~9.2e18 units "unpaid".
    let network = Transport {
        supply: BTreeMap::from([("ex:src".to_owned(), u64::MAX)]),
        demand: BTreeMap::from([("ex:dst".to_owned(), u64::MAX)]),
        capacity: BTreeMap::from([(("ex:src".to_owned(), "ex:dst".to_owned()), u64::MAX)]),
        witnesses: BTreeMap::new(),
        declarations: BTreeMap::new(),
    };
    let flow = solve_transport(&network);
    assert_eq!(
        flow.edges.get(&("ex:src".to_owned(), "ex:dst".to_owned())),
        Some(&u64::MAX),
        "the full u64::MAX capacity must flow, not a value clamped to i64::MAX: {flow:#?}"
    );
    assert_eq!(
        flow.residual.get("ex:dst"),
        Some(&0),
        "u64::MAX supply/capacity fully pays u64::MAX demand — no residual left unpaid: {flow:#?}"
    );
}

fn vocab(prefix: &str, ns: &[&str], dc: u64) -> crate::model::ProjectionVocabulary {
    crate::model::ProjectionVocabulary {
        prefix: prefix.to_owned(),
        namespaces: ns.iter().map(|s| (*s).to_owned()).collect(),
        subsumed_by: "s".to_owned(),
        owner: "s".to_owned(),
        count_kind: crate::model::CountKind::TypedAxiom,
        default_ceiling: dc,
        preservation: "p".to_owned(),
        alignment_predicates: Vec::new(),
        counted_predicates: Vec::new(),
    }
}

#[test]
fn registry_ratchet_reds_on_weakening_and_silent_on_strengthening() {
    let base = vec![vocab("bfo", &["obo/BFO_"], 0), vocab("gufo", &["g#"], 0)];
    // gufo deleted; bfo namespace narrowed AND default-ceiling raised.
    let weaker = vec![vocab("bfo", &[], 1)];
    let v = registry_ratchet_monotonicity("module.ttl", &base, &weaker);
    assert!(
        v.iter()
            .any(|m| m.contains("gufo") && m.contains("DELETED")),
        "{v:#?}"
    );
    assert!(
        v.iter()
            .any(|m| m.contains("bfo") && m.contains("NARROWED")),
        "{v:#?}"
    );
    assert!(
        v.iter()
            .any(|m| m.contains("bfo") && m.contains("default-ceiling RAISED")),
        "{v:#?}"
    );

    // A new vocab + a WIDER namespace on bfo is pure strengthening — clean.
    let stronger = vec![
        vocab("bfo", &["obo/BFO_", "obo/BFO2_"], 0),
        vocab("gufo", &["g#"], 0),
        vocab("sumo", &["sumo#"], 0),
    ];
    assert!(
        registry_ratchet_monotonicity("module.ttl", &base, &stronger).is_empty(),
        "strengthening must not red"
    );
}
