// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared original operator compilation exported for native consumer execution.

use crate::stages::parse_sources::SourceCatalog;
use gmeow_logic::operator_rules::{
    OPERATOR_SOURCE_IRI, OPERATOR_SOURCE_PATH, PREPARED_OPERATOR_CHANNEL, PreparedOperatorRules,
};
use std::collections::BTreeMap;

pub(super) fn record(
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<PreparedOperatorRules> {
    let compiled =
        catalog.compiled_document(OPERATOR_SOURCE_PATH, Some(OPERATOR_SOURCE_IRI.to_owned()))?;
    let prepared = PreparedOperatorRules::from_compiled_source(
        &compiled,
        catalog.document_digest(OPERATOR_SOURCE_PATH)?,
    )?;
    artifacts.insert(
        PREPARED_OPERATOR_CHANNEL.to_owned(),
        serde_json::to_vec(&prepared).map_err(|error| {
            super::stage_err(&format!("encode prepared operator rules: {error}"))
        })?,
    );
    Ok(prepared)
}
