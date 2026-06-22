// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Dependency-free dense-id graph primitives (#823 acceleration, Phase 5).
//!
//! Graph algorithms over RDF IRIs are naturally written against `String` keys,
//! but `BTreeMap<String, BTreeSet<String>>` adjacency pays a hashing/allocation
//! cost on every traversal step. This module provides two small building blocks
//! that let the hot algorithms run over dense `u32` node ids with bit-parallel
//! set operations, then map back to the exact same `String`-typed boundary:
//!
//! - [`DenseInterner`] — first-seen `&str → u32` assignment plus an id→str
//!   reverse table, so any `String`-keyed graph can be lowered to `u32` ids and
//!   the results raised back to the original IRIs.
//! - [`BitSet`] — a `Vec<u64>`-backed bitset with bit-parallel [`BitSet::union_with`]
//!   for O(words) set union and an ascending-order [`BitSet::iter`].
//!
//! Both are intentionally allocation-light and carry no external dependencies.

use std::collections::HashMap;

/// First-seen `&str → u32` interner with an id→str reverse table.
///
/// Ids are assigned densely in the order strings are first interned, so a graph
/// over `n` distinct node strings interns to the contiguous range `0..n`. The
/// reverse table ([`DenseInterner::resolve`]) recovers the original string for
/// the boundary mapping.
#[derive(Debug, Default, Clone)]
pub(crate) struct DenseInterner {
    /// `str → id` forward map.
    forward: HashMap<String, u32>,
    /// `id → str` reverse table, indexed by id.
    reverse: Vec<String>,
}

impl DenseInterner {
    /// A fresh, empty interner.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Intern `s`, returning its existing id or assigning the next free id.
    pub(crate) fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.forward.get(s) {
            return id;
        }
        let id = self.reverse.len() as u32;
        self.reverse.push(s.to_owned());
        self.forward.insert(s.to_owned(), id);
        id
    }

    /// The id previously assigned to `s`, if any (no assignment).
    pub(crate) fn get(&self, s: &str) -> Option<u32> {
        self.forward.get(s).copied()
    }

    /// The string for `id`. Panics if `id` was never assigned.
    pub(crate) fn resolve(&self, id: u32) -> &str {
        &self.reverse[id as usize]
    }

    /// The number of distinct interned strings (= the dense id count).
    pub(crate) fn len(&self) -> usize {
        self.reverse.len()
    }

    /// Whether nothing has been interned yet.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.reverse.is_empty()
    }
}

/// A `Vec<u64>`-backed bitset over a fixed bit capacity.
///
/// Bit `i` lives in word `i / 64` at offset `i % 64`. [`BitSet::union_with`] is a
/// word-wise `|=`, and [`BitSet::iter`] yields set bit indices in ascending order
/// (the deterministic order the `BTreeSet` boundary requires).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    /// A bitset able to hold indices `0..n_bits`, all clear.
    pub(crate) fn with_capacity(n_bits: usize) -> Self {
        let words = n_bits.div_ceil(64);
        Self {
            words: vec![0u64; words],
        }
    }

    /// Set bit `i`.
    pub(crate) fn insert(&mut self, i: usize) {
        self.words[i / 64] |= 1u64 << (i % 64);
    }

    /// Whether bit `i` is set.
    ///
    /// Part of the complete `BitSet` contract (#823) and exercised by the unit
    /// tests; backs the Warshall closure's membership test (`reach[i].contains(k)`)
    /// in [`crate::entrenchment`].
    pub(crate) fn contains(&self, i: usize) -> bool {
        let w = i / 64;
        w < self.words.len() && (self.words[w] >> (i % 64)) & 1 == 1
    }

    /// Bit-parallel union: set every bit that is set in `other`.
    ///
    /// Both bitsets are sized from the same node count, so word vectors share a
    /// length; the loop is a straight word-wise `|=`.
    pub(crate) fn union_with(&mut self, other: &BitSet) {
        debug_assert_eq!(self.words.len(), other.words.len());
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a |= *b;
        }
    }

    /// Whether no bit is set.
    pub(crate) fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    /// Iterate set bit indices in ascending order.
    pub(crate) fn iter(&self) -> BitSetIter<'_> {
        BitSetIter {
            words: &self.words,
            word_idx: 0,
            cur: self.words.first().copied().unwrap_or(0),
        }
    }
}

/// Ascending iterator over the set bit indices of a [`BitSet`].
pub(crate) struct BitSetIter<'a> {
    words: &'a [u64],
    word_idx: usize,
    /// Remaining set bits of the current word (consumed low-to-high).
    cur: u64,
}

impl Iterator for BitSetIter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        loop {
            if self.cur != 0 {
                let bit = self.cur.trailing_zeros() as usize;
                self.cur &= self.cur - 1; // clear lowest set bit
                return Some(self.word_idx * 64 + bit);
            }
            self.word_idx += 1;
            if self.word_idx >= self.words.len() {
                return None;
            }
            self.cur = self.words[self.word_idx];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interner_round_trips_first_seen_ids() {
        let mut it = DenseInterner::new();
        assert!(it.is_empty());
        assert_eq!(it.intern("alpha"), 0);
        assert_eq!(it.intern("beta"), 1);
        assert_eq!(it.intern("alpha"), 0); // stable on re-intern
        assert_eq!(it.intern("gamma"), 2);
        assert_eq!(it.len(), 3);

        assert_eq!(it.get("beta"), Some(1));
        assert_eq!(it.get("missing"), None);

        assert_eq!(it.resolve(0), "alpha");
        assert_eq!(it.resolve(1), "beta");
        assert_eq!(it.resolve(2), "gamma");
        assert!(!it.is_empty());
    }

    #[test]
    fn bitset_insert_contains_capacity() {
        let mut b = BitSet::with_capacity(130);
        assert!(b.is_empty());
        b.insert(0);
        b.insert(63);
        b.insert(64);
        b.insert(129);
        assert!(b.contains(0));
        assert!(b.contains(63));
        assert!(b.contains(64));
        assert!(b.contains(129));
        assert!(!b.contains(1));
        assert!(!b.contains(128));
        assert!(!b.is_empty());
        // Out-of-range query is a clean `false`, not a panic.
        assert!(!b.contains(10_000));
    }

    #[test]
    fn bitset_iter_is_ascending_and_word_crossing() {
        let mut b = BitSet::with_capacity(200);
        for i in [199usize, 64, 65, 0, 63, 130] {
            b.insert(i);
        }
        let got: Vec<usize> = b.iter().collect();
        assert_eq!(got, vec![0, 63, 64, 65, 130, 199]);
    }

    #[test]
    fn bitset_union_with_is_bit_parallel() {
        let mut a = BitSet::with_capacity(128);
        a.insert(1);
        a.insert(100);
        let mut b = BitSet::with_capacity(128);
        b.insert(1); // overlap
        b.insert(2);
        b.insert(127);
        a.union_with(&b);
        let got: Vec<usize> = a.iter().collect();
        assert_eq!(got, vec![1, 2, 100, 127]);
    }

    #[test]
    fn empty_bitset_iter_yields_nothing() {
        let b = BitSet::with_capacity(0);
        assert!(b.is_empty());
        assert_eq!(b.iter().count(), 0);
        let b2 = BitSet::with_capacity(10);
        assert_eq!(b2.iter().count(), 0);
    }
}
