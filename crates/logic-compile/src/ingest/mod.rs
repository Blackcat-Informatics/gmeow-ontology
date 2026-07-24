// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Wasm-clean ingestion of the alignment DSL + ontology into the correspondence
//! lowering pipeline.
//!
//! The historical SSSOM/EDOAL/FnO/SPARQL emitters read an oxigraph `Store`. These
//! modules reproduce that read surface over the oxigraph-free [`DatasetView`] read
//! trait so the correspondence lowerings ingest with no oxigraph dependency and build
//! for `wasm32`. File reading + Turtle parsing happen in the caller (the pipeline
//! stage, which hands in already-parsed datasets); nothing here touches the
//! filesystem.
//!
//! [`DatasetView`]: purrdf::dataset_view::DatasetView

pub mod dataset;
pub mod prefixes;

pub use dataset::{DslTerm, DslView, ReifiedStatement};
pub use prefixes::{PREFIX_REGISTRY, ns_to_prefix, registry_iri, registry_pairs, sssom_id};
