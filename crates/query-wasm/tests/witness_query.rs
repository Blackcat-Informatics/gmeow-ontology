// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the query-engine parity WITNESS.
//!
//! This runs the committed corpus and query set through `Dataset::query` — the SAME
//! function the `#[wasm_bindgen]` surface exposes — and pins the results to
//! `crates/docs/assets/query/WITNESS.query.txt`. The Node lane
//! (`crates/query-wasm/js/tests/witness.test.mjs`) runs the identical corpus through
//! the SHIPPED wasm build and asserts byte-identity against the same file, so the two
//! halves compute the same function rather than two similar ones.
//!
//! Refreshed with `GMEOW_WITNESS_BLESS=1`.
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
fn queries() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(
        repo_root().join("crates/query-wasm/js/tests/queries.json"),
    )
    .expect("read queries.json");
    // A deliberately tiny reader for the fixed two-key shape, so this test adds no
    // JSON dependency to a crate whose whole point is a minimal wasm dependency tree.
    let mut out = Vec::new();
    for chunk in text.split("\"name\":").skip(1) {
        let name = chunk
            .split('"')
            .nth(1)
            .expect("name value")
            .to_string();
        let sparql_raw = chunk
            .split("\"sparql\":")
            .nth(1)
            .and_then(|s| s.split('"').nth(1))
            .expect("sparql value");
        out.push((name, sparql_raw.replace("\\n", "\n")));
    }
    out
}

#[test]
fn native_query_results_match_the_witness_attestation() {
    let corpus = std::fs::read_to_string(
        repo_root().join("crates/query-wasm/js/tests/corpus.trig"),
    )
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
    if std::env::var("GMEOW_WITNESS_BLESS").as_deref() == Ok("1") {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create asset dir");
        }
        std::fs::write(&path, &rendered).expect("write witness");
        eprintln!("blessed query witness at {}", path.display());
        return;
    }

    let recorded = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "query witness attestation {} missing (bless with GMEOW_WITNESS_BLESS=1): {e}",
            path.display()
        )
    });
    assert_eq!(
        recorded, rendered,
        "the native query results drifted from the recorded witness"
    );
}
