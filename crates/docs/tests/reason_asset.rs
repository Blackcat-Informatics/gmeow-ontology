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

/// Verify exports, digests and required witness presence under the current substrate.
#[test]
fn vendored_reason_asset_passes_the_anti_rot_gate() {
    let root = repo_root();
    REASON_ASSET.verify(&root);
    if let Some(message) = REASON_ASSET.attestation_status(&root) {
        panic!("{message}");
    }
}

/// Refuse either stale member of the vendored pair even when both files are present.
#[test]
fn attestation_refuses_either_stale_reason_witness_file() {
    let root = tempfile::tempdir().expect("selected checkout");
    let vendored = REASON_ASSET.asset_dir(root.path());
    let tested = root.path().join("crates/reason-wasm/tests");
    std::fs::create_dir_all(&vendored).expect("vendored witness directory");
    std::fs::create_dir_all(&tested).expect("tested witness directory");
    let names = ["WITNESS.reason.gts", "WITNESS.reason.json"];
    for name in names {
        std::fs::write(tested.join(name), b"tested").expect("tested witness");
        std::fs::write(vendored.join(name), b"tested").expect("vendored witness");
    }
    for name in names {
        std::fs::write(vendored.join(name), b"stale").expect("stale vendored witness");
        let failure = REASON_ASSET
            .attestation_status(root.path())
            .expect("attestation rejects a stale member");
        assert!(failure.contains(&format!(
            "differs from the tested witness 'crates/reason-wasm/tests/{name}'"
        )));
        std::fs::write(vendored.join(name), b"tested").expect("restore vendored witness");
    }
}

/// Resolve this test crate's checkout for read-only reasoning asset verification.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
