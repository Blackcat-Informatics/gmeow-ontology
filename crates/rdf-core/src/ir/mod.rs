// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The immutable, value-interned RDF 1.2 dataset IR (#819 C1).
//!
//! This module tree realizes the normative C0 semantic contract from
//! `docs/design/819-rdf-ir-dataflow.md`. Task 2 (C1.a) landed the **interning
//! half** (typed term ids in [`term`] and the `intern_*` entry points in
//! [`builder`]); Task 3 (C1.b) completes C1 with the quad/reifier/annotation/
//! location builder methods, the validate-then-freeze path ([`validate`]), and the
//! frozen, infallible, zero-allocation [`dataset`] iteration surface. The
//! GTS-bundle bridge arrives in later tasks (C2+).

pub mod builder;
pub mod bundle;
// The IR→`RdfStore` coexistence bridge (#819 C1/C2): lets a frozen `RdfDataset`
// feed existing owned-model consumers (SHACL/validate/LOGIC/oxigraph) unported.
pub mod compat;
// The `RdfDataset`-direct, blank-aware structural comparator (#819 C1/C2): the
// equality oracle for importer equivalence — explicitly NOT oxigraph.
pub mod compare;
pub mod dataset;
// The shared GTS term resolver (#819 C2): the single home of the eager
// `&Graph` → `RdfTerm` traversal used by `crate::gts::GtsGraphStore`. Gated on
// `gts`, like its only consumers.
#[cfg(feature = "gts")]
pub(crate) mod gts_resolve;
// The consuming `Graph`-by-value importer (#819 C2.b): moves owned term strings
// into the interner, recording the `bnode-scope-flatten` loss.
#[cfg(feature = "gts")]
pub mod import_graph;
// The authoritative GTS ingestion path needs `gmeow-gts`/`ciborium`, both gated
// behind the `gts` feature (#819 C2.a).
#[cfg(feature = "gts")]
pub mod import_sink;
// Evented, ID-addressed OUTPUT of a frozen dataset (#819 C6): the dual of the
// import sink, for chase / SHACL-result / projection consumers.
pub mod event_sink;
pub mod term;
pub mod validate;

pub use builder::RdfDatasetBuilder;
pub use bundle::{GtsBundle, RdfEnvelope};
pub use compare::{dataset_diff, datasets_isomorphic, DatasetDiff};
pub use dataset::{QuadHandle, QuadIds, QuadRef, RdfDataset, RdfDatasetIter, TermRef};
pub use event_sink::RdfEventSink;
#[cfg(feature = "gts")]
pub use import_graph::import_gts_graph;
#[cfg(feature = "gts")]
pub use import_sink::import_gts_events;
pub use term::{BlankScope, TermId, TermValue};
