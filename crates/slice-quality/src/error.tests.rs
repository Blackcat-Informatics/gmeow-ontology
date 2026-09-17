// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::collections::BTreeSet;

#[test]
fn every_slice_quality_code_interns_with_no_collision() {
    register_all();
    let codes: BTreeSet<_> = SLICE_QUALITY_DIAG_CODES.iter().map(|reg| reg()).collect();
    assert_eq!(
        codes.len(),
        SLICE_QUALITY_DIAG_CODES.len(),
        "slice-quality diagnostic codes must be collision-free"
    );
}
