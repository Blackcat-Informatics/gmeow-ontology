// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The opt-in slice-quality tier ratchet.
//!
//! A slice opts in by declaring `gmeow:sliceQualityTier` in its manifest (the sole
//! tier truth, Principle 16). The gate then enforces two things, both pure lattice
//! comparisons:
//! - **measured ≥ declared** — the slice must currently hold the tier it promises;
//! - **declared ≥ committed floor** — the declaration is a ratchet: it may only be
//!   raised, checked against a committed floor artifact so lowering is detectable
//!   without git archaeology.
//!
//! An undeclared slice is purely advisory — it never fails the gate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::axes;
use crate::graph::{self, id, instances_of, one_iri};
use crate::model::{Rubric, Tier};

/// The pipeline projection surfaces the rubric must account for — each must be
/// covered by a landed quality axis OR by a dated `gmeow:AxisExemption`. Adding a
/// new pipeline projection target means adding it here (this is one of the
/// enumerated projection-target-add sites) with either an axis that measures it or
/// a dated exemption, so the quality rubric can never silently fall behind the
/// pipeline it measures. `covered_by_axis == true` means a landed axis measures the
/// surface; `false` means it must carry an exemption keyed by the producer symbol.
pub const PROJECTION_SURFACES: &[(&str, bool)] = &[
    ("shacl", true),
    ("shex", true),
    ("sssom", true),
    ("edoal", true),
    ("fno", true),
    ("docs-pages", true),
    ("loss-ledger", true),
    ("gmn", false),
    ("doc-maturity", true),
    ("docs-panels", false),
];

/// The producer symbol each not-yet-landed projection surface's exemption must name.
fn exemption_producer_for(surface: &str) -> Option<&'static str> {
    match surface {
        "gmn" => Some("GmnProjectionTarget"),
        "doc-maturity" => Some("DocMaturity"),
        "docs-panels" => Some("DocMaturityPanels"),
        _ => None,
    }
}

/// Axis→producer AST-binding gate. Two independent proofs, both of which must hold:
///
/// 1. **Bijection with the kernel's closed set** — the rubric's `gmeow:axisProducer`
///    strings and [`axes::IMPLEMENTED`] must be in bijection. A renamed/removed
///    rubric producer becomes an unbound axis; a renamed/removed entry in the closed
///    set becomes an orphan.
/// 2. **Real symbol resolution** — every rubric producer must additionally `resolves`
///    to an actual Rust *item* definition (the caller passes the constitution-gate
///    AST resolver over the crate source, so this is a real `fn`/item lookup, not a
///    substring or list-membership test). This catches the drift the bijection alone
///    cannot: a producer that survives in the hand-kept `IMPLEMENTED` list but whose
///    backing primitive `fn` in `axes.rs` (or `reasoner.rs`) is gone or renamed reds
///    here instead of passing. A producer that is a strict *prefix* of a real item
///    (e.g. `grounding_ax` vs `grounding_axis`) does NOT resolve — the resolver is
///    identifier-boundary-correct.
///
/// Any of the three conditions reds.
#[must_use]
pub fn binding_gate(rubric: &Rubric, resolves: impl Fn(&str) -> bool) -> Vec<String> {
    let implemented: BTreeSet<&str> = axes::IMPLEMENTED.iter().copied().collect();
    let bound: BTreeSet<&str> = rubric.axes.iter().map(|a| a.producer.as_str()).collect();
    let mut errs = Vec::new();
    for axis in &rubric.axes {
        let producer = axis.producer.as_str();
        if !implemented.contains(producer) {
            errs.push(format!(
                "axis {} names producer '{}' with no implemented primitive (stale binding)",
                axis.iri, producer
            ));
        }
        if !resolves(producer) {
            errs.push(format!(
                "axis {} names producer '{producer}' that resolves to no Rust primitive item in the crate source (unbound producer — the backing fn is missing or renamed)",
                axis.iri
            ));
        }
    }
    for imp in &implemented {
        if !bound.contains(imp) {
            errs.push(format!(
                "implemented primitive '{imp}' is bound by no rubric axis (orphan)"
            ));
        }
    }
    errs
}

