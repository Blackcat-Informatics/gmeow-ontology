// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Mutex;

use purrdf::gts_view::GtsFoldView;
use purrdf_lex::cbor::Value;

use super::*;
use crate::bundle_blobs::REP_REASONING;

fn graph(members: &[(&str, &[u8])]) -> Graph {
    let bytes = purrdf::ustar::write_archive_borrowed(members.iter().copied()).unwrap();
    archive_graph(bytes)
}

fn archive_graph(bytes: Vec<u8>) -> Graph {
    let digest = format!("blake3:{}", blake3::hash(&bytes).to_hex());
    let mut graph = Graph::default();
    graph.set_blob(digest.clone(), bytes);
    graph.set_blob_meta(
        digest,
        Value::Map(vec![(
            Value::Text("rep".into()),
            Value::Text(REP_REASONING.into()),
        )]),
    );
    graph
}

fn bundle(graph: Graph) -> Bundle {
    Bundle {
        view: GtsFoldView::new(graph).unwrap(),
        prepared_reasoned_gates: Mutex::new(None),
    }
}

#[test]
fn native_law_member_is_selected_once_per_exact_retained_bundle() {
    let first = bundle(graph(&[
        ("other.json", b"unrelated"),
        (REASONED_GATES_MEMBER, b"first native laws"),
    ]));
    let bytes = first.prepared_reasoned_gates().unwrap();
    assert_eq!(&*bytes, b"first native laws");
    assert!(Arc::ptr_eq(
        &bytes,
        &first.prepared_reasoned_gates().unwrap()
    ));
    let second = bundle(graph(&[(REASONED_GATES_MEMBER, b"different native laws")]));
    let other = second.prepared_reasoned_gates().unwrap();
    assert_eq!(&*other, b"different native laws");
    assert!(!Arc::ptr_eq(&bytes, &other));
}

#[test]
fn native_law_selection_rejects_missing_ambiguous_and_corrupt_sources() {
    assert!(read_member(&Graph::default()).is_err());
    assert!(read_member(&graph(&[("other.json", b"unrelated")])).is_err());
    assert!(
        read_member(&graph(&[
            (REASONED_GATES_MEMBER, b"first"),
            (REASONED_GATES_MEMBER, b"second")
        ]))
        .is_err()
    );
    let mut repeated_rep = graph(&[(REASONED_GATES_MEMBER, b"first")]);
    repeated_rep
        .blob_meta
        .push(("another-digest".into(), repeated_rep.blob_meta[0].1.clone()));
    assert!(read_member(&repeated_rep).is_err());
    let mut bad_digest = graph(&[(REASONED_GATES_MEMBER, b"first")]);
    bad_digest.blobs[0].0 = "blake3:wrong".into();
    bad_digest.blob_meta[0].0 = "blake3:wrong".into();
    assert!(read_member(&bad_digest).is_err());
    let mut missing_blob = graph(&[(REASONED_GATES_MEMBER, b"first")]);
    missing_blob.blobs.clear();
    assert!(read_member(&missing_blob).is_err());
}

#[test]
fn selected_native_laws_do_not_hide_a_malformed_later_archive_member() {
    let mut bytes = purrdf::ustar::write_archive_borrowed([
        (REASONED_GATES_MEMBER, b"selected laws".as_slice()),
        ("later.json", b"later member".as_slice()),
    ])
    .unwrap();
    // The selected body's header plus padded body occupy 1024 bytes. Corrupt
    // the following member's size while retaining a correct whole-blob digest.
    bytes[1024 + 124] = b'8';
    assert!(read_member(&archive_graph(bytes)).is_err());
}

#[test]
/// Admission access consumes only the retained archive, never a source checkout.
fn canon_admission_reader_retains_typed_observation_and_rejects_bad_members() {
    use gmeow_logic_compile::frontend::{
        DocumentSelection, FofSemanticProfile, RootSelection, SourceGraphSelection, SourceSelection,
    };
    use gmeow_logic_compile::tptp::CANON_ADMISSION_MEMBER;

    let selection = SourceSelection {
        graph: SourceGraphSelection::DefaultGraph,
        documents: DocumentSelection::AnonymousSource,
        formulas: RootSelection::All,
        rules: RootSelection::All,
        owners: RootSelection::All,
    };
    // Inert observation fixture: no source compilation, stages or corpus producer.
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "admission": {
            "profile": FofSemanticProfile::default(),
            "source_digest": "synthetic source identity",
            "program_digest": "synthetic program identity",
            "selection_digest": selection.digest(),
            "selection": selection,
            "status": "Empty",
            "units": [], "declared_classes": [], "declared_builtins": []
        }
    }))
    .unwrap();
    let selected = bundle(graph(&[(CANON_ADMISSION_MEMBER, &bytes)]));
    let observation = selected.canon_proof_admission(bytes.len()).unwrap();
    assert_eq!(
        observation.admission().source_digest,
        "synthetic source identity"
    );
    assert!(selected.canon_proof_admission(bytes.len() - 1).is_err());
    assert!(
        bundle(Graph::default())
            .canon_proof_admission(bytes.len())
            .is_err()
    );
    assert!(
        bundle(graph(&[("other.json", &bytes)]))
            .canon_proof_admission(bytes.len())
            .is_err()
    );
    assert!(
        bundle(graph(&[
            (CANON_ADMISSION_MEMBER, &bytes),
            (CANON_ADMISSION_MEMBER, &bytes)
        ]))
        .canon_proof_admission(bytes.len())
        .is_err()
    );
    assert!(
        bundle(archive_graph(b"not an archive".to_vec()))
            .canon_proof_admission(bytes.len())
            .is_err()
    );
    let error = bundle(graph(&[(CANON_ADMISSION_MEMBER, b"not JSON")]))
        .canon_proof_admission(bytes.len())
        .unwrap_err();
    assert_eq!(
        gmeow_errors::code::code_str(error.code()),
        "logic-compile.frontend"
    );
    assert!(error.to_string().contains("decode canon proof admission"));
}
