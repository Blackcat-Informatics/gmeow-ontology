// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! F4/F5 attestation gate: every interactive documentation capability is causally
//! downstream of a present, current native↔wasm witness-attestation.

use gmeow_docs::vendored_asset::{VENDORED_ASSETS, check_capability_attestations};

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
    // Quantified over the WHOLE vendored inventory, not a hand-listed pair: an asset added
    // to `VENDORED_ASSETS` is an asset this gate covers, and a hand-listed pair is how a
    // newly vendored engine ships with an attestation nobody checks. The three retired shims
    // (validator / reasoner / GMN codec) are gone and the console's two MCP segments carry
    // every interactive capability; the vendored purrdf engine declares no attestation at
    // all — `attestation_status` is vacuously OK for it, which is the honest answer for an
    // engine that backs no capability and reproduces no native output.
    for asset in VENDORED_ASSETS {
        if let Some(e) = asset.attestation_status() {
            panic!("engine '{}' attestation not current: {e}", asset.name);
        }
    }
}
