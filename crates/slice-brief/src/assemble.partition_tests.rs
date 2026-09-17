// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{CHUNK, batch_count, batch_range};

/// An empty (zero-term) slice partitions into zero batches, and every batch
/// index is out of range.
#[test]
fn empty_slice_has_zero_batches() {
    assert_eq!(batch_count(0), 0, "an empty slice must have zero batches");
    assert_eq!(
        batch_range(0, 0),
        None,
        "batch 0 of an empty slice must be out of range"
    );
}

/// A term count that is an EXACT multiple of `CHUNK` partitions into exactly
/// `len / CHUNK` full batches — no trailing empty batch.
#[test]
fn exact_multiple_has_no_remainder_batch() {
    let len = CHUNK * 3;
    assert_eq!(
        batch_count(len),
        3,
        "an exact multiple of CHUNK must partition into len / CHUNK batches"
    );
    assert_eq!(batch_range(0, len), Some(0..CHUNK));
    assert_eq!(batch_range(1, len), Some(CHUNK..(2 * CHUNK)));
    assert_eq!(batch_range(2, len), Some((2 * CHUNK)..len));
    assert_eq!(
        batch_range(3, len),
        None,
        "the batch just past an exact multiple must be out of range"
    );
}

/// A term count with a nonzero remainder over `CHUNK` gets one extra, short
/// final batch covering only the remainder.
#[test]
fn remainder_gets_a_short_final_batch() {
    let len = CHUNK * 2 + 7;
    assert_eq!(
        batch_count(len),
        3,
        "a remainder must round the batch count up (ceil)"
    );
    assert_eq!(batch_range(0, len), Some(0..CHUNK));
    assert_eq!(batch_range(1, len), Some(CHUNK..(2 * CHUNK)));
    assert_eq!(
        batch_range(2, len),
        Some((2 * CHUNK)..len),
        "the final batch must be short, covering only the remainder"
    );
    assert_eq!(
        batch_range(3, len),
        None,
        "past the last (short) batch must be out of range"
    );
}
