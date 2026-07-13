// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fuzz the RDF-1.2 ↔ OWL statement transforms
//! `purrdf::statements::{project_owl_to_rdf12, normalize_rdf12_to_owl}`, which
//! parse untrusted Turtle. Contract: reject malformed, never panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = purrdf::statements::project_owl_to_rdf12(text);
        let _ = purrdf::statements::normalize_rdf12_to_owl(text);
    }
});
