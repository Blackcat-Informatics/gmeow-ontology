// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn native_audit_replay_retains_identities_and_the_gate_verdict() {
    let report = super::super::tests::synthetic_claim_audit("sh:Violation");
    let restored = restore(&encode(&report).unwrap()).unwrap();
    assert_eq!(
        claim_audit_diagnostics(&report),
        claim_audit_diagnostics(&restored)
    );
    assert_eq!(report.query_rows, restored.query_rows);
    assert_eq!(report.ok(), restored.ok());
    assert!(!restored.ok());
    assert!(restore(br#"{"schema":2,"query_rows":{},"nodes":[],"claims":[]}"#).is_err());
}