/// Projection-target completeness gate: every enumerated projection surface maps to
/// a landed axis or a dated exemption, and every exemption is well-formed (names a
/// real axis, a reason, a date, and a producer). Reds on a surface with no covering
/// axis and no exemption, or a malformed exemption.
#[must_use]
pub fn completeness_gate(rubric: &Rubric) -> Vec<String> {
    let mut errs = Vec::new();
    let exemption_producers: BTreeSet<&str> = rubric
        .exemptions
        .iter()
        .map(|e| e.producer.as_str())
        .collect();

    for (surface, covered_by_axis) in PROJECTION_SURFACES {
        if *covered_by_axis {
            continue; // a landed axis measures this surface
        }
        let Some(producer) = exemption_producer_for(surface) else {
            errs.push(format!("projection surface '{surface}' has no covering axis and no known exemption producer"));
            continue;
        };
        if !exemption_producers.contains(producer) {
            errs.push(format!(
                "projection surface '{surface}' is unlanded but carries no dated exemption (producer '{producer}')"
            ));
        }
    }
    for ex in &rubric.exemptions {
        if ex.axis_iri.is_empty() {
            errs.push(format!("exemption {} names no axis", ex.iri));
        }
        if ex.reason.trim().is_empty() {
            errs.push(format!(
                "exemption {} has an empty/whitespace reason — a dated exemption must carry a doctrine-anchored justification",
                ex.iri
            ));
        }
        if ex.date.is_empty() {
            errs.push(format!("exemption {} is undated", ex.iri));
        }
        if ex.producer.is_empty() {
            errs.push(format!("exemption {} names no producer symbol", ex.iri));
        }
        if !rubric.axes.iter().any(|a| a.iri == ex.axis_iri) {
            errs.push(format!(
                "exemption {} exempts unknown axis {}",
                ex.iri, ex.axis_iri
            ));
        }
    }
    errs
}

/// Exemption-staleness gate: an exemption whose producer symbol now RESOLVES in the
/// repo is stale — the producer has landed, so the exemption must be retired and the
/// axis built. `resolves` reports whether a symbol is defined in-repo.
#[must_use]
pub fn stale_exemptions(rubric: &Rubric, resolves: impl Fn(&str) -> bool) -> Vec<String> {
    rubric
        .exemptions
        .iter()
        .filter(|e| resolves(&e.producer))
        .map(|e| {
            format!(
                "exemption {} is STALE: its producer '{}' now resolves in-repo — remove the exemption and build the axis",
                e.iri, e.producer
            )
        })
        .collect()
}

/// The verdict for one slice's ratchet check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatchetVerdict {
    /// Undeclared (advisory) or declared-and-holding — the gate passes.
    Pass,
    /// The measured roll-up tier is below the declared tier.
    MeasuredBelowDeclared,
    /// The declared tier is below the committed ratchet floor (a lowering).
    DeclaredBelowFloor,
}

impl RatchetVerdict {
    /// Whether this verdict fails the gate.
    #[must_use]
    pub fn is_failure(self) -> bool {
        !matches!(self, Self::Pass)
    }
}

/// Evaluate the ratchet for one slice from three tier ranks.
///
/// `declared_rank` is `None` when the slice has not opted in (advisory → pass).
/// `floor_rank` is `None` when the slice is absent from the committed floor file.
#[must_use]
pub fn evaluate_ratchet(
    declared_rank: Option<i64>,
    measured_rank: i64,
    floor_rank: Option<i64>,
) -> RatchetVerdict {
    let Some(declared) = declared_rank else {
        return RatchetVerdict::Pass; // undeclared → advisory, never gates
    };
    if let Some(floor) = floor_rank
        && declared < floor
    {
        return RatchetVerdict::DeclaredBelowFloor;
    }
    if measured_rank < declared {
        return RatchetVerdict::MeasuredBelowDeclared;
    }
    RatchetVerdict::Pass
}

