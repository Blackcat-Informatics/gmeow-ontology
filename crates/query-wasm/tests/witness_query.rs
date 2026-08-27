// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the query-engine parity WITNESS.
//!
//! This runs the committed corpus and query set through `Dataset::query` — the SAME
//! function the `#[wasm_bindgen]` surface exposes — and pins the results to
//! `crates/docs/assets/query/WITNESS.query.txt`. Two Node lanes replay the identical
//! corpus and assert byte-identity against the same file:
//! `crates/query-wasm/js/tests/witness.test.mjs` (the freshly-built `js/pkg/`
//! package, gating `maint-refresh-query-asset` before it vendors anything) and
//! `crates/query-wasm/js/tests/shipped.test.mjs` (the COMMITTED engine under
//! `crates/docs/assets/query/` — the bytes that actually ship). All three run the
//! same function rather than three similar ones.
//!
//! Refreshed only by an explicit maintainer producer; this test is read-only.
//!
//! Why a committed corpus and not `gmeow.gts`: engine parity is a property of the
//! engine. A bundle-scoped witness would red on every ontology edit — parity noise
//! attributable to content, not to the engine — and could not run before the bundle
//! is materialized.

use gmeow_query_wasm::Dataset;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is under <repo>/crates")
        .to_path_buf()
}

fn attestation_path() -> PathBuf {
    repo_root().join("crates/docs/assets/query/WITNESS.query.txt")
}

/// The query set, read from the SAME file the Node lane reads, so the two halves
/// cannot drift apart in what they ask.
///
/// Parsed with `serde_json::Value`, not a hand-rolled string splitter: a splitter
/// that finds the first `"` after `"sparql":` truncates at any embedded quoted
/// literal (e.g. `FILTER(?x = "literal")`) instead of respecting JSON's `\"` escape,
/// silently sending the two halves different query text. `serde_json` is a
/// `[dev-dependencies]` entry — `tests/` is never linked into the `cdylib`
/// wasm-bindgen artifact this crate ships, so this adds nothing to the shipped
/// engine's dependency tree.
fn queries() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(repo_root().join("crates/query-wasm/js/tests/queries.json"))
        .expect("read queries.json");
    let value: serde_json::Value = serde_json::from_str(&text).expect("parse queries.json");
    value
        .as_array()
        .expect("queries.json is a JSON array")
        .iter()
        .map(|entry| {
            let name = entry["name"].as_str().expect("name value").to_string();
            let sparql = entry["sparql"].as_str().expect("sparql value").to_string();
            (name, sparql)
        })
        .collect()
}

#[test]
fn native_query_results_match_the_witness_attestation() {
    let corpus =
        std::fs::read_to_string(repo_root().join("crates/query-wasm/js/tests/corpus.trig"))
            .expect("read corpus.trig");
    let dataset = Dataset::parse(&corpus, "trig").expect("parse the committed corpus");

    let mut rendered = String::new();
    let entries = queries();
    assert!(!entries.is_empty(), "the query corpus must not be empty");
    for (name, sparql) in &entries {
        let result = dataset
            .query(sparql, None)
            .unwrap_or_else(|e| panic!("query {name} failed: {e:?}"));
        rendered.push_str(&format!("=== {name} ===\n{result}\n"));
    }

    let path = attestation_path();
    let recorded = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "query witness attestation {} missing; refresh it through the explicit maintainer producer: {e}",
            path.display()
        )
    });
    assert_eq!(
        recorded, rendered,
        "the native query results drifted from the recorded witness"
    );
}
