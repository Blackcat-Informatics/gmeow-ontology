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

use std::path::Path;

use crate::graph::{self, id, instances_of, one_iri};
use crate::model::{Rubric, Tier};

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
