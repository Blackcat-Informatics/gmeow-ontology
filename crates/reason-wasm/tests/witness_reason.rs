// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native complete-artifact identity for the shipped browser reason operation.
//! The matching WASM case consumes the same producer-owned frozen JSON witness.

use std::path::PathBuf;

#[path = "support/closure_witness.rs"]
mod closure_witness;
use closure_witness::{INPUT, reason_artifact};

fn attestation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
        .join("crates/reason-wasm/tests/WITNESS.reason.json")
}

#[test]
fn native_reason_closure_matches_the_witness_attestation() {
    let out = reason_artifact(INPUT, "turtle");
    // Complete-artifact bytes are deterministic and match the explicit producer.
    assert_eq!(
        out,
        reason_artifact(INPUT, "turtle"),
        "reasoning is deterministic"
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
