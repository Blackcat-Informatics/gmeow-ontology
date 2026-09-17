// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The formal-concept lattice DERIVED from the `Surface × Capability` incidence.
//!
//! The object set is [`crate::formats::DistributionSurface::ALL`] (the four rendered
//! [`crate::formats::DocFormat`]s plus the interactive console), the attribute set is
//! [`crate::formats::Capability::ALL`], and the incidence is the single authored table
//! [`crate::formats::DistributionSurface::dropped`]. Nothing here re-authors a
//! cell: every order fact below is *computed* from that table by the standard Galois
//! connection, so an edit to the incidence moves the lattice and the tests that pin it.
//!
//! # The order
//!
//! For surfaces, `S ≤ T ⟺ representable(T) ⊆ representable(S)` — the LOSS order, in which
//! the lossless site is the least element and the print PDF / flat snippets are the
//! greatest. [`crate::surface_lattice::surface_leq`] realizes it, and
//! `surface_leq_is_the_object_concept_order`
//! proves it is exactly the concept-lattice order restricted to the object concepts, rather
//! than a second, parallel definition.
//!
//! For concepts, the usual FCA order applies: `(A₁,B₁) ≤ (A₂,B₂) ⟺ A₁ ⊆ A₂ ⟺ B₂ ⊆ B₁`,
//! with `join = ((B₁∩B₂)′, B₁∩B₂)` and `meet = (A₁∩A₂, (A₁∩A₂)′)`.
//! [`crate::surface_lattice::SurfaceConcept`] implements
//! [`gmeow_errors::grade::BoundedLattice`], which requires `Copy + Eq` — hence the two bit masks
//! rather than owned sets.
//!
//! # This order is NOT the projection DAG
//!
//! [`crate::formats::PROJECTION_DAG_EDGES`] is the hand-declared PROVENANCE order (which
//! artifact is rendered from which). This module is the DERIVED CAPABILITY order (which
//! surface represents more). Neither is a function of the other, and both are gated
//! independently — see the `PROJECTION_DAG_EDGES` doc comment.
//!
//! # The two bound traps
//!
//! * `BOTTOM` is `(M′, M)` — the objects carrying EVERY attribute, paired with all of them.
//!   Since `site` drops nothing, that is `({site}, ALL_CAPS)`, **not** `(∅, ALL_CAPS)`:
//!   an empty extent there is not a formal concept at all, would never appear in
//!   [`crate::surface_lattice::concepts`], and would break the least-element law.
//! * `ALL_SURFACES` is derived from [`crate::formats::DistributionSurface::ALL`]'s length — the count of
//!   capability-BEARING surfaces (4 formats + console = 5). Sizing it to the whole
//!   distribution catalog instead would leave `TOP` unreachable by any join, because the
//!   serialization slugs carry no capability partition and so can never enter an extent.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::grade::BoundedLattice;

use crate::formats::{Capability, DistributionSurface};

/// The number of capability-bearing surfaces — the width of [`SurfaceMask`].
///
/// Derived from [`DistributionSurface::ALL`], NOT from the distribution catalog's slug
/// count: the serialization distributions carry no capability partition, so a wider mask
/// would make [`SurfaceConcept::TOP`] unreachable by any join.
pub const SURFACE_COUNT: usize = DistributionSurface::ALL.len();

/// The number of capabilities — the width of [`CapMask`].
pub const CAPABILITY_COUNT: usize = Capability::ALL.len();

/// A set of surfaces, one bit per index into [`DistributionSurface::ALL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceMask(pub u16);

/// A set of capabilities, one bit per [`Capability::index`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapMask(pub u8);

/// Every surface.
pub const ALL_SURFACES: SurfaceMask = SurfaceMask(((1u32 << SURFACE_COUNT) - 1) as u16);

/// Every capability.
pub const ALL_CAPS: CapMask = CapMask(((1u16 << CAPABILITY_COUNT) - 1) as u8);

/// No capability — the top concept's intent over this context.
pub const NO_CAPS: CapMask = CapMask(0);

