// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fuzz the XML Common Logic reader. Contract: arbitrary UTF-8 input returns a
//! program or a parse error and never panics.
#![no_main]

use gmeow_logic_compile::xcl::parse_xcl_str;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = parse_xcl_str(text, None);
    }
});
