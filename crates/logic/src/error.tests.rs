// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::intern_code;
use std::collections::HashSet;

/// Both native integrity kinds appear exactly once in the catalog and eager registration.
#[test]
fn native_integrity_diagnostics_are_eagerly_registered() {
    let handles = register_all();
    for code in [BuiltinRefusal::CODE, NativeBuiltin::CODE] {
        assert_eq!(
            LOGIC_DIAG_CODES
                .iter()
                .filter(|listed| **listed == code)
                .count(),
            1
        );
    }
    for expected in [BuiltinRefusal::register(), NativeBuiltin::register()] {
        assert_eq!(
            handles.iter().filter(|handle| **handle == expected).count(),
            1
        );
    }
}

/// The complete catalog and eager registration retain unique, matching diagnostic codes.
#[test]
fn every_logic_code_interns_with_no_collision() {
    let handles = register_all();
    assert_eq!(
        handles.len(),
        LOGIC_DIAG_CODES.len(),
        "register_all() and LOGIC_DIAG_CODES must enumerate the same kinds"
    );
    for code in LOGIC_DIAG_CODES {
        assert!(
            intern_code(code).is_ok(),
            "logic code `{code}` did not intern after register_all()"
        );
    }
    let distinct_strings: HashSet<&&str> = LOGIC_DIAG_CODES.iter().collect();
    assert_eq!(
        distinct_strings.len(),
        LOGIC_DIAG_CODES.len(),
        "duplicate logic diagnostic code string detected"
    );
    let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
    assert_eq!(distinct_handles.len(), handles.len());
}
