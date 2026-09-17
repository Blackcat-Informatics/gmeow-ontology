// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Required source statistics over the same admission that drives target emission.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::correspondence_frontend::CorrespondenceAnalysis;
use serde::Serialize;

#[derive(Serialize)]
struct Statistics<'a> {
    cells_by_set: BTreeMap<&'a str, usize>,
    equivalences: usize,
    functions: usize,
    mapping_sets: usize,
    projections: usize,
}

pub(super) fn emit(
    view: &DslView,
    analysis: &CorrespondenceAnalysis,
    vocab: &purrdf::slice::SliceVocab,
) -> gmeow_errors::Result<String> {
    let error = |message: String| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-mappings".to_owned(),
            message: format!("DSL statistics: {message}"),
        })
    };
    let mut cells_by_set = BTreeMap::new();
    for cell in analysis.alignment_cells() {
        *cells_by_set.entry(cell.sssom_file.as_str()).or_insert(0) += 1;
    }
    let mut files = BTreeSet::new();
    for set in view.subjects_of_type(&vocab.mapping_set()) {
        let file = view
            .object_literal(&set, &vocab.sssom_file())
            .ok_or_else(|| error(format!("mapping set <{set}> has no sssomFile")))?;
        files.insert(file);
    }
    let statistics = Statistics {
        cells_by_set,
        equivalences: analysis.alignment_cells().len(),
        functions: view.subjects_of_type(&vocab.projection_function()).len(),
        mapping_sets: files.len(),
        projections: analysis.projection_cells().len(),
    };
    let mut output = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    statistics
        .serialize(&mut serde_json::Serializer::with_formatter(
            &mut output,
            formatter,
        ))
        .map_err(|why| error(why.to_string()))?;
    output.push(b'\n');
    String::from_utf8(output).map_err(|why| error(why.to_string()))
}

#[path = "statistics.tests.rs"]
#[cfg(test)]
mod tests;
