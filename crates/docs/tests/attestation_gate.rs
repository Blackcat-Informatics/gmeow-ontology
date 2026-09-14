// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! F4/F5 attestation gate: every interactive documentation capability is causally
//! downstream of a present, current native↔wasm witness-attestation.

use gmeow_docs::vendored_asset::check_capability_attestations;

/// Require each advertised interactive capability to have current backing evidence.
#[test]
fn every_interactive_capability_has_a_current_attestation() {
    let errors = check_capability_attestations(&repo_root());
    assert!(
        errors.is_empty(),
        "an interactive capability lacks a current witness-attestation:\n{}",
        errors.join("\n")
    );
}

/// Check every registered engine witness without maintaining a second asset list.
#[test]
fn each_witnessed_engine_attestation_is_present_and_current() {
    // The ONE registry, not a copy of it: a fifth engine added to the renderer must be
    // attested here without anyone remembering to extend a second list.
    for asset in gmeow_docs::vendored_asset::ALL_ASSETS {
        if let Some(e) = asset.attestation_status(&repo_root()) {
            panic!("engine '{}' attestation not current: {e}", asset.name);
        }
    }
}

/// Resolve this test crate's checkout for read-only asset verification.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Reject missing witnesses in the selected checkout even when the build tree has assets.
#[test]
fn selected_checkout_with_missing_assets_cannot_use_compiled_checkout_attestations() {
    let root = tempfile::tempdir().expect("selected checkout");
    let errors = check_capability_attestations(root.path());
    assert!(!errors.is_empty(), "the selected checkout has no witnesses");
    assert!(errors.iter().any(|error| error.contains("is missing")));
}
