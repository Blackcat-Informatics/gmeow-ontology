// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot gate for the vendored purrdf wasm engine (the offline docs SPARQL
//! playground runtime).
//!
//! The playground ships a PINNED copy of the purrdf wasm package under
//! `crates/docs/assets/purrdf/`, refreshed by `make maint-refresh-purrdf-asset`. The
//! pipeline never rebuilds wasm, so nothing structurally forces the vendored blob to
//! stay in step with `crates/rdf-wasm`. This gating test (on `make check`) proves the
//! vendored artifact is a real wasm module that still carries the SPARQL `query`
//! surface — so an implementer who adds/renames the binding but forgets to re-vendor
//! is caught here rather than shipping a dead playground. Behaviour (does a query
//! actually evaluate?) is covered by the Node execution lane
//! (`crates/rdf-wasm/js/tests/vendored_asset.test.mjs`, on `make wasm-pkg-test`).

use std::path::PathBuf;

fn asset(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("purrdf")
        .join(name)
}

#[test]
fn vendored_wasm_is_a_real_module() {
    let wasm = std::fs::read(asset("gmeow_rdf_wasm_bg.wasm"))
        .expect("vendored gmeow_rdf_wasm_bg.wasm must exist (run make maint-refresh-purrdf-asset)");
    // The WebAssembly binary magic is the four bytes `\0asm`.
    assert_eq!(
        &wasm[..4],
        b"\0asm",
        "vendored .wasm does not start with the WebAssembly magic — corrupt or truncated"
    );
    // A real engine build is far larger than any placeholder; guard against an empty
    // or stub file slipping in.
    assert!(
        wasm.len() > 100_000,
        "vendored .wasm is implausibly small ({} bytes) — a broken build was vendored",
        wasm.len()
    );
}

#[test]
fn vendored_bindings_expose_the_sparql_query_surface() {
    let js = std::fs::read_to_string(asset("gmeow_rdf_wasm.js"))
        .expect("vendored gmeow_rdf_wasm.js must exist");
    // The wasm-bindgen glue must carry the `Dataset.query` method and its imported
    // `dataset_query` symbol; their absence means the vendored bindings predate the
    // SPARQL surface (stale re-vendor).
    assert!(
        js.contains("query(sparql, base)"),
        "vendored bindings lack the Dataset.query method — re-run make maint-refresh-purrdf-asset"
    );
    assert!(
        js.contains("dataset_query"),
        "vendored bindings lack the dataset_query wasm import — stale vendored engine"
    );

    let dts = std::fs::read_to_string(asset("gmeow_rdf_wasm.d.ts"))
        .expect("vendored gmeow_rdf_wasm.d.ts must exist");
    assert!(
        dts.contains("query(sparql: string, base?: string | null): string"),
        "vendored .d.ts lacks the query type signature — stale vendored engine"
    );
}
