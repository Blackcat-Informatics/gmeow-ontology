// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-rdf` -- PyO3-free RDF 1.2 kernel for the GMEOW Rust workspace.
//!
//! The crate is the narrow waist between transport/runtime stores (GTS and future
//! logic stores) and consumers such as SHACL, validate, and LOGIC. It models RDF 1.2
//! terms directly, preserves source/location context where adapters can provide it,
//! and keeps reporting structured but SARIF-free.
//!
//! # Crate boundary (#885 / purrdf P2b)
//!
//! The oxigraph-free, PyO3-free kernel — the immutable IR, the owned value model,
//! diagnostics, dataset capability flags, the loss ledger, provenance, the FnO and
//! SSSOM codecs, the content store, and the GTS reader path — lives in the
//! ring-fenced sibling crate [`gmeow_rdf_core`]. `gmeow-rdf` **re-exports** every one
//! of those modules at its own root so that both the public `gmeow_rdf::…` API and
//! the crate's own internal `crate::…` paths keep resolving unchanged. What remains
//! *here* is the native text/statement/normalize surface ([`native_codecs`],
//! [`native_quads`], [`statements`], [`turtle_normalize`]), the [`gts_compose`]
//! author, the `flattened_dataset_from_bytes` GTS helper in [`gts`], and the
//! `python`-gated PyO3 bindings. EPIC #906 removed the last oxigraph adapters, so the
//! entire crate is now oxigraph-free.

// ---------------------------------------------------------------------------
// Re-exported kernel modules (live in `gmeow-rdf-core`). The re-export keeps the
// public `gmeow_rdf::ir::…` surface AND this crate's internal `crate::ir::…`
// references resolving against the ring-fenced core, so the oxigraph/py adapters
// below need no path edits.
// ---------------------------------------------------------------------------
#[cfg(feature = "gts")]
pub use gmeow_rdf_core::gts_write;
pub use gmeow_rdf_core::{
    backend, bundle, content_store, dataset_view, diagnostic, fno, ir, lookaside, loss, model,
    provenance, sssom, store, turtle, turtle_render,
};

#[cfg(feature = "gts")]
pub mod gts;
#[cfg(feature = "gts")]
pub mod gts_view;
// The native RDF text codecs (#909 / EPIC #906 S3): the codec-only `GtsCodecBackend`
// over the `gmeow-gts` Turtle/TriG/NT/NQ/RDF-XML codecs, oxigraph-free. Rides the
// always-on `gts` feature so every Rust consumer parses/serializes RDF text natively.
#[cfg(feature = "gts")]
pub mod native_codecs;
// Oxigraph-free `RdfQuad` ⇄ `RdfDataset` conversions (EPIC #906): the native twins of
// the oxigraph-quad helpers, available to every Rust consumer that holds the `gts`
// feature WITHOUT pulling the oxigraph Store adapter.
#[cfg(feature = "gts")]
pub mod native_quads;
// The pyo3-free GTS snapshot compose core (#861 P6): SnapshotBuilder + emit_gts +
// BlobRow, lifted out of the python-gated py_gts surface so gmeow-pipeline can
// author a full multi-named-graph snapshot without pulling pyo3. Oxigraph-free
// (EPIC #906); rides the always-on `gts` feature.
#[cfg(feature = "gts")]
pub mod dataset_io;
#[cfg(feature = "gts")]
pub mod gts_compose;
// PyO3 bindings — the only module that imports pyo3, built only under the
// `python` feature (maturin). Keeps the kernel rlib PyO3-free for Rust consumers.
#[cfg(feature = "python")]
pub mod py;
// The native oxigraph Store/SPARQL/parse/canonicalize surface for `gmeow_rdf`
// that replaces the external `pyoxigraph` package (#667). Python-only, like `py`.
#[cfg(feature = "python")]
pub mod py_store;
// The native `RDF → GTS` producer surface (snapshot author + compile_gts) and the
// `PyRdfDataset` Arc handle (#819 Task 8 / C7). Python-only; needs `gts`, which the
// `python` feature now implies.
#[cfg(feature = "python")]
pub mod py_gts;
#[cfg(feature = "python")]
pub mod py_gts_dataset;
#[cfg(feature = "python")]
pub mod py_gts_view;
// The native SSSOM codec surface for `gmeow_rdf` (parse + validate + RDF
// serialize), replacing the `sssom` PyPI package (#848). Python-only, like `py`.
#[cfg(feature = "python")]
pub mod py_sssom;
// The native OWL ↔ RDF 1.2 statement codec is fully oxigraph-free (it folds over the
// native flat-quad stream), so it rides the always-on `gts` feature (EPIC #906).
#[cfg(feature = "gts")]
pub mod statements;
// Shared corpus-classification helpers (EPIC #906 Task 2): the pure corpus
// enumeration / classification helpers the native golden-capture binary
// (src/bin/capture_sparql_goldens.rs) uses. Oxigraph-free.
#[cfg(feature = "gts")]
pub mod capture_support;
// Canonical, review-friendly Turtle serializer over the IR (#819 Task 9): the
// native replacement for rdflib `longturtle` in `gmeow normalize`. Oxigraph-free.
#[cfg(feature = "gts")]
pub mod turtle_normalize;

