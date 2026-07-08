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

use std::collections::BTreeSet;
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
    ("doc-maturity", false),
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

/// Axis→producer AST-binding gate: the rubric's axes and the kernel's implemented
/// primitives must be in bijection. A renamed/removed rubric producer becomes an
/// unbound axis; a renamed/removed Rust primitive becomes an orphan. Either reds.
#[must_use]
pub fn binding_gate(rubric: &Rubric) -> Vec<String> {
    let implemented: BTreeSet<&str> = axes::IMPLEMENTED.iter().copied().collect();
    let bound: BTreeSet<&str> = rubric.axes.iter().map(|a| a.producer.as_str()).collect();
    let mut errs = Vec::new();
    for axis in &rubric.axes {
        if !implemented.contains(axis.producer.as_str()) {
            errs.push(format!(
                "axis {} names producer '{}' with no implemented primitive (stale binding)",
                axis.iri, axis.producer
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

/// The `gmeow:sliceQualityTier` a slice's `manifest.ttl` declares, resolved against
/// the rubric's ladder — `None` when the slice has not opted in.
///
/// # Errors
/// Returns a message if the manifest cannot be read or names a tier the rubric
/// does not define (a hard error — an unknown tier is not silently ignored).
pub fn declared_tier(slice_dir: &Path, rubric: &Rubric) -> Result<Option<Tier>, String> {
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
        Some(tier_iri) => rubric
            .tier(&tier_iri)
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("{slice_iri} declares unknown quality tier {tier_iri}")),
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
