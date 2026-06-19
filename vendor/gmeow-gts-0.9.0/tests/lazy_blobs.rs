// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

use ciborium::value::Value;
use gmeow_gts::codec::Codec;
use gmeow_gts::model::{Graph, Term, TermKind};
use gmeow_gts::reader::read;
use gmeow_gts::wire::{canonical, content_id, digest_str};
use gmeow_gts::writer::Writer;

const BLOB_MT: &str = "text/plain";
type TestQuad = (usize, usize, usize, Option<usize>);

fn iv(n: i64) -> Value {
    Value::Integer(ciborium::value::Integer::from(n))
}

fn term_triple() -> ([Term; 3], [TestQuad; 1]) {
    (
        [
            Term {
                kind: TermKind::Iri,
                value: Some("https://example.org/s".into()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            Term {
                kind: TermKind::Iri,
                value: Some("https://example.org/p".into()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            Term {
                kind: TermKind::Iri,
                value: Some("https://example.org/o".into()),
                datatype: None,
                lang: None,
                reifier: None,
            },
        ],
        [(0, 1, 2, None)],
    )
}

fn zstd_bytes(data: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Uncompressed)
}

fn blob_pub(digest: Option<&str>) -> Vec<(Value, Value)> {
    let mut pub_meta = vec![("mt".into(), BLOB_MT.into())];
    if let Some(digest) = digest {
        pub_meta.push(("digest".into(), digest.into()));
    }
    pub_meta
}

fn blob_frame(
    prev: &[u8],
    raw: Option<Vec<u8>>,
    codecs: &[i64],
    pub_meta: Vec<(Value, Value)>,
) -> Vec<u8> {
    let mut frame: Vec<(Value, Value)> = vec![
        ("t".into(), "blob".into()),
        ("prev".into(), Value::Bytes(prev.to_vec())),
    ];
    if let Some(raw) = raw {
        frame.push(("d".into(), Value::Bytes(raw)));
    }
    if !codecs.is_empty() {
        frame.push((
            "x".into(),
            Value::Array(codecs.iter().map(|codec| iv(*codec)).collect()),
        ));
    }
    if !pub_meta.is_empty() {
        frame.push(("pub".into(), Value::Map(pub_meta)));
    }
    frame.sort_by_key(|(key, _)| canonical(key));
    let id = content_id(&frame);
    frame.push(("id".into(), Value::Bytes(id)));
    frame.sort_by_key(|(key, _)| canonical(key));
    canonical(&Value::Map(frame))
}

fn zstd_blob_file(blob: &[u8]) -> Vec<u8> {
    let writer = Writer::new("generic");
    let digest = digest_str(blob);
    let mut out = writer.to_bytes();
    out.extend(blob_frame(
        writer.head(),
        Some(zstd_bytes(blob)),
        &[2],
        blob_pub(Some(&digest)),
    ));
    out
}

#[test]
fn blob_deferred_until_access() {
    let (terms, quads) = term_triple();
    let blob = b"this raw payload is intentionally not zstd";
    let digest = digest_str(b"declared decoded bytes");

    let mut writer = Writer::new("generic");
    writer.add_terms(&terms);
    writer.add_quads(&quads);
    let mut data = writer.to_bytes();
    data.extend(blob_frame(
        writer.head(),
        Some(blob.to_vec()),
        &[2],
        blob_pub(Some(&digest)),
    ));

    let mut graph = read(&data, true, None);
    assert_eq!(graph.terms.len(), 3);
    assert_eq!(graph.quads.len(), 1);
    assert!(graph.diagnostics.is_empty());
    assert!(graph
        .blob_entry(&digest)
        .is_some_and(|entry| entry.is_lazy()));
    assert!(graph.blob_bytes(&digest).is_err());
}

#[test]
fn blob_access_decompresses_and_caches() {
    let blob = b"hello world ".repeat(1024);
    let digest = digest_str(&blob);
    let mut graph = read(&zstd_blob_file(&blob), true, None);

    assert!(graph
        .blob_entry(&digest)
        .is_some_and(|entry| entry.is_lazy()));
    assert_eq!(graph.blob_bytes(&digest).unwrap(), Some(blob.as_slice()));
    assert!(graph
        .blob_entry(&digest)
        .is_some_and(|entry| !entry.is_lazy()));
    assert_eq!(graph.blob_bytes(&digest).unwrap(), Some(blob.as_slice()));
}

#[test]
fn lazy_blob_identity_codec_is_direct() {
    let blob = b"plain bytes";
    let mut writer = Writer::new("generic");
    writer.add_blob(blob, Some(BLOB_MT), None);

    let graph = read(&writer.to_bytes(), true, None);
    let digest = digest_str(blob);
    let entry = graph.blob_entry(&digest).expect("blob entry");
    assert!(!entry.is_lazy());
    assert_eq!(entry.cached_bytes(), Some(blob.as_slice()));
}

#[test]
fn lazy_blob_no_pub_digest_falls_back_to_eager_decode() {
    let blob = b"legacy blob";
    let writer = Writer::new("generic");
    let mut data = writer.to_bytes();
    data.extend(blob_frame(
        writer.head(),
        Some(zstd_bytes(blob)),
        &[2],
        blob_pub(None),
    ));

    let digest = digest_str(blob);
    let graph = read(&data, true, None);
    let entry = graph.blob_entry(&digest).expect("eager decoded blob");
    assert!(!entry.is_lazy());
    assert_eq!(entry.cached_bytes(), Some(blob.as_slice()));
    assert!(graph.blob_meta.iter().any(|(d, _)| d == &digest));
}

#[test]
fn lazy_blob_unknown_codec_degrades() {
    let writer = Writer::new("generic");
    let mut data = writer.to_bytes();
    data.extend(blob_frame(
        writer.head(),
        Some(b"x".to_vec()),
        &[99],
        blob_pub(None),
    ));

    let graph = read(&data, true, None);
    assert!(graph
        .opaque
        .iter()
        .any(|node| node.reason == "unknown-codec"));
    assert!(graph
        .diagnostics
        .iter()
        .any(|diag| diag.code == "UnknownCodec"));
}

#[test]
fn lazy_blob_meta_is_eager() {
    let blob = b"metadata test";
    let digest = digest_str(blob);
    let graph = read(&zstd_blob_file(blob), true, None);

    assert!(graph
        .blob_entry(&digest)
        .is_some_and(|entry| entry.is_lazy()));
    let meta = graph
        .blob_meta
        .iter()
        .find(|(d, _)| d == &digest)
        .map(|(_, meta)| meta)
        .expect("metadata");
    let Value::Map(entries) = meta else {
        panic!("blob metadata is a map");
    };
    assert!(entries.iter().any(|(key, value)| {
        matches!((key, value), (Value::Text(k), Value::Text(v)) if k == "mt" && v == BLOB_MT)
    }));
}

#[test]
fn lazy_blob_multi_segment_union_does_not_decode() {
    let blob_a = b"segment a ".repeat(128);
    let blob_b = b"segment b ".repeat(128);
    let mut data = zstd_blob_file(&blob_a);
    data.extend(zstd_blob_file(&blob_b));

    let graph = read(&data, true, None);
    assert_eq!(graph.blobs.len(), 2);
    assert!(graph.blobs.iter().all(|(_, entry)| entry.is_lazy()));
}

#[test]
fn lazy_blob_external_records_layout_iou() {
    let digest = format!("blake3:{}", "00".repeat(32));
    let writer = Writer::new("generic");
    let mut data = writer.to_bytes();
    data.extend(blob_frame(
        writer.head(),
        None,
        &[],
        blob_pub(Some(&digest)),
    ));

    let graph = read(&data, true, None);
    assert!(graph.blobs.is_empty());
    assert!(graph.blob_meta.iter().any(|(d, _)| d == &digest));
    assert!(graph.diagnostics.is_empty());
}

#[test]
fn lazy_blob_lookup_and_iteration_helpers() {
    let mut graph = Graph::default();
    graph.set_blob("a".into(), b"A".to_vec());
    graph.set_lazy_blob(
        "b".into(),
        b"B".to_vec(),
        vec![Codec {
            name: "identity".into(),
            cls: "encode".into(),
        }],
    );

    assert_eq!(graph.blobs.len(), 2);
    assert_eq!(graph.blob_bytes("a").unwrap(), Some(&b"A"[..]));
    assert_eq!(graph.blob_bytes_cloned("b").unwrap(), Some(b"B".to_vec()));
    assert!(graph.blob_entry("b").is_some_and(|entry| !entry.is_lazy()));
    assert_eq!(
        graph.decoded_blobs().unwrap(),
        vec![("a".into(), b"A".to_vec()), ("b".into(), b"B".to_vec())]
    );
}

#[test]
fn lazy_blob_decode_failure_raises_on_access() {
    let mut graph = Graph::default();
    graph.set_lazy_blob(
        "a".into(),
        b"not zstd".to_vec(),
        vec![Codec {
            name: "zstd".into(),
            cls: "compress".into(),
        }],
    );

    assert!(graph.blob_entry("a").is_some_and(|entry| entry.is_lazy()));
    assert!(graph.blob_bytes("a").is_err());
    assert!(graph.blob_entry("a").is_some_and(|entry| entry.is_lazy()));
}
