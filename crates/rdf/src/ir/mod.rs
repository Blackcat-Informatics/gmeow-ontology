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
pub mod dataset;
// The authoritative GTS ingestion path needs `gmeow-gts`/`ciborium`, both gated
// behind the `gts` feature (#819 C2.a).
#[cfg(feature = "gts")]
pub mod import_sink;
pub mod term;
pub mod validate;

pub use builder::RdfDatasetBuilder;
pub use bundle::{RdfBundle, RdfEnvelope};
pub use dataset::{QuadHandle, QuadIds, QuadRef, RdfDataset, TermRef};
#[cfg(feature = "gts")]
pub use import_sink::import_gts_events;
pub use term::{BlankScope, TermId};