impl SurfaceMask {
    /// The surfaces in this mask, in [`DistributionSurface::ALL`] order.
    pub fn members(self) -> Vec<DistributionSurface> {
        DistributionSurface::ALL
            .into_iter()
            .enumerate()
            .filter(|(index, _)| self.0 & (1u16 << index) != 0)
            .map(|(_, surface)| surface)
            .collect()
    }

    /// Whether this mask is a subset of `other`.
    pub const fn is_subset_of(self, other: SurfaceMask) -> bool {
        self.0 & other.0 == self.0
    }
}

impl CapMask {
    /// The capabilities in this mask, in [`Capability::ALL`] order.
    pub fn members(self) -> Vec<Capability> {
        Capability::ALL
            .into_iter()
            .filter(|cap| self.0 & (1u8 << cap.index()) != 0)
            .collect()
    }

    /// Whether this mask is a subset of `other`.
    pub const fn is_subset_of(self, other: CapMask) -> bool {
        self.0 & other.0 == self.0
    }
}

/// The bit position of a surface within [`SurfaceMask`] — its index in
/// [`DistributionSurface::ALL`]. `const fn` so the authored incidence can be folded into
/// the lattice bounds at compile time.
const fn surface_index(surface: DistributionSurface) -> usize {
    let mut index = 0;
    while index < SURFACE_COUNT {
        // `DistributionSurface` is `Copy` and structurally comparable by slug position;
        // a const-context `==` needs `PartialEq`, which is not const, so compare the
        // discriminant-carrying pair by hand.
        if same_surface(DistributionSurface::ALL[index], surface) {
            return index;
        }
        index += 1;
    }
    // Unreachable: `DistributionSurface::ALL` is total over the enum by construction, and
    // `every_format_is_a_surface_and_the_console_is_the_only_extra` proves it.
    panic!("DistributionSurface::ALL is not total over DistributionSurface");
}

/// `const`-context equality for [`DistributionSurface`] (derived `PartialEq` is not const).
const fn same_surface(left: DistributionSurface, right: DistributionSurface) -> bool {
    match (left, right) {
        (DistributionSurface::Console, DistributionSurface::Console) => true,
        (DistributionSurface::Format(a), DistributionSurface::Format(b)) => a as u8 == b as u8,
        _ => false,
    }
}

/// The capabilities a surface REPRESENTS, as a mask — the incidence row, derived from the
/// authored [`DistributionSurface::dropped`] table.
pub const fn intent_of_surface(surface: DistributionSurface) -> CapMask {
    let dropped = surface.dropped();
    let mut mask = ALL_CAPS.0;
    let mut index = 0;
    while index < dropped.len() {
        mask &= !(1u8 << dropped[index].index());
        index += 1;
    }
    CapMask(mask)
}

/// The AUTHORED incidence, one intent per surface in [`DistributionSurface::ALL`] order.
/// Every derivation below runs over an incidence slice of this shape, so a test can perturb
/// one cell and re-derive without touching the authored table.
pub const AUTHORED_INCIDENCE: [CapMask; SURFACE_COUNT] = authored_incidence();

const fn authored_incidence() -> [CapMask; SURFACE_COUNT] {
    let mut out = [NO_CAPS; SURFACE_COUNT];
    let mut index = 0;
    while index < SURFACE_COUNT {
        out[index] = intent_of_surface(DistributionSurface::ALL[index]);
        index += 1;
    }
    out
}

/// The Galois `′` from a surface set to the capabilities ALL of them represent.
pub const fn intent_of(extent: SurfaceMask, incidence: &[CapMask]) -> CapMask {
    let mut mask = ALL_CAPS.0;
    let mut index = 0;
    while index < incidence.len() {
        if extent.0 & (1u16 << index) != 0 {
            mask &= incidence[index].0;
        }
        index += 1;
    }
    CapMask(mask)
}

