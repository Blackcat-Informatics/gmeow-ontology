// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The phase-scoped row/tuple bump arena.
//!
//! # Two arenas, one contradiction resolved
//!
//! The engine needs two DIFFERENT arenas that a single structure cannot be:
//!
//! * a **persistent term arena** — never reset within its lifetime — backing the
//!   interned terms.  That is exactly what [`crate::facts::TermInterner`] already is
//!   (insertion-ordered, per-store, never truncated); it is the content-addressed DAG
//!   seam a future structured-term DAG grows function-symbol nodes into,
//!   addressed through the [`crate::physical::id::TermRef`] handle.  This module does
//!   NOT duplicate it.
//!
//! * a **phase-scoped row/tuple arena** — genuinely reset every round — where the
//!   semi-naive fixpoint bump-allocates a round's argument tuples, reads them back at
//!   the sorted commit, then TRUNCATES the backing buffer at the round boundary
//!   (allocate-within-round → sort-commit → reset).  That is [`RowArena`] below.
//!
//! A single arena cannot be both persistent AND per-round-reset, so they are split.
//!
//! # Why a bump arena and not a `Vec<Vec<TermRef>>`
//!
//! An argument tuple is small and short-lived (one round).  The arena keeps every
//! round's tuples in ONE contiguous [`TermRef`] buffer and hands out `(start, len)`
//! offset ranges, so a round's worth of tuples is one allocation that a single
//! [`RowArena::reset`] `truncate(0)` reclaims — no per-tuple `Vec` alloc/free churn.
//! A tuple whose arity fits inline (≤ [`INLINE`], the binary/ternary common case)
//! skips the buffer entirely via [`smallvec::SmallVec`]; only wider n-ary tuples spill
//! into the contiguous backing buffer.  Reset is a real length-truncation of that
//! real buffer, never a no-op.
//!
//! # Thread-locality
//!
//! The forward core parallelizes both independent worlds and immutable per-rule work
//! within one semi-naive round (see [`crate::physical::seminaive`]). A `RowArena` is
//! therefore owned by the rule/phase invocation that creates it: arenas are never shared
//! across rule tasks, and completed task buffers cross the scheduling boundary only as
//! owned values before the single sorted commit. There is no cross-thread arena aliasing
//! to guard.

#[cfg(test)]
mod test_support;

use smallvec::SmallVec;

use crate::physical::id::TermRef;

/// The inline argument-tuple arity: a tuple of at most this many [`TermRef`]s stays on
/// the stack (the binary engine's arity-2 rows, plus headroom for the ternary
/// world-slotted and small n-ary shapes) and never touches the arena's backing buffer.
pub(crate) const INLINE: usize = 4;

/// A handle to one argument tuple allocated in a [`RowArena`] round.
///
/// Either the tuple's [`TermRef`]s inline (arity ≤ [`INLINE`]) or a `(start, len)`
/// offset range into the arena's contiguous backing buffer (wider n-ary tuples).  A
/// handle is only valid against the arena that produced it, and only until that arena
/// is [`reset`](RowArena::reset).
#[derive(Debug, Clone)]
pub(crate) enum RowTuple {
    /// The tuple's arguments inline — no backing-buffer slot used.
    Inline(SmallVec<[TermRef; INLINE]>),
    /// A `[start, start + len)` range into the arena's contiguous backing buffer.
    Arena { start: u32, len: u32 },
}

/// A phase-scoped bump arena for a round's argument tuples.
///
/// Allocate every tuple of a round with [`alloc`](Self::alloc), read them back through
/// [`get`](Self::get) at the sorted commit, then [`reset`](Self::reset) at the round
/// boundary.  The backing buffer is a single contiguous [`Vec`] truncated to length 0
/// on reset — a genuine buffer reclaim, not an inline-storage no-op.
#[derive(Debug, Default)]
pub(crate) struct RowArena {
    /// Contiguous backing buffer for the tuples that overflow the inline arity.
    backing: Vec<TermRef>,
}

impl RowArena {
    /// A fresh, empty arena.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Bump-allocate `args` as one tuple, returning its handle.
    ///
    /// A tuple whose arity fits inline stays on the stack; a wider tuple is appended to
    /// the contiguous backing buffer as a `(start, len)` range.
    pub(crate) fn alloc(&mut self, args: &[TermRef]) -> RowTuple {
        if args.len() <= INLINE {
            RowTuple::Inline(SmallVec::from_slice(args))
        } else {
            let start = u32::try_from(self.backing.len())
                .expect("RowArena backing overflow: more than u32::MAX TermRefs in one round");
            let len =
                u32::try_from(args.len()).expect("RowArena tuple overflow: arity exceeds u32::MAX");
            self.backing.extend_from_slice(args);
            RowTuple::Arena { start, len }
        }
    }

    /// The argument slice a handle addresses.
    ///
    /// # Panics
    ///
    /// Panics if `tuple` is an [`RowTuple::Arena`] range that falls outside the current
    /// backing buffer — i.e. a handle from a prior (already-[`reset`](Self::reset))
    /// round, a programming error, never a data state.
    pub(crate) fn get<'a>(&'a self, tuple: &'a RowTuple) -> &'a [TermRef] {
        match tuple {
            RowTuple::Inline(v) => v.as_slice(),
            RowTuple::Arena { start, len } => {
                let start = *start as usize;
                let end = start + *len as usize;
                self.backing.get(start..end).unwrap_or_else(|| {
                    panic!(
                        "RowArena handle [{start}, {end}) is out of bounds (backing len {}): \
                         a tuple handle must never outlive its round's reset",
                        self.backing.len()
                    )
                })
            }
        }
    }

    /// Truncate the backing buffer to length 0 — the round/stratum-boundary reset.
    ///
    /// A real length-truncation of the contiguous buffer to zero (`Vec::clear`, which
    /// retains the buffer's capacity for the next round), NOT a no-op on inline storage:
    /// every [`RowTuple::Arena`] handle minted before the reset is invalidated, matching
    /// the fixpoint's allocate → commit → reset phases.
    pub(crate) fn reset(&mut self) {
        self.backing.clear();
    }
}

#[path = "arena.tests.rs"]
#[cfg(test)]
mod tests;
