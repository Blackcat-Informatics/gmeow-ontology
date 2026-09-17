// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::intern_code;
use std::collections::HashSet;

#[test]
fn every_slicetest_code_interns_with_no_collision() {
    let handles = register_all();
    assert_eq!(
        handles.len(),
        SLICETEST_DIAG_CODES.len(),
        "register_all() and SLICETEST_DIAG_CODES must enumerate the same kinds"
    );
    for code in SLICETEST_DIAG_CODES {
        assert!(
            intern_code(code).is_ok(),
            "slicetest code `{code}` did not intern after register_all()"
        );
    }
    let distinct_strings: HashSet<&&str> = SLICETEST_DIAG_CODES.iter().collect();
    assert_eq!(
        distinct_strings.len(),
        SLICETEST_DIAG_CODES.len(),
        "duplicate slicetest diagnostic code string detected"
    );
    let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
    assert_eq!(distinct_handles.len(), handles.len());
}