// Mirror the kernel's root-level re-exports so `gmeow_rdf::RdfTerm`,
// `gmeow_rdf::RdfDiagnostic`, … keep resolving exactly as before. The two
// `gts`-gated IR import helpers are re-exported under the matching gate.
#[cfg(feature = "gts")]
pub use dataset_io::dataset_from_bytes;
pub use gmeow_rdf_core::{
    canonicalize, canonicalize_with, check_provenance, dataset_diff, datasets_isomorphic,
    emit_annotation, emit_quad, emit_reifier, emit_resource, emit_term, fno_to_ntriples,
    fno_to_quads, gts_to_rdf_loss_ledger, loss_matrix_json, pair_loss_ledger,
    rdf_to_gts_loss_ledger, rule_iri, transcode_loss_matrix_json, ArtifactId, ArtifactIndex,
    ArtifactInterner, ArtifactRecord, AssertionOccurrence, Attribution, AttributionRole,
    BlankScope, BundleError, Bytes, CanonHash, Canonicalized, ContentDigest, ContentStore,
    ContentStoreError, DatasetDiff, DatasetMut, DatasetProvenance, DatasetSink, DatasetView,
    FnFunction, FnImpl, FnMapping, FnOutput, FnParam, FnParamMapping, FnReturnMapping, FnoCatalog,
    FrozenDatasetSource, GraphMatch, GraphMatchValue, GtsBundle, HandleEntry, HandleKey, LossEntry,
    LossLedger, MutableDataset, OriginKind, OriginSetId, OriginSetInterner, PipelineBundle,
    PipelineBundleError, ProvenanceError, QuadHandle, QuadIds, QuadRef, QuadValues, RdfAnnotation,
    RdfBlobOrigin, RdfBlobRecord, RdfBundle, RdfDataset, RdfDatasetBuilder, RdfDatasetVisitor,
    RdfDiagnostic, RdfEnvelope, RdfLiteral, RdfLocation, RdfLookaside, RdfLookasideKind,
    RdfLookasideResource, RdfLoss, RdfMetadataEntry, RdfMetadataValue, RdfOpaqueNodeRecord,
    RdfParseRequest, RdfParserBackend, RdfQuad, RdfReifier, RdfSegmentRecord, RdfSerializeRequest,
    RdfSerializer, RdfSeverity, RdfSignatureRecord, RdfStoreCapabilities, RdfSuppressionRecord,
    RdfTerm, RdfTermKind, RdfTextDirection, RdfTriple, SegmentUnitMap, SerializeGraph,
    SparqlEngine, SparqlRequest, SparqlResult, SssomDiagnostic, SssomMapping, SssomMappingSet,
    SssomMeta, TermFactory, TermId, TermRef, TermValue, UnitCatalog, UnitId, UnitInterner,
    UnitMetadata, PROJECTION_CODECS, SSSOM_DEFAULT_VALIDATION_TYPES,
};
#[cfg(feature = "gts")]
pub use gmeow_rdf_core::{import_gts_events, import_gts_graph};
#[cfg(feature = "gts")]
pub use native_codecs::{
    classify, parse_dataset, serialize_dataset, serialize_dataset_base_only,
    serialize_dataset_to_format, GtsCodecBackend, NativeRdfFormat, SerializeOutcome,
};
#[cfg(feature = "gts")]
pub use native_quads::{
    canonical_flat_nquads, canonical_flat_nquads_with, dataset_from_quads, flat_dataset_from_quads,
    flat_rdf_quads_from_dataset,
};

// Shared USTAR (tar) codec: byte-deterministic writer + reader used by both the
// snapshot stage (writer) and the validate path (reader). Unconditional — no
// oxigraph or PyO3 dependency.
pub mod ustar;

/// The common gmeow-rdf surface, for `use gmeow_rdf::prelude::*;`.
///
/// Pulls in the owned value model, the immutable IR + builder, term identity,
/// capability flags, and the diagnostic type — the set a typical consumer reaches
/// for first. Mirrors
/// the ring-fenced kernel's own [`gmeow_rdf_core::prelude`].
pub mod prelude {
    pub use gmeow_rdf_core::prelude::*;
}

// Re-export the module-registration entrypoint (python feature only) so the
// unified `gmeow_native` cdylib can populate the `gmeow_native.rdf` submodule
// (#630). Gated, like `py`/`py_store`, so the kernel rlib stays PyO3-free.
#[cfg(feature = "python")]
pub use py::register;
