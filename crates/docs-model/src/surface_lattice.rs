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
//! greatest. [`crate::surface_lattice::surface_leq`] realizes it, and the
//! `surface_leq_is_the_object_concept_order` test proves it is exactly the concept-lattice
//! order restricted to the object concepts, rather than a second, parallel definition.
//!
//! For concepts, the usual FCA order applies: `(A₁,B₁) ≤ (A₂,B₂) ⟺ A₁ ⊆ A₂ ⟺ B₂ ⊆ B₁`,
//! with `join = ((B₁∩B₂)′, B₁∩B₂)` and `meet = (A₁∩A₂, (A₁∩A₂)′)`.
//! [`crate::surface_lattice::SurfaceConcept`] implements
//! [`gmeow_errors::grade::BoundedLattice`], which requires `Copy + Eq` — hence the two bit
//! masks rather than owned sets.
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
//! * `ALL_SURFACES` is derived from [`crate::formats::DistributionSurface::ALL`]'s length —
//!   the count of capability-BEARING surfaces (4 formats + console = 5). Sizing it to the
//!   whole distribution catalog instead would leave `TOP` unreachable by any join, because the
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{DocFormat, PROJECTION_DAG_EDGES, surface_capabilities};

    fn surface(s: DistributionSurface) -> usize {
        surface_index(s)
    }

    /// The mask widths are derived from the authored vocabularies, never hand-sized.
    #[test]
    fn mask_widths_are_derived_from_the_capability_bearing_surfaces() {
        assert_eq!(SURFACE_COUNT, DocFormat::ALL.len() + 1);
        assert_eq!(SURFACE_COUNT, 5);
        assert_eq!(CAPABILITY_COUNT, 6);
        assert_eq!(ALL_SURFACES.0.count_ones() as usize, SURFACE_COUNT);
        assert_eq!(ALL_CAPS.0.count_ones() as usize, CAPABILITY_COUNT);
    }

    /// The derived incidence agrees, cell for cell, with the owned partition the rest of
    /// the codebase reads — one authority, two encodings.
    #[test]
    fn the_incidence_masks_agree_with_the_owned_partitions() {
        for (index, surface) in DistributionSurface::ALL.into_iter().enumerate() {
            let owned = surface_capabilities(surface);
            assert_eq!(
                AUTHORED_INCIDENCE[index].members(),
                owned.representable,
                "{surface:?} representable mask disagrees with the owned partition"
            );
            let dropped = CapMask(ALL_CAPS.0 & !AUTHORED_INCIDENCE[index].0);
            assert_eq!(
                dropped.members(),
                owned.dropped,
                "{surface:?} dropped mask disagrees with the owned partition"
            );
        }
    }

    /// The lattice has exactly the concepts the authored intents admit. The intents form a
    /// chain (site = all six, mdbook = five, console = four, pdf = snippets = none), so the
    /// lattice is the four-element chain of those distinct closed sets.
    #[test]
    fn the_authored_lattice_has_exactly_four_concepts() {
        let nodes = authored_concepts();
        assert_eq!(
            nodes.len(),
            4,
            "{:?}",
            nodes.iter().map(|c| concept_slugs(*c)).collect::<Vec<_>>()
        );

        let rendered: Vec<(Vec<&str>, Vec<&str>)> =
            nodes.iter().map(|c| concept_slugs(*c)).collect();
        assert_eq!(
            rendered,
            vec![
                (
                    vec!["site"],
                    vec![
                        "search-index",
                        "live-sparql",
                        "interactivity",
                        "live-reasoning",
                        "diagrams",
                        "cross-link-fidelity"
                    ]
                ),
                (
                    vec!["site", "mdbook"],
                    vec![
                        "live-sparql",
                        "interactivity",
                        "live-reasoning",
                        "diagrams",
                        "cross-link-fidelity"
                    ]
                ),
                (
                    vec!["site", "mdbook", "console"],
                    vec!["live-sparql", "interactivity", "live-reasoning", "diagrams"]
                ),
                (vec!["site", "mdbook", "pdf", "snippets", "console"], vec![]),
            ]
        );
    }

    /// Every emitted node really is a formal concept: `extent′ = intent` and
    /// `intent′ = extent`.
    #[test]
    fn every_emitted_node_is_galois_closed() {
        for node in authored_concepts() {
            assert_eq!(
                intent_of(node.extent, &AUTHORED_INCIDENCE),
                node.intent,
                "{node:?} extent′ ≠ intent"
            );
            assert_eq!(
                extent_of(node.intent, &AUTHORED_INCIDENCE),
                node.extent,
                "{node:?} intent′ ≠ extent"
            );
        }
    }

    /// ACCEPTANCE 6: `BOTTOM` and `TOP` satisfy the bounded-lattice laws over the EMITTED
    /// concept set — exhaustively, because the carrier is a four-element finite set.
    #[test]
    fn bounded_lattice_laws_hold_over_the_emitted_concept_set() {
        let nodes = authored_concepts();

        // Trap 1: the bottom is `(M′, M)` = ({site}, ALL_CAPS), NOT (∅, ALL_CAPS).
        assert_eq!(SurfaceConcept::BOTTOM.intent, ALL_CAPS);
        assert_eq!(
            SurfaceConcept::BOTTOM.extent,
            SurfaceMask(1u16 << surface(DistributionSurface::Format(DocFormat::Site))),
            "the bottom concept's extent is the representable-total surface set, not ∅"
        );
        assert_ne!(
            SurfaceConcept::BOTTOM.extent,
            SurfaceMask(0),
            "(∅, ALL_CAPS) is not a formal concept and would break the least-element law"
        );

        // Trap 2: the top's extent is EVERY capability-bearing surface, and its intent is
        // computed (∅ here), so it is reachable and not a hand-sized constant.
        assert_eq!(SurfaceConcept::TOP.extent, ALL_SURFACES);
        assert_eq!(SurfaceConcept::TOP.intent, NO_CAPS);

        // Both bounds are members of the emitted set — a bound outside it is meaningless.
        assert!(
            nodes.contains(&SurfaceConcept::BOTTOM),
            "BOTTOM is not one of the emitted concepts"
        );
        assert!(
            nodes.contains(&SurfaceConcept::TOP),
            "TOP is not one of the emitted concepts"
        );

        for a in &nodes {
            // Least / greatest element laws.
            assert_eq!(
                SurfaceConcept::BOTTOM.join(*a),
                *a,
                "BOTTOM ∨ {a:?} ≠ {a:?}"
            );
            assert_eq!(SurfaceConcept::BOTTOM.meet(*a), SurfaceConcept::BOTTOM);
            assert_eq!(SurfaceConcept::TOP.meet(*a), *a, "TOP ∧ {a:?} ≠ {a:?}");
            assert_eq!(SurfaceConcept::TOP.join(*a), SurfaceConcept::TOP);
            assert!(SurfaceConcept::BOTTOM.leq(*a) && a.leq(SurfaceConcept::TOP));

            // Idempotence.
            assert_eq!(a.join(*a), *a);
            assert_eq!(a.meet(*a), *a);

            for b in &nodes {
                // Closure: the lattice operations never leave the emitted set.
                assert!(
                    nodes.contains(&a.join(*b)),
                    "{a:?} ∨ {b:?} left the lattice"
                );
                assert!(
                    nodes.contains(&a.meet(*b)),
                    "{a:?} ∧ {b:?} left the lattice"
                );
                // Commutativity + absorption.
                assert_eq!(a.join(*b), b.join(*a));
                assert_eq!(a.meet(*b), b.meet(*a));
                assert_eq!(a.join(a.meet(*b)), *a);
                assert_eq!(a.meet(a.join(*b)), *a);
                // The order agrees on both sides.
                assert_eq!(a.leq(*b), a.meet(*b) == *a);
                assert_eq!(a.leq(*b), a.extent.is_subset_of(b.extent));
                for c in &nodes {
                    assert_eq!(a.join(b.join(*c)), a.join(*b).join(*c));
                    assert_eq!(a.meet(b.meet(*c)), a.meet(*b).meet(*c));
                }
            }
        }
    }

    /// The surface order is not a second definition: it IS the concept order restricted to
    /// the object concepts `γ(g)`.
    #[test]
    fn surface_leq_is_the_object_concept_order() {
        for lesser in DistributionSurface::ALL {
            for greater in DistributionSurface::ALL {
                let by_definition = surface_leq(lesser, greater, &AUTHORED_INCIDENCE);
                let by_lattice = SurfaceConcept::of_surface(lesser, &AUTHORED_INCIDENCE)
                    .leq(SurfaceConcept::of_surface(greater, &AUTHORED_INCIDENCE));
                assert_eq!(
                    by_definition, by_lattice,
                    "{lesser:?} ≤ {greater:?}: definition {by_definition} vs lattice {by_lattice}"
                );
            }
        }
    }

    /// ACCEPTANCE 4: the DERIVED order reproduces every hand-declared provenance edge and
    /// the capability chain, with BOTH strictness directions on the console's two
    /// neighbours.
    #[test]
    fn the_derived_order_reproduces_every_declared_edge_and_chain() {
        // Every declared PROVENANCE covering edge is also a capability-order relation.
        for &(src, tgt) in PROJECTION_DAG_EDGES {
            assert!(
                surface_leq(
                    DistributionSurface::Format(src),
                    DistributionSurface::Format(tgt),
                    &AUTHORED_INCIDENCE
                ),
                "declared DAG edge {src:?} → {tgt:?} is not reproduced by the derived order"
            );
        }

        let site = DistributionSurface::Format(DocFormat::Site);
        let mdbook = DistributionSurface::Format(DocFormat::Mdbook);
        let pdf = DistributionSurface::Format(DocFormat::Pdf);
        let snippets = DistributionSurface::Format(DocFormat::Snippets);
        let console = DistributionSurface::Console;

        // The full authored chain, derived.
        for (lower, upper) in [
            (site, mdbook),
            (mdbook, console),
            (console, pdf),
            (pdf, snippets),
        ] {
            assert!(
                surface_leq(lower, upper, &AUTHORED_INCIDENCE),
                "{lower:?} ≤ {upper:?} is not derived"
            );
        }
        // pdf and snippets are order-EQUIVALENT (identical partitions), not strict.
        assert!(surface_leq(snippets, pdf, &AUTHORED_INCIDENCE));

        // dropped(mdbook) ⊊ dropped(console) ⊊ dropped(pdf), both strictness directions.
        let dropped =
            |s: DistributionSurface| CapMask(ALL_CAPS.0 & !AUTHORED_INCIDENCE[surface(s)].0);
        assert!(dropped(mdbook).is_subset_of(dropped(console)));
        assert!(!dropped(console).is_subset_of(dropped(mdbook)));
        assert_ne!(dropped(mdbook), dropped(console));
        assert!(dropped(console).is_subset_of(dropped(pdf)));
        assert!(!dropped(pdf).is_subset_of(dropped(console)));
        assert_ne!(dropped(console), dropped(pdf));
    }

    /// ACCEPTANCE 5, the perturbation negative test: flipping ONE incidence cell changes the
    /// derived order. Giving the pdf a bundled `SearchIndex` — the one capability the
    /// console drops — makes the two incomparable, breaks the `console ≤ pdf` chain link,
    /// and moves the concept lattice, its basis, and its Hasse diagram.
    #[test]
    fn flipping_one_incidence_cell_changes_the_derived_order() {
        let mut perturbed = AUTHORED_INCIDENCE;
        let pdf = surface_index(DistributionSurface::Format(DocFormat::Pdf));
        assert!(
            perturbed[pdf].0 & (1u8 << Capability::SearchIndex.index()) == 0,
            "the pdf must not already carry a search index, or this test proves nothing"
        );
        perturbed[pdf] = CapMask(perturbed[pdf].0 | (1u8 << Capability::SearchIndex.index()));

        // The order fact that HELD under the authored incidence now FAILS.
        assert!(surface_leq(
            DistributionSurface::Console,
            DistributionSurface::Format(DocFormat::Pdf),
            &AUTHORED_INCIDENCE
        ));
        assert!(
            !surface_leq(
                DistributionSurface::Console,
                DistributionSurface::Format(DocFormat::Pdf),
                &perturbed
            ),
            "flipping pdf/search-index must break console ≤ pdf — otherwise the order is \
             not derived from the incidence at all"
        );
        // …and the reverse does not silently take its place: the two become incomparable.
        assert!(!surface_leq(
            DistributionSurface::Format(DocFormat::Pdf),
            DistributionSurface::Console,
            &perturbed
        ));

        // The lattice itself moves: the concept set and the DG basis both change.
        assert_ne!(
            concepts(&perturbed),
            authored_concepts(),
            "a flipped cell must move the concept lattice"
        );
        assert_ne!(
            dg_basis(&perturbed),
            authored_dg_basis(),
            "a flipped cell must move the implication basis"
        );
        assert_ne!(
            concept_hasse_edges(&perturbed),
            concept_hasse_edges(&AUTHORED_INCIDENCE),
            "a flipped cell must move the Hasse diagram the console renders"
        );
    }

    /// A concept of the perturbed context, built exactly as the perturbation test builds
    /// its lattice: `({pdf}″, {pdf}′)` where the pdf has been given a bundled search index.
    fn foreign_context_concept() -> (SurfaceConcept, [CapMask; SURFACE_COUNT]) {
        let mut perturbed = AUTHORED_INCIDENCE;
        let pdf = surface_index(DistributionSurface::Format(DocFormat::Pdf));
        perturbed[pdf] = CapMask(perturbed[pdf].0 | (1u8 << Capability::SearchIndex.index()));
        let concept =
            SurfaceConcept::of_surface(DistributionSurface::Format(DocFormat::Pdf), &perturbed);
        (concept, perturbed)
    }

    /// The formal CONTEXT of a concept is decidable, not assumed: a concept of the
    /// perturbed incidence is provably not one of the authored incidence.
    #[test]
    fn a_perturbed_context_concept_is_not_a_concept_of_the_authored_context() {
        let (foreign, perturbed) = foreign_context_concept();
        assert!(
            foreign.is_closed_under(&perturbed),
            "{foreign:?} must be Galois-closed in the context it was derived from"
        );
        assert!(
            !foreign.is_closed_under(&AUTHORED_INCIDENCE),
            "{foreign:?} must NOT be a concept of the authored context, or this test — and \
             the refusal it backs — proves nothing"
        );
    }

    /// …and the bounded-lattice operations REFUSE it rather than silently closing it
    /// against the authored context. Before the refusal, `join` returned a well-formed
    /// concept of a lattice neither operand belonged to.
    #[test]
    #[should_panic(expected = "is not a concept of the incidence it is being combined under")]
    fn joining_a_foreign_context_concept_under_the_authored_lattice_is_refused() {
        let (foreign, _) = foreign_context_concept();
        let _ = foreign.join(SurfaceConcept::TOP);
    }

    /// The same for `meet` — both halves of the lattice are guarded, not just one.
    #[test]
    #[should_panic(expected = "is not a concept of the incidence it is being combined under")]
    fn meeting_a_foreign_context_concept_under_the_authored_lattice_is_refused() {
        let (foreign, _) = foreign_context_concept();
        let _ = foreign.meet(SurfaceConcept::BOTTOM);
    }

    /// The explicit-context operations are the supported way to work in ANY context: over
    /// the perturbed incidence, the perturbed bounds and the perturbed operations satisfy
    /// the same bounded-lattice laws the authored ones do.
    #[test]
    fn the_explicit_context_operations_form_a_lattice_over_the_perturbed_incidence() {
        let (_, perturbed) = foreign_context_concept();
        let bottom = SurfaceConcept::bottom_in(&perturbed);
        let top = SurfaceConcept::top_in(&perturbed);
        let nodes = concepts(&perturbed);
        assert!(nodes.contains(&bottom) && nodes.contains(&top));
        for a in &nodes {
            assert_eq!(bottom.join_in(*a, &perturbed), *a);
            assert_eq!(top.meet_in(*a, &perturbed), *a);
            for b in &nodes {
                assert!(nodes.contains(&a.join_in(*b, &perturbed)));
                assert!(nodes.contains(&a.meet_in(*b, &perturbed)));
                assert_eq!(a.join_in(*b, &perturbed), b.join_in(*a, &perturbed));
                assert_eq!(a.meet_in(*b, &perturbed), b.meet_in(*a, &perturbed));
            }
        }
    }

    /// The DG basis is COMPLETE and SOUND over the authored incidence: every implication
    /// holds in the context, and iterating the basis to a fixpoint reproduces the true
    /// closure of every attribute set.
    #[test]
    fn the_dg_basis_is_sound_and_complete() {
        let basis = authored_dg_basis();
        assert!(!basis.is_empty());

        // Sound: every law holds of every surface.
        for implication in &basis {
            for intent in AUTHORED_INCIDENCE {
                if implication.premise.is_subset_of(intent) {
                    assert!(
                        implication.conclusion.is_subset_of(intent),
                        "{implication:?} is violated by a surface with intent {intent:?}"
                    );
                }
            }
        }

        // Complete: basis-closure == Galois closure, for every attribute set.
        for bits in 0u16..(1u16 << CAPABILITY_COUNT) {
            let start = CapMask(bits as u8);
            let mut current = start;
            loop {
                let mut next = current;
                for implication in &basis {
                    if implication.premise.is_subset_of(next) {
                        next = CapMask(next.0 | implication.conclusion.0);
                    }
                }
                if next == current {
                    break;
                }
                current = next;
            }
            let galois = intent_of(extent_of(start, &AUTHORED_INCIDENCE), &AUTHORED_INCIDENCE);
            assert_eq!(
                current, galois,
                "basis closure of {start:?} disagrees with the Galois closure"
            );
        }
    }

    /// The authored basis, pinned by its rendered content so a silent incidence edit is
    /// legible in the diff rather than a bare count change.
    #[test]
    fn the_authored_dg_basis_is_the_six_singleton_laws() {
        let rendered: Vec<(Vec<&str>, Vec<&str>)> = authored_dg_basis()
            .into_iter()
            .map(|i| {
                (
                    i.premise.members().iter().map(|c| c.slug()).collect(),
                    i.conclusion.members().iter().map(|c| c.slug()).collect(),
                )
            })
            .collect();
        assert_eq!(
            rendered,
            vec![
                (
                    vec!["search-index"],
                    vec![
                        "live-sparql",
                        "interactivity",
                        "live-reasoning",
                        "diagrams",
                        "cross-link-fidelity"
                    ]
                ),
                (
                    vec!["live-sparql"],
                    vec!["interactivity", "live-reasoning", "diagrams"]
                ),
                (
                    vec!["interactivity"],
                    vec!["live-sparql", "live-reasoning", "diagrams"]
                ),
                (
                    vec!["live-reasoning"],
                    vec!["live-sparql", "interactivity", "diagrams"]
                ),
                (
                    vec!["diagrams"],
                    vec!["live-sparql", "interactivity", "live-reasoning"]
                ),
                (
                    vec!["cross-link-fidelity"],
                    vec!["live-sparql", "interactivity", "live-reasoning", "diagrams"]
                ),
            ]
        );
    }

    /// Over the AUTHORED incidence every law is witnessed by at least one surface, so the
    /// unrealized subset is empty. The predicate is nonetheless live: a context in which a
    /// premise has an empty extent marks its law as a vacuous-truth boundary.
    #[test]
    fn unrealized_laws_are_the_vacuously_true_ones() {
        for implication in authored_dg_basis() {
            assert!(
                !implication.is_unrealized(&AUTHORED_INCIDENCE),
                "{implication:?} is vacuous over the authored incidence — every authored law \
                 must have a witnessing surface"
            );
        }

        // A synthetic context where a premise IS unwitnessed: two surfaces that split the
        // attributes, so `{search-index, live-sparql}` has an empty extent.
        let split = [
            CapMask(1 << Capability::SearchIndex.index()),
            CapMask(1 << Capability::LiveSparql.index()),
            CapMask(0),
            CapMask(0),
            CapMask(0),
        ];
        let unrealized: Vec<Implication> = dg_basis(&split)
            .into_iter()
            .filter(|i| i.is_unrealized(&split))
            .collect();
        assert!(
            !unrealized.is_empty(),
            "the vacuous-law predicate must be reachable, or it is dead branch"
        );
        for implication in unrealized {
            assert_eq!(extent_of(implication.premise, &split), SurfaceMask(0));
        }
    }

    /// The Hasse edges of the authored four-element chain.
    #[test]
    fn the_authored_hasse_diagram_is_the_three_chain_edges() {
        let edges = concept_hasse_edges(&AUTHORED_INCIDENCE);
        assert_eq!(edges.len(), 3, "{edges:?}");
        for (lower, upper) in &edges {
            assert!(lower.extent.is_subset_of(upper.extent));
            assert_ne!(lower, upper);
        }
    }

    #[test]
    fn intents_by_slug_reports_every_surface() {
        let map = intents_by_slug(&AUTHORED_INCIDENCE);
        assert_eq!(map.len(), SURFACE_COUNT);
        assert_eq!(map["pdf"], Vec::<&str>::new());
        assert_eq!(map["console"].len(), 4);
    }
}
