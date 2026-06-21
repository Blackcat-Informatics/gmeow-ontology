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
// The immutable, value-interned RDF 1.2 dataset IR (#819 C1).
pub mod ir;
pub mod lookaside;
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
#[cfg(feature = "oxigraph")]
pub mod statements;
pub mod store;
pub mod turtle;

pub use diagnostic::{RdfDiagnostic, RdfLocation, RdfLoss, RdfSeverity};
pub use ir::{BlankScope, RdfDatasetBuilder, TermId};
pub use lookaside::{
    RdfBlobRecord, RdfLookaside, RdfLookasideKind, RdfLookasideResource, RdfMetadataEntry,
    RdfMetadataValue, RdfOpaqueNodeRecord, RdfSegmentRecord, RdfSignatureRecord,
    RdfSuppressionRecord,
};
pub use loss::{
    gts_to_rdf_loss_ledger, loss_matrix_json, rdf_to_gts_loss_ledger, LossEntry, LossLedger,
};
pub use model::{
    RdfAnnotation, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTermKind, RdfTextDirection,
    RdfTriple,
};
pub use store::{RdfStore, RdfStoreCapabilities, VecRdfStore};
pub use turtle::{emit_annotation, emit_quad, emit_reifier, emit_resource, emit_term, rule_iri};

// Re-export the module-registration entrypoint (python feature only) so the
// unified `gmeow_native` cdylib can populate the `gmeow_native.rdf` submodule
// (#630). Gated, like `py`/`py_store`, so the kernel rlib stays PyO3-free.
#[cfg(feature = "python")]
pub use py::register;
