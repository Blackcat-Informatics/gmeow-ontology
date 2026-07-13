// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fuzz the GTS container reader `purrdf::gts::read_graph`. Contract: reject
//! malformed, never panic — for both the single- and multi-segment modes.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = purrdf::gts::read_graph(data, false);
    let _ = purrdf::gts::read_graph(data, true);
});
