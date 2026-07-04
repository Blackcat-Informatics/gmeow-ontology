// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Dependency-free UTC timestamp formatting for the deposit / compliance lanes.
//!
//! The CrossRef deposit batch id and the compliance report both stamp the
//! current UTC time. To keep the crate's dependency surface minimal (no chrono),
//! the civil date is computed from the Unix epoch via Howard Hinnant's
//! `civil_from_days` algorithm.

use std::time::{SystemTime, UNIX_EPOCH};

/// `(year, month, day)` for the given count of days since 1970-01-01 (UTC),
/// via Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u32, d as u32)
}

/// Split the current time into `(year, month, day, hour, minute, second)` UTC.
fn now_utc_parts() -> (i64, u32, u32, u32, u32, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = (rem / 3600) as u32;
    let minute = ((rem % 3600) / 60) as u32;
    let second = (rem % 60) as u32;
    let (y, m, d) = civil_from_days(days);
    (y, m, d, hour, minute, second)
}

/// Current UTC time as `YYYYMMDDHHMMSS` (CrossRef deposit batch stamp).
pub fn utc_compact() -> String {
    let (y, m, d, hh, mm, ss) = now_utc_parts();
    format!("{y:04}{m:02}{d:02}{hh:02}{mm:02}{ss:02}")
}

/// Current UTC time as an ISO-8601 second-precision instant with a `+00:00`
/// offset (the compliance report `meta:generatedAt` value).
pub fn utc_iso_seconds() -> String {
    let (y, m, d, hh, mm, ss) = now_utc_parts();
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}+00:00")
}

#[cfg(test)]
mod tests {
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
}
