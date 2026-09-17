// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

/// Parse rubric-module Turtle TEXT (a `git show <base>:module.ttl` blob) through the
/// SAME rubric loader the working tree uses, so the base and working floor sets are
/// projected identically for the monotonicity diff. `source_label` names the origin
/// (a `<base>:<file>` git spec) in any parse/freeze error.
///
/// # Errors
/// A HARD FAIL when the base module text cannot be parsed/frozen or is not a
/// structurally-complete rubric — the gate never compares against an unreadable base.
#[cfg(test)]
pub(super) fn load_rubric_from_ttl(text: &str, source_label: &str) -> gmeow_errors::Result<Rubric> {
    let ds = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None)
        .map_err(|e| sqe(format!("{source_label}: parse failed: {e}")))?;
    let mut b = purrdf::RdfDatasetBuilder::new();
    b.push_dataset(&ds);
    let frozen = b
        .freeze()
        .map_err(|e| sqe(format!("{source_label}: dataset freeze failed: {e}")))?;
    gmeow_slice_quality::rubric::load_rubric(&frozen)
}
