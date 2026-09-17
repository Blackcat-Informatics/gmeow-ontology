// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The arity-generic **binding pattern** — the adornment lattice shared by the
//! backward magic-sets demand keying ([`crate::physical::magic`]) and the forward
//! generic evaluator's query-plan index selection ([`crate::physical::generic`]).
//!
//! A [`BindingPattern`] is a bitset over an atom's argument positions: position `i`
//! set means that position is **bound** (a constant, or a variable already bound by
//! the sideways-information-passing chain / the goal). It replaces the binary-only
//! `Adorn{subj_bound, obj_bound}` with an arity-generic form that carries the Boolean
//! subsumption lattice.
//!
//! # The subsumption order (picked deliberately)
//!
//! **A is more general than B (`A ⊑ B`) iff `bound(A) ⊆ bound(B)`.** Fewer bound
//! positions ⇒ more general. The all-free pattern is the bottom (⊥, most general —
//! it demands nothing / restricts nothing); the all-bound pattern is the top (⊤, most
//! specific). A demand keyed on the more-general `A` serves any more-specific `B`,
//! because `A` propagates a superset of the bindings `B` would.
//!
//! The lattice operations are consistent with that order — the pattern positions form
//! a Boolean algebra isomorphic to the powerset of `{0..arity}` ordered by ⊆:
//!
//! - [`meet`](BindingPattern::meet) — greatest lower bound = the most-general common
//!   subsumer = the **intersection** of the two bound sets.
//! - [`join`](BindingPattern::join) — least upper bound = the **union** of the two
//!   bound sets.
//!
//! Same-arity is a precondition of both (all patterns for one predicate share its
//! arity); it is asserted.

/// A compact bitset over an atom's argument positions: bit `i` set ⇒ position `i` is
/// bound. Dense small-integer positions per LOGIC-PERFORMANCE.md's dense-ID doctrine
/// (a `u64` bitset covers every arity the engine can carry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BindingPattern {
    /// Bit `i` set ⇒ argument position `i` is bound.
    bound: u64,
    /// The atom's arity (number of argument positions). Positions `>= arity` are
    /// never set.
    arity: u16,
}

impl BindingPattern {
    /// The maximum arity a `u64` bound-bitset can represent.
    const MAX_ARITY: usize = 64;

    /// Build a pattern from a per-position boundness iterator (position 0 first).
    ///
    /// # Panics
    ///
    /// Panics if the iterator yields more than [`Self::MAX_ARITY`] positions.
    pub(crate) fn from_bools<I: IntoIterator<Item = bool>>(bits: I) -> Self {
        let mut bound: u64 = 0;
        let mut arity: u16 = 0;
        for (i, b) in bits.into_iter().enumerate() {
            assert!(
                i < Self::MAX_ARITY,
                "BindingPattern arity exceeds the {} the u64 bitset carries",
                Self::MAX_ARITY
            );
            if b {
                bound |= 1u64 << i;
            }
            arity += 1;
        }
        Self { bound, arity }
    }

    /// Build an all-free pattern of the given `arity`, then set the given bound
    /// positions.
    ///
    /// # Panics
    ///
    /// Panics if `arity` exceeds [`Self::MAX_ARITY`] or a bound position is `>= arity`.
    pub(crate) fn from_bound_positions<I: IntoIterator<Item = usize>>(
        arity: usize,
        positions: I,
    ) -> Self {
        assert!(
            arity <= Self::MAX_ARITY,
            "BindingPattern arity {arity} exceeds the {} the u64 bitset carries",
            Self::MAX_ARITY
        );
        let mut bound: u64 = 0;
        for p in positions {
            assert!(
                p < arity,
                "bound position {p} out of range for arity {arity}"
            );
            bound |= 1u64 << p;
        }
        Self {
            bound,
            arity: arity as u16,
        }
    }

    /// The atom's arity (number of argument positions).
    pub(crate) fn arity(&self) -> usize {
        self.arity as usize
    }

    /// Is argument position `pos` bound? Positions `>= arity` are never bound.
    pub(crate) fn is_bound(&self, pos: usize) -> bool {
        pos < self.arity() && (self.bound & (1u64 << pos)) != 0
    }

    /// The bound argument positions, ascending.
    pub(crate) fn bound_positions(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.arity()).filter(move |&p| self.is_bound(p))
    }

    /// `true` iff NO position is bound (the old `ff` all-free adornment — the ⊥ of the
    /// lattice, demanding/restricting nothing).
    pub(crate) fn is_all_free(&self) -> bool {
        self.bound == 0
    }

    /// `self ⊑ other`: `self` is MORE GENERAL than (or equal to) `other`, i.e. every
    /// position bound in `self` is bound in `other` (`bound(self) ⊆ bound(other)`),
    /// at the same arity. A demand keyed on the more-general `self` serves any
    /// more-specific `other`.
    pub(crate) fn subsumes(&self, other: &BindingPattern) -> bool {
        self.arity == other.arity && (self.bound & !other.bound) == 0
    }

    /// Greatest lower bound: the most-general common subsumer = the **intersection**
    /// of the two bound sets.
    ///
    /// # Panics
    ///
    /// Panics on an arity mismatch (all patterns for one predicate share its arity).
    pub(crate) fn meet(&self, other: &BindingPattern) -> BindingPattern {
        assert_eq!(
            self.arity, other.arity,
            "meet requires equal arity (one predicate, one arity)"
        );
        BindingPattern {
            bound: self.bound & other.bound,
            arity: self.arity,
        }
    }

    /// Least upper bound: the **union** of the two bound sets.
    ///
    /// # Panics
    ///
    /// Panics on an arity mismatch (all patterns for one predicate share its arity).
    pub(crate) fn join(&self, other: &BindingPattern) -> BindingPattern {
        assert_eq!(
            self.arity, other.arity,
            "join requires equal arity (one predicate, one arity)"
        );
        BindingPattern {
            bound: self.bound | other.bound,
            arity: self.arity,
        }
    }

    /// The deterministic per-position code string: one char per position, `'b'`
    /// (bound) or `'f'` (free), position 0 first. Arity-2 yields the legacy
    /// `"bb"`/`"bf"`/`"fb"`/`"ff"`; arity-3 yields e.g. `"bfb"`. Round-trips with
    /// [`from_code`](BindingPattern::from_code).
    pub(crate) fn code(&self) -> String {
        (0..self.arity())
            .map(|p| if self.is_bound(p) { 'b' } else { 'f' })
            .collect()
    }

    /// Reconstruct a pattern from its per-position [`code`](BindingPattern::code)
    /// string (`'b'` = bound, `'f'` = free); the string length is the arity.
    ///
    /// # Panics
    ///
    /// Panics on a char other than `'b'`/`'f'`, or a length over [`Self::MAX_ARITY`].
    pub(crate) fn from_code(code: &str) -> BindingPattern {
        Self::from_bools(code.chars().map(|c| match c {
            'b' => true,
            'f' => false,
            other => panic!("invalid binding-pattern code char {other:?} in {code:?}"),
        }))
    }
}

#[path = "binding_pattern.tests.rs"]
#[cfg(test)]
mod tests;
