// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{
    CODE_DRIFT, CODE_SUPERSET_MISSING, DiagLedger, attach_canonical_drift_findings,
    attach_pipeline_finding,
};

/// F3: the drift/superset producers intern their diagnostics into the carrier
/// ledger, and the wire findings are the ledger's projection — the ledger is
/// load-bearing, not a dark parallel path. This exercises the exact helper and
/// projection `run_full` uses.
#[test]
fn drift_findings_flow_through_the_carrier_ledger() {
    let mut ledger = DiagLedger::new();
    attach_pipeline_finding(
        &mut ledger,
        CODE_DRIFT,
        "generated/a.ttl",
        "generated/a.ttl differs from the committed artifact".to_owned(),
    );
    attach_pipeline_finding(
        &mut ledger,
        CODE_SUPERSET_MISSING,
        "generated/b.ttl",
        "generated/b.ttl has no carrier representative in gmeow.gts".to_owned(),
    );

    // Two distinct drifting paths → two distinct content-addressed witnesses
    // (the path is the focus, so a shared code never collapses them).
    assert_eq!(ledger.len(), 2, "each drifting path is a distinct witness");

    // The wire findings `run_full` returns ARE the ledger projection.
    let findings = ledger.project_report("gmeow-pipeline").findings;
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().any(|f| f.code == CODE_DRIFT));
    assert!(findings.iter().any(|f| f.code == CODE_SUPERSET_MISSING));
    // Deleting the fold (an empty ledger) yields zero findings — proving the
    // findings are sourced from the ledger, not a bypass path.
    assert!(
        DiagLedger::new()
            .project_report("gmeow-pipeline")
            .findings
            .is_empty()
    );
}

#[test]
/// Canonical drift becomes consumer-visible findings rather than path-only state.
fn canonical_drift_paths_are_visible_in_the_carrier_ledger() {
    let mut ledger = DiagLedger::new();
    attach_canonical_drift_findings(
        &mut ledger,
        &["CITATION.cff".to_owned(), "ontology/gmeow.ttl".to_owned()],
    );

    let findings = ledger.project_report("gmeow-pipeline").findings;
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|finding| finding.code == CODE_DRIFT));
    assert!(findings.iter().any(|finding| {
        finding.message == "CITATION.cff differs from the canonical abstract projection"
    }));
}
