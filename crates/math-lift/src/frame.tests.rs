// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const BASE: &str = "https://blackcatinformatics.ca/gmeow/examples/math/lift/";

fn framed(kind: BridgeKind, rung: Rung, source: &[u8]) -> (RunFrame, String) {
    let frame = RunFrame::mint(kind, BASE, source);
    let mut sink = Sink::new();
    frame.emit(&mut sink, rung);
    let ttl = sink.serialize();
    (frame, ttl)
}

#[test]
fn the_frame_carries_every_ingest_run_shape_obligation() {
    let (_, ttl) = framed(
        BridgeKind::R,
        Rung::lossy_vague_with_witness(),
        b"fit <- lm(y ~ x)",
    );
    for required in [
        "RIngestRun",
        "parseSource",
        "instantiatesSchema",
        "instantiatesPlan",
        "ingestCorrespondence",
        "loadBearing",
        "Correspondence",
        "morphismClass",
        "preservationKind",
        "mnemomorphic",
    ] {
        assert!(
            ttl.contains(required),
            "frame is missing `{required}`:\n{ttl}"
        );
    }
}

#[test]
fn the_same_source_mints_the_same_iris() {
    let (a, ttl_a) = framed(
        BridgeKind::Onnx,
        Rung::lossy_crisp_with_witness(),
        b"\x08\x07",
    );
    let (b, ttl_b) = framed(
        BridgeKind::Onnx,
        Rung::lossy_crisp_with_witness(),
        b"\x08\x07",
    );
    assert_eq!(a.run_iri, b.run_iri, "run IRI is a function of the source");
    assert_eq!(ttl_a, ttl_b, "a re-lift is byte-identical (idempotent)");
}

#[test]
fn a_different_source_mints_a_different_run() {
    let (a, _) = framed(
        BridgeKind::R,
        Rung::lossy_vague_with_witness(),
        b"lm(y ~ x)",
    );
    let (b, _) = framed(
        BridgeKind::R,
        Rung::lossy_vague_with_witness(),
        b"lm(y ~ z)",
    );
    assert_ne!(a.run_iri, b.run_iri);
}

#[test]
fn each_bridge_kind_emits_its_own_run_class() {
    for (kind, class) in [
        (BridgeKind::R, "RIngestRun"),
        (BridgeKind::Onnx, "ONNXIngestRun"),
        (BridgeKind::Proof, "ProofIngestRun"),
    ] {
        let (_, ttl) = framed(kind, Rung::lossy_crisp_with_witness(), b"src");
        assert!(ttl.contains(class), "{kind:?} must emit math:{class}");
    }
}

#[test]
fn an_empty_codomain_is_refused_before_serialization() {
    let frame = RunFrame::mint(BridgeKind::Proof, BASE, b"src");
    let mut sink = Sink::new();
    frame.emit(&mut sink, Rung::section_retraction());
    let err = Lifted::seal(&frame, sink, 0).expect_err("an empty codomain must not seal");
    assert!(
        format!("{err}").contains("unliftable ingest"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn a_generated_node_carries_the_back_edge_the_native_lint_requires() {
    let frame = RunFrame::mint(BridgeKind::R, BASE, b"src");
    let mut sink = Sink::new();
    frame.emit(&mut sink, Rung::lossy_vague_with_witness());
    let node = frame.node("fit", "mtcarsFit");
    sink.typed(&node, &math("FittedModel"));
    frame.generated(&mut sink, &node);
    let lifted = Lifted::seal(&frame, sink, 1).expect("one codomain node seals");
    assert!(lifted.turtle.contains("wasGeneratedBy"));
    assert_eq!(lifted.run_iri, frame.run_iri);
}

#[test]
#[should_panic(expected = "section/retraction rung requires")]
fn a_section_retraction_without_a_witness_is_a_programming_error() {
    let rung = Rung {
        mnemomorphic: false,
        ..Rung::section_retraction()
    };
    let frame = RunFrame::mint(BridgeKind::Proof, BASE, b"src");
    frame.emit(&mut Sink::new(), rung);
}
