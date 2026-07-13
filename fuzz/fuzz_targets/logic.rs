// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fuzz the canonical RDF 1.2 logic frontend. Contract: arbitrary UTF-8 input
//! returns a parsed program or a diagnostic and never panics.
#![no_main]

use gmeow_logic_compile::frontend::parse_logic_str;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = parse_logic_str(text, None);
    }
});
