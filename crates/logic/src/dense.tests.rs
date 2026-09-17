// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
