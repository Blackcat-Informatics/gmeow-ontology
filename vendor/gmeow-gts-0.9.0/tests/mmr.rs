// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

use ciborium::value::Value;

use gmeow_gts::mmr::{prove, verify_proof, Proof};
use gmeow_gts::reader::read;
use gmeow_gts::writer::Writer;

fn proofs() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vectors/proofs")
}

fn iv(n: u64) -> Value {
    Value::from(n)
}

#[test]
fn positive_proof_fixture_verifies() {
    let text = std::fs::read_to_string(proofs().join("mmr-basic-proof.json")).unwrap();
    let proof = Proof::from_json(&text).expect("fixture parses");
    verify_proof(&proof).expect("fixture verifies");
}

#[test]
fn negative_proof_fixture_fails() {
    let text = std::fs::read_to_string(proofs().join("mmr-basic-proof-bad-root.json")).unwrap();
    let proof = Proof::from_json(&text).expect("fixture parses");
    assert!(verify_proof(&proof).is_err());
}

#[test]
fn detached_proof_verifies_without_source_file() {
    let frame_ids = vec![vec![1; 32], vec![2; 32], vec![3; 32], vec![4; 32]];
    let proof = prove(&frame_ids, 2).expect("proof exists");
    let json = proof.to_json();
    let parsed = Proof::from_json(&json).expect("proof JSON parses");
    verify_proof(&parsed).expect("proof verifies");
}

#[test]
fn detached_proof_rejects_tampered_path() {
    let frame_ids = vec![vec![1; 32], vec![2; 32], vec![3; 32], vec![4; 32]];
    let mut proof = prove(&frame_ids, 2).expect("proof exists");
    proof.path[0].hash[0] ^= 1;
    assert!(verify_proof(&proof).is_err());
}

#[test]
fn reader_diagnoses_bad_index_mmr() {
    let mut w = Writer::new("generic");
    w.add_blob(b"hello", Some("text/plain"), None);
    let head = w.add_blob(b"world", Some("text/plain"), None);
    w.add_frame(
        "index",
        Some(Value::Map(vec![
            ("count".into(), iv(2)),
            ("head".into(), Value::Bytes(head)),
            ("mmr".into(), Value::Bytes(vec![0; 32])),
        ])),
        None,
        None,
        None,
    );
    let graph = read(&w.to_bytes(), true, None);
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "IndexMmrError"),
        "diagnostics: {:?}",
        graph.diagnostics
    );
}
