// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot gate for the vendored purrdf wasm engine (the offline docs SPARQL
//! playground runtime).
//!
//! The playground ships a PINNED copy of the purrdf wasm package under
//! `crates/docs/assets/purrdf/`, refreshed by `make maint-refresh-purrdf-asset`. The
//! pipeline never rebuilds wasm, so nothing structurally forces the vendored blob to
//! stay in step with `crates/rdf-wasm`. These gating tests (on `make check`) prove the
//! vendored artifact is a real wasm module that still carries the SPARQL `query`
//! surface — so an implementer who adds/renames the binding but forgets to re-vendor
//! is caught here rather than shipping a dead playground. Behaviour (does a query
//! actually evaluate?) is covered by the Node execution lane
//! (`crates/rdf-wasm/js/tests/vendored_asset.test.mjs`, on `make wasm-pkg-test`).
//!
//! The structural checks alone let a *stale-but-still-functional* engine (one that
//! kept the `query` glue string but drifted in its wasm bytes) slip through, so a
//! BLAKE3 content-digest manifest (`assets/purrdf/DIGESTS.blake3`) pins the exact
//! vendored bytes: any change to a vendored file without re-running
//! `make maint-refresh-purrdf-asset` (which rewrites the manifest under
//! `GMEOW_PURRDF_BLESS=1`) fails the digest gate.

use std::path::PathBuf;

/// The vendored files whose bytes the digest manifest pins — exactly the set the
/// `maint-refresh-purrdf-asset` target copies out of `crates/rdf-wasm/js/pkg/`.
const VENDORED_FILES: &[&str] = &[
    "gmeow_rdf_wasm.d.ts",
    "gmeow_rdf_wasm.js",
    "gmeow_rdf_wasm_bg.wasm",
    "gmeow_rdf_wasm_bg.wasm.d.ts",
];

const DIGEST_MANIFEST: &str = "DIGESTS.blake3";

fn purrdf_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("purrdf")
}

fn asset(name: &str) -> PathBuf {
    purrdf_dir().join(name)
}

/// The manifest content for the current on-disk vendored bytes: one
/// `<blake3-hex>  <filename>` line per vendored file, sorted by filename, LF-terminated.
fn current_manifest() -> String {
    // Order by filename (not by the formatted `<hash>  <name>` line) so a change to one
    // file's hash never reshuffles the other rows — the manifest diff stays minimal.
    let mut names: Vec<&str> = VENDORED_FILES.to_vec();
    names.sort_unstable();
    let lines: Vec<String> = names
        .into_iter()
        .map(|name| {
            let bytes = std::fs::read(asset(name))
                .unwrap_or_else(|e| panic!("vendored {name} must exist: {e}"));
            format!("{}  {name}", blake3::hash(&bytes).to_hex())
        })
        .collect();
    let mut out = lines.join("\n");
    out.push('\n');
    out
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

#[test]
fn vendored_bytes_match_the_blake3_manifest() {
    let manifest_path = asset(DIGEST_MANIFEST);
    let current = current_manifest();

    // Re-vendoring rewrites the manifest through this same path (invoked by
    // `make maint-refresh-purrdf-asset`), so the pinned digests always describe the
    // exact bytes the maint target produced — no external `b3sum` needed.
    if std::env::var_os("GMEOW_PURRDF_BLESS").is_some() {
        std::fs::write(&manifest_path, &current).expect("write purrdf digest manifest");
        return;
    }

    let committed = std::fs::read_to_string(&manifest_path).unwrap_or_else(|e| {
        panic!("missing {DIGEST_MANIFEST} (run make maint-refresh-purrdf-asset): {e}")
    });
    assert_eq!(
        committed, current,
        "vendored purrdf bytes drifted from {DIGEST_MANIFEST}: a vendored file changed \
         without re-running `make maint-refresh-purrdf-asset`. The structural checks pass \
         a stale-but-still-functional engine; this digest gate does not."
    );
}
