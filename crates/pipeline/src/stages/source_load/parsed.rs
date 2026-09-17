// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One parse of each authored document, shared by the source-stage consumers.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::RdfDataset;
use purrdf::provenance::OriginKind;

use crate::ingest::{Ingested, PurrdfAdapter, SourceAdapter, SpanIndex};

/// A document retains its original native scope and source positions until all
/// consumers have finished. A transport graph is never its source identity.
pub(crate) struct ParsedAuthoredSource {
    /// Physical path used by the existing public provenance partition.
    pub(crate) path: PathBuf,
    /// Portable document name used by provenance and diagnostic records.
    pub(crate) relative_path: String,
    /// Exact source bytes, committed before parsing or graph normalization.
    pub(crate) content_digest: String,
    /// Original-byte BLAKE3 for native consumer identities that already use that digest.
    pub(crate) blake3_digest: String,
    /// Authored/import role, independent of a logical evaluation context.
    pub(crate) kind: OriginKind,
    /// Native parse and its source-position contribution.
    pub(crate) ingested: Ingested,
}

/// Producer-local input collection. It holds original documents, never a cache
/// of cumulative stage carriers or a globally asserted union of every role.
pub(crate) struct ParsedAuthoredSources {
    sources: Vec<ParsedAuthoredSource>,
}

impl ParsedAuthoredSources {
    /// Discover inputs once and parse each document through the source adapter.
    pub(crate) fn load(root: &Path) -> gmeow_errors::Result<Self> {
        let mut sources = Vec::new();
        for path in super::authored_files(root)? {
            let relative_path = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path)?;
            let ingested = PurrdfAdapter.ingest(&relative_path, "text/turtle", &bytes)?;
            sources.push(ParsedAuthoredSource {
                content_digest: purrdf::ContentDigest::of(&bytes).to_hex(),
                blake3_digest: blake3::hash(&bytes).to_hex().to_string(),
                kind: super::authored_origin_kind(root, &path),
                path,
                relative_path,
                ingested,
            });
        }
        Ok(Self { sources })
    }

    /// Original inputs in deterministic discovery order, before union/deduplication.
    pub(crate) fn sources(&self) -> &[ParsedAuthoredSource] {
        &self.sources
    }

    /// The full compile-input catalog, preserving the existing union contract.
    pub(crate) fn merged_dataset(&self) -> Arc<RdfDataset> {
        let datasets: Vec<&RdfDataset> = self
            .sources
            .iter()
            .map(|source| source.ingested.dataset.as_ref())
            .collect();
        Arc::new(RdfDataset::union(&datasets))
    }

    /// Root and module inputs only. Imports keep their independent graph role.
    pub(crate) fn authored_dataset(&self) -> Arc<RdfDataset> {
        let datasets: Vec<&RdfDataset> = self
            .sources
            .iter()
            .filter(|source| !matches!(source.kind, OriginKind::Import))
            .map(|source| source.ingested.dataset.as_ref())
            .collect();
        Arc::new(RdfDataset::union(&datasets))
    }

    /// The explicitly selected import partition, from those same native parses.
    pub(crate) fn imports_dataset(&self) -> Arc<RdfDataset> {
        let datasets: Vec<&RdfDataset> = self
            .sources
            .iter()
            .filter(|source| matches!(source.kind, OriginKind::Import))
            .map(|source| source.ingested.dataset.as_ref())
            .collect();
        Arc::new(RdfDataset::union(&datasets))
    }

    /// Public diagnostic policy: root/module spans are emitted; import spans
    /// remain with their original input and are excluded from this projection.
    pub(crate) fn source_span_index(&self) -> SpanIndex {
        let mut index = SpanIndex::new();
        for source in &self.sources {
            if !matches!(source.kind, OriginKind::Import) {
                index.extend_from_index(source.ingested.spans.index());
            }
        }
        index
    }
}

#[path = "parsed.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_support;
