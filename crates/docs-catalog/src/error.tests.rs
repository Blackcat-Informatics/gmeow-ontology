// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::intern_code;
use std::collections::HashSet;

#[test]
fn every_docs_catalog_code_interns_with_no_collision() {
    let handles = register_all();
    assert_eq!(
        handles.len(),
        DOCS_CATALOG_DIAG_CODES.len(),
        "register_all() and DOCS_CATALOG_DIAG_CODES must enumerate the same kinds"
    );
    for code in DOCS_CATALOG_DIAG_CODES {
        assert!(
            intern_code(code).is_ok(),
            "docs-catalog code `{code}` did not intern after register_all()"
        );
    }
    let distinct_strings: HashSet<&&str> = DOCS_CATALOG_DIAG_CODES.iter().collect();
    assert_eq!(
        distinct_strings.len(),
        DOCS_CATALOG_DIAG_CODES.len(),
        "duplicate docs-catalog diagnostic code string detected"
    );
    let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
    assert_eq!(
        distinct_handles.len(),
        handles.len(),
        "two docs-catalog diagnostic kinds interned to the same code handle"
    );
}
