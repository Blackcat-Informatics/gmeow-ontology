// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the W4b reasoner `reason` WITNESS (T1/F3).
//!
//! `gmeow-reason-wasm::reason` is a thin shim over exactly this pipeline (parse →
//! `gmeow_logic::reason::reason_closure_dataset` → serialize N-Quads), so the browser
//! reasoner produces byte-identical output to native. This test pins the NATIVE
//! reasoned closure of a fixed input to a committed content-addressed attestation
//! (`crates/reason-wasm/tests/WITNESS.reason.nq`); the Node lane runs the WASM
//! `reason` over the SAME input and asserts byte-identity with that attestation.
//! Both matching the one attestation proves native ≡ wasm. The attestation is
//! refreshed only by an explicit maintainer producer.

use std::path::PathBuf;

#[path = "support/closure_witness.rs"]
mod closure_witness;
use closure_witness::{INPUT, reason_nquads};

fn attestation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
        .join("crates/reason-wasm/tests/WITNESS.reason.nq")
}

#[test]
fn native_reason_closure_matches_the_witness_attestation() {
    let out = reason_nquads(INPUT, "turtle");
    // Deterministic + non-empty: the closure must carry the entailed memberships.
    assert_eq!(
        out,
        reason_nquads(INPUT, "turtle"),
        "reasoning is deterministic"
    );
    assert!(
        !out.trim().is_empty(),
        "the structured-DL closure of the fixture must be non-empty:\n{out}"
    );

    let path = attestation_path();
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "reason witness attestation {} missing; refresh it through the explicit maintainer producer: {e}",
            path.display()
        )
    });
    assert_eq!(
        out, committed,
        "native reasoned closure drifted from the committed witness attestation"
    );
}
