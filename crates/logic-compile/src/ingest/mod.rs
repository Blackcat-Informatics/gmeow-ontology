// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Wasm-clean ingestion of the alignment DSL + ontology into the correspondence
//! lowering pipeline.
//!
//! The lowerings borrow parsed datasets and use PurRDF's native values and indexed
//! statement reads. Quoted terms, literal direction and blank scope remain intact.
//! Source parsing belongs to the caller; this wasm-clean layer never reads files.

pub mod dataset;
pub mod prefixes;

pub use dataset::{DslTerm, DslView, ReifiedStatement, literal_lexical};
pub use prefixes::{PREFIX_REGISTRY, ns_to_prefix, registry_iri, registry_pairs, sssom_id};