/// The Galois `′` from a capability set to the surfaces that represent ALL of them.
pub const fn extent_of(intent: CapMask, incidence: &[CapMask]) -> SurfaceMask {
    let mut mask = 0u16;
    let mut index = 0;
    while index < incidence.len() {
        if intent.0 & !incidence[index].0 == 0 {
            mask |= 1u16 << index;
        }
        index += 1;
    }
    SurfaceMask(mask)
}

/// One node of the concept lattice: a closed `(extent, intent)` pair.
///
/// `Copy + Eq` because [`BoundedLattice`] requires it — which is exactly why the two sides
/// are bit masks rather than owned collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceConcept {
    /// The surfaces in the concept's extent.
    pub extent: SurfaceMask,
    /// The capabilities in the concept's intent.
    pub intent: CapMask,
}

impl SurfaceConcept {
    /// The concept generated by an arbitrary surface set: `(A″, A′)`.
    pub const fn from_extent(extent: SurfaceMask, incidence: &[CapMask]) -> Self {
        let intent = intent_of(extent, incidence);
        Self {
            extent: extent_of(intent, incidence),
            intent,
        }
    }

    /// The object concept of a single surface, `γ(g) = ({g}″, {g}′)`.
    pub fn of_surface(surface: DistributionSurface, incidence: &[CapMask]) -> Self {
        Self::from_extent(SurfaceMask(1u16 << surface_index(surface)), incidence)
    }

    /// Whether this pair really is a formal concept OF `incidence`: `extent′ = intent`
    /// and `intent′ = extent`.
    ///
    /// The membership test that makes the context of a concept decidable rather than
    /// assumed. A concept derived from a perturbed context is (in general) not closed
    /// under the authored one, which is exactly what [`join_in`](Self::join_in) and
    /// [`meet_in`](Self::meet_in) refuse.
    #[must_use]
    pub fn is_closed_under(self, incidence: &[CapMask]) -> bool {
        intent_of(self.extent, incidence) == self.intent
            && extent_of(self.intent, incidence) == self.extent
    }

    /// The least concept of `incidence`: `(M′, M)` — the surfaces carrying EVERY
    /// capability, with all of them. Over the authored context that is
    /// `({site}, ALL_CAPS)`; `(∅, ALL_CAPS)` is not a concept.
    #[must_use]
    pub const fn bottom_in(incidence: &[CapMask]) -> Self {
        Self {
            extent: extent_of(ALL_CAPS, incidence),
            intent: ALL_CAPS,
        }
    }

    /// The greatest concept of `incidence`: `(G, G′)` — every surface, with the
    /// capabilities all of them share (none over the authored context, since the pdf and
    /// the snippets represent nothing).
    #[must_use]
    pub const fn top_in(incidence: &[CapMask]) -> Self {
        Self {
            extent: ALL_SURFACES,
            intent: intent_of(ALL_SURFACES, incidence),
        }
    }

    /// `((B₁∩B₂)′, B₁∩B₂)` — the join IN `incidence`.
    ///
    /// # Panics
    ///
    /// If either operand is not a concept of `incidence`. Closing a foreign-context pair
    /// against this one yields a well-formed-LOOKING concept of the wrong lattice, which
    /// is the silent-misleading failure this refusal exists to make impossible.
    #[must_use]
    pub fn join_in(self, other: Self, incidence: &[CapMask]) -> Self {
        self.assert_same_context(other, incidence, "join");
        let intent = CapMask(self.intent.0 & other.intent.0);
        Self {
            extent: extent_of(intent, incidence),
            intent,
        }
    }

    /// `(A₁∩A₂, (A₁∩A₂)′)` — the meet IN `incidence`.
    ///
    /// # Panics
    ///
    /// If either operand is not a concept of `incidence` — see [`join_in`](Self::join_in).
    #[must_use]
    pub fn meet_in(self, other: Self, incidence: &[CapMask]) -> Self {
        self.assert_same_context(other, incidence, "meet");
        let extent = SurfaceMask(self.extent.0 & other.extent.0);
        Self {
            extent,
            intent: intent_of(extent, incidence),
        }
    }

