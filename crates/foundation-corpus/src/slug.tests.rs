// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn basic_slug() {
    assert_eq!(slug("Rowan Cogsworth"), "rowan-cogsworth");
    assert_eq!(slug("The Open Ledger"), "the-open-ledger");
    assert_eq!(
        slug("Rowan swears the steerswoman's oath"),
        "rowan-swears-the-steerswoman-s-oath"
    );
}

#[test]
fn empty_falls_back() {
    assert_eq!(slug("   "), "x");
    assert_eq!(slug("---"), "x");
}

#[test]
fn suffix_last_24() {
    let s = slug("https://blackcatinformatics.ca/gmeow/corpus/foundation/book/1");
    // slug of the full IRI then last 24 chars.
    assert_eq!(char_suffix(&s, 24), "corpus-foundation-book-1");
}
