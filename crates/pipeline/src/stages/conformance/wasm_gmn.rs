// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Browser GMN verdict observations using the shared original-source dictionary.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_lang_bridge::{Gmn1Document, gmn_validation::validate_gmn_document};

use crate::stages::parse_sources::SourceCatalog;

pub(crate) const CHANNEL: &str = "pipeline/wasm-gmn-verdicts.json";
const VECTORS: [&str; 2] = [
    "slices/grounding/lang/tests/gmn1-vectors/claim-basic.gmn",
    "slices/grounding/lang/tests/gmn1-vectors/negative-codec/neg-uncovered-term.gmn",
];

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    VECTORS.iter().map(|path| root.join(path)).collect()
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let language = catalog.language()?;
    let mut verdicts = BTreeMap::new();
    for path in VECTORS {
        let text = std::fs::read_to_string(root.join(path)).map_err(|error| {
            super::stage_err(&format!("read GMN browser vector {path}: {error}"))
        })?;
        verdicts.insert(
            path,
            validate_gmn_document(&Gmn1Document::from_text(&text), &language.dictionary),
        );
    }
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&verdicts)
            .map_err(|error| super::stage_err(&format!("encode GMN browser verdicts: {error}")))?,
    );
    Ok(())
}