    fn assert_same_context(self, other: Self, incidence: &[CapMask], op: &str) {
        for operand in [self, other] {
            assert!(
                operand.is_closed_under(incidence),
                "surface-lattice {op}: {operand:?} is not a concept of the incidence it is \
                 being combined under ({incidence:?}) — a concept derived from a DIFFERENT \
                 formal context cannot be closed against this one. Use the `*_in` operations \
                 with the incidence the concept came from."
            );
        }
    }
}

/// The bounded-lattice instance is the AUTHORED context's, and ONLY the authored
/// context's.
///
/// [`BoundedLattice`] carries no context parameter — `BOTTOM`/`TOP` are associated
/// consts and `join`/`meet` are binary — so this impl fixes the incidence to
/// [`AUTHORED_INCIDENCE`]. That used to be a silent choice: [`SurfaceConcept::from_extent`]
/// and [`SurfaceConcept::of_surface`] accept an ARBITRARY incidence, so a concept derived
/// from a perturbed context (as `flipping_one_incidence_cell_changes_the_derived_order`
/// constructs) could be fed to `join`/`meet` and come back closed against a context it
/// never belonged to. It is now enforced: every operation checks that its operands are
/// concepts of the incidence it closes against, and the context-carrying
/// [`SurfaceConcept::join_in`] / [`SurfaceConcept::meet_in`] /
/// [`SurfaceConcept::bottom_in`] / [`SurfaceConcept::top_in`] are the operations to use
/// for any other context.
impl BoundedLattice for SurfaceConcept {
    const BOTTOM: Self = Self::bottom_in(&AUTHORED_INCIDENCE);
    const TOP: Self = Self::top_in(&AUTHORED_INCIDENCE);

    fn join(self, other: Self) -> Self {
        self.join_in(other, &AUTHORED_INCIDENCE)
    }

    fn meet(self, other: Self) -> Self {
        self.meet_in(other, &AUTHORED_INCIDENCE)
    }
}

/// Every formal concept of an incidence, sorted by `(extent, intent)`.
///
/// Exhaustive by construction: every concept's extent is `A″` for some `A ⊆ G`, and the
/// object sets are enumerated in full (`2^SURFACE_COUNT` = 32 over the authored context).
pub fn concepts(incidence: &[CapMask]) -> Vec<SurfaceConcept> {
    let width = incidence.len();
    let mut out: BTreeSet<SurfaceConcept> = BTreeSet::new();
    for bits in 0u32..(1u32 << width) {
        out.insert(SurfaceConcept::from_extent(
            SurfaceMask(bits as u16),
            incidence,
        ));
    }
    out.into_iter().collect()
}

/// The concept lattice of the AUTHORED incidence.
pub fn authored_concepts() -> Vec<SurfaceConcept> {
    concepts(&AUTHORED_INCIDENCE)
}

/// One implication of the Duquenne–Guigues (canonical / stem) basis: `premise → conclusion`,
/// where `conclusion = premise″ ∖ premise` and `premise` is pseudo-closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Implication {
    /// The pseudo-closed premise.
    pub premise: CapMask,
    /// The attributes the premise forces, minus the premise itself. Never empty.
    pub conclusion: CapMask,
}

impl Implication {
    /// Whether this law is only VACUOUSLY true over the incidence it was derived from: no
    /// surface represents the whole premise, so nothing in the authored context witnesses
    /// it. Such a law is an honest expressiveness gap rather than a grounded catalog fact,
    /// and the emitter marks it with `logic:expressivenessBoundary`.
    pub fn is_unrealized(&self, incidence: &[CapMask]) -> bool {
        extent_of(self.premise, incidence) == SurfaceMask(0)
    }
}

