// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

#![cfg(feature = "gts")]

//! "Reject malformed, never panic" property gate (T7, #788) for the gmeow-rdf
//! format frontends.
//!
//! Every parser given arbitrary input must return — `Ok` or `Err` — and NEVER
//! panic/abort. proptest runs each generated input through the parser; a panic is
//! caught and shrunk to the minimal failing input, which becomes a checked-in
//! regression. This is the always-on, portable realization of the contract (runs
//! in the existing `cargo nextest` lane); the `fuzz/` cargo-fuzz crate does the
//! deeper coverage-guided pass nightly.
//!
//! Inputs are BOUNDED (≤4 KiB, modest case count) so a superlinear parser cannot
//! turn a pathological input into a spurious timeout — a panic is a real find, a
//! timeout would be a false red.
//!
//! `parse_quads` itself is `#[cfg(feature = "python")]`-gated (unreachable from a
//! default nextest run), so — like `proptest_roundtrip.rs` — the N-Quads/Turtle
//! contract is exercised through the identical oxigraph `RdfParser` path that
//! `parse_quads` wraps.
//!
//! INTENTIONAL oxigraph cross-check (#909) — NOT a production native-codec path.
//! These properties deliberately hammer the *oxigraph* `RdfParser` because the
//! python-gated `py_store::parse_quads` it guards wraps that exact reader; the
//! native text codec's own "never panic" contract is covered by the cargo-fuzz
//! `nquads` target (`fuzz/fuzz_targets/nquads.rs`, re-pointed at
//! `gmeow_rdf::parse_dataset`). So the `oxigraph::io` use here is an explicit,
//! documented carve-out from the #909 grep gate, not a production parse.

use oxigraph::io::{RdfFormat, RdfParser};
use proptest::prelude::*;

/// Raw arbitrary bytes, bounded to keep parsing cheap.
fn arbitrary_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..4096)
}

/// Structure-aware text: a random interleaving of real RDF/Turtle fragments and
/// noise, so the generator reaches deep parser states (prefix tables, quoted
/// triples, blank-node scopes) instead of bouncing off the lexer immediately.
fn structured_turtle() -> impl Strategy<Value = String> {
    let fragments: Vec<&'static str> = vec![
        "@prefix ex: <https://example.org/> .\n",
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n",
        "ex:a ex:b ex:c .\n",
        "ex:a ex:b \"lit\"@en .\n",
        "ex:a ex:b \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n",
        "<<ex:a ex:b ex:c>> ex:d ex:e .\n",
        "ex:a ex:b [ ex:c ex:d ] .\n",
        "_:b0 ex:p _:b1 .\n",
        "ex:a ex:b ex:c, ex:d ;\n  ex:e ex:f .\n",
        "\u{0}\u{1}\u{7f}",
        "<not a valid iri> . . ;;",
        "@prefix",
        "\"unterminated",
    ];
    prop::collection::vec(prop::sample::select(fragments), 0..24).prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// The oxigraph parse path `parse_quads` wraps (lenient, every format) must
    /// never panic on arbitrary bytes.
    #[test]
    fn rdf_parser_lenient_never_panics(data in arbitrary_bytes()) {
        for format in [RdfFormat::NQuads, RdfFormat::Turtle, RdfFormat::TriG, RdfFormat::N3] {
            for quad in RdfParser::from_format(format).lenient().for_slice(&data) {
                // Consume every item; `Err` is fine, a panic is a failure.
                let _ = quad;
            }
        }
    }

    /// Structured-Turtle variant: reach deep parser states without timing out.
    #[test]
    fn rdf_parser_lenient_never_panics_structured(text in structured_turtle()) {
        for quad in RdfParser::from_format(RdfFormat::Turtle).lenient().for_slice(text.as_bytes()) {
            let _ = quad;
        }
    }

    /// The GTS container reader must never panic on arbitrary bytes, with or
    /// without multi-segment support.
    #[test]
    fn gts_read_graph_never_panics(data in arbitrary_bytes()) {
        let _ = gmeow_rdf::gts::read_graph(&data, false);
        let _ = gmeow_rdf::gts::read_graph(&data, true);
    }

    /// The SSSOM TSV parser must never panic on arbitrary text.
    #[test]
    fn sssom_parse_tsv_never_panics(data in arbitrary_bytes()) {
        if let Ok(text) = std::str::from_utf8(&data) {
            let _ = gmeow_rdf::sssom::parse_tsv(text);
        }
    }

    /// SSSOM with tab/newline structure (the format's delimiters), so the row
    /// splitter and header logic are exercised, not just the UTF-8 gate.
    #[test]
    fn sssom_parse_tsv_never_panics_structured(
        rows in prop::collection::vec(
            prop::collection::vec("[a-z:#/\\t]{0,12}", 0..8),
            0..32,
        )
    ) {
        let text = rows.into_iter().map(|r| r.join("\t")).collect::<Vec<_>>().join("\n");
        let _ = gmeow_rdf::sssom::parse_tsv(&text);
    }

    /// The RDF-1.2 ↔ OWL statement transforms parse untrusted Turtle; neither
    /// direction may panic.
    #[test]
    fn statements_transforms_never_panic(text in structured_turtle()) {
        let _ = gmeow_rdf::statements::project_owl_to_rdf12(&text);
        let _ = gmeow_rdf::statements::normalize_rdf12_to_owl(&text);
    }

    #[test]
    fn statements_transforms_never_panic_raw(data in arbitrary_bytes()) {
        if let Ok(text) = std::str::from_utf8(&data) {
            let _ = gmeow_rdf::statements::project_owl_to_rdf12(text);
            let _ = gmeow_rdf::statements::normalize_rdf12_to_owl(text);
        }
    }
}
