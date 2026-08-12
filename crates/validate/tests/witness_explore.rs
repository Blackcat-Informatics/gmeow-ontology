// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the W2b bundle-explorer `describe` WITNESS (T1/F2).
//!
//! The browser bundle explorer answers `describe <term>` by running a `DESCRIBE`
//! through `gmeow_query_wasm::Dataset::query` — the exact `#[wasm_bindgen]`-exposed
//! function `crates/query-wasm/src/lib.rs` compiles to wasm for the browser. This test
//! calls that SAME function natively (`gmeow-query-wasm`'s `[lib] crate-type` includes
//! `rlib`, so it compiles and runs on every target; only the `cdylib` build ships to
//! the browser), so the DESCRIBE witness proves both sides run the identical code,
//! never a hand-rolled approximation of it. purrdf's `DESCRIBE` is a Symmetric Concise
//! Bounded Description (`purrdf_sparql_eval::describe_query`), not merely "every quad
//! with the term as subject" — it also pulls in the incoming edges the browser
//! explorer actually shows. The result is pinned to a committed content-addressed
//! attestation (`crates/docs/assets/query/WITNESS.describe.nt`): the explorer's
//! describe is proven against the same `gmeow-query-wasm` engine + the same core
//! bundle the site ships. The engine build is anti-rot-gated by
//! `crates/docs/tests/query_asset.rs` and native↔wasm-parity-proven by
//! `crates/query-wasm`'s own Node lanes (`js/tests/witness.test.mjs` against the
//! freshly-built package, `js/tests/shipped.test.mjs` against the committed one).
//!
//! Refreshed with the bundle/asset via `GMEOW_WITNESS_BLESS=1`.

use std::path::PathBuf;

use gmeow_query_wasm::Dataset;
use purrdf::{DatasetView, GraphMatch, TermRef};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn attestation_path() -> PathBuf {
    repo_root().join("crates/docs/assets/query/WITNESS.describe.nt")
}

#[test]
fn native_core_bundle_describe_matches_the_witness_attestation() {
    let root = repo_root();
    let full = std::fs::read(root.join("generated/dist/gmeow.gts"))
        .unwrap_or_else(|e| panic!("witness needs the generated bundle (run `make check`): {e}"));
    let core_nq = gmeow_validate::store::core_browser_bundle_nquads(&full, &[])
        .expect("build core browser bundle");

    // A deterministic subject: the lexicographically smallest GMEOW-namespace IRI that
    // appears in subject position (the same term the explorer would describe). This
    // selection is a plain purrdf dataset scan — a policy choice about WHICH term to
    // describe, not part of the DESCRIBE computation itself, so it stays independent
    // of the shared `Dataset::query` call below.
    let scan = purrdf::parse_dataset(core_nq.as_bytes(), "application/n-quads", None)
        .expect("parse core bundle N-Quads for subject selection");
    let ns = "https://blackcatinformatics.ca/gmeow/";
    let mut subject: Option<String> = None;
    for q in scan.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = scan.resolve(q.s)
            && iri.starts_with(ns)
            && subject.as_deref().map(|s| iri < s).unwrap_or(true)
        {
            subject = Some(iri.to_owned());
        }
    }
    let subject = subject.expect("core bundle carries a GMEOW-namespace subject");

    // The SAME function the browser calls: `Dataset::query` with a DESCRIBE query,
    // over a `gmeow_query_wasm::Dataset` built from the identical core N-Quads text.
    let explorer_ds =
        Dataset::parse(&core_nq, "application/n-quads").expect("parse core bundle N-Quads");
    let rendered = explorer_ds
        .query(&format!("DESCRIBE <{subject}>"), None)
        .expect("DESCRIBE evaluates over the core bundle");
    assert!(
        !rendered.is_empty(),
        "the describe of {subject} must be non-empty"
    );

    let attestation = format!("# describe <{subject}>\n{rendered}");
    let path = attestation_path();
    // Require the EXACT documented value: only `GMEOW_WITNESS_BLESS=1` may overwrite the
    // committed witness (an empty or `=0` value must not silently replace it).
    if std::env::var("GMEOW_WITNESS_BLESS").as_deref() == Ok("1") {
        std::fs::write(&path, &attestation).expect("write");
        eprintln!("blessed describe witness at {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "describe witness attestation {} missing (bless with GMEOW_WITNESS_BLESS=1): {e}",
            path.display()
        )
    });
    assert_eq!(
        attestation, committed,
        "native core-bundle describe drifted from the committed witness attestation — re-bless"
    );
}
