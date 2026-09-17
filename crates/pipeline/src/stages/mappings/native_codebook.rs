// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact browser projection of the producer's shared native language tables.

use std::collections::BTreeMap;

use gmeow_lang_bridge::gmn1_codec::native;

use crate::stages::parse_sources::SourceCatalog;

/// Emit the complete native codebook into the existing language-projection family.
pub(super) fn emit(
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let language = catalog.language()?;
    let bytes = native::encode(
        &language.dictionary,
        &language.codebook,
        catalog.document_digest(native::SOURCE_PATH)?,
        catalog.document_blake3_digest(native::SOURCE_PATH)?,
    )?;
    artifacts.insert(native::GENERATED_PATH.to_owned(), bytes);
    Ok(())
}
