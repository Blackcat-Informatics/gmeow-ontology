// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot gate for the vendored purrdf wasm engine (the offline docs SPARQL
//! playground runtime).
//!
//! The playground ships a PINNED copy of the purrdf wasm package under
//! `crates/docs/assets/purrdf/`, refreshed by `make maint-refresh-purrdf-asset`. The
//! pipeline never rebuilds wasm, so nothing structurally forces the vendored blob to
//! stay in step with `crates/rdf-wasm`. This gating test (on `make check`) drives the
//! SHARED vendored-wasm-asset harness ([`gmeow_docs::vendored_asset`]) over the single
//! [`PURRDF_ASSET`] descriptor: it proves the vendored `.wasm` is a real wasm module
//! that still carries the SPARQL `query` surface — so an implementer who adds/renames
//! the binding but forgets to re-vendor is caught here rather than shipping a dead
//! playground — and that a BLAKE3 content-digest manifest (`assets/purrdf/DIGESTS.blake3`)
//! pins the exact vendored bytes, so a *stale-but-still-functional* engine (one that
//! kept the `query` glue string but drifted in its wasm bytes) cannot slip through.
//! Any change to a vendored file without re-running `make maint-refresh-purrdf-asset`
//! (which rewrites the manifest under `GMEOW_PURRDF_BLESS=1`) fails this gate.
//!
//! Behaviour (does a query actually evaluate?) is covered by the Node execution lane
//! (`crates/rdf-wasm/js/tests/vendored_asset.test.mjs`, on `make wasm-pkg-test`).

use gmeow_docs::vendored_asset::PURRDF_ASSET;

#[test]
fn vendored_purrdf_asset_passes_the_anti_rot_gate() {
    // Structural (real `\0asm` module + plausible size), export-surface (the
    // `Dataset.query` glue + `dataset_query` import + the `.d.ts` signature), and the
    // `DIGESTS.blake3` equality gate — all defined once on the shared descriptor.
    PURRDF_ASSET.verify();
}
