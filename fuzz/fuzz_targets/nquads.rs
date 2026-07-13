// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fuzz the native RDF text codec (`purrdf::parse_dataset`) — the canonical
//! parse path the whole workspace now routes through. Contract: reject malformed,
//! never panic (it returns `Err`). libFuzzer aborts only on a panic. Fuzzed across
//! every text serialization the native codec accepts, so a panic in any format's
//! grammar (Turtle / TriG / N-Triples / N-Quads) is caught.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for media_type in [
        "application/n-quads",
        "text/turtle",
        "application/trig",
        "application/n-triples",
    ] {
        // Must never panic on arbitrary input — malformed bytes come back as Err.
        let _ = purrdf::parse_dataset(data, media_type, None);
    }
});
