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
use serde_json::{json, Value};

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
        "nquads": nquads,
    })
}

#[test]
fn corpus_matches_frozen_expectations() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../generated/gts-vectors");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .expect("generated/gts-vectors must exist — run `uv run gmeow regenerate gts-vectors`")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".gts").map(str::to_string)
        })
        .collect();
    names.sort();
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
