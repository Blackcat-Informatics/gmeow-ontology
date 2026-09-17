// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Browser-codebook legend from the shared original-source language analysis.
//! This observation stays independent of the legend reconstructed from the GTS bundle.

use std::collections::BTreeMap;

use crate::stages::parse_sources::SourceCatalog;

pub(super) const CHANNEL: &str = "pipeline/source-glyph-legend.json";

pub(super) fn record(
    sources: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let language = sources.language()?;
    let legend = gmeow_lang_bridge::glyph_legend_json(language.dictionary.glyph_registry())
        .map_err(|error| super::stage_err(&error.to_string()))?;
    artifacts.insert(CHANNEL.to_owned(), legend.into_bytes());
    Ok(())
}
