// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The data model of the slice-quality rubric and its computed grades.
//!
//! Everything here mirrors the ontology-resident rubric authored in
//! `slices/core/slice-quality-rubric/module.ttl`. The Rust side carries only the
//! shape; the axes, tiers, thresholds, advice text, and scopes are loaded from
//! that slice (see [`crate::rubric`]). Grades form a bounded lattice: the roll-up
//! tier is the **unweighted meet** of the per-axis grades (see [`crate::lattice`]).

use std::cmp::Ordering;

/// The GMEOW namespace prefix every rubric IRI shares.
pub const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// One rung of the quality ladder — a `gmeow:QualityTier` individual.
///
/// Tiers are totally ordered by [`Tier::rank`]; the ordering is what makes the
/// ladder a lattice. Two tiers are compared by rank alone, and a stable lexical
/// tie-break on the IRI keeps sort order deterministic even if two tiers ever
/// shared a rank (a case the structural gate forbids, but the code must not rely
/// on derived `Ord` over a floating field).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tier {
    /// The full tier IRI (e.g. `…/tierGrounded`).
    pub iri: String,
    /// The human label (`rdfs:label`).
    pub label: String,
    /// The integer rank giving the ladder its total order.
    pub rank: i64,
}

impl Tier {
    /// A stable, deterministic sort key: rank first, IRI as tie-break. Never
    /// derive `Ord` on the struct — the label must not influence order.
    #[must_use]
    pub fn sort_key(&self) -> (i64, &str) {
        (self.rank, self.iri.as_str())
    }
}

impl PartialOrd for Tier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Tier {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

/// The read-breadth coeffect an axis is granted — how much graph its primitive
/// may consult. Advice is single-slice at every scope; this bounds reads only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextScope {
    /// Only the target slice's own module, examples, and tests.
    SliceLocal,
    /// The target slice plus its declared dependency closure.
    DepsClosure,
    /// The whole merged, reasoned closure.
    MergedClosure,
}

impl ContextScope {
    /// Resolve a `gmeow:AxisContextScope` individual's local name.
    #[must_use]
    pub fn from_local(local: &str) -> Option<Self> {
        match local {
            "scopeSliceLocal" => Some(Self::SliceLocal),
            "scopeDepsClosure" => Some(Self::DepsClosure),
            "scopeMergedClosure" => Some(Self::MergedClosure),
            _ => None,
        }
    }
}

/// A per-tier score floor for an axis — a `gmeow:AxisThreshold` individual.
#[derive(Debug, Clone, PartialEq)]
pub struct Threshold {
    /// The tier this floor unlocks.
    pub tier_iri: String,
    /// The minimum normalized score (0.0–1.0) required to earn `tier_iri`.
    pub floor: f64,
}

/// One measured quality axis — a `gmeow:QualityAxis` individual with its bindings.
#[derive(Debug, Clone)]
pub struct Axis {
    /// The full axis IRI.
    pub iri: String,
    /// The human label.
    pub label: String,
    /// The measurement-primitive key (`gmeow:axisProducer`) — resolved to a
    /// closed Rust primitive; an unknown key is a hard fail, never a silent skip.
    pub producer: String,
    /// The quality dimension the score is emitted under (`gmeow:axisDimension`).
    pub dimension_iri: String,
    /// The per-tier floors, sorted ascending by floor.
    pub thresholds: Vec<Threshold>,
    /// The advice-ranking weight (`gmeow:axisWeight`) — ranking only, never the meet.
    pub weight: f64,
    /// The read-breadth coeffect (`gmeow:axisContextScope`).
    pub scope: ContextScope,
    /// The opinionated uplift advice (`gmeow:axisAdviceTemplate`).
    pub advice: String,
}

/// A dated, self-cleaning exemption — a `gmeow:AxisExemption` individual.
#[derive(Debug, Clone)]
pub struct Exemption {
    /// The exemption IRI.
    pub iri: String,
    /// The axis it covers.
    pub axis_iri: String,
    /// The doctrine-anchored reason.
    pub reason: String,
    /// The date it was minted (ISO `xsd:date` lexical form).
    pub date: String,
    /// The Rust symbol whose resolution in-repo makes the exemption stale.
    pub producer: String,
}

/// The whole rubric loaded from the slice — axes, tiers, and exemptions.
#[derive(Debug, Clone, Default)]
pub struct Rubric {
    /// The tier ladder, sorted ascending by rank.
    pub tiers: Vec<Tier>,
    /// The quality axes, sorted by IRI for deterministic iteration.
    pub axes: Vec<Axis>,
    /// The dated exemptions.
    pub exemptions: Vec<Exemption>,
}

impl Rubric {
    /// The floor tier (least rank), if the ladder is non-empty.
    #[must_use]
    pub fn bottom_tier(&self) -> Option<&Tier> {
        self.tiers.iter().min()
    }

    /// Look up a tier by IRI.
    #[must_use]
    pub fn tier(&self, iri: &str) -> Option<&Tier> {
        self.tiers.iter().find(|t| t.iri == iri)
    }
}

/// The grade one axis earned on a slice: its measured score and the resulting tier.
#[derive(Debug, Clone)]
pub struct AxisGrade {
    /// The axis IRI.
    pub axis_iri: String,
    /// The normalized measured score in 0.0–1.0.
    pub score: f64,
    /// The tier the score earns (the highest tier whose floor the score meets),
    /// or the bottom tier when it meets no floor.
    pub tier: Tier,
}

/// The full assessment of a slice: the per-axis grade vector (the primary object)
/// plus the roll-up tier (its lossy meet projection).
#[derive(Debug, Clone)]
pub struct SliceAssessment {
    /// The slice IRI (or path key) under assessment.
    pub slice: String,
    /// The per-axis grades — the primary object, never discarded.
    pub grades: Vec<AxisGrade>,
    /// The roll-up tier: the unweighted lattice meet of the axis grades.
    pub rollup: Tier,
}
