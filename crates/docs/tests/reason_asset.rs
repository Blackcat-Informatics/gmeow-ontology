// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot gate for the vendored gmeow-reason-wasm engine (the native GMEOW
//! structured-DL reasoner, run client-side in the docs site).
//!
//! The site ships a PINNED copy of the reasoner wasm package under
//! `crates/docs/assets/reason/`, refreshed by `make maint-refresh-reason-asset`. This
//! gating test drives the SHARED vendored-wasm-asset harness over the single
//! [`REASON_ASSET`] descriptor: a real `\0asm` module of plausible size, the `reason`
//! JS + `.d.ts` export surface, and the `DIGESTS.blake3` content-digest equality gate.
//! A change to a vendored file without re-vendoring fails this gate.
//!
//! Behaviour (does the reasoner actually infer, byte-identically to native?) is
//! covered by the native↔wasm parity witness (`crates/reason-wasm/tests/witness_reason.rs`
//! + the Node lane).

use gmeow_docs::vendored_asset::REASON_ASSET;

/// Verify the shipped reasoning image, required exports, and pinned file digests.
#[test]
fn vendored_reason_asset_passes_the_anti_rot_gate() {
    REASON_ASSET.verify(&repo_root());
}

/// Resolve this test crate's checkout for read-only reasoning asset verification.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
