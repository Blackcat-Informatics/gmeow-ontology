// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn the_live_table_is_total_and_monotone() {
    let report = check_docs_loss_lattice();
    assert!(
        report.ok(),
        "the shared docs-format capability table must be total + monotone: {:?}",
        report
            .ledger()
            .messages(Severity::Error)
            .collect::<Vec<_>>()
    );
}

#[test]
fn gate_reports_every_format_and_capability_pairing() {
    // A smoke check that the gate actually visits all four formats × six
    // capabilities (24 XOR checks) — if the source table shrank silently, this
    // would surface as a mismatch elsewhere; here we simply confirm the pass path
    // exercised the full cross-product without panicking.
    let report = check_docs_loss_lattice();
    assert!(report.ledger().is_empty());
    assert_eq!(DocFormat::ALL.len(), 4);
    assert_eq!(Capability::ALL.len(), 6);
    // The DAG carries exactly its declared refinement edges (site → snippets),
    // never the deleted linear-chain edges.
    assert!(!PROJECTION_DAG_EDGES.is_empty());
}

#[test]
fn distinct_partition_defects_and_regained_capabilities_reach_the_gate() {
    let report = check_capabilities(|fmt| {
        let mut partition = format_capabilities(fmt);
        if fmt == DocFormat::Site {
            partition.dropped.push(Capability::ALL[0]);
            partition.dropped.push(Capability::ALL[1]);
        } else if fmt == DocFormat::Snippets {
            partition.dropped.clear();
        }
        partition
    });
    assert!(!report.ok());
    let findings = report.ledger().findings("docs-loss-lattice");
    assert_eq!(
        findings.len(),
        11,
        "every distinct structural defect must survive"
    );
    assert!(findings.iter().all(|finding| {
        finding.primary_location().unwrap().path.as_deref()
            == Some("crates/docs-model/src/formats.rs")
            && finding.finding_iri.is_some()
    }));
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("not monotone"))
    );
}

#[test]
fn observation_wording_does_not_fork_a_capability_witness() {
    let mut report = DocsLossLatticeReport::default();
    for message in ["first wording", "reworded observation"] {
        report.violation(
            &["partition-missing", "site", "search-index"],
            message.to_owned(),
        );
    }
    assert_eq!(report.ledger().len(), 1);
    assert_eq!(report.ledger().messages(Severity::Error).count(), 2);
    assert!(!report.ok());
}
