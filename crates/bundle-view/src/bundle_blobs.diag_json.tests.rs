// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored byte-adapter controls only; no GTS or repository fixture is read or produced.
use super::*;

#[test]
fn json_diag_bundle_vector_empty_is_distinct_from_stage_contribution_tuple() {
    assert!(diag_nodes_from_json(b"[]").unwrap().is_empty());
    for bytes in [
        b"[\"stage\",[]]".as_slice(),
        b"null",
        b"{}",
        b"[{}]",
        b"[] trailing",
    ] {
        let error =
            diag_nodes_from_json(bytes).expect_err("malformed public node vector must refuse");
        assert!(error.to_string().contains("diagnostics nodes JSON"));
    }
}

#[test]
fn json_diag_bundle_vector_preserves_native_node_provenance_for_ledger_replay() {
    let mut ledger = gmeow_errors::DiagLedger::new();
    ledger.attach(
        gmeow_errors::Diag::from(std::io::Error::other("authored read"))
            .with_focus("urn:authored:bundle-focus"),
        gmeow_errors::StageId::new("source"),
    );
    let nodes: Vec<_> = ledger.emit_sorted().into_iter().cloned().collect();
    let bytes = purrdf_lex::json::record::to_vec(&nodes);
    let restored = diag_nodes_from_json(&bytes).unwrap();
    assert_eq!(restored, nodes);
    let mut replay = gmeow_errors::DiagLedger::new();
    replay.replay(restored);
    assert_eq!(replay.verdict(), ledger.verdict());
}