/// The verdict for one slice's PER-AXIS committed-floor check — distinct from
/// and additional to [`RatchetVerdict`]'s roll-up-tier ratchet. A
/// per-axis floor gates one axis's raw MEASURED score directly (never a tier), so a
/// grounding slice cannot clear the gate on `axisGmn1Coverage < 1.0` regardless of
/// its other axes' scores or its own roll-up tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisRatchetVerdict {
    /// The measured score meets or exceeds the committed floor.
    Pass,
    /// The measured score has fallen below the committed floor (a regression).
    MeasuredBelowFloor,
}

impl AxisRatchetVerdict {
    /// Whether this verdict fails the gate.
    #[must_use]
    pub fn is_failure(self) -> bool {
        !matches!(self, Self::Pass)
    }
}

/// Evaluate one axis's committed-floor check: `Pass` iff `measured >= floor`, a pure
/// comparator with no new scoring path — the caller supplies both the measured
/// `gmeow:AxisGrade.score` and the floor resolved from `governance/
/// slice-quality-axis-floors.tsv` (defaulting to `1.0` for a grounding slice absent
/// from the file — see the caller in `gmeow-dev-cli`'s `slice_quality_gate`).
#[must_use]
pub fn evaluate_axis_floor(measured: f64, floor: f64) -> AxisRatchetVerdict {
    if measured + f64::EPSILON >= floor {
        AxisRatchetVerdict::Pass
    } else {
        AxisRatchetVerdict::MeasuredBelowFloor
    }
}

/// A parsed committed TIER-floor entry. The ladder `rank` drives the monotonic
/// comparison; the `local` tier name is retained verbatim so a violation message
/// can echo exactly what the floor file recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierFloor {
    /// The tier's ladder rank (resolved through the rubric at parse time).
    pub rank: i64,
    /// The tier local name exactly as the floor file recorded it.
    pub local: String,
}

/// Floor-monotonicity check for the per-slice TIER floor file: diff the committed
/// floor at the merge base (`base`) against the working tree (`working`) and red on
/// any *lowering* of a floor line, or on the *deletion* of a floor for a slice that
/// is still live (`live_slice` returns `true`). This is the enforcement of the
/// file's own "may only be raised" ratchet promise — the existing
/// [`evaluate_ratchet`] only checks the declared tier against the CURRENT floor and
/// so cannot notice a PR that silently lowers the floor line itself.
///
/// Rules (all pure, order-deterministic via the `BTreeMap` iteration):
/// - a `(slice)` present in BOTH maps must satisfy `rank_now >= rank_before`;
/// - an **addition** (`working` only) is always allowed;
/// - a **deletion** (`base` only) is allowed ONLY when the slice is no longer live
///   (greenfield removal); deleting a still-live floor reds.
///
/// Returns one message per violation, empty when monotonic.
#[must_use]
pub fn tier_floor_monotonicity(
    file_label: &str,
    base: &BTreeMap<String, TierFloor>,
    working: &BTreeMap<String, TierFloor>,
    live_slice: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut errs = Vec::new();
    for (slice, before) in base {
        match working.get(slice) {
            Some(now) if now.rank < before.rank => errs.push(format!(
                "{file_label}: slice {slice} tier floor LOWERED {} → {} — a committed floor may only be raised, never lowered",
                before.local, now.local
            )),
            Some(_) => {}
            None if live_slice(slice) => errs.push(format!(
                "{file_label}: slice {slice} tier floor {} DELETED while the slice is still live — a live floor may not be removed",
                before.local
            )),
            None => {}
        }
    }
    errs
}

