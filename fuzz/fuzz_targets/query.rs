// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fuzz the native `.logic` query-program parser. Contract: arbitrary UTF-8
//! input returns a program or a typed diagnostic and never panics.
#![no_main]

use gmeow_logic::query_ir::parse_query_program;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = parse_query_program(text);
    }
});
