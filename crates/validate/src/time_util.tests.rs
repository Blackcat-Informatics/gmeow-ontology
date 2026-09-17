// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn epoch_day_zero_is_unix_epoch() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
}

#[test]
fn known_civil_dates() {
    // 2000-03-01 is day 11017 since the epoch.
    assert_eq!(civil_from_days(11_017), (2000, 3, 1));
    // A leap day.
    assert_eq!(civil_from_days(19_782), (2024, 2, 29));
}

#[test]
fn compact_is_fourteen_digits() {
    let s = utc_compact();
    assert_eq!(s.len(), 14);
    assert!(s.bytes().all(|b| b.is_ascii_digit()));
}

#[test]
fn iso_has_offset_and_separators() {
    let s = utc_iso_seconds();
    assert!(s.ends_with("+00:00"));
    assert_eq!(s.as_bytes()[4], b'-');
    assert_eq!(s.as_bytes()[10], b'T');
}
