// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// Authored codebook, pack and shipped-projection contracts run together over
// authenticated producer observations in the language contract runner.
#[test]
fn report_is_clean_iff_no_failures() {
    let clean = Gmn1RoundTripReport::default();
    assert!(clean.is_clean());
    let dirty = Gmn1RoundTripReport {
        failures: vec![Gmn1RoundTripFailure {
            path: "x".to_owned(),
            error: Gmn1Error::NonDecodableGrammar {
                detail: "y".to_owned(),
            },
        }],
    };
    assert!(!dirty.is_clean());
}