/// Floor-monotonicity check for the PER-AXIS floor file — the axis-level analogue
/// of [`tier_floor_monotonicity`]. A `(slice, axis)` present in both maps must
/// satisfy `floor_now >= floor_before` under the SAME `f64::EPSILON` tolerance
/// [`evaluate_axis_floor`] uses; additions are allowed; a deletion is allowed only
/// when the `(slice, axis)` is no longer live (`live` returns `true` iff the slice
/// still exists AND the axis is still a rubric axis). Reds on a lowering or on the
/// deletion of a still-live floor. Pure; the caller feeds both parsed maps.
#[must_use]
pub fn axis_floor_monotonicity(
    file_label: &str,
    base: &BTreeMap<(String, String), f64>,
    working: &BTreeMap<(String, String), f64>,
    live: impl Fn(&str, &str) -> bool,
) -> Vec<String> {
    let mut errs = Vec::new();
    for ((slice, axis), before) in base {
        match working.get(&(slice.clone(), axis.clone())) {
            Some(now) if *now + f64::EPSILON < *before => errs.push(format!(
                "{file_label}: slice {slice} axis {axis} floor LOWERED {before:.6} → {now:.6} — a committed floor may only be raised, never lowered"
            )),
            Some(_) => {}
            None if live(slice, axis) => errs.push(format!(
                "{file_label}: slice {slice} axis {axis} floor {before:.6} DELETED while still live — a live floor may not be removed"
            )),
            None => {}
        }
    }
    errs
}

