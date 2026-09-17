// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Isolated synthetic-family helpers; production uses the native joint engine.
use super::*;

/// Refresh the complete current snapshot, retaining cumulative analysis usage.
/// Both registered families borrow the same world state and caches. Previous
/// snapshot receipts belong to their committed derivations, not to a growing list
/// of stale pending outcomes in the current completion claim.
pub(crate) fn analyze(
    input: &NativeFamilyInput<'_>,
    values: &mut SchemaValues,
    lists: &mut LogicalListCache,
    ledger: &mut NativeFamilyLedger,
) -> Result<()> {
    ledger.outcomes.clear();
    crate::reason::refute::counting::analyze(input, values, lists, ledger)?;
    crate::reason::refute::datatype::analyze(input, values, lists, ledger)?;
    ledger.validate()
}
