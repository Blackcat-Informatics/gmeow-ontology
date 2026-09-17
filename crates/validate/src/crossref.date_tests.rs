// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::validate_iso_date;

#[test]
fn accepts_well_formed_iso_date() {
    assert!(validate_iso_date("2026-06-21").is_ok());
}

#[test]
fn rejects_malformed_dates() {
    // A malformed release_date must surface as an Err, never an out-of-range panic.
    for bad in ["2026", "2026-06", "not-a-date", "2026/06/21", "26-6-1", ""] {
        let err = validate_iso_date(bad).expect_err("malformed date must be rejected");
        let message = err.message();
        assert!(message.contains("YYYY-MM-DD"), "{message}");
    }
}
