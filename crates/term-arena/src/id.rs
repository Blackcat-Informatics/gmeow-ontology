// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Branded niche IDs for the arena's entity classes.
//!
//! # Doctrine
//!
//! Every dense handle the arena mints — an interned atomic term, a DAG node, a
//! unification metavariable — is an [`Id<C>`]: a `NonZeroU32` in a `PhantomData`
//! brand.  The brand `C` makes IDs of different classes DISTINCT TYPES, so a
//! [`TermId`] can never be passed where a [`NodeId`] is expected — cross-class ID
//! confusion is a compile error, not a runtime bug.  The `NonZeroU32` niche keeps
//! `Option<Id<C>>` pointer-width for free.
//!
//! # Ordering (read this before sorting on an `Id`)
//!
//! [`Id`]'s [`Ord`] is by RAW INDEX — i.e. MINT ORDER (insertion order within the
//! space that minted it).  Mint order is **meaningless for emission**: two runs
//! that intern the same terms in the same sequence mint the same ids, but the id
//! integers carry no lexical meaning.  An `Id` integer is never serialized and never
//! hashed for provenance — the [`ContentKey`](crate::ContentKey) is the persistent
//! identity.
//!
//! # Why this is engine-tier, not façade
//!
//! These dense integers NEVER escape the runtime: the crate-root façade hands out an
//! opaque, arena-branded [`StructNode`](crate::StructNode) instead.  A consumer that
//! genuinely operates the arena (the reasoning runtime's unifier, proof checker, and
//! backward resolver) reaches them through [`crate::engine`]; a parser front-end never
//! needs to.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::num::NonZeroU32;

/// A dense, per-space handle for entity class `C`.
///
/// Stored as a `NonZeroU32` (niche ⇒ `Option<Id<C>>` is pointer-width) branded by
/// `PhantomData<fn() -> C>`.  The `fn() -> C` form is covariant in `C` and imposes
/// no auto-trait bound on `C`, so `Id<C>: Copy + Send + Sync` regardless of the
/// brand — and the brand type never needs to be constructible (the markers below
/// are uninhabited).
pub struct Id<C>(NonZeroU32, PhantomData<fn() -> C>);

impl<C> Id<C> {
    /// The zero-based slot index this id addresses in its space.
    ///
    /// The niche offset is `+1`: slot `0` ↔ `NonZeroU32(1)`.
    #[inline]
    pub fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }

    /// Mint the id for zero-based slot `index`.
    ///
    /// The niche offset is `+1`, so slot `0` becomes `NonZeroU32(1)` and
    /// `Option<Id<C>>` stays pointer-width.
    #[inline]
    pub fn from_index(index: usize) -> Self {
        let raw = u32::try_from(index + 1)
            .expect("Id space overflow: more than u32::MAX - 1 distinct entities in one space");
        Self(
            NonZeroU32::new(raw).expect("index + 1 is nonzero by construction"),
            PhantomData,
        )
    }
}

// Manual trait impls: deriving would place spurious `C: Trait` bounds on the brand
// (which is uninhabited and never satisfies them).  The `fn() -> C` brand carries
// no data, so every impl is over the `NonZeroU32` payload alone.

impl<C> Clone for Id<C> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for Id<C> {}

impl<C> PartialEq for Id<C> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<C> Eq for Id<C> {}

impl<C> Hash for Id<C> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<C> Ord for Id<C> {
    /// By raw index (mint order). See the module doctrine: mint order is NEVER an
    /// emission-order source.
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl<C> PartialOrd for Id<C> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<C> fmt::Debug for Id<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Print the 0-based slot index; the brand type is elided (it is never a
        // value, only a phantom).
        write!(f, "Id({})", self.index())
    }
}

// ── Brand markers (uninhabited: pure type-level tags, never constructed) ─────────

/// Brand: an interned atomic term handle. See [`TermId`].
pub enum Term {}
/// Brand: a hash-consed structured-term DAG node handle. See [`NodeId`].
pub enum Node {}
/// Brand: a unification metavariable handle. See [`MetaId`].
pub enum Meta {}

/// A dense per-interner atomic-term handle.
pub type TermId = Id<Term>;

/// A dense per-DAG structured-term node handle.
///
/// The insertion-ordered slot a [`TermDag`](crate::engine::TermDag) node lives in.
/// Because bound occurrences are locally-nameless de-Bruijn refs and every node is
/// content-keyed, alpha-equivalent terms hash-cons to the SAME `NodeId` — structural
/// `NodeId` equality *is* alpha-equivalence.  Like every [`Id`], it is a runtime handle
/// only: never serialized, never hashed for provenance (the content key in
/// [`crate::term_key`] is the persistent identity — see the module doctrine).
pub type NodeId = Id<Node>;

/// A dense per-DAG unification-metavariable handle.
///
/// Identity-bearing: two occurrences of the same metavariable share one `MetaId` (and so
/// one [`NodeId`]), while a fresh metavariable mints a new one.  Same runtime-only
/// doctrine as every [`Id`] — the metavariable's ordinal enters the content key, never a
/// serialized surface.
pub type MetaId = Id<Meta>;

#[cfg(test)]
mod tests {
    use super::*;

    /// `index()`/`from_index()` round-trip the 0-based slot ↔ 1-based niche at the
    /// boundary values (the `+1` niche offset must be exact everywhere).
    #[test]
    fn id_niche_offset_round_trips_at_boundaries() {
        for slot in [0usize, 1, (u32::MAX - 2) as usize] {
            let id = TermId::from_index(slot);
            assert_eq!(id.index(), slot, "slot {slot} must round-trip");
        }
        // Slot 0 is stored as NonZeroU32(1) — the niche is genuinely used.
        assert_eq!(TermId::from_index(0).index(), 0);
    }

    /// The `NonZeroU32` niche makes `Option<Id<C>>` pointer-width (no discriminant
    /// word), for EVERY brand this crate mints.
    #[test]
    fn id_option_is_pointer_width() {
        assert_eq!(
            std::mem::size_of::<Option<TermId>>(),
            std::mem::size_of::<TermId>(),
            "Option<TermId> must be niche-packed to TermId's width"
        );
        assert_eq!(std::mem::size_of::<TermId>(), std::mem::size_of::<u32>());
        assert_eq!(
            std::mem::size_of::<Option<NodeId>>(),
            std::mem::size_of::<NodeId>()
        );
        assert_eq!(
            std::mem::size_of::<Option<MetaId>>(),
            std::mem::size_of::<MetaId>()
        );
    }

    /// `Ord` is by raw index (mint order) — earlier-minted sorts first.
    #[test]
    fn id_ord_is_mint_order() {
        let a = TermId::from_index(0);
        let b = TermId::from_index(1);
        assert!(a < b, "mint order: slot 0 precedes slot 1");
        assert_eq!(a, TermId::from_index(0));
    }
}
