// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Literal tuples and in-memory diagnostic values only; no stage or corpus runs.
use super::*;

#[test]
fn json_diag_contribution_empty_literal_binds_stage_and_has_no_trailing_newline() {
    let bytes = crate::node::encode_diag_nodes("étape\n😀", &[]);
    assert_eq!(bytes, "[\"étape\\n😀\",[]]".as_bytes());
    let (stage, nodes) = decode_contribution(&bytes).unwrap();
    assert_eq!(stage, "étape\n😀");
    assert!(nodes.is_empty());
    assert_ne!(bytes, crate::node::encode_diag_nodes("different", &[]));
}

#[test]
fn json_diag_contribution_preserves_real_node_identity_and_replay() {
    let mut ledger = gmeow_errors::DiagLedger::new();
    ledger.attach(
        gmeow_errors::Diag::from(std::io::Error::other("native cause"))
            .with_focus("urn:authored:focus"),
        gmeow_errors::StageId::new("source"),
    );
    let nodes: Vec<_> = ledger.emit_sorted().into_iter().cloned().collect();
    let bytes = crate::node::encode_diag_nodes("selected-stage", &nodes);
    let (stage, restored) = decode_contribution(&bytes).unwrap();
    assert_eq!(stage, "selected-stage");
    assert_eq!(restored, nodes);
    let mut replay = gmeow_errors::DiagLedger::new();
    replay.replay(restored);
    assert_eq!(replay.verdict(), ledger.verdict());
    assert_eq!(
        crate::node::encode_diag_nodes(
            &stage,
            &replay
                .emit_sorted()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        ),
        bytes
    );
}

#[test]
fn json_diag_contribution_refuses_wrong_tuple_arity_types_and_malformed_nodes() {
    for bytes in [
        b"[]".as_slice(),
        b"[\"stage\"]",
        b"[\"stage\",[],null]",
        b"[null,[]]",
        b"[\"stage\",null]",
        b"[\"stage\",[{}]]",
        b"{\"stage\":\"stage\",\"nodes\":[]}",
        b"[\"stage\",[]] trailing",
    ] {
        assert!(decode_contribution(bytes).is_err(), "{bytes:?}");
    }
}
