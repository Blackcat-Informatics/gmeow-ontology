// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gate this crate against the frozen language-neutral conformance corpus
//! (GTS-SPEC §18): `vectors/*.gts` + `*.expected.json`, both
//! committed and drift-gated on the Python side. The Python reference oracle
//! and this crate must produce IDENTICAL summaries from the same bytes.

use std::fs;
use std::path::Path;

use gmeow_gts::model::Graph;
use gmeow_gts::nquads::to_nquads;
use gmeow_gts::reader::read;
use gmeow_gts::wire::hex;
use serde_json::{json, Map, Value};

/// Rebuild the `.expected.json` summary shape from a folded graph.
fn summarize(g: Graph, mode: &str) -> Value {
    let mut nquads: Vec<String> = to_nquads(&g).lines().map(str::to_string).collect();
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
        "blobs": blob_summary(&g),
        "streamable": g.segment_streamable.iter().map(|info| json!({
            "claimed": info.claimed,
            "covered": info.covered,
            "tail": info.tail,
        })).collect::<Vec<_>>(),
        "nquads": nquads,
    })
}

/// Inline blobs: digest -> {size, declared media type} — pins blob folding
/// and metadata retention (§12) across implementations.
fn blob_summary(g: &Graph) -> Value {
    let mut out = Map::new();
    for (digest, entry) in &g.blobs {
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
        let size = entry
            .decoded_len()
            .expect("conformance blobs must decode for size summaries");
        out.insert(digest.clone(), json!({"size": size, "mt": mt}));
    }
    Value::Object(out)
}

fn manifest_top_level_vectors(dir: &Path) -> Vec<(String, String)> {
    let manifest: Value =
        serde_json::from_slice(&fs::read(dir.join("manifest.json")).expect("manifest json"))
            .expect("manifest json parses");
    assert_eq!(
        manifest["schema"].as_str(),
        Some("https://blackcatinformatics.ca/gts/vector-manifest/v1")
    );
    let vectors = manifest["vectors"]
        .as_array()
        .expect("manifest vectors array");

    let mut out = Vec::new();
    for entry in vectors {
        let id = entry["id"].as_str().expect("manifest vector id");
        let input = entry["input"]["path"]
            .as_str()
            .expect("manifest input path");
        let Some(rest) = input.strip_prefix("vectors/") else {
            continue;
        };
        if rest.contains('/') || !rest.ends_with(".gts") {
            continue;
        }

        let expected_input = format!("vectors/{id}.gts");
        assert_eq!(input, expected_input, "manifest input path must match id");
        let expected_graph = format!("vectors/{id}.expected.json");
        assert_eq!(
            entry["expected"]["graph"].as_str(),
            Some(expected_graph.as_str()),
            "manifest expected graph path must match id"
        );
        let manifest_mode = entry["mode"].as_str().expect("manifest mode");
        let expected_mode = match manifest_mode {
            "permissive-read" => "default",
            "pre-segment" => "pre-segment",
            other => panic!("top-level manifest vector {id} has unsupported read mode {other}"),
        };
        assert!(
            entry["subsets"]
                .as_array()
                .expect("manifest subsets")
                .iter()
                .any(|subset| subset.as_str() == Some("streaming-property")),
            "top-level manifest vector {id} must declare streaming-property"
        );
        out.push((id.to_string(), expected_mode.to_string()));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        !out.is_empty(),
        "manifest must declare top-level GTS vectors"
    );
    out
}

#[test]
fn corpus_matches_frozen_expectations() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors");
    let manifest_vectors = manifest_top_level_vectors(&dir);
    let manifest_names: Vec<String> = manifest_vectors
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    let mut names: Vec<String> = Vec::new();
    let mut expected_names: Vec<String> = Vec::new();
    for entry in
        fs::read_dir(&dir).expect("vectors must exist — run `python scripts/gen_vectors.py`")
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
    assert_eq!(
        names, manifest_names,
        "vector basename mismatch between manifest and top-level corpus files"
    );
    assert!(
        names.len() >= 16,
        "corpus too small ({} vectors) — generation incomplete?",
        names.len()
    );

    for (name, manifest_mode) in &manifest_vectors {
        let data = fs::read(dir.join(format!("{name}.gts"))).expect("vector bytes");
        let expected: Value = serde_json::from_slice(
            &fs::read(dir.join(format!("{name}.expected.json"))).expect("expected json"),
        )
        .expect("expected json parses");
        let mode = expected["mode"].as_str().expect("mode field");
        assert_eq!(
            mode,
            manifest_mode.as_str(),
            "vector {name}: manifest mode must match expected JSON mode"
        );
        let g = read(&data, mode != "pre-segment", None);
        let actual = summarize(g, mode);
        assert_eq!(
            actual, expected,
            "vector {name}: Rust fold diverges from the frozen oracle expectation"
        );
    }
}

/// §10.1/§14.1: the frozen 25b bytes are the cross-engine determinism oracle —
/// compacting the frozen 25 bytes with the frozen timestamp must reproduce
/// them EXACTLY, byte for byte, in every engine.
#[test]
fn compact_reproduces_the_frozen_25b_bytes() {
    use gmeow_gts::compact::compact_streamable;

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors");
    let source = fs::read(dir.join("25-streamable-source.gts")).expect("vector 25 bytes");
    let frozen = fs::read(dir.join("25b-streamable-compacted.gts")).expect("vector 25b bytes");
    let compacted =
        compact_streamable(&source, "2026-01-01T00:00:00Z", false).expect("clean input compacts");
    assert_eq!(
        compacted, frozen,
        "vector 25b: Rust compact diverges from the frozen determinism oracle"
    );
}

/// §3.2/§18.23: every item-boundary prefix of every vector folds without
/// error, and growing prefixes only ever extend the folded tables — the
/// prefix-fold streaming property, tested rather than asserted.
#[test]
fn prefix_fold_streaming_property() {
    use std::collections::HashSet;

    use gmeow_gts::wire::iter_items;

    fn ground(g: &Graph) -> HashSet<String> {
        to_nquads(g)
            .lines()
            .filter(|l| !l.contains("_:"))
            .map(str::to_string)
            .collect()
    }

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors");
    for entry in fs::read_dir(&dir).expect("corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("gts") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let data = fs::read(&path).expect("vector bytes");
        let (items, torn) = iter_items(&data);
        // the last TRUE item boundary of a torn file is the torn offset
        let end_of_items = torn.unwrap_or(data.len());
        let mut boundaries: Vec<usize> = items.iter().skip(1).map(|(off, _)| *off).collect();
        boundaries.push(end_of_items);
        let mut prev: Option<Graph> = None;
        for end in boundaries {
            let g = read(&data[..end], true, None); // MUST be total: never panics
            if let Some(p) = &prev {
                if p.segment_heads.len() == g.segment_heads.len() {
                    // .get() so a shrinking table is a clean assertion, not a panic
                    assert_eq!(g.terms.get(..p.terms.len()), Some(&p.terms[..]), "{name}");
                    assert_eq!(g.quads.get(..p.quads.len()), Some(&p.quads[..]), "{name}");
                } else {
                    assert!(ground(p).is_subset(&ground(&g)), "{name}");
                }
            }
            prev = Some(g);
        }
        if torn.is_some() {
            // §3.2: a stream cut mid-item folds exactly like the torn file
            let full = read(&data, true, None);
            let p = prev.expect("at least the header boundary");
            assert_eq!(full.terms, p.terms, "{name}");
            assert_eq!(full.quads, p.quads, "{name}");
        }
    }
}
