// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot gate for the vendored gmeow-validate-wasm engine (the repo-free Tier-1
//! GMEOW validator, run client-side in the docs site).
//!
//! The site ships a PINNED copy of the validator wasm package under
//! `crates/docs/assets/validate/`, refreshed by `make maint-refresh-validate-asset`.
//! The pipeline never rebuilds wasm, so nothing structurally forces the vendored blob
//! to stay in step with `crates/validate-wasm`. This gating test (on `make check`)
//! drives the SHARED vendored-wasm-asset harness ([`gmeow_docs::vendored_asset`]) over
//! the single [`VALIDATE_ASSET`] descriptor: it proves the vendored `.wasm` is a real
//! wasm module that still exposes the `validate` surface — so an implementer who
//! adds/renames the binding but forgets to re-vendor is caught here — and that a
//! BLAKE3 content-digest manifest (`assets/validate/DIGESTS.blake3`) pins the exact
//! vendored bytes, so a *stale-but-still-functional* engine cannot slip through. Any
//! change to a vendored file without re-running `make maint-refresh-validate-asset`
//! (which rewrites the manifest under `GMEOW_VALIDATE_BLESS=1`) fails this gate.
//!
//! Behaviour (does a validation actually run?) is covered by the Node execution lane
//! (`crates/validate-wasm/js/tests/validate.test.mjs`, on `make validate-wasm-pkg-test`).

use gmeow_docs::vendored_asset::VALIDATE_ASSET;

#[test]
fn vendored_validate_asset_passes_the_anti_rot_gate() {
    // Structural (real `\0asm` module + plausible size), export-surface (the
    // `validate` JS export + the `.d.ts` signature), and the `DIGESTS.blake3`
    // equality gate — all defined once on the shared descriptor.
    VALIDATE_ASSET.verify();
}
