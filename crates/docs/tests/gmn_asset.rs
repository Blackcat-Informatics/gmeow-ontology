// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot gate for the vendored gmeow-gmn-wasm engine (the shipped GMN-0↔GMN-1
//! codec + glyph symbology, run client-side in the docs site).
//!
//! The site ships a PINNED copy of the codec wasm package under
//! `crates/docs/assets/gmn/`, refreshed by `make maint-refresh-gmn-asset`. This gating
//! test drives the SHARED vendored-wasm-asset harness over the single [`GMN_ASSET`]
//! descriptor: a real `\0asm` module of plausible size, the `to_gmn1`/`from_gmn1` JS +
//! `.d.ts` export surface, and the `DIGESTS.blake3` content-digest equality gate. A
//! change to a vendored file without re-vendoring fails this gate.
//!
//! Behaviour (does the transcode round-trip byte-identically to native?) is covered by
//! the native↔wasm parity witness (`crates/gmn-wasm/tests/witness_gmn.rs` + the Node
//! lane).

use gmeow_docs::vendored_asset::GMN_ASSET;

#[test]
fn vendored_gmn_asset_passes_the_anti_rot_gate() {
    GMN_ASSET.verify();
}
