// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the W4c GMN transcode WITNESS (T1).
//!
//! `gmeow-gmn-wasm`'s wasm exports `to_gmn1`/`from_gmn1` are thin marshals over the
//! crate's own [`transcode_to_gmn1`]/[`transcode_from_gmn1`] — this test calls THOSE
//! SAME functions natively, so the browser transcode is byte-identical to what is
//! pinned here. It proves two things over a fixed GMEOW-namespace input whose tokens
//! resolve entirely through the embedded codebook (so the GMN-1 surface carries no
//! out-of-band `r_<hash>` by-reference literal and reads back from raw text):
//!
//!   1. `transcode_to_gmn1(input)` is deterministic and matches the committed GMN-1
//!      attestation `crates/docs/assets/gmn/WITNESS.gmn1.txt`;
//!   2. `transcode_from_gmn1(that GMN-1 text)` reproduces the input's canonical
//!      N-Quads byte-for-byte — the round-trip the widget shows.
//!
//! The Node lane runs the WASM `to_gmn1`/`from_gmn1` over the SAME input and asserts
//! byte-identity with the same attestation; both matching proves native ≡ wasm.
//! Refreshed via `GMEOW_WITNESS_BLESS=1`.

use std::path::PathBuf;

use gmeow_gmn_wasm::{glyph_legend_json, transcode_from_gmn1, transcode_to_gmn1};

/// A self-contained GMN-0 EDB in Turtle: three GMEOW-namespace claims (two IRI
/// objects, one plain-string value). Every term resolves through the codebook's
/// prefix registry / dictionary, so the emitted GMN-1 surface is self-contained.
const INPUT: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
     gmeow:gate1 gmeow:hasState gmeow:doorGate1 .\n\
     gmeow:gate1 gmeow:locatedIn gmeow:yardNorth .\n\
     gmeow:gate1 gmeow:statusLabel \"open\" .\n";

fn attestation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
        .join("crates/docs/assets/gmn/WITNESS.gmn1.txt")
}

/// The canonical N-Quads of `INPUT` — the round-trip target.
fn input_canonical_nquads() -> String {
    let ds = purrdf::parse_dataset(INPUT.as_bytes(), "turtle", None).expect("parse INPUT");
    let bytes = purrdf::serialize_dataset(
        &ds,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .expect("serialize INPUT");
    String::from_utf8(bytes).expect("nquads is utf-8")
}

#[test]
fn native_gmn1_transcode_matches_the_witness_attestation_and_round_trips() {
    let gmn1 = transcode_to_gmn1(INPUT, "turtle").expect("transcode to GMN-1");
    // Deterministic + a real GMN-1 surface (header + at least one record).
    assert_eq!(
        gmn1,
        transcode_to_gmn1(INPUT, "turtle").expect("transcode to GMN-1"),
        "GMN-1 encoding is deterministic"
    );
    assert!(
        gmn1.starts_with("@gmn{"),
        "GMN-1 surface must carry the @gmn header:\n{gmn1}"
    );

    // The round-trip: GMN-1 text -> canonical N-Quads reproduces the input exactly.
    let round = transcode_from_gmn1(&gmn1).expect("transcode from GMN-1");
    assert_eq!(
        round,
        input_canonical_nquads(),
        "GMN-1 round-trip must reproduce the input's canonical N-Quads byte-for-byte"
    );

    let path = attestation_path();
    if std::env::var("GMEOW_WITNESS_BLESS").is_ok() {
        std::fs::write(&path, &gmn1).expect("write gmn witness");
        eprintln!("blessed gmn witness at {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "gmn witness attestation {} missing (bless with GMEOW_WITNESS_BLESS=1): {e}",
            path.display()
        )
    });
    assert_eq!(
        gmn1, committed,
        "native GMN-1 transcode drifted from the committed witness attestation — re-bless"
    );
}

#[test]
fn glyph_legend_is_deterministic_and_carries_real_token_costs() {
    let legend = glyph_legend_json().expect("glyph legend");
    assert_eq!(
        legend,
        glyph_legend_json().expect("glyph legend"),
        "the glyph legend is a pure function of the embedded codebook"
    );
    // A non-empty JSON array whose entries carry the two symbology primitives.
    assert!(
        legend.starts_with('[') && legend.ends_with(']') && legend.len() > 2,
        "legend must be a non-empty JSON array: {legend}"
    );
    assert!(
        legend.contains("\"glyph\"") && legend.contains("\"tokenCost\""),
        "legend entries must carry the glyph + its token cost: {legend}"
    );
}
