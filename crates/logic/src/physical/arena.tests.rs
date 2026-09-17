// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::physical::id::TermId;

fn tref(slot: usize) -> TermRef {
    TermRef::term(TermId::from_index(slot))
}

/// A binary/ternary tuple stays inline and never touches the backing buffer.
#[test]
fn arena_small_tuple_is_inline_and_leaves_buffer_empty() {
    let mut arena = RowArena::new();
    let binary = arena.alloc(&[tref(0), tref(1)]);
    assert!(
        matches!(binary, RowTuple::Inline(_)),
        "arity 2 must be inline"
    );
    assert_eq!(arena.get(&binary), &[tref(0), tref(1)]);
    // A full-inline-capacity tuple (arity == INLINE) still stays inline.
    let quad = arena.alloc(&[tref(2), tref(3), tref(4), tref(5)]);
    assert!(
        matches!(quad, RowTuple::Inline(_)),
        "arity == INLINE must be inline"
    );
    assert_eq!(
        arena.backing_len(),
        0,
        "inline tuples must never grow the backing buffer"
    );
}

/// A wider-than-inline n-ary tuple spills into the contiguous backing buffer, and
/// `reset` genuinely truncates that real buffer (not a no-op).
#[test]
fn arena_wide_tuple_spills_and_reset_truncates_real_buffer() {
    let mut arena = RowArena::new();
    // Arity 5 > INLINE(4): must spill into the backing buffer as a range.
    let args: Vec<TermRef> = (0..5).map(tref).collect();
    let wide = arena.alloc(&args);
    match wide {
        RowTuple::Arena { start, len } => {
            assert_eq!((start, len), (0, 5), "first spill occupies [0, 5)");
        }
        RowTuple::Inline(_) => panic!("arity 5 must spill into the arena buffer"),
    }
    assert_eq!(arena.backing_len(), 5, "the buffer holds the spilled tuple");
    assert_eq!(arena.get(&wide), args.as_slice());

    // A second spill appends after the first.
    let more: Vec<TermRef> = (10..16).map(tref).collect();
    let wide2 = arena.alloc(&more);
    assert!(matches!(wide2, RowTuple::Arena { start: 5, len: 6 }));
    assert_eq!(arena.backing_len(), 11);
    assert_eq!(arena.get(&wide2), more.as_slice());

    // Reset is a REAL truncation of the contiguous buffer.
    arena.reset();
    assert_eq!(
        arena.backing_len(),
        0,
        "reset must truncate the real buffer to 0"
    );

    // After reset the buffer is reusable and offsets restart at 0.
    let reused = arena.alloc(&args);
    assert!(matches!(reused, RowTuple::Arena { start: 0, len: 5 }));
    assert_eq!(arena.get(&reused), args.as_slice());
}
