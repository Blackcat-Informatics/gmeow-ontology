// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bounded nested-GTS Full Reader behavior.

use std::path::Path;

use gmeow_gts::model::{Term, TermKind};
use gmeow_gts::nested::{read_nested, GTS_MEDIA_TYPE};
use gmeow_gts::wire::digest_str;
use gmeow_gts::writer::Writer;

const EX: &str = "https://example.org/";

fn iri(value: &str) -> Term {
    Term {
        kind: TermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn lit(value: &str) -> Term {
    Term {
        kind: TermKind::Literal,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn tiny_graph(label: &str) -> Vec<u8> {
    let mut writer = Writer::new("dist");
    writer.add_terms(&[
        iri(&(EX.to_string() + label)),
        iri(&(EX.to_string() + "label")),
        lit(label),
    ]);
    writer.add_quads(&[(0, 1, 2, None)]);
    writer.to_bytes()
}

fn bundle(child: &[u8]) -> Vec<u8> {
    let mut writer = Writer::new("bundle");
    writer.add_blob(child, Some(GTS_MEDIA_TYPE), None);
    writer.to_bytes()
}

fn labeled_bundle(child: &[u8], label: &str) -> Vec<u8> {
    let mut writer = Writer::new("bundle");
    writer.add_terms(&[
        iri(&(EX.to_string() + label)),
        iri(&(EX.to_string() + "label")),
        lit(label),
    ]);
    writer.add_quads(&[(0, 1, 2, None)]);
    writer.add_blob(child, Some(GTS_MEDIA_TYPE), None);
    writer.to_bytes()
}

#[test]
fn read_nested_exposes_subgraph_by_blob_digest() {
    let child = tiny_graph("child");
    let outer = bundle(&child);

    let result = read_nested(&outer, 3, 16 * 1024 * 1024);

    let digest = digest_str(&child);
    assert!(result.subgraph(&digest).is_some());
    assert_eq!(
        result.subgraph(&digest).unwrap().quads,
        vec![(0, 1, 2, None)]
    );
    assert!(!result
        .diagnostics
        .iter()
        .any(|d| d.code == "RecursionLimit"));
}

#[test]
fn read_nested_stops_at_recursion_limit() {
    let grandchild = tiny_graph("grandchild");
    let child = bundle(&grandchild);
    let outer = bundle(&child);

    let result = read_nested(&outer, 1, 16 * 1024 * 1024);

    assert!(result.subgraph(&digest_str(&child)).is_some());
    assert!(result.subgraph(&digest_str(&grandchild)).is_none());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "RecursionLimit"));
}

#[test]
fn read_nested_stops_at_decoded_size_budget() {
    let child = tiny_graph("oversized");
    let outer = bundle(&child);

    let result = read_nested(&outer, 3, child.len() - 1);

    assert!(result.subgraph(&digest_str(&child)).is_none());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "RecursionLimit"));
}

#[test]
fn read_nested_charges_duplicate_nested_digest_once() {
    let grandchild = tiny_graph("shared-grandchild");
    let child_a = labeled_bundle(&grandchild, "child-a");
    let child_b = labeled_bundle(&grandchild, "child-b");
    let mut writer = Writer::new("bundle");
    writer.add_blob(&child_a, Some(GTS_MEDIA_TYPE), None);
    writer.add_blob(&child_b, Some(GTS_MEDIA_TYPE), None);

    let result = read_nested(
        &writer.to_bytes(),
        3,
        child_a.len() + child_b.len() + grandchild.len(),
    );

    assert!(result.subgraph(&digest_str(&child_a)).is_some());
    assert!(result.subgraph(&digest_str(&child_b)).is_some());
    assert!(result.subgraph(&digest_str(&grandchild)).is_some());
    assert!(!result
        .diagnostics
        .iter()
        .any(|d| d.code == "RecursionLimit"));
}

#[test]
fn read_nested_records_damaged_nested_payload() {
    let damaged = b"not a cbor sequence";
    let outer = bundle(damaged);

    let result = read_nested(&outer, 3, 16 * 1024 * 1024);

    assert!(result.subgraph(&digest_str(damaged)).is_none());
    assert!(result.diagnostics.iter().any(|d| d.code == "DamagedFrame"));
}

#[test]
fn nested_recursion_security_vector_descriptor_is_still_present() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../vectors/security/nested-recursion-limit.json");
    let vector: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(vector["id"], "nested-recursion-limit");
    assert_eq!(vector["expected_diagnostics"][0], "RecursionLimit");
}
