// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional Sophia dataset adapter.
//!
//! This module is compiled only with `--features sophia-adapter`. It uses
//! Sophia's in-memory `LightDataset` plus its N-Quads parser/serializer. The
//! feature is intentionally opt-in so default transport users do not inherit an
//! RDF toolkit dependency.

use std::fmt;

use sophia_api::dataset::Dataset;
use sophia_api::parser::QuadParser;
use sophia_api::serializer::{QuadSerializer, Stringifier};
use sophia_api::source::QuadSource;
use sophia_inmem::dataset::LightDataset;
use sophia_turtle::parser::nq::NQuadsParser;
use sophia_turtle::serializer::nq::NQuadsSerializer;

use crate::from_nquads::{from_nquads, NQuadsParseError};
use crate::model::Graph;
use crate::nquads::to_nquads;

/// Default in-memory Sophia dataset used by the adapter.
pub type SophiaDataset = LightDataset;

/// Error raised by the optional Sophia adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SophiaAdapterError {
    detail: String,
}

impl SophiaAdapterError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    fn wrap(context: &str, err: impl fmt::Display) -> Self {
        Self::new(format!("{context}: {err}"))
    }

    /// Human-readable error detail.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for SophiaAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for SophiaAdapterError {}

impl From<NQuadsParseError> for SophiaAdapterError {
    fn from(value: NQuadsParseError) -> Self {
        Self::wrap("GTS N-Quads import failed", value)
    }
}

/// Convert a folded GTS graph into a Sophia `LightDataset`.
///
/// The adapter uses the same RDF 1.2 N-Quads projection as the public
/// `gmeow_gts::nquads` bridge. That keeps Sophia interop semantically aligned
/// with the existing zero-dependency path while keeping Sophia dependencies
/// behind this feature gate.
pub fn to_sophia_dataset(graph: &Graph) -> Result<SophiaDataset, SophiaAdapterError> {
    let nquads = to_nquads(graph);
    NQuadsParser::new()
        .with_preserve_bn_labels(true)
        .parse_str(&nquads)
        .collect_quads::<SophiaDataset>()
        .map_err(|err| SophiaAdapterError::wrap("Sophia N-Quads parse failed", err))
}

/// Convert a Sophia dataset into canonical GTS bytes using the `dist` profile.
///
/// Sophia can represent RDF 1.2 quoted triple terms. The current bridge
/// serializes through Sophia's N-Quads writer and then uses the GTS N-Quads
/// importer, so GTS-only state such as blobs, signatures, suppressions, and
/// diagnostics is intentionally outside this pure RDF projection.
pub fn from_sophia_dataset<D>(dataset: &D) -> Result<Vec<u8>, SophiaAdapterError>
where
    D: Dataset,
{
    let mut serializer = NQuadsSerializer::new_stringifier();
    serializer
        .serialize_dataset(dataset)
        .map_err(|err| SophiaAdapterError::wrap("Sophia N-Quads serialization failed", err))?;
    Ok(from_nquads(serializer.as_str())?)
}
