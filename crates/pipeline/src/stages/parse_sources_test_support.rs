// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

/// A tiny in-memory source product for synthetic stage tests. No repository
/// discovery, file reads, stage execution or corpus fixture production occurs.
#[cfg(test)]
pub(crate) fn synthetic_product(nquads: &str) -> StageProduct {
    let sources = ParsedAuthoredSources::synthetic(nquads);
    let catalog = Arc::new(SourceCatalog::from_sources(sources).unwrap());
    source_catalog_product(catalog).unwrap()
}
