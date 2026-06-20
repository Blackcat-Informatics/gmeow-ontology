// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-rdf` -- PyO3-free RDF 1.2 kernel for the GMEOW Rust workspace.
//!
//! The crate is the narrow waist between transport/runtime stores (GTS,
//! oxigraph, and future logic stores) and consumers such as SHACL, validate, and
//! LOGIC. It models RDF 1.2 terms directly, preserves source/location context
//! where adapters can provide it, and keeps reporting structured but SARIF-free.

pub mod diagnostic;
#[cfg(feature = "gts")]
pub mod gts;
#[cfg(feature = "gts")]
pub mod gts_write;
pub mod lookaside;
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
#[cfg(feature = "oxigraph")]
pub mod statements;
pub mod store;
pub mod turtle;

pub use diagnostic::{RdfDiagnostic, RdfLocation, RdfLoss, RdfSeverity};
pub use lookaside::{
    RdfBlobRecord, RdfLookaside, RdfLookasideKind, RdfLookasideResource, RdfMetadataEntry,
    RdfMetadataValue, RdfOpaqueNodeRecord, RdfSegmentRecord, RdfSignatureRecord,
    RdfSuppressionRecord,
};
pub use model::{
    RdfAnnotation, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTermKind, RdfTextDirection,
    RdfTriple,
};
pub use store::{RdfStore, RdfStoreCapabilities, VecRdfStore};
pub use turtle::{emit_annotation, emit_quad, emit_reifier, emit_resource, emit_term, rule_iri};
