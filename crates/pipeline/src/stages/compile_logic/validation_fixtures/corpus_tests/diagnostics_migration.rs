// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Consume the exact producer comparison; never compile a corpus shape in a test.

#[test]
fn diagnostics_legacy_upset_matches_its_exact_generated_constraint() {
    super::module("slices/core/diagnostics/module.ttl");
    let parity = super::fixtures()
        .diagnostics_migration
        .as_ref()
        .unwrap_or_else(|error| panic!("diagnostics migration is not qualified: {error}"));
    assert_eq!(
        parity.legacy_shape,
        "https://blackcatinformatics.ca/gmeow/GateFatalUpsetShape"
    );
    assert_eq!(
        parity.projected_shape,
        "https://blackcatinformatics.ca/gmeow/GateFatalUpsetConstraintProceduralConstraintShape"
    );
    assert_eq!(parity.checked_focuses.len(), 329);
    assert_eq!(parity.expected_violations.len(), 6);
    assert_eq!(parity.legacy, parity.projected);
    assert_eq!(parity.projected.len(), 6);
    let observed: std::collections::BTreeSet<_> = parity
        .projected
        .iter()
        .map(|finding| finding.focus.clone())
        .collect();
    assert_eq!(observed, parity.expected_violations);
}
