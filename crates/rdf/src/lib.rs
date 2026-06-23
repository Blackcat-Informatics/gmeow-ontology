// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-rdf` -- PyO3-free RDF 1.2 kernel for the GMEOW Rust workspace.
//!
//! The crate is the narrow waist between transport/runtime stores (GTS,
//! oxigraph, and future logic stores) and consumers such as SHACL, validate, and
//! LOGIC. It models RDF 1.2 terms directly, preserves source/location context
//! where adapters can provide it, and keeps reporting structured but SARIF-free.
//!
//! # `no_std` readiness (#841)
//!
//! The immutable IR ([`ir`]) is the kernel's purest layer and is **file-IO-free**
//! (no `std::fs`/`std::io`), the first prerequisite for an eventual `alloc`-only
//! `no_std` core for embedded / C-ABI consumers. The remaining blocker is the
//! interner's `std::collections::{HashMap, HashSet}` (not in `alloc`); migrating it
//! to `hashbrown` is tracked as **P3c (#880)**. New IR code therefore prefers
//! `core::`/`alloc::` over `std::` where the item exists in both (e.g. `core::fmt`,
//! `alloc::sync::Arc`) so the eventual `#![no_std]` flip stays mechanical. Per the
//! purrdf plan, `no_std` is for embedded/C-ABI targets and is **not** a WASM
//! prerequisite. Common types are re-exported from [`prelude`].

pub mod bundle;
pub mod content_store;
// The static, allocation-free read view over an RDF dataset (purrdf P2, #836):
// `DatasetView` + `GraphMatch`. PyO3-free, oxigraph-free — pure kernel.
pub mod dataset_view;
pub mod diagnostic;
// Native FnO (W3C Function Ontology) typed catalog model + serializer (#848).
// PyO3-free; the `gmeow-slice` FnO emitter builds a `FnoCatalog` from the slice
// framework and serializes it here, replacing rdflib `emit_fno`/`_emit_fnom`.
pub mod fno;
#[cfg(feature = "gts")]
pub mod gts;
// The pyo3-free GTS snapshot compose core (#861 P6): SnapshotBuilder + emit_gts +
// BlobRow, lifted out of the python-gated py_gts surface so gmeow-pipeline can
// author a full multi-named-graph snapshot without pulling pyo3.
#[cfg(feature = "gts")]
pub mod gts_compose;
#[cfg(feature = "gts")]
pub mod gts_write;
// The immutable, value-interned RDF 1.2 dataset IR (#819 C1).
pub mod ir;
// Generic provenance sidecar for the immutable RDF 1.2 dataset (#820 S2):
// UnitId/ArtifactId/OriginSetId newtypes, interners, AssertionOccurrence,
// DatasetProvenance, and the provenance gate. No GMEOW-specific concepts here.
pub mod lookaside;
pub mod provenance;
// The machine-readable RDF↔GTS loss ledger and its drift-gated matrix (#819 C0).
pub mod loss;
pub mod model;
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
// Native SSSOM (Simple Standard for Sharing Ontology Mappings) TSV codec +
// validator + RDF serializer (#848). PyO3-free; replaces the `sssom` PyPI
// package's parse+validate behaviour for the GMEOW mapping artifacts.
pub mod sssom;
pub mod store;
pub mod turtle;
// Canonical, review-friendly Turtle serializer over the IR (#819 Task 9): the
// native replacement for rdflib `longturtle` in `gmeow normalize`.
#[cfg(feature = "oxigraph")]
pub mod turtle_normalize;

pub use bundle::{
    ArtifactIndex, ArtifactRecord, BundleError, RdfBundle, SegmentUnitMap, UnitCatalog,
    UnitMetadata,
};
pub use content_store::{Bytes, ContentDigest, ContentStore, ContentStoreError};
pub use dataset_view::{DatasetView, GraphMatch};
pub use diagnostic::{RdfDiagnostic, RdfLocation, RdfLoss, RdfSeverity};
pub use fno::{
    to_ntriples as fno_to_ntriples, to_quads as fno_to_quads, FnFunction, FnImpl, FnMapping,
    FnOutput, FnParam, FnParamMapping, FnReturnMapping, FnoCatalog,
};
pub use ir::{
    dataset_diff, datasets_isomorphic, BlankScope, DatasetDiff, GtsBundle, QuadHandle, QuadIds,
    QuadRef, RdfDataset, RdfDatasetBuilder, RdfEnvelope, RdfEventSink, TermId, TermRef,
};
#[cfg(feature = "gts")]
pub use ir::{import_gts_events, import_gts_graph};
pub use lookaside::{
    RdfBlobOrigin, RdfBlobRecord, RdfLookaside, RdfLookasideKind, RdfLookasideResource,
    RdfMetadataEntry, RdfMetadataValue, RdfOpaqueNodeRecord, RdfSegmentRecord, RdfSignatureRecord,
    RdfSuppressionRecord,
};
pub use loss::{
    gts_to_rdf_loss_ledger, loss_matrix_json, rdf_to_gts_loss_ledger, LossEntry, LossLedger,
};
pub use model::{
    RdfAnnotation, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTermKind, RdfTextDirection,
    RdfTriple,
};
pub use provenance::{
    check_provenance, ArtifactId, ArtifactInterner, AssertionOccurrence, Attribution,
    AttributionRole, DatasetProvenance, OriginKind, OriginSetId, OriginSetInterner,
    ProvenanceError, UnitId, UnitInterner,
};
pub use sssom::{
    SssomDiagnostic, SssomMapping, SssomMappingSet, SssomMeta, SSSOM_DEFAULT_VALIDATION_TYPES,
};
pub use store::{RdfStore, RdfStoreCapabilities, VecRdfStore};
pub use turtle::{emit_annotation, emit_quad, emit_reifier, emit_resource, emit_term, rule_iri};

/// The common gmeow-rdf surface, for `use gmeow_rdf::prelude::*;`.
///
/// Pulls in the owned value model, the immutable IR + builder, term identity, the
/// store trait, and the diagnostic type — the set a typical consumer (a SHACL/
/// validate/logic adapter, or an external Rust crate) reaches for first.
pub mod prelude {
    pub use crate::dataset_view::{DatasetView, GraphMatch};
    pub use crate::diagnostic::{RdfDiagnostic, RdfLocation, RdfSeverity};
    pub use crate::ir::{QuadIds, QuadRef, RdfDataset, RdfDatasetBuilder, TermId, TermRef};
    pub use crate::model::{
        RdfAnnotation, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTermKind, RdfTextDirection,
        RdfTriple,
    };
    pub use crate::store::{RdfStore, RdfStoreCapabilities};
}

// Re-export the module-registration entrypoint (python feature only) so the
// unified `gmeow_native` cdylib can populate the `gmeow_native.rdf` submodule
// (#630). Gated, like `py`/`py_store`, so the kernel rlib stays PyO3-free.
#[cfg(feature = "python")]
pub use py::register;
