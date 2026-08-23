// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The dogfooding drift gate for the three EXECUTABLE lift producers.
//!
//! `slices/grounding/math/tests/fixtures/lifted-{r,onnx,proof}.ttl` are not hand-authored
//! worked examples that happen to resemble a lift: each is the EXACT output of
//! [`gmeow_math::producers::r_lift`] / [`onnx_lift`](gmeow_math::producers::onnx_lift) /
//! [`proof_lift`](gmeow_math::producers::proof_lift), which in turn is the exact output of
//! the shipped `gmeow_math_lift` front-end over a real committed artifact. `lifted-r.ttl`
//! carries a second duty: it is the `rBridge` flagship's `gmeow:demonstratedByExample`, so
//! `crates/pipeline/tests/math_flagship_discharge.rs` additionally asserts it is
//! graph-isomorphic to `r_lift`'s output.
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

/// Re-bless every lift fixture from its producer, in place.
///
/// A drift pin is only useful if re-blessing it is mechanical: a maintainer who has to
/// hand-transcribe a 400-line graph will eventually weaken the pin instead. Run with
/// `GMEOW_LIFT_BLESS=1 cargo test -p gmeow-math --test lift_fixture_drift` after a
/// deliberate parser or lift change, then READ THE DIFF — this rewrites the RDF body and
/// the header's codomain count, and a change you did not intend shows up there.
///
/// It is a no-op without the environment variable, so it can never re-bless a fixture as a
/// side effect of an ordinary test run — which would silently disarm the pin.
#[test]
fn bless_lift_fixtures() {
    if std::env::var_os("GMEOW_LIFT_BLESS").is_none() {
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding/math/tests/fixtures");
    for (fixture, name, turtle, count) in [
        (
            LIFTED_R,
            "lifted-r.ttl",
            r_lift().turtle,
            r_lift().codomain_nodes,
        ),
        (
            LIFTED_ONNX,
            "lifted-onnx.ttl",
            onnx_lift().turtle,
            onnx_lift().codomain_nodes,
        ),
        (
            LIFTED_PROOF,
            "lifted-proof.ttl",
            proof_lift().turtle,
            proof_lift().codomain_nodes,
        ),
    ] {
        let start = fixture.find("@prefix").expect("header cut point");
        // Rewrite the header's stated count in place, so the narrative and the body cannot
        // disagree after a bless.
        let header = regex_free_replace_count(&fixture[..start], count);
        std::fs::write(root.join(name), format!("{header}{turtle}")).expect("bless");
    }
}

/// Replace the `codomain of N nodes` claim in a fixture header with the live count.
///
/// Hand-rolled rather than a regex dependency: the phrase is fixed, so scanning for it and
/// splicing the digits is total and needs no crate.
fn regex_free_replace_count(header: &str, count: usize) -> String {
    let Some(at) = header.find("codomain of ") else {
        return header.to_owned();
    };
    let rest = &header[at + "codomain of ".len()..];
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    format!("{}codomain of {count}{}", &header[..at], &rest[digits..])
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
