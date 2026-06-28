// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-rdf-core` -- oxigraph-free, PyO3-free RDF 1.2 kernel for the GMEOW Rust workspace.
//!
//! This crate is the ring-fenced core (#885 / purrdf P2b) extracted out of
//! `gmeow-rdf`: the immutable value-interned IR, the owned value model, structured
//! diagnostics, dataset capability flags, the loss ledger, provenance, and the
//! oxigraph-free GTS reader path. It models RDF 1.2 terms directly, preserves
//! source/location context where adapters can provide it, and keeps reporting
//! structured but SARIF-free. The oxigraph adapters and the PyO3 extension surface
//! live in the sibling `gmeow-rdf` crate; **nothing here may pull oxigraph** — that
//! is the acceptance gate of #885.
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
// Narrow purrdf backend traits (P2d, #887): term interning, parser ingress,
// SPARQL execution, and serializer egress. PyO3-free, oxigraph-free — pure
// contract only; concrete adapters live in `gmeow-rdf`.
pub mod backend;
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
// Native SSSOM (Simple Standard for Sharing Ontology Mappings) TSV codec +
// validator + RDF serializer (#848). PyO3-free; replaces the `sssom` PyPI
// package's parse+validate behaviour for the GMEOW mapping artifacts.
pub mod sssom;
pub mod store;
pub mod turtle;
// The canonical, review-friendly Turtle RENDERER over the IR — the oxigraph-free half
// of the on-disk normalizer (the oxigraph-coupled text parser stays in `gmeow-rdf`).
// The wasm-clean canonical-Turtle authority for the correspondence EDOAL lowering.
pub mod turtle_render;

pub use backend::{
    RdfParseRequest, RdfParserBackend, RdfSerializeRequest, RdfSerializer, SerializeGraph,
    SparqlEngine, SparqlRequest, SparqlResult, TermFactory,
};
pub use bundle::{
    ArtifactIndex, ArtifactRecord, BundleError, RdfBundle, SegmentUnitMap, UnitCatalog,
    UnitMetadata,
};
pub use content_store::{Bytes, ContentDigest, ContentStore, ContentStoreError};
pub use dataset_view::{DatasetMut, DatasetView, GraphMatch, GraphMatchValue};
pub use diagnostic::{RdfDiagnostic, RdfLocation, RdfLoss, RdfSeverity};
pub use fno::{
    to_ntriples as fno_to_ntriples, to_quads as fno_to_quads, FnFunction, FnImpl, FnMapping,
    FnOutput, FnParam, FnParamMapping, FnReturnMapping, FnoCatalog,
};
pub use ir::{
    canonicalize, canonicalize_with, dataset_diff, datasets_isomorphic, BlankScope, CanonHash,
    Canonicalized, DatasetDiff, DatasetSink, FrozenDatasetSource, GtsBundle, MutableDataset,
    QuadHandle, QuadIds, QuadRef, QuadValues, RdfDataset, RdfDatasetBuilder, RdfDatasetVisitor,
    RdfEnvelope, TermId, TermRef, TermValue,
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
pub use store::RdfStoreCapabilities;
pub use turtle::{emit_annotation, emit_quad, emit_reifier, emit_resource, emit_term, rule_iri};
pub use turtle_render::render as render_canonical_turtle;

/// The common gmeow-rdf-core surface, for `use gmeow_rdf_core::prelude::*;`.
///
/// Pulls in the owned value model, the immutable IR + builder, term identity,
/// capability flags, and the diagnostic type — the set a typical consumer reaches
/// for first.
pub mod prelude {
    pub use crate::backend::{
        RdfParseRequest, RdfParserBackend, RdfSerializeRequest, RdfSerializer, SerializeGraph,
        SparqlEngine, SparqlRequest, SparqlResult, TermFactory,
    };
    pub use crate::dataset_view::{DatasetView, GraphMatch};
    pub use crate::diagnostic::{RdfDiagnostic, RdfLocation, RdfSeverity};
    pub use crate::ir::{
        QuadIds, QuadRef, RdfDataset, RdfDatasetBuilder, TermId, TermRef, TermValue,
    };
    pub use crate::model::{
        RdfAnnotation, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTermKind, RdfTextDirection,
        RdfTriple,
    };
    pub use crate::store::RdfStoreCapabilities;
}
