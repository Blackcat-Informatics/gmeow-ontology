// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the shared deterministic emitter: content-address digest, N-Triples
//! canonicalization, and the digest-collision hard-fail guard.

use gmeow_lang_bridge::{assert_no_digest_collision, digest16, ntriples_sorted};

#[test]
fn digest16_is_deterministic_and_pinned() {
    // Determinism: same inputs, same output, across calls.
    assert_eq!(digest16("unit", "hello"), digest16("unit", "hello"));
    // 16 hex chars (8 bytes).
    assert_eq!(digest16("unit", "hello").len(), 16);
    // Pinned value — locks the domain-separated SHA-256 algorithm byte-for-byte.
    assert_eq!(digest16("unit", "hello"), "a896638e689645b3");
    // The domain separates the address space: same key, different domain, different digest.
    assert_ne!(digest16("unit", "hello"), digest16("form", "hello"));
}

#[test]
fn ntriples_sorted_is_sorted_deduped_and_deterministic() {
    let lines = vec![
        "<c> <p> <o> .".to_owned(),
        "<a> <p> <o> .".to_owned(),
        "<b> <p> <o> .".to_owned(),
        "<a> <p> <o> .".to_owned(), // duplicate
    ];
    let out = ntriples_sorted(lines.clone());
    let expected = "<a> <p> <o> .\n<b> <p> <o> .\n<c> <p> <o> .\n";
    assert_eq!(String::from_utf8(out).unwrap(), expected);

    // Input order does not matter — canonicalization is a pure function of the line set.
    let mut shuffled = lines;
    shuffled.reverse();
    assert_eq!(
        ntriples_sorted(shuffled),
        expected.as_bytes(),
        "canonicalization must be order-independent"
    );
}

#[test]
fn assert_no_digest_collision_passes_on_distinct_digests() {
    let entries = vec![
        ("key-a".to_owned(), "d0".to_owned()),
        ("key-b".to_owned(), "d1".to_owned()),
        // Same full key repeated with the same digest is not a collision.
        ("key-a".to_owned(), "d0".to_owned()),
    ];
    assert!(assert_no_digest_collision(&entries).is_ok());
}

#[test]
fn assert_no_digest_collision_errors_on_forced_collision() {
    let entries = vec![
        ("key-a".to_owned(), "same".to_owned()),
        ("key-b".to_owned(), "same".to_owned()),
    ];
    let err = assert_no_digest_collision(&entries).expect_err("distinct keys, same digest");
    assert!(err.message().contains("key-a"));
    assert!(err.message().contains("key-b"));
}
