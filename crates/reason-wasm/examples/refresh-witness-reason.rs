// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for both native reasoner parity attestations.

use std::path::PathBuf;

#[path = "../tests/support/conjecture_witness.rs"]
mod conjecture_witness;

#[path = "../tests/support/closure_witness.rs"]
mod closure_witness;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let closure = closure_witness::reason_nquads(closure_witness::INPUT, "turtle");
    assert_eq!(
        closure,
        closure_witness::reason_nquads(closure_witness::INPUT, "turtle"),
        "native closure must be deterministic"
    );
    assert_eq!(
        closure.lines().count(),
        7,
        "the selected nonempty-domain closure must retain three asserted facts, three consequences and one domain witness"
    );
    for expected in [
        "<https://example.org/Cat> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/Animal> .",
        "<https://example.org/Animal> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/Organism> .",
        "<https://example.org/felix> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/Cat> .",
        "<https://example.org/Cat> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/Organism> .",
        "<https://example.org/felix> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/Animal> .",
        "<https://example.org/felix> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/Organism> .",
    ] {
        assert!(
            closure.lines().any(|line| line == expected),
            "missing expected closure fact: {expected}"
        );
    }
    assert!(closure.lines().any(|line| line.starts_with("<https://blackcatinformatics.ca/gmeow/skolem/") && line.ends_with(" <https://blackcatinformatics.ca/logic/instanceOf> <https://blackcatinformatics.ca/logic/Thing> .")), "missing nonempty-domain witness");
    let witness = conjecture_witness::verified_witness()?;
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/WITNESS.conjecture.nq");
    std::fs::write(&output, witness.attestation.as_bytes())?;
    let closure_output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/WITNESS.reason.nq");
    std::fs::write(&closure_output, closure.as_bytes())?;
    println!(
        "refreshed {} ({} bytes)",
        closure_output.display(),
        closure.len()
    );
    println!(
        "refreshed {} from verified {} + {} verdicts ({} bytes)",
        output.display(),
        witness.proof.lifecycle,
        witness.refutation.lifecycle,
        witness.attestation.len()
    );
    Ok(())
}
