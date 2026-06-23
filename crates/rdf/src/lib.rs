// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-rdf` -- PyO3-free RDF 1.2 kernel for the GMEOW Rust workspace.
//!
//! The crate is the narrow waist between transport/runtime stores (GTS,
//! oxigraph, and future logic stores) and consumers such as SHACL, validate, and
//! LOGIC. It models RDF 1.2 terms directly, preserves source/location context
//! where adapters can provide it, and keeps reporting structured but SARIF-free.
//!
//! # Crate boundary (#885 / purrdf P2b)
//!
//! The oxigraph-free, PyO3-free kernel — the immutable IR, the owned value model,
//! diagnostics, the store trait surface, the loss ledger, provenance, the FnO and
//! SSSOM codecs, the content store, and the oxigraph-free GTS reader path — now
//! lives in the ring-fenced sibling crate [`gmeow_rdf_core`]. `gmeow-rdf`
//! **re-exports** every one of those modules at its own root so that both the
//! public `gmeow_rdf::…` API and the crate's own internal `crate::…` paths keep
//! resolving unchanged. What remains *here* is exactly the surface that depends on
//! oxigraph or PyO3 (the [`oxigraph`]/[`statements`]/[`turtle_normalize`]
//! adapters, the [`gts_compose`] author, the `flattened_*` GTS helpers in
//! [`gts`], and the `python`-gated PyO3 bindings) — none of which may enter the
//! core's dependency tree. That ring-fence is the acceptance gate of #885: nothing
//! reachable from `gmeow-rdf-core` pulls oxigraph.

// ---------------------------------------------------------------------------
// Re-exported kernel modules (live in `gmeow-rdf-core`). The re-export keeps the
// public `gmeow_rdf::ir::…` surface AND this crate's internal `crate::ir::…`
// references resolving against the ring-fenced core, so the oxigraph/py adapters
// below need no path edits.
// ---------------------------------------------------------------------------
#[cfg(feature = "gts")]
pub use gmeow_rdf_core::gts_write;
pub use gmeow_rdf_core::{
    bundle, content_store, dataset_view, diagnostic, fno, ir, lookaside, loss, model, provenance,
    sssom, store, turtle,
};

#[cfg(feature = "gts")]
pub mod gts;
// The pyo3-free GTS snapshot compose core (#861 P6): SnapshotBuilder + emit_gts +
// BlobRow, lifted out of the python-gated py_gts surface so gmeow-pipeline can
// author a full multi-named-graph snapshot without pulling pyo3. It ingests a flat
// oxigraph quad list (RDF 1.1 base graph) and parses RDF bytes via oxigraph, so it
// now needs the `oxigraph` feature explicitly — `gts` no longer implies it (#885).
#[cfg(all(feature = "gts", feature = "oxigraph"))]
pub mod gts_compose;
#[cfg(feature = "oxigraph")]
pub mod oxigraph;
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
// The native SSSOM codec surface for `gmeow_rdf` (parse + validate + RDF
// serialize), replacing the `sssom` PyPI package (#848). Python-only, like `py`.
#[cfg(feature = "python")]
pub mod py_sssom;
#[cfg(feature = "oxigraph")]
pub mod statements;
// Canonical, review-friendly Turtle serializer over the IR (#819 Task 9): the
// native replacement for rdflib `longturtle` in `gmeow normalize`.
#[cfg(feature = "oxigraph")]
pub mod turtle_normalize;

// Mirror the kernel's root-level re-exports so `gmeow_rdf::RdfTerm`,
// `gmeow_rdf::RdfDiagnostic`, … keep resolving exactly as before. The two
// `gts`-gated IR import helpers are re-exported under the matching gate.
pub use gmeow_rdf_core::{
    check_provenance, dataset_diff, datasets_isomorphic, emit_annotation, emit_quad, emit_reifier,
    emit_resource, emit_term, fno_to_ntriples, fno_to_quads, gts_to_rdf_loss_ledger,
    loss_matrix_json, rdf_to_gts_loss_ledger, rule_iri, ArtifactId, ArtifactIndex,
    ArtifactInterner, ArtifactRecord, AssertionOccurrence, Attribution, AttributionRole,
    BlankScope, BundleError, Bytes, ContentDigest, ContentStore, ContentStoreError, DatasetDiff,
    DatasetProvenance, DatasetView, FnFunction, FnImpl, FnMapping, FnOutput, FnParam,
    FnParamMapping, FnReturnMapping, FnoCatalog, GraphMatch, GtsBundle, LossEntry, LossLedger,
    OriginKind, OriginSetId, OriginSetInterner, ProvenanceError, QuadHandle, QuadIds, QuadRef,
    RdfAnnotation, RdfBlobOrigin, RdfBlobRecord, RdfBundle, RdfDataset, RdfDatasetBuilder,
    RdfDiagnostic, RdfEnvelope, RdfEventSink, RdfLiteral, RdfLocation, RdfLookaside,
    RdfLookasideKind, RdfLookasideResource, RdfLoss, RdfMetadataEntry, RdfMetadataValue,
    RdfOpaqueNodeRecord, RdfQuad, RdfReifier, RdfSegmentRecord, RdfSeverity, RdfSignatureRecord,
    RdfStore, RdfStoreCapabilities, RdfSuppressionRecord, RdfTerm, RdfTermKind, RdfTextDirection,
    RdfTriple, SegmentUnitMap, SssomDiagnostic, SssomMapping, SssomMappingSet, SssomMeta, TermId,
    TermRef, TermValue, UnitCatalog, UnitId, UnitInterner, UnitMetadata, VecRdfStore,
    SSSOM_DEFAULT_VALIDATION_TYPES,
};
#[cfg(feature = "gts")]
pub use gmeow_rdf_core::{import_gts_events, import_gts_graph};

/// The common gmeow-rdf surface, for `use gmeow_rdf::prelude::*;`.
///
/// Pulls in the owned value model, the immutable IR + builder, term identity, the
/// store trait, and the diagnostic type — the set a typical consumer (a SHACL/
/// validate/logic adapter, or an external Rust crate) reaches for first. Mirrors
/// the ring-fenced kernel's own [`gmeow_rdf_core::prelude`].
pub mod prelude {
    pub use gmeow_rdf_core::prelude::*;
}

// Re-export the module-registration entrypoint (python feature only) so the
// unified `gmeow_native` cdylib can populate the `gmeow_native.rdf` submodule
// (#630). Gated, like `py`/`py_store`, so the kernel rlib stays PyO3-free.
#[cfg(feature = "python")]
pub use py::register;
