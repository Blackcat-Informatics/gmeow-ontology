// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native complete-artifact identity for the shipped browser reason operation.
//! The matching WASM case consumes the same frozen response envelope and GTS payload.

use std::path::PathBuf;

#[path = "support/closure_witness.rs"]
mod closure_witness;
use closure_witness::{INPUT, reason_artifact, split_artifact};

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
    let response: serde_json::Value =
        serde_json::from_str(&out).expect("the admitted reasoning response is JSON");
    let closure = response["closure_nquads"]
        .as_str()
        .expect("the admitted artifact carries its closure");
    let dataset = purrdf::parse_dataset(closure.as_bytes(), "application/n-quads", None)
        .expect("the admitted closure is valid N-Quads");
    let subject = purrdf::RdfTerm::iri("https://example.org/felix");
    let predicate = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    for class in ["https://example.org/Animal", "https://example.org/Organism"] {
        let object = purrdf::RdfTerm::iri(class);
        assert!(
            dataset.owned_quads().any(|quad| quad.subject == subject
                && quad.predicate == predicate
                && quad.object == object),
            "the admitted closure entails Felix's membership in {class}"
        );
    }

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
    let (envelope, gts) =
        split_artifact(&out, &response["artifact"]).expect("the response carries canonical GTS");
    assert_eq!(
        envelope, committed,
        "reason response drifted from the witness envelope"
    );
    let committed_gts =
        std::fs::read(path.with_extension("gts")).expect("read the producer-owned GTS witness");
    assert_eq!(
        gts, committed_gts,
        "reason artifact drifted from the GTS witness"
    );
}
