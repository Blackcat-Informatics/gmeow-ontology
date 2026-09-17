// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

/// Hydrate the producer-selected native kernel laws for the independent law census.
/// Full lowering residue and failure classes travel with their rules. Missing or
/// mismatched authentication fails before any law executes; tests never compile
/// the authored module themselves.
pub(super) fn compiled_law_report() -> &'static ViolationLowering {
    &crate::verify::prepared_gates::shared().enactment
}
