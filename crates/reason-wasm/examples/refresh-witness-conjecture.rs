// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for `tests/WITNESS.conjecture.nq`.

use std::path::PathBuf;

#[path = "../tests/support/conjecture_witness.rs"]
mod conjecture_witness;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let witness = conjecture_witness::verified_witness()?;
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/WITNESS.conjecture.nq");
    std::fs::write(&output, witness.attestation.as_bytes())?;
    println!(
        "refreshed {} from verified {} + {} verdicts ({} bytes)",
        output.display(),
        witness.proof.lifecycle,
        witness.refutation.lifecycle,
        witness.attestation.len()
    );
    Ok(())
}