/// The `gmeow:sliceQualityTier` a slice's `manifest.ttl` declares, resolved against
/// the rubric's ladder — `None` when the slice has not opted in.
///
/// # Errors
/// Returns a message if the manifest cannot be read or names a tier the rubric
/// does not define (a hard error — an unknown tier is not silently ignored).
pub fn declared_tier(slice_dir: &Path, rubric: &Rubric) -> gmeow_errors::Result<Option<Tier>> {
    let manifest = slice_dir.join("manifest.ttl");
    let ds = crate::dataset_from_paths(&[&manifest])?;
    let Some(slice_iri) = instances_of(&ds, &graph::g("Slice")).into_iter().next() else {
        return Ok(None);
    };
    let (Some(sid), Some(pred)) = (id(&ds, &slice_iri), id(&ds, &graph::g("sliceQualityTier")))
    else {
        return Ok(None);
    };
    match one_iri(&ds, sid, pred) {
        None => Ok(None),
        Some(tier_iri) => rubric.tier(&tier_iri).cloned().map(Some).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Gate {
                detail: format!("{slice_iri} declares unknown quality tier {tier_iri}"),
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            tiers: vec![],
            axes,
            exemptions: vec![],
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
            tiers: vec![],
            axes: vec![mk_axis("grounding_ax")],
            exemptions: vec![],
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
            tiers: vec![],
            axes: vec![],
            exemptions: vec![Exemption {
                iri: "ex:e".to_owned(),
                axis_iri: "ex:a".to_owned(),
                reason: "unlanded".to_owned(),
                date: "2026-07-07".to_owned(),
                producer: "DocMaturity".to_owned(),
            }],
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
            tiers: vec![],
            axes: vec![axis],
            exemptions: vec![Exemption {
                iri: "ex:e".to_owned(),
                axis_iri: "ex:a".to_owned(),
                reason: "   ".to_owned(),
                date: "2026-07-08".to_owned(),
                producer: "DocMaturity".to_owned(),
            }],
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
    fn tier_floor_monotonicity_reds_on_lowering_and_live_deletion() {
        let mut base = BTreeMap::new();
        base.insert("ex:logic".to_owned(), tf(2, "tierLinked"));
        base.insert("ex:math".to_owned(), tf(1, "tierGrounded"));
        base.insert("ex:gone".to_owned(), tf(1, "tierGrounded"));

        // A lowered floor (logic 2→1), a raised floor (math 1→3, allowed), an added
        // slice (tags, allowed), a live deletion (math? no — `gone` deleted). `gone`
        // is no longer live → its deletion is allowed; `logic` lowering reds.
        let mut working = BTreeMap::new();
        working.insert("ex:logic".to_owned(), tf(1, "tierGrounded"));
        working.insert("ex:math".to_owned(), tf(3, "tierExemplified"));
        working.insert("ex:tags".to_owned(), tf(0, "tierRegistered"));

        let live = |s: &str| s != "ex:gone"; // every base slice but `gone` still exists
        let errs = tier_floor_monotonicity("floors.tsv", &base, &working, live);
        assert_eq!(errs.len(), 1, "only the lowering reds: {errs:#?}");
        assert!(
            errs[0].contains("ex:logic")
                && errs[0].contains("LOWERED")
                && errs[0].contains("tierLinked")
                && errs[0].contains("tierGrounded"),
            "the red names the slice and old → new: {errs:#?}"
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
        let errs = tier_floor_monotonicity("floors.tsv", &base, &working, |_| true);
        assert_eq!(errs.len(), 1, "still-live deletion reds: {errs:#?}");
        assert!(errs[0].contains("DELETED") && errs[0].contains("ex:logic"));
        // Slice no longer exists → deletion allowed (greenfield removal).
        assert!(tier_floor_monotonicity("floors.tsv", &base, &working, |_| false).is_empty());
    }

    #[test]
    fn tier_floor_monotonicity_passes_on_raise_and_addition() {
        let mut base = BTreeMap::new();
        base.insert("ex:logic".to_owned(), tf(1, "tierGrounded"));
        let mut working = BTreeMap::new();
        working.insert("ex:logic".to_owned(), tf(2, "tierLinked")); // raise — allowed
        working.insert("ex:new".to_owned(), tf(0, "tierRegistered")); // addition — allowed
        assert!(
            tier_floor_monotonicity("floors.tsv", &base, &working, |_| true).is_empty(),
            "a raise plus an addition is clean"
        );
        // Holding exactly at the same rank is also clean.
        let mut same = BTreeMap::new();
        same.insert("ex:logic".to_owned(), tf(1, "tierGrounded"));
        assert!(tier_floor_monotonicity("floors.tsv", &base, &same, |_| true).is_empty());
    }

    #[test]
    fn axis_floor_monotonicity_reds_on_lowering_passes_on_raise() {
        let key = |s: &str| ("ex:logic".to_owned(), s.to_owned());
        let mut base = BTreeMap::new();
        base.insert(key("axisGmn1Coverage"), 0.98_f64);
        let mut working = BTreeMap::new();
        // Lowered below tolerance → reds.
        working.insert(key("axisGmn1Coverage"), 0.90_f64);
        let errs = axis_floor_monotonicity("axis.tsv", &base, &working, |_, _| true);
        assert_eq!(errs.len(), 1, "an axis-floor lowering reds: {errs:#?}");
        assert!(
            errs[0].contains("ex:logic")
                && errs[0].contains("axisGmn1Coverage")
                && errs[0].contains("LOWERED"),
            "names the slice, axis, and lowering: {errs:#?}"
        );
        // A raise passes.
        let mut raised = BTreeMap::new();
        raised.insert(key("axisGmn1Coverage"), 1.0_f64);
        assert!(axis_floor_monotonicity("axis.tsv", &base, &raised, |_, _| true).is_empty());
        // Holding exactly at the floor passes (within EPSILON).
        let mut same = BTreeMap::new();
        same.insert(key("axisGmn1Coverage"), 0.98_f64);
        assert!(axis_floor_monotonicity("axis.tsv", &base, &same, |_, _| true).is_empty());
    }

    #[test]
    fn axis_floor_monotonicity_deletion_liveness() {
        let key = ("ex:logic".to_owned(), "axisGmn1Coverage".to_owned());
        let mut base = BTreeMap::new();
        base.insert(key, 1.0_f64);
        let working = BTreeMap::new();
        // Slice + axis still live → deletion reds.
        let errs = axis_floor_monotonicity("axis.tsv", &base, &working, |_, _| true);
        assert_eq!(errs.len(), 1, "still-live axis deletion reds: {errs:#?}");
        assert!(errs[0].contains("DELETED"));
        // Axis (or slice) no longer live → deletion allowed.
        assert!(axis_floor_monotonicity("axis.tsv", &base, &working, |_, _| false).is_empty());
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
}
