// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Dense `u64`-word bitset over [`RowId`]s.
//!
//! # Why a bitset, not a hash set
//!
//! The semi-naive fixpoint tests, per selected row, whether that row's underlying
//! fact is in the current round's **delta** ("new in the round just committed").  The
//! interim form was a `HashSet<(PredId, TermId, TermId)>` composite-key probe: a hash
//! per selected row on the hottest inner loop.  Every committed store row already
//! carries a store-global **dense** [`RowId`] (assigned in insertion order by
//! [`crate::physical::store::RelationStore`]), so delta membership collapses to a
//! single word test on a contiguous `u64` array — `words[row >> 6] & (1 << (row & 63))`
//! — with **no hashing at all** on this path.
//!
//! # Determinism
//!
//! A [`DenseBitset`] is a pure membership structure: it is NEVER iterated to produce
//! output, and a [`RowId`] is mint (insertion) order, meaningless for emission.  Every
//! commit / emission / budget-charge ordering stays at the resolved-lexical `FactKey`
//! sort at round commit (see [`crate::physical::seminaive`]); this bitset only answers
//! "is this row in the delta", never "in what order".
//!
//! # Sizing
//!
//! The word array is sized to the store's current row count, so every selectable row
//! is addressable.  [`set`](DenseBitset::set) grows the backing array on demand, and
//! [`contains`](DenseBitset::contains) treats an out-of-range row as absent — a row
//! whose id exceeds the current delta simply is not in it.

use crate::physical::id::RowId;

/// The number of [`RowId`] bits packed into one backing word.
const BITS_PER_WORD: usize = 64;

/// A dense `u64`-word bitset keyed by [`RowId`].
///
/// Membership is one word test; there is no hashing and no per-row allocation.  Used
/// as the semi-naive round delta: the set of rows "new in the round just committed".
#[derive(Debug, Clone, Default)]
pub(crate) struct DenseBitset {
    /// Backing words; row `r` is `words[r >> 6]` bit `r & 63`.  Absent trailing words
    /// are implicitly zero (an unaddressed row is not a member).
    words: Vec<u64>,
}

impl DenseBitset {
    /// An empty bitset (no rows are members).
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A zeroed bitset pre-sized to address `rows` distinct [`RowId`]s (slots
    /// `0..rows`).  No row is a member until [`set`](Self::set); this only reserves the
    /// backing words so the round-delta build does not reallocate per `set`.
    pub(crate) fn with_capacity(rows: usize) -> Self {
        Self {
            words: vec![0; rows.div_ceil(BITS_PER_WORD)],
        }
    }

    /// A bitset with EVERY row in `0..rows` set — the semi-naive round-1 seed, where the
    /// delta is the whole accumulated store (`RelationStore` mints RowIds densely as
    /// `0..row_count`, so the seed is exactly the low `rows` bits).
    pub(crate) fn all_set(rows: usize) -> Self {
        let full_words = rows / BITS_PER_WORD;
        let remainder = rows % BITS_PER_WORD;
        let mut words = vec![u64::MAX; full_words];
        if remainder > 0 {
            // The low `remainder` bits of the final partial word.
            words.push((1u64 << remainder) - 1);
        }
        Self { words }
    }

    /// The `(word index, bit mask)` addressing `row`.
    #[inline]
    fn locate(row: RowId) -> (usize, u64) {
        let slot = row.index();
        (slot / BITS_PER_WORD, 1u64 << (slot % BITS_PER_WORD))
    }

    /// Add `row` to the set, growing the backing words if it is beyond the current
    /// capacity (a row minted after this bitset was sized).
    pub(crate) fn set(&mut self, row: RowId) {
        let (word, mask) = Self::locate(row);
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        self.words[word] |= mask;
    }

    /// Whether `row` is in the set.  A row beyond the addressed range is absent (one
    /// bounds check, then one word test — no hashing).
    #[inline]
    pub(crate) fn contains(&self, row: RowId) -> bool {
        let (word, mask) = Self::locate(row);
        self.words.get(word).is_some_and(|w| w & mask != 0)
    }

    /// The number of rows currently in the set (popcount over the backing words).
    pub(crate) fn len(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Whether the set is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.words.iter().all(|w| *w == 0)
    }
}

#[path = "bitset.tests.rs"]
#[cfg(test)]
mod tests;
