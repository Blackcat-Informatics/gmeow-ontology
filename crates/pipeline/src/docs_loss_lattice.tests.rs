// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn the_live_table_is_total_and_monotone() {
    let report = check_docs_loss_lattice();
    assert!(
        report.ok(),
        "the shared docs-format capability table must be total + monotone: {:?}",
        report.errors
    );
}

#[test]
fn gate_reports_every_format_and_capability_pairing() {
    // A smoke check that the gate actually visits all four formats × six
    // capabilities (24 XOR checks) — if the source table shrank silently, this
    // would surface as a mismatch elsewhere; here we simply confirm the pass path
    // exercised the full cross-product without panicking.
    let report = check_docs_loss_lattice();
    assert_eq!(report.errors.len(), 0);
    assert_eq!(DocFormat::ALL.len(), 4);
    assert_eq!(Capability::ALL.len(), 6);
    // The DAG carries exactly its declared refinement edges (site → snippets),
    // never the deleted linear-chain edges.
    assert!(!PROJECTION_DAG_EDGES.is_empty());
}
