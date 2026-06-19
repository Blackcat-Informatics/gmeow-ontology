// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rust writer transform/encryption parity with Python `Writer.add_frame`.

use ciborium::value::Value;
use gmeow_gts::codec::Codec;
use gmeow_gts::reader::{read, read_with_options, ReadOptions};
use gmeow_gts::wire::{iter_items, map_get, unwrap_header};
use gmeow_gts::writer::{Encrypt0Options, FrameOptions, Writer, WriterOptions};

fn text_map(key: &str, value: &str) -> Value {
    Value::Map(vec![(key.into(), value.into())])
}

fn bool_map(key: &str, value: bool) -> Value {
    Value::Map(vec![(key.into(), Value::Bool(value))])
}

fn meta_text(graph: &gmeow_gts::model::Graph, key: &str) -> Option<String> {
    graph.meta.iter().find_map(|(stored, value)| match value {
        Value::Text(value) if stored == key => Some(value.clone()),
        _ => None,
    })
}

fn meta_bool(graph: &gmeow_gts::model::Graph, key: &str) -> Option<bool> {
    graph.meta.iter().find_map(|(stored, value)| match value {
        Value::Bool(value) if stored == key => Some(*value),
        _ => None,
    })
}

#[test]
fn writer_authors_compressed_frames_that_fold_cleanly() {
    for codec in ["gzip", "zstd", "zstd-rsyncable"] {
        let mut writer = Writer::new("generic");
        writer
            .add_frame_with_options(
                "meta",
                FrameOptions {
                    payload: Some(text_map("codec", codec)),
                    transform: vec![codec.to_string()],
                    ..FrameOptions::default()
                },
            )
            .expect("transformed frame is authored");

        let graph = read(&writer.to_bytes(), false, None);
        assert!(
            graph.diagnostics.is_empty(),
            "{codec}: {:?}",
            graph.diagnostics
        );
        assert_eq!(meta_text(&graph, "codec").as_deref(), Some(codec));
    }
}

#[test]
fn encrypted_writer_frame_is_opaque_without_key_and_clear_with_key() {
    let key = [7u8; 32];
    let mut writer = Writer::new("opaque");
    writer
        .add_frame_with_options(
            "meta",
            FrameOptions {
                payload: Some(bool_map("private", true)),
                transform: vec!["zstd".to_string()],
                encrypt: Some(Encrypt0Options {
                    kid: "did:court".to_string(),
                    key,
                }),
                ..FrameOptions::default()
            },
        )
        .expect("encrypted frame is authored");
    let data = writer.to_bytes();

    let opaque = read(&data, false, None);
    assert!(opaque
        .diagnostics
        .iter()
        .any(|diag| diag.code == "MissingKey"));
    assert_eq!(opaque.opaque.len(), 1);
    assert_eq!(opaque.opaque[0].reason, "missing-key");
    assert_eq!(
        opaque.opaque[0]
            .recipients
            .as_ref()
            .and_then(|recipients| recipients.first())
            .and_then(|recipient| match recipient {
                Value::Map(entries) => entries.iter().find_map(|(key, value)| match (key, value) {
                    (Value::Text(key), Value::Text(value)) if key == "kid" => {
                        Some(value.as_str())
                    }
                    _ => None,
                }),
                _ => None,
            }),
        Some("did:court")
    );
    assert_eq!(meta_bool(&opaque, "private"), None);

    let resolver = |kid: &str| (kid == "did:court").then_some(key);
    let clear = read_with_options(
        &data,
        ReadOptions::new(false, None).with_content_key(&resolver),
    );
    assert!(clear.diagnostics.is_empty(), "{:?}", clear.diagnostics);
    assert!(clear.opaque.is_empty());
    assert_eq!(meta_bool(&clear, "private"), Some(true));
}

#[test]
fn encrypted_writer_frame_with_wrong_key_degrades_to_missing_key() {
    let mut writer = Writer::new("opaque");
    writer
        .add_frame_with_options(
            "meta",
            FrameOptions {
                payload: Some(bool_map("private", true)),
                encrypt: Some(Encrypt0Options {
                    kid: "did:court".to_string(),
                    key: [7u8; 32],
                }),
                ..FrameOptions::default()
            },
        )
        .expect("encrypted frame is authored");

    let wrong = [8u8; 32];
    let resolver = |kid: &str| (kid == "did:court").then_some(wrong);
    let graph = read_with_options(
        &writer.to_bytes(),
        ReadOptions::new(false, None).with_content_key(&resolver),
    );

    assert!(graph
        .diagnostics
        .iter()
        .any(|diag| diag.code == "MissingKey"));
    assert_eq!(graph.opaque[0].reason, "missing-key");
    assert_eq!(meta_bool(&graph, "private"), None);
}

#[test]
fn writer_options_support_header_metadata_magic_toggle_and_custom_catalog() {
    let mut writer = Writer::with_options(
        "generic",
        WriterOptions {
            catalog: Some(vec![
                (
                    0,
                    Codec {
                        name: "identity".to_string(),
                        cls: "encode".to_string(),
                    },
                ),
                (
                    7,
                    Codec {
                        name: "cose-encrypt0".to_string(),
                        cls: "encrypt".to_string(),
                    },
                ),
            ]),
            meta: Some(text_map("source", "rust")),
            magic_tag: false,
            layout: None,
        },
    )
    .expect("custom writer options");
    writer.add_meta(text_map("payload", "visible"));

    let data = writer.to_bytes();
    let (items, torn) = iter_items(&data);
    assert_eq!(torn, None);
    let header = unwrap_header(&items[0].1).expect("header without magic tag still unwraps");
    let Some(Value::Map(header_meta)) = map_get(header, "meta") else {
        panic!("header metadata is present");
    };
    assert_eq!(
        map_get(header_meta, "source"),
        Some(&Value::Text("rust".to_string()))
    );

    let graph = read(&data, false, None);
    assert_eq!(meta_text(&graph, "payload").as_deref(), Some("visible"));

    let err = writer
        .add_frame_with_options(
            "meta",
            FrameOptions {
                payload: Some(text_map("codec", "gzip")),
                transform: vec!["gzip".to_string()],
                ..FrameOptions::default()
            },
        )
        .expect_err("custom catalog lacks gzip");
    assert!(err.to_string().contains("gzip"));
}
