// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Isolated synthetic-family helpers; production uses the native joint engine.
use super::*;

pub(super) const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";

pub(super) const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";

pub(super) const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";

pub(super) const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

pub(super) const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

pub(crate) fn analyze(
    input: &NativeFamilyInput<'_>,
    values: &mut SchemaValues,
    lists: &mut LogicalListCache,
    ledger: &mut NativeFamilyLedger,
) -> gmeow_errors::Result<()> {
    let model = Model { input };
    cardinality(&model, values, lists, ledger)?;
    identity(&model, values, lists, ledger)?;
    has_self(&model, values, ledger)
}
