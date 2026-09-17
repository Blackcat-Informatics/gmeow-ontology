// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn every_declared_code_registers() {
    let registered = register_all();
    assert_eq!(registered.len(), GTS_PROFILE_DIAG_CODES.len());
}
