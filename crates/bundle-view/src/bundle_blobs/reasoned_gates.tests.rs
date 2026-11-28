// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Mutex;

use gmeow_gts_profile::archive::{CANON_ADMISSION_REP, PREPARED_REASONING_REP};
use purrdf::gts::model::BlobEntry;
use purrdf::gts_view::GtsFoldView;
use purrdf_lex::cbor::Value;

use super::*;

/// A correctly hashed blob cannot masquerade as native canon under another media type.
#[test]
fn the_native_canon_reader_requires_exact_media() {
    use gmeow_gts_profile::archive::PREPARED_CANON_REP;

    let fixture = [0xa0_u8];
    let graph = |media_type: &str| {
        let mut graph = blob_graph(PREPARED_CANON_REP, fixture.to_vec());
        let Value::Map(fields) = &mut graph.blob_meta[0].1 else {
            panic!("the synthetic public metadata is a map");
        };
        fields.push((Value::Text("mt".into()), Value::Text(media_type.into())));
        graph
    };
    assert!(
        bundle(graph("application/json"))
            .prepared_native_canon()
            .is_err()
    );
    assert_eq!(
        bundle(graph("application/cbor"))
            .prepared_native_canon()
            .expect("exact native blob")
            .as_ref(),
        fixture
    );
}

/// Package inert members through the real archive codec, without invoking producers.
fn graph(members: &[(&str, &[u8])]) -> Graph {
    let bytes = purrdf::ustar::write_archive_borrowed(members.iter().copied()).unwrap();
    blob_graph(PREPARED_REASONING_REP, bytes)
}

/// Bind retained bytes to their actual digest and representation.
fn blob_graph(rep: &str, bytes: Vec<u8>) -> Graph {
    let digest = format!(
        "blake3:{}",
        purrdf_hash::hex::encode(purrdf_hash::blake3::hash(&bytes).as_bytes())
    );
    let mut graph = Graph::default();
    graph.set_blob(digest.clone(), bytes);
    graph.set_blob_meta(
        digest,
        Value::Map(vec![(Value::Text("rep".into()), Value::Text(rep.into()))]),
    );
    graph
}

/// Give the reader an isolated retained bundle and an initially empty member cache.
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
    let error = read_member(&blob_graph(PREPARED_REASONING_REP, bytes)).unwrap_err();
    let refusal = error
        .downcast_ref::<gmeow_gts_profile::error::NativeArchive>()
        .expect("the selected-member boundary retains native archive refusal");
    assert!(
        std::error::Error::source(refusal)
            .unwrap()
            .downcast_ref::<purrdf::ustar::UstarError>()
            .is_some()
    );
}

#[test]
/// Canon admission authenticates its own blob and applies the byte cap to decoding.
fn canon_admission_reader_authenticates_the_blob_under_the_caller_limit() {
    use gmeow_logic_compile::frontend::{
        DocumentSelection, FofSemanticProfile, RootSelection, SourceGraphSelection, SourceSelection,
    };

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
    let mut graph_with_canon = blob_graph(CANON_ADMISSION_REP, bytes.clone());
    let encoded = purrdf::gts::codec::encode_chain(&["gzip".to_owned()], &bytes)
        .expect("synthetic canon admission compresses");
    graph_with_canon.blobs[0].1 = BlobEntry::lazy(
        encoded,
        vec![purrdf::gts::codec::Codec::new("gzip", "compress")],
    );
    let selected = bundle(graph_with_canon);
    let observation = selected.canon_proof_admission(bytes.len()).unwrap();
    assert_eq!(
        observation.admission().source_digest,
        "synthetic source identity"
    );
    let limit_error = selected.canon_proof_admission(bytes.len() - 1).unwrap_err();
    let refusal = limit_error
        .downcast_ref::<gmeow_gts_profile::error::Archive>()
        .expect("canon admission retains the archive diagnostic");
    let cause = std::error::Error::source(refusal)
        .and_then(|cause| cause.downcast_ref::<purrdf::gts::codec::CodecError>());
    assert!(matches!(
        cause,
        Some(purrdf::gts::codec::CodecError::Limit { limit }) if *limit == bytes.len() - 1
    ));
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
    let mut ambiguous = blob_graph(CANON_ADMISSION_REP, bytes.clone());
    ambiguous
        .blob_meta
        .push(("another-digest".into(), ambiguous.blob_meta[0].1.clone()));
    assert!(
        bundle(ambiguous)
            .canon_proof_admission(bytes.len())
            .is_err()
    );
    let error = bundle(blob_graph(CANON_ADMISSION_REP, b"invalid JSON".to_vec()))
        .canon_proof_admission(bytes.len())
        .unwrap_err();
    assert_eq!(
        gmeow_errors::code::code_str(error.code()),
        "logic-compile.frontend"
    );
    assert!(error.to_string().contains("decode canon proof admission"));
}
