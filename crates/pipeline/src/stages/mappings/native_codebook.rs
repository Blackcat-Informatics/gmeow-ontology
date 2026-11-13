// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact browser projection of the producer's shared native language tables.

use std::collections::BTreeMap;

use gmeow_lang_bridge::gmn1_codec::native;

use crate::node::{StageBody, StageInput, StageOutput, StageProduct};
use crate::stages::parse_sources::SourceCatalog;

/// Independent producer whose closure does not reconcile shipped browser assets.
pub(crate) const STAGE_ID: &str = "stage-native-codebook";
/// Final artifact owned by this producer and consumed by archives and browser bootstrap.
pub(crate) const CHANNEL: &str = native::GENERATED_PATH;

/// Encode the original native tables through their single production encoder.
fn encode(catalog: &SourceCatalog) -> gmeow_errors::Result<Vec<u8>> {
    let language = catalog.language()?;
    native::encode(
        &language.dictionary,
        &language.codebook,
        catalog.document_digest(native::SOURCE_PATH)?,
        catalog.document_blake3_digest(native::SOURCE_PATH)?,
    )
}

/// Authenticated native-codebook production before browser substrate reconciliation.
pub(crate) struct NativeCodebookStage {}

impl Default for NativeCodebookStage {
    /// Construct the stage over its declared original-source producer.
    fn default() -> Self {
        Self {}
    }
}

impl StageBody for NativeCodebookStage {
    fn id(&self) -> &str {
        STAGE_ID
    }
    fn output_family(&self) -> crate::node::OutputFamily {
        crate::node::OutputFamily::Generated
    }

    fn impl_version(&self) -> &str {
        "native-codebook.v2-final-artifact-owner"
    }

    fn run(&self, input: StageInput<'_>) -> gmeow_errors::Result<StageOutput> {
        let bytes = encode(crate::stages::parse_sources::catalog(&input)?)?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            BTreeMap::from([(CHANNEL.to_owned(), bytes)]),
        )?))
    }
}
