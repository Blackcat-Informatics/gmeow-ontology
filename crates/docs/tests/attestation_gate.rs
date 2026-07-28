// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! F4/F5 attestation gate: every interactive documentation capability is causally
//! downstream of a present, current native↔wasm witness-attestation.

use gmeow_docs::vendored_asset::{MCP_ASSET, MCP_CORE_ASSET, check_capability_attestations};

#[test]
fn every_interactive_capability_has_a_current_attestation() {
    let errors = check_capability_attestations();
    assert!(
        errors.is_empty(),
        "an interactive capability lacks a current witness-attestation:\n{}",
        errors.join("\n")
    );
}

#[test]
fn each_witnessed_engine_attestation_is_present_and_current() {
    // The four retired engines (validator / reasoner / GMN codec shims, and the vendored
    // purrdf runtime) are gone; the console's two MCP segments carry every interactive
    // capability and every attestation now.
    for asset in [&MCP_CORE_ASSET, &MCP_ASSET] {
        if let Some(e) = asset.attestation_status() {
            panic!("engine '{}' attestation not current: {e}", asset.name);
        }
    }
}
