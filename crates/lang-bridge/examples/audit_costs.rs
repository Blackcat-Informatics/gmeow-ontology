// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Print the REAL `cl100k_base` token cost of each argument, so a pinned audit-cost row is
//! measured rather than guessed. The pinning exists to keep tiktoken's ~1.7 MB vocabulary out
//! of the browser reasoning segment; this example is the native side that produces the numbers.

fn main() {
    for token in std::env::args().skip(1) {
        println!(
            "    ({token:?}, {}),",
            gmeow_lang_bridge::gmn_symbology::gmn_glyph_token_cost(&token)
        );
    }
}
