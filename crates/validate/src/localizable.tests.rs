// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::LOCALIZABLE_PREDICATES;

/// A botched relocation that drops entries fails immediately.
#[test]
fn authority_pins_the_full_localizable_surface() {
    assert_eq!(
        LOCALIZABLE_PREDICATES.len(),
        14,
        "the localizable authority must carry all 14 predicates"
    );
    // No duplicates snuck in.
    let mut sorted = LOCALIZABLE_PREDICATES.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), LOCALIZABLE_PREDICATES.len());
}
