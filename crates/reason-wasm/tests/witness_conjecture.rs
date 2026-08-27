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
//! that attestation. Both matching the one attestation proves native ≡ wasm. Refreshed via
//! The attestation is refreshed only by an explicit maintainer producer.

use std::path::PathBuf;

use gmeow_logic::conjecture_eval::evaluate_conjecture_ttl;

/// The reified standpoint the demo verdicts are scoped to (Principle 9: never global).
const STANDPOINT: &str = "https://blackcatinformatics.ca/gmeow/examples/conjecture/demo-standpoint";

/// Demo 1 — the PROOF leg. A reified ground atom `ex:a rdf:type ex:B`; the KB already asserts
/// it, so `KB ⊨ φ` (asserting φ derives nothing new ⇒ redundant ⇒ Belnap supported ⇒
/// CORROBORATED). No counterproof, no witness.
const PROOF_FORMULA: &str = "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:phi a logic:Formula ;\n\
         logic:relation rdf:type ;\n\
         logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n\
         logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n";

const PROOF_KB: &str = "@prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:a rdf:type ex:B .\n";

/// Demo 2 — the COUNTERPROOF leg. A universally-quantified Horn candidate
/// `∀x. trigger(x, mark) → rdf:type(x, B)`; the KB triggers `ex:a` and makes `ex:B` disjoint
/// with `ex:a`'s asserted type `ex:A`, so firing the head forces an `owl:Nothing` clash ⇒
/// `KB ∪ {φ} ⊨ ⊥` ⇒ Belnap opposed ⇒ REFUTED-IN-STANDPOINT with a contradiction witness.
const REFUTE_FORMULA: &str = "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:cand a logic:Formula ;\n\
         logic:forall ex:body ;\n\
         logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .\n\
     ex:body a logic:Formula ;\n\
         logic:antecedent ex:ant ;\n\
         logic:consequent ex:con .\n\
     ex:ant a logic:Formula ;\n\
         logic:relation ex:trigger ;\n\
         logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
         logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n\
     ex:con a logic:Formula ;\n\
         logic:relation rdf:type ;\n\
         logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
         logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n";

const REFUTE_KB: &str = "@prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
     ex:a ex:trigger ex:mark .\n\
     ex:a rdf:type ex:A .\n\
     ex:A owl:disjointWith ex:B .\n";

/// The delimiter separating the two verdict bodies in the attestation. The JS lane joins the
/// two `conjecture()` outputs with this EXACT literal, so the whole bundle is byte-identical.
const DELIM: &str = "# ── conjecture witness · counterproof leg ──────────────────────────────\n";

fn attestation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
        .join("crates/reason-wasm/tests/WITNESS.conjecture.nq")
}

#[test]
fn native_conjecture_verdicts_match_the_witness_attestation() {
    // Demo 1: the proof leg fires (corroborated), no counterproof.
    let proof =
        evaluate_conjecture_ttl(PROOF_FORMULA, PROOF_KB, STANDPOINT).expect("proof verdict");
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
    let refute =
        evaluate_conjecture_ttl(REFUTE_FORMULA, REFUTE_KB, STANDPOINT).expect("refute verdict");
    assert!(
        refute.has_counterproof,
        "demo 2 must refute (KB ∪ {{φ}} ⊨ ⊥): {refute:?}"
    );
    assert!(
        refute.witness.is_some(),
        "a refuted verdict must carry a witness"
    );
    assert_eq!(refute.lifecycle, "refuted-in-standpoint");

    // Determinism: both legs are stable across re-runs.
    let proof2 =
        evaluate_conjecture_ttl(PROOF_FORMULA, PROOF_KB, STANDPOINT).expect("proof verdict");
    let refute2 =
        evaluate_conjecture_ttl(REFUTE_FORMULA, REFUTE_KB, STANDPOINT).expect("refute verdict");
    assert_eq!(
        proof.verdict_nt, proof2.verdict_nt,
        "proof verdict is deterministic"
    );
    assert_eq!(
        refute.verdict_nt, refute2.verdict_nt,
        "refute verdict is deterministic"
    );

    // The attestation bundle: proof body, the delimiter, then the counterproof body — the
    // EXACT byte string the JS lane rebuilds from two `conjecture()` calls.
    let out = format!("{}{DELIM}{}", proof.verdict_nt, refute.verdict_nt);

    let path = attestation_path();
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "conjecture witness attestation {} missing; refresh it through the explicit maintainer producer: {e}",
            path.display()
        )
    });
    assert_eq!(
        out, committed,
        "native conjecture verdicts drifted from the committed witness attestation"
    );
}
