// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fuzz the lenient oxigraph RDF parse path that `gmeow_rdf::py_store::parse_quads`
//! wraps (python-gated, so fuzzed through the identical oxigraph entry). Contract:
//! reject malformed, never panic. libFuzzer aborts only on a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use oxigraph::io::{RdfFormat, RdfParser};

fuzz_target!(|data: &[u8]| {
    for format in [
        RdfFormat::NQuads,
        RdfFormat::Turtle,
        RdfFormat::TriG,
        RdfFormat::N3,
    ] {
        for quad in RdfParser::from_format(format).lenient().for_slice(data) {
            let _ = quad;
        }
    }
});
