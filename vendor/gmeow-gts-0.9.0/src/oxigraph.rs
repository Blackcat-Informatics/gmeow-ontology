// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional Oxigraph store adapter.
//!
//! This module is compiled only with `--features oxigraph-adapter`. The feature
//! is intentionally opt-in and uses Oxigraph's in-memory `Store` without its
//! default RocksDB feature, so default GTS transport users do not inherit a
//! graph database dependency.

use std::fmt;

use ::oxigraph::model::Quad as OxQuad;
use ::oxigraph::store::{StorageError, Store};
use ciborium::value::Value;

use crate::model::{
    BlobEntry, Diagnostic, Graph, OpaqueNode, Signature, StreamableInfo, Suppression,
};
use crate::rdf::{to_oxrdf_quads, writer_from_oxrdf_dataset_with_profile, RdfAdapterError};
use crate::writer::Writer;

/// Error raised by the optional Oxigraph adapter.
#[derive(Debug)]
pub enum OxigraphAdapterError {
    /// RDF model conversion failed before reaching the store.
    Rdf(RdfAdapterError),
    /// Oxigraph storage operation failed.
    Storage(StorageError),
}

impl fmt::Display for OxigraphAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rdf(err) => write!(f, "RDF adapter error: {err}"),
            Self::Storage(err) => write!(f, "Oxigraph storage error: {err}"),
        }
    }
}

impl std::error::Error for OxigraphAdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rdf(err) => Some(err),
            Self::Storage(err) => Some(err),
        }
    }
}

impl From<RdfAdapterError> for OxigraphAdapterError {
    fn from(value: RdfAdapterError) -> Self {
        Self::Rdf(value)
    }
}

impl From<StorageError> for OxigraphAdapterError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

/// GTS-specific state carried next to a pure RDF store projection.
///
/// The Oxigraph store receives only RDF quads. Suppressions, signatures, blobs,
/// diagnostics, segment heads, profiles, and metadata stay in this sidecar so
/// callers can inspect them without encoding GTS implementation details into
/// the RDF graph.
#[derive(Clone, Debug, Default)]
pub struct GtsSidecar {
    pub blob_meta: Vec<(String, Value)>,
    pub blobs: Vec<(String, BlobEntry)>,
    pub meta: Vec<(String, Value)>,
    pub suppressions: Vec<Suppression>,
    pub opaque: Vec<OpaqueNode>,
    pub signatures: Vec<Signature>,
    pub diagnostics: Vec<Diagnostic>,
    pub segment_heads: Vec<Vec<u8>>,
    pub segment_profiles: Vec<String>,
    pub segment_meta: Vec<Vec<(String, Value)>>,
    pub segment_streamable: Vec<StreamableInfo>,
}

impl GtsSidecar {
    /// Clone the GTS-only state from a folded graph.
    pub fn from_graph(graph: &Graph) -> Self {
        Self {
            blob_meta: graph.blob_meta.clone(),
            blobs: graph.blobs.clone(),
            meta: graph.meta.clone(),
            suppressions: graph.suppressions.clone(),
            opaque: graph.opaque.clone(),
            signatures: graph.signatures.clone(),
            diagnostics: graph.diagnostics.clone(),
            segment_heads: graph.segment_heads.clone(),
            segment_profiles: graph.segment_profiles.clone(),
            segment_meta: graph.segment_meta.clone(),
            segment_streamable: graph.segment_streamable.clone(),
        }
    }
}

/// Oxigraph store plus the GTS-only sidecar state split out of the source graph.
pub struct StoreWithSidecar {
    pub store: Store,
    pub sidecar: GtsSidecar,
}

/// Owning iterator over Oxigraph quads projected from a GTS graph.
pub struct IntoQuads {
    inner: std::vec::IntoIter<OxQuad>,
}

impl Iterator for IntoQuads {
    type Item = OxQuad;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// Project a folded graph into Oxigraph quads without materializing N-Quads text.
pub fn graph_into_quads(graph: Graph) -> Result<IntoQuads, OxigraphAdapterError> {
    let quads = to_oxrdf_quads(&graph)?;
    Ok(IntoQuads {
        inner: quads.into_iter(),
    })
}

/// Project a folded graph into Oxigraph quads and return GTS-only sidecar state.
pub fn graph_into_quads_with_sidecar(
    graph: Graph,
) -> Result<(IntoQuads, GtsSidecar), OxigraphAdapterError> {
    let sidecar = GtsSidecar::from_graph(&graph);
    Ok((graph_into_quads(graph)?, sidecar))
}

/// Project a folded graph into a new in-memory Oxigraph store.
pub fn graph_to_store(graph: &Graph) -> Result<Store, OxigraphAdapterError> {
    let store = Store::new()?;
    for quad in to_oxrdf_quads(graph)? {
        store.insert(quad.as_ref())?;
    }
    Ok(store)
}

/// Project a folded graph into an Oxigraph store and split GTS-only state aside.
pub fn graph_to_store_with_sidecar(graph: Graph) -> Result<StoreWithSidecar, OxigraphAdapterError> {
    let sidecar = GtsSidecar::from_graph(&graph);
    Ok(StoreWithSidecar {
        store: graph_to_store(&graph)?,
        sidecar,
    })
}

/// Build a deterministic GTS writer from an Oxigraph store.
///
/// The conversion walks native Oxigraph quads and never serializes through
/// N-Quads. GTS-only sidecar state is intentionally not folded back into the
/// pure RDF writer; callers that need signatures, blobs, suppressions, or
/// diagnostics should keep the [`GtsSidecar`] next to the store.
pub fn store_to_writer(store: &Store, profile: &str) -> Result<Writer, OxigraphAdapterError> {
    let mut dataset = ::oxigraph::model::Dataset::new();
    for quad in store.iter() {
        let quad = quad?;
        dataset.insert(quad.as_ref());
    }
    Ok(writer_from_oxrdf_dataset_with_profile(&dataset, profile)?)
}

/// Build GTS bytes from an Oxigraph store.
pub fn store_to_gts_bytes(store: &Store, profile: &str) -> Result<Vec<u8>, OxigraphAdapterError> {
    Ok(store_to_writer(store, profile)?.to_bytes())
}
