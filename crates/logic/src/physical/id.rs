// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Branded niche IDs for the engine's entity classes.
//!
//! # Doctrine
//!
//! Every dense entity handle in the engine — an interned term, a predicate, a
//! rule, a materialized row — is an [`Id<C>`]: a `NonZeroU32` in a `PhantomData`
//! brand.  The brand `C` makes IDs of different classes DISTINCT TYPES, so a
//! [`TermId`] can never be passed where a [`PredId`] is expected — cross-class ID
//! confusion is a compile error, not a runtime bug.  The `NonZeroU32` niche keeps
//! `Option<Id<C>>` pointer-width for free (a `None` term/row slot costs nothing
//! extra).
//!
//! # One definition, two homes
//!
//! [`Id`] itself, and the three brands the shared term arena mints
//! ([`TermId`]/[`NodeId`]/[`MetaId`]), live in [`gmeow_term_arena::engine`] — the arena
//! moved out of this runtime so a front-end can intern terms without linking the
//! reasoner, and its handles have to travel with it.  They are RE-EXPORTED here, not
//! redefined: there is exactly one `Id<C>` in the workspace.  The brands below
//! ([`Pred`]/[`Rule`]/[`Row`]) and [`TermRef`] are engine-only classes with no arena
//! meaning, so they stay.
//!
//! # Ordering (read this before sorting on an `Id`)
//!
//! [`Id`]'s [`Ord`] is by RAW INDEX — i.e. MINT ORDER (insertion order within the
//! space that minted it).  Mint order is **meaningless for emission**: two runs
//! that intern the same terms in the same sequence mint the same ids, but the id
//! integers carry no lexical meaning.  It is therefore used ONLY where the code
//! already sorts on mint order — e.g. the galloping leapfrog intersection
//! ([`crate::physical::cursor`]) of `Vec`-indexed row buckets that hold row indices
//! in insertion order.  Every emission / commit /
//! budget-charge ordering ALWAYS derives from the resolved lexical surface at the
//! sorted round commit — NEVER from `Id` order.  An `Id` integer is never
//! serialized and never hashed for provenance (content hashing stays over the
//! `TermValue` / N3 surfaces in [`crate::provenance`]).

use std::fmt;

/// The one branded niche-ID definition, minted by the shared term arena.
pub(crate) use gmeow_term_arena::engine::Id;

/// The engine's dense per-interner atomic-term handle — the arena's own brand.
pub(crate) use gmeow_term_arena::engine::TermId;

/// The dense per-DAG structured-term node handle minted by the shared arena.
///
/// Because bound occurrences are locally-nameless de-Bruijn refs and every node is
/// content-keyed, alpha-equivalent terms hash-cons to the SAME `NodeId` — structural
/// `NodeId` equality *is* alpha-equivalence.  Like every [`Id`], it is a runtime handle
/// only: never serialized, never hashed for provenance (the arena's `ContentKey` is the
/// persistent identity).
pub(crate) use gmeow_term_arena::engine::NodeId;

/// The dense per-DAG unification-metavariable handle minted by the shared arena.
///
/// Identity-bearing: two occurrences of the same metavariable share one `MetaId` (and so
/// one [`NodeId`]), while a fresh metavariable mints a new one.
pub(crate) use gmeow_term_arena::engine::MetaId;

// ── Engine-only brand markers (uninhabited: pure type-level tags, never constructed) ──

/// Brand: an interned predicate IRI handle. See [`PredId`].
pub(crate) enum Pred {}
/// Brand: a rule handle. See [`RuleId`].
pub(crate) enum Rule {}
/// Brand: a materialized-row handle. See [`RowId`].
pub(crate) enum Row {}

/// The argument handle every arena'd row tuple uses.
///
/// It is ALWAYS an atomic interned [`TermId`] — a single wrapping variant, a plain
/// newtype, NOT a two-variant enum.  It is the **seam** the structured-term DAG work
/// extends: when function-symbol / proof-object terms land, a second
/// variant (a DAG-node offset into the persistent term arena) is *added here*, so the
/// row-tuple substrate is additive to that future work rather than a rewrite.
///
/// The second variant has NO consumer in this crate, so it is not built — a one-armed
/// enum with a dead arm is optionality this codebase forbids.  The seam is documented
/// as a plain newtype, not pre-built as dead machinery.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TermRef(TermId);

impl TermRef {
    /// Wrap an atomic interned term as a row-tuple argument handle.
    #[inline]
    pub(crate) fn term(id: TermId) -> Self {
        Self(id)
    }

    /// The interned [`TermId`] this handle addresses.
    ///
    /// Total today (the newtype has one variant); when a future DAG-offset
    /// variant lands this becomes the atomic-term projection.
    #[inline]
    pub(crate) fn id(self) -> TermId {
        self.0
    }
}

impl fmt::Debug for TermRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TermRef({})", self.0.index())
    }
}
/// A dense per-store predicate-IRI handle.
pub(crate) type PredId = Id<Pred>;
/// A dense per-program rule handle.
pub(crate) type RuleId = Id<Rule>;
/// A dense per-stratum materialized-row handle.
pub(crate) type RowId = Id<Row>;

#[cfg(test)]
mod tests {
    use super::*;

    /// `index()`/`from_index()` round-trip the 0-based slot ↔ 1-based niche at the
    /// boundary values (the `+1` niche offset must be exact everywhere) for the
    /// engine-only brands too — the arena crate pins its own brands.
    #[test]
    fn id_niche_offset_round_trips_at_boundaries() {
        for slot in [0usize, 1, (u32::MAX - 2) as usize] {
            let id = PredId::from_index(slot);
            assert_eq!(id.index(), slot, "slot {slot} must round-trip");
        }
        assert_eq!(PredId::from_index(0).index(), 0);
    }

    /// The `NonZeroU32` niche makes `Option<Id<C>>` pointer-width (no discriminant
    /// word), for EVERY brand.
    #[test]
    fn id_option_is_pointer_width() {
        assert_eq!(
            std::mem::size_of::<Option<TermId>>(),
            std::mem::size_of::<TermId>(),
            "Option<TermId> must be niche-packed to TermId's width"
        );
        assert_eq!(std::mem::size_of::<TermId>(), std::mem::size_of::<u32>());
        assert_eq!(
            std::mem::size_of::<Option<PredId>>(),
            std::mem::size_of::<PredId>()
        );
        assert_eq!(
            std::mem::size_of::<Option<RowId>>(),
            std::mem::size_of::<RowId>()
        );
        assert_eq!(
            std::mem::size_of::<Option<RuleId>>(),
            std::mem::size_of::<RuleId>()
        );
        // A TermRef is exactly its wrapped TermId — the row-tuple argument handle adds
        // no width over the atomic handle it carries.
        assert_eq!(
            std::mem::size_of::<TermRef>(),
            std::mem::size_of::<TermId>()
        );
    }

    /// `Ord` is by raw index (mint order) — earlier-minted sorts first.
    #[test]
    fn id_ord_is_mint_order() {
        let a = PredId::from_index(0);
        let b = PredId::from_index(1);
        assert!(a < b, "mint order: slot 0 precedes slot 1");
        assert_eq!(a, PredId::from_index(0));
    }
}
