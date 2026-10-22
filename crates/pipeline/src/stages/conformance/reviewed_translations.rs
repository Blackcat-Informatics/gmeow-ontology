// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated catalog parsing and reviewed-coverage observations.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) const CHANNEL: &str = "pipeline/reviewed-translation-observation.json";

pub(super) fn input_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    gmeow_docs::reviewed_coverage::input_files(root)
}

pub(super) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let observation = gmeow_docs::reviewed_coverage::ReviewedCoverageOutcome(
        gmeow_docs::reviewed_coverage::observe(root).map_err(super::record_failure),
    );
    artifacts.insert(
        CHANNEL.to_owned(),
        purrdf_lex::json::record::to_vec(&observation),
    );
    Ok(())
}
