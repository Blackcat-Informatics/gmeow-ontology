// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The dogfooding drift gate for the three EXECUTABLE lift producers.
//!
//! `slices/grounding/math/tests/fixtures/lifted-{r,onnx,proof}.ttl` are not hand-authored
//! worked examples that happen to resemble a lift: each is the EXACT output of
//! [`gmeow_math::producers::r_lift`] / [`onnx_lift`](gmeow_math::producers::onnx_lift) /
//! [`proof_lift`](gmeow_math::producers::proof_lift), which in turn is the exact output of
//! the shipped `gmeow_math_lift` front-end over a real committed artifact. The same
//! discipline the `math:` slice already applies to
//! `tests/conformance-fixtures/ingest-run-lifted.ttl` (byte-identical to
//! `r_bridge_lift`'s graph).
//!
//! A fixture that is merely *committed* rots silently: a parser improvement changes the
//! producer, `gmeow.gts` carries the new graph, and the on-disk file quietly stops
//! describing anything real. So each fixture is REGENERATED here and byte-compared. A
//! parser change is therefore not blocked — it is simply required to re-bless the fixture
//! in the same commit, which is what keeps the two honest about being one artifact.
//!
//! The fixtures are embedded with `include_str!` (the precedent set by
//! `tests/external_corpus_crosswalk.rs`), so a deleted or misplaced fixture is a COMPILE
//! error rather than a test that silently skips.

use gmeow_math::producers::{onnx_lift, proof_lift, r_lift};

/// The R lift fixture, embedded at compile time.
const LIFTED_R: &str = include_str!("../../../slices/grounding/math/tests/fixtures/lifted-r.ttl");
/// The ONNX lift fixture, embedded at compile time.
const LIFTED_ONNX: &str =
    include_str!("../../../slices/grounding/math/tests/fixtures/lifted-onnx.ttl");
/// The proof lift fixture, embedded at compile time.
const LIFTED_PROOF: &str =
    include_str!("../../../slices/grounding/math/tests/fixtures/lifted-proof.ttl");

/// Strip a fixture's leading SPDX/provenance comment block, returning the RDF body.
///
/// Every producer's Turtle opens with the codec's `@prefix` header, and every line of the
/// comment block starts with `#`, so the first `@prefix` is an unambiguous cut point. The
/// body from there on must equal the producer's output byte for byte.
fn body<'a>(fixture: &'a str, name: &str) -> &'a str {
    let start = fixture
        .find("@prefix")
        .unwrap_or_else(|| panic!("{name} must carry the producer's Turtle after its header"));
    let header = &fixture[..start];
    for line in header.lines() {
        assert!(
            line.is_empty() || line.starts_with('#'),
            "{name}'s header block must be comments only, found: {line}"
        );
    }
    assert!(
        header.contains("SPDX-License-Identifier: CC-BY-4.0"),
        "{name} must carry the CC-BY-4.0 SPDX header"
    );
    &fixture[start..]
}

#[test]
fn lifted_r_fixture_is_exactly_what_the_producer_emits() {
    let produced = r_lift();
    assert_eq!(
        body(LIFTED_R, "lifted-r.ttl"),
        produced.turtle,
        "slices/grounding/math/tests/fixtures/lifted-r.ttl has drifted from \
         gmeow_math::producers::r_lift — re-bless the fixture with the producer's output"
    );
}

#[test]
fn lifted_onnx_fixture_is_exactly_what_the_producer_emits() {
    let produced = onnx_lift();
    assert_eq!(
        body(LIFTED_ONNX, "lifted-onnx.ttl"),
        produced.turtle,
        "slices/grounding/math/tests/fixtures/lifted-onnx.ttl has drifted from \
         gmeow_math::producers::onnx_lift — re-bless the fixture with the producer's output"
    );
}

#[test]
fn lifted_proof_fixture_is_exactly_what_the_producer_emits() {
    let produced = proof_lift();
    assert_eq!(
        body(LIFTED_PROOF, "lifted-proof.ttl"),
        produced.turtle,
        "slices/grounding/math/tests/fixtures/lifted-proof.ttl has drifted from \
         gmeow_math::producers::proof_lift — re-bless the fixture with the producer's output"
    );
}

/// Each fixture's header states the codomain node count its producer yields; the three
/// counts must be the live ones, so a lift that starts producing MORE or FEWER structures
/// cannot slip past with a stale narrative even if the RDF body were re-blessed carelessly.
#[test]
fn each_fixture_header_pins_the_live_codomain_count() {
    for (fixture, name, count) in [
        (LIFTED_R, "lifted-r.ttl", r_lift().codomain_nodes),
        (LIFTED_ONNX, "lifted-onnx.ttl", onnx_lift().codomain_nodes),
        (
            LIFTED_PROOF,
            "lifted-proof.ttl",
            proof_lift().codomain_nodes,
        ),
    ] {
        let start = fixture.find("@prefix").expect("header cut point");
        let header = &fixture[..start];
        let claim = format!("codomain of {count} nodes");
        assert!(
            header.contains(&claim),
            "{name}'s header must state the live codomain count (`{claim}`)"
        );
    }
}
