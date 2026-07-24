// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the W4b reasoner `reason` WITNESS (T1/F3).
//!
//! `gmeow-reason-wasm::reason` is a thin shim over exactly this pipeline (parse →
//! `gmeow_logic::reason::reason_closure_dataset` → serialize N-Quads), so the browser
//! reasoner produces byte-identical output to native. This test pins the NATIVE
//! reasoned closure of a fixed input to a committed content-addressed attestation
//! (`crates/docs/assets/reason/WITNESS.reason.nq`); the Node lane runs the WASM
//! `reason` over the SAME input and asserts byte-identity with that attestation.
//! Both matching the one attestation proves native ≡ wasm. Refreshed via
//! `GMEOW_WITNESS_BLESS=1`.

use std::path::PathBuf;

/// A self-contained EDB whose structured-DL closure is non-empty: an RDFS subclass
/// axiom + a typed individual entails the individual's membership in the superclass.
const INPUT: &str = "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix ex: <https://example.org/> .\n\
     ex:Cat rdfs:subClassOf ex:Animal .\n\
     ex:Animal rdfs:subClassOf ex:Organism .\n\
     ex:felix rdf:type ex:Cat .\n";

fn attestation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
        .join("crates/docs/assets/reason/WITNESS.reason.nq")
}

/// The exact pipeline the wasm `reason(data, format)` shim runs.
fn reason_nquads(data: &str, format: &str) -> String {
    let edb = purrdf::parse_dataset(data.as_bytes(), format, None).expect("parse EDB");
    let closure = gmeow_logic::reason::reason_closure_dataset(&edb).expect("reason closure");
    let bytes = purrdf::serialize_dataset(
        &*closure,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .expect("serialize closure");
    String::from_utf8(bytes).expect("closure is utf-8")
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
    // Require the EXACT documented value: only `GMEOW_WITNESS_BLESS=1` may overwrite the
    // committed witness (an empty or `=0` value must not silently replace it).
    if std::env::var("GMEOW_WITNESS_BLESS").as_deref() == Ok("1") {
        std::fs::write(&path, &out).expect("write reason attestation");
        eprintln!("blessed reason witness at {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "reason witness attestation {} missing (bless with GMEOW_WITNESS_BLESS=1): {e}",
            path.display()
        )
    });
    assert_eq!(
        out, committed,
        "native reasoned closure drifted from the committed witness attestation — re-bless"
    );
}
