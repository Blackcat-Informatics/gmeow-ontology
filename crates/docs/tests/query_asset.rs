// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot gate for the gmeow-query-wasm engine (the offline docs SPARQL
//! playground + bundle-explorer runtime).
//!
//! The playground ships a PINNED build of `gmeow-query-wasm` under
//! `crates/docs/assets/query/`, refreshed by `make maint-refresh-query-asset`. The
//! regeneration pipeline never rebuilds wasm, so nothing in the pipeline forces the
//! committed blob to stay in step with `crates/query-wasm`. This gating test (on
//! `make check`) drives the SHARED wasm-asset harness
//! ([`gmeow_docs::vendored_asset`]) over the single [`QUERY_ASSET`] descriptor: it
//! proves the committed `.wasm` is a real wasm module that still carries the SPARQL
//! `query` surface — so an implementer who adds/renames the binding but forgets to
//! re-vendor is caught here rather than shipping a dead playground — and that a
//! BLAKE3 content-digest manifest (`assets/query/DIGESTS.blake3`) pins the exact
//! bytes, so a *stale-but-still-functional* engine (one that kept the `query` glue
//! string but drifted in its wasm bytes) cannot slip through. Any change to a
//! committed file without re-running `make maint-refresh-query-asset` (which
//! rewrites the manifest under `GMEOW_QUERY_BLESS=1`) fails this gate.
//!
use gmeow_docs::vendored_asset::QUERY_ASSET;

#[test]
fn the_query_engine_asset_passes_the_anti_rot_gate() {
    // Structural (real `\0asm` module + plausible size), export-surface (the
    // `Dataset.query` glue + `dataset_query` import + the `.d.ts` signature), and the
    // `DIGESTS.blake3` equality gate — all defined once on the shared descriptor.
    QUERY_ASSET.verify();
}
