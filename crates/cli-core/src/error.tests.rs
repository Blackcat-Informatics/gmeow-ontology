// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::intern_code;
use std::collections::HashSet;

#[test]
fn every_cli_core_code_interns_with_no_collision() {
    let handles = register_all();
    // register_all() and the catalog enumerate the same kinds in the same order.
    assert_eq!(
        handles.len(),
        CLI_CORE_DIAG_CODES.len(),
        "register_all() and CLI_CORE_DIAG_CODES must enumerate the same kinds"
    );

    // Every catalogued code interns (register_all seeded the registry).
    for code in CLI_CORE_DIAG_CODES {
        assert!(
            intern_code(code).is_ok(),
            "cli-core code `{code}` did not intern after register_all()"
        );
    }

    // No two kinds may share a code literal: distinct strings AND distinct
    // interned handles. A duplicate `code = "..."` would fail loudly here.
    let distinct_strings: HashSet<&&str> = CLI_CORE_DIAG_CODES.iter().collect();
    assert_eq!(
        distinct_strings.len(),
        CLI_CORE_DIAG_CODES.len(),
        "duplicate cli-core diagnostic code string detected"
    );
    let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
    assert_eq!(
        distinct_handles.len(),
        handles.len(),
        "two cli-core diagnostic kinds interned to the same code handle"
    );
}
