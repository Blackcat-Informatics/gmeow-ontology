// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::intern_code;
use std::collections::HashSet;

#[test]
fn every_bundle_view_code_interns_with_no_collision() {
    let handles = register_all();
    assert_eq!(
        handles.len(),
        BUNDLE_VIEW_DIAG_CODES.len(),
        "register_all() and BUNDLE_VIEW_DIAG_CODES must enumerate the same kinds"
    );
    for code in BUNDLE_VIEW_DIAG_CODES {
        assert!(
            intern_code(code).is_ok(),
            "bundle-view code `{code}` did not intern after register_all()"
        );
    }
    let distinct_strings: HashSet<&&str> = BUNDLE_VIEW_DIAG_CODES.iter().collect();
    assert_eq!(
        distinct_strings.len(),
        BUNDLE_VIEW_DIAG_CODES.len(),
        "duplicate bundle-view diagnostic code string detected"
    );
    let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
    assert_eq!(
        distinct_handles.len(),
        handles.len(),
        "two bundle-view diagnostic kinds interned to the same code handle"
    );
}
