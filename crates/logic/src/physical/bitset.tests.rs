// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn row(slot: usize) -> RowId {
    RowId::from_index(slot)
}

/// `set`/`contains` round-trip across word boundaries — the `>> 6` / `& 63` split
/// must address the right word and bit at slot 0, mid-word, the word edge (63/64),
/// and well past the first word.
#[test]
fn bitset_set_contains_across_word_boundaries() {
    let mut b = DenseBitset::new();
    for &slot in &[0usize, 1, 63, 64, 65, 127, 128, 4095] {
        assert!(!b.contains(row(slot)), "slot {slot} absent before set");
        b.set(row(slot));
        assert!(b.contains(row(slot)), "slot {slot} present after set");
    }
    // A never-set neighbour of a set bit stays absent (no bit bleed across the mask).
    assert!(!b.contains(row(2)));
    assert!(!b.contains(row(62)));
    assert!(!b.contains(row(126)));
    assert_eq!(b.len(), 8, "exactly the eight set rows are members");
}

/// `contains` on a row beyond the addressed words is absent, never a panic — a row
/// minted after this bitset was sized simply is not in the delta.
#[test]
fn bitset_out_of_range_is_absent_not_panic() {
    let b = DenseBitset::with_capacity(10);
    assert!(!b.contains(row(9)));
    assert!(!b.contains(row(10)));
    assert!(!b.contains(row(1_000_000)));
}

/// `all_set(n)` sets EXACTLY rows `0..n` — the round-1 seed over a store of `n`
/// densely-minted rows — and nothing at `n` or beyond.
#[test]
fn bitset_all_set_covers_zero_to_n_exclusive() {
    for n in [0usize, 1, 63, 64, 65, 130] {
        let b = DenseBitset::all_set(n);
        assert_eq!(b.len(), n, "all_set({n}) has exactly {n} members");
        for slot in 0..n {
            assert!(b.contains(row(slot)), "all_set({n}) must contain {slot}");
        }
        assert!(!b.contains(row(n)), "all_set({n}) must NOT contain {n}");
        assert_eq!(b.is_empty(), n == 0);
    }
}

/// `set` grows the backing array when a row lands beyond the initial capacity, so a
/// row minted after sizing is still recorded.
#[test]
fn bitset_set_grows_beyond_initial_capacity() {
    let mut b = DenseBitset::with_capacity(4);
    assert!(!b.contains(row(500)));
    b.set(row(500));
    assert!(b.contains(row(500)));
    assert_eq!(b.len(), 1);
}
