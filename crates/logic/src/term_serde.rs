// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Value-only codec for retained native rule constants. No dataset-local term IDs.

pub use gmeow_logic_compile::term_serde::{deserialize, optional, serialize, vec};

#[path = "term_serde.tests.rs"]
#[cfg(test)]
mod tests;
