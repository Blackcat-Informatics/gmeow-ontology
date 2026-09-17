// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact source declarations used by the documentation coverage incidence.

use crate::stages::parse_sources::SourceCatalog;
use purrdf::TermRef;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const CHANNEL: &str = "pipeline/documentation-coverage-dimensions.json";

pub(super) fn record(
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let dataset = catalog.document("slices/core/documentation/module.ttl")?;
    let declared: BTreeSet<String> = dataset
        .quad_refs()
        .filter_map(|quad| match (quad.s, quad.p, quad.o) {
            (
                TermRef::Iri(subject),
                TermRef::Iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
                TermRef::Iri("https://blackcatinformatics.ca/gmeow/DocCoverageDimension"),
            ) => subject
                .strip_prefix("https://blackcatinformatics.ca/gmeow/")
                .map(str::to_owned),
            _ => None,
        })
        .collect();
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&declared).map_err(|error| {
            super::stage_err(&format!("encode documentation dimensions: {error}"))
        })?,
    );
    Ok(())
}
