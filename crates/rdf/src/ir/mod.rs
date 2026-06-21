// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The immutable, value-interned RDF 1.2 dataset IR (#819 C1).
//!
//! This module tree realizes the normative C0 semantic contract from
//! `docs/design/819-rdf-ir-dataflow.md`. Task 2 (C1.a) lands the **interning
//! half**: typed term ids ([`term`]), the interned-term storage, and the builder's
//! `intern_*` entry points ([`builder`]). Freeze, structural validation, the
//! dataset type, and the GTS-bundle bridge arrive in later tasks.

pub mod builder;
pub mod term;

pub use builder::RdfDatasetBuilder;
pub use term::{BlankScope, TermId};
