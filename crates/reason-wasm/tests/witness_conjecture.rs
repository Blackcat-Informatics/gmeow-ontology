// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the W4 conjecture-playground WITNESS (symmetric proof / counterproof).
//!
//! `gmeow-reason-wasm::conjecture` is a thin shim over exactly this pipeline (parse the
//! candidate `logic:` document → re-home the KB into the isolated scenario world → run the
//! native symmetric [`gmeow_logic::conjecture::conjecture_test`] → project the verdict to
//! deterministic N-Triples), so the browser conjecture playground produces byte-identical
//! output to native. This test pins the NATIVE verdict of TWO curated conjectures — one
//! CORROBORATED (the proof leg `KB ⊨ φ` fires) and one REFUTED-IN-STANDPOINT (the
//! counterproof leg `KB ∪ {φ} ⊨ ⊥` fires, with a concrete contradiction witness) — to a
//! committed content-addressed attestation (`crates/reason-wasm/tests/WITNESS.conjecture.nq`).
//! The Node lane runs the WASM `conjecture` over the SAME inputs and asserts byte-identity with
//! that attestation. Both matching the one attestation proves native ≡ wasm. The
//! attestation is refreshed only by `make maint-refresh-conjecture-witness`.

use std::path::PathBuf;

#[path = "support/conjecture_witness.rs"]
mod conjecture_witness;

use conjecture_witness::verified_witness;

fn attestation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
        .join("crates/reason-wasm/tests/WITNESS.conjecture.nq")
}

#[test]
fn native_conjecture_verdicts_match_the_witness_attestation() {
    let witness = verified_witness().expect("verified native conjecture witness");
    let proof = &witness.proof;
    assert!(
        proof.has_proof,
        "demo 1 must corroborate (KB ⊨ φ): {proof:?}"
    );
    assert!(
        !proof.has_counterproof,
        "demo 1 must not be refuted: {proof:?}"
    );
    assert!(
        proof.witness.is_none(),
        "a corroborated verdict carries no witness"
    );
    assert_eq!(proof.lifecycle, "corroborated");

    // Demo 2: the counterproof leg fires (refuted) with a concrete contradiction witness.
    let refute = &witness.refutation;
    assert!(
        refute.has_counterproof,
        "demo 2 must refute (KB ∪ {{φ}} ⊨ ⊥): {refute:?}"
    );
    assert!(
        refute.witness.is_some(),
        "a refuted verdict must carry a witness"
    );
    assert_eq!(refute.lifecycle, "refuted-in-standpoint");

    let path = attestation_path();
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "conjecture witness attestation {} missing; run `make maint-refresh-conjecture-witness`: {e}",
            path.display()
        )
    });
    assert_eq!(
        witness.attestation, committed,
        "native conjecture verdicts drifted from the committed witness attestation"
    );
}
