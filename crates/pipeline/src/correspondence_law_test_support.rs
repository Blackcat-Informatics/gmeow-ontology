// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

#[cfg(test)]
pub(super) fn term_str(term: &purrdf::RdfTerm) -> String {
    gmeow_logic::correspondence_exec::term_key(term)
}