/// The Duquenne–Guigues basis of an incidence, sorted by `(premise, conclusion)`.
///
/// Computed by the textbook induction on set size: `P` is pseudo-closed iff `P ≠ P″` and
/// every pseudo-closed `Q ⊊ P` has `Q″ ⊆ P`. Because `Q ⊊ P` forces `|Q| < |P|`, walking
/// the `2^CAPABILITY_COUNT` attribute subsets in increasing popcount order decides each
/// candidate against the pseudo-closed sets already found.
pub fn dg_basis(incidence: &[CapMask]) -> Vec<Implication> {
    let closure = |set: CapMask| intent_of(extent_of(set, incidence), incidence);

    let mut candidates: Vec<u8> = (0u16..(1u16 << CAPABILITY_COUNT))
        .map(|b| b as u8)
        .collect();
    candidates.sort_by_key(|bits| (bits.count_ones(), *bits));

    let mut pseudo_closed: Vec<CapMask> = Vec::new();
    for bits in candidates {
        let candidate = CapMask(bits);
        let closed = closure(candidate);
        if closed == candidate {
            continue; // closed, not pseudo-closed
        }
        let admissible = pseudo_closed.iter().all(|q| {
            // Only PROPER subsets constrain the candidate.
            !(q.is_subset_of(candidate) && *q != candidate) || closure(*q).is_subset_of(candidate)
        });
        if admissible {
            pseudo_closed.push(candidate);
        }
    }

    let mut out: Vec<Implication> = pseudo_closed
        .into_iter()
        .map(|premise| Implication {
            premise,
            conclusion: CapMask(closure(premise).0 & !premise.0),
        })
        .collect();
    out.sort();
    out
}

/// The Duquenne–Guigues basis of the AUTHORED incidence.
pub fn authored_dg_basis() -> Vec<Implication> {
    dg_basis(&AUTHORED_INCIDENCE)
}

/// The DERIVED capability order over surfaces: `S ≤ T ⟺ representable(T) ⊆ representable(S)`.
///
/// Read as a loss order — the richer surface is the SMALLER element, so the lossless site is
/// the least and the print pdf / flat snippets the greatest.
pub fn surface_leq(
    lesser: DistributionSurface,
    greater: DistributionSurface,
    incidence: &[CapMask],
) -> bool {
    let l = incidence[surface_index(lesser)];
    let g = incidence[surface_index(greater)];
    g.is_subset_of(l)
}

/// The covering (Hasse) edges of the derived concept order, sorted — the edge set the
/// console's rendered lattice diagram draws.
pub fn concept_hasse_edges(incidence: &[CapMask]) -> Vec<(SurfaceConcept, SurfaceConcept)> {
    let nodes = concepts(incidence);
    let leq = |a: &SurfaceConcept, b: &SurfaceConcept| a.extent.is_subset_of(b.extent);
    let mut out: Vec<(SurfaceConcept, SurfaceConcept)> = Vec::new();
    for lower in &nodes {
        for upper in &nodes {
            if lower == upper || !leq(lower, upper) {
                continue;
            }
            let covered = !nodes
                .iter()
                .any(|mid| mid != lower && mid != upper && leq(lower, mid) && leq(mid, upper));
            if covered {
                out.push((*lower, *upper));
            }
        }
    }
    out.sort();
    out
}

/// A stable, human-readable rendering of a concept as `extent-slugs | intent-slugs`, used
/// by the catalog emitter to mint a content-addressed subject name and by the tests to
/// report a mismatch legibly.
pub fn concept_slugs(concept: SurfaceConcept) -> (Vec<&'static str>, Vec<&'static str>) {
    (
        concept.extent.members().iter().map(|s| s.slug()).collect(),
        concept.intent.members().iter().map(|c| c.slug()).collect(),
    )
}

/// The per-surface intent map, keyed by slug — a convenience for reporting.
pub fn intents_by_slug(incidence: &[CapMask]) -> BTreeMap<&'static str, Vec<&'static str>> {
    DistributionSurface::ALL
        .into_iter()
        .enumerate()
        .map(|(index, surface)| {
            (
                surface.slug(),
                incidence[index]
                    .members()
                    .iter()
                    .map(|c| c.slug())
                    .collect(),
            )
        })
        .collect()
}

#[path = "surface_lattice.tests.rs"]
#[cfg(test)]
mod tests;
