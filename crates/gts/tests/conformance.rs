// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! Gate this crate against the frozen language-neutral conformance corpus
//! (GTS-SPEC §18): `generated/gts-vectors/*.gts` + `*.expected.json`, both
//! committed and drift-gated on the Python side. The Python reference oracle
//! and this crate must produce IDENTICAL summaries from the same bytes.

use std::fs;
use std::path::Path;

use gts::model::Graph;
use gts::nquads::to_nquads;
use gts::reader::read;
use gts::wire::hex;
use serde_json::{json, Map, Value};

/// Rebuild the `.expected.json` summary shape from a folded graph.
fn summarize(g: &Graph, mode: &str) -> Value {
    let mut nquads: Vec<String> = to_nquads(g).lines().map(str::to_string).collect();
    nquads.sort();
    let mut opaque_reasons: Vec<String> = g.opaque.iter().map(|o| o.reason.clone()).collect();
    opaque_reasons.sort();
    json!({
        "mode": mode,
        "diagnostics": g.diagnostics.iter().map(|d| d.code.clone()).collect::<Vec<_>>(),
        "terms": g.terms.len(),
        "quads": g.quads.len(),
        "segments": g.segment_heads.len(),
        "segment_heads": g.segment_heads.iter().map(|h| hex(h)).collect::<Vec<_>>(),
        "profiles": g.segment_profiles.clone(),
        "opaque_reasons": opaque_reasons,
        "suppressions": g.suppressions.len(),
        "blobs": blob_summary(g),
        "nquads": nquads,
    })
}

/// Inline blobs: digest -> {size, declared media type} — pins blob folding
/// and metadata retention (§12) across implementations.
fn blob_summary(g: &Graph) -> Value {
    let mut out = Map::new();
    for (digest, data) in &g.blobs {
        let mt =
            g.blob_meta
                .iter()
                .find(|(d, _)| d == digest)
                .and_then(|(_, meta)| {
                    if let ciborium::value::Value::Map(entries) = meta {
                        entries.iter().find_map(|(k, v)| match (k, v) {
                            (
                                ciborium::value::Value::Text(key),
                                ciborium::value::Value::Text(text),
                            ) if key == "mt" => Some(text.clone()),
                            _ => None,
                        })
                    } else {
                        None
                    }
                });
        out.insert(digest.clone(), json!({"size": data.len(), "mt": mt}));
    }
    Value::Object(out)
}

#[test]
fn corpus_matches_frozen_expectations() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../generated/gts-vectors");
    let mut names: Vec<String> = Vec::new();
    let mut expected_names: Vec<String> = Vec::new();
    for entry in fs::read_dir(&dir)
        .expect("generated/gts-vectors must exist — run `uv run gmeow regenerate gts-vectors`")
    {
        let Ok(name) = entry.expect("dir entry").file_name().into_string() else {
            continue;
        };
        if let Some(base) = name.strip_suffix(".gts") {
            names.push(base.to_string());
        } else if let Some(base) = name.strip_suffix(".expected.json") {
            expected_names.push(base.to_string());
        }
    }
    names.sort();
    expected_names.sort();
    // every .gts has an .expected.json and vice versa — an orphan on either
    // side means the corpus generation is incomplete or stale
    assert_eq!(
        names, expected_names,
        "vector basename mismatch between .gts and .expected.json files"
    );
    assert!(
        names.len() >= 16,
        "corpus too small ({} vectors) — generation incomplete?",
        names.len()
    );

    for name in &names {
        let data = fs::read(dir.join(format!("{name}.gts"))).expect("vector bytes");
        let expected: Value = serde_json::from_slice(
            &fs::read(dir.join(format!("{name}.expected.json"))).expect("expected json"),
        )
        .expect("expected json parses");
        let mode = expected["mode"].as_str().expect("mode field");
        let g = read(&data, mode != "pre-segment", None);
        let actual = summarize(&g, mode);
        assert_eq!(
            actual, expected,
            "vector {name}: Rust fold diverges from the frozen oracle expectation"
        );
    }
}

/// §3.2/§18.23: every item-boundary prefix of every vector folds without
/// error, and growing prefixes only ever extend the folded tables — the
/// prefix-fold streaming property, tested rather than asserted.
#[test]
fn prefix_fold_streaming_property() {
    use std::collections::HashSet;

    use gts::wire::iter_items;

    fn ground(g: &Graph) -> HashSet<String> {
        to_nquads(g)
            .lines()
            .filter(|l| !l.contains("_:"))
            .map(str::to_string)
            .collect()
    }

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../generated/gts-vectors");
    for entry in fs::read_dir(&dir).expect("corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("gts") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let data = fs::read(&path).expect("vector bytes");
        let (items, _) = iter_items(&data);
        let mut boundaries: Vec<usize> = items.iter().skip(1).map(|(off, _)| *off).collect();
        boundaries.push(data.len());
        let mut prev: Option<Graph> = None;
        for end in boundaries {
            let g = read(&data[..end], true, None); // MUST be total: never panics
            if let Some(p) = &prev {
                if p.segment_heads.len() == g.segment_heads.len() {
                    assert_eq!(&g.terms[..p.terms.len()], &p.terms[..], "{name}");
                    assert_eq!(&g.quads[..p.quads.len()], &p.quads[..], "{name}");
                } else {
                    assert!(ground(p).is_subset(&ground(&g)), "{name}");
                }
            }
            prev = Some(g);
        }
    }
}
