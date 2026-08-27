// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native slice-validation engine (T3 of the Rust-first testing epic).
//!
//! GMEOW's doctrine is "one canonical source, everything else a generated or
//! checked projection". T2 minted a declarative test-DSL
//! (`dsl/tests/vocabulary.ttl`) and authored each slice's tests AS ontology
//! data under `slices/<group>/<name>/tests/*.ttl`: a competency question is a
//! SPARQL ASK/SELECT plus an expected outcome; a structural assertion is a
//! MUST / MUST-NOT triple pattern over the module; an example-conformance
//! fixture binds an example file to its expected validation outcome.
//!
//! T2 deferred *execution* of those specs to T3. This crate supplies it. The
//! explicit pre-test producer discovers every `tests/*.ttl` spec, executes the
//! cells once, and publishes an input- and implementation-keyed verdict. A warm
//! producer and every test-facing verifier only authenticate that immutable
//! receipt; no test executable can discover or rebuild the repository corpus.
//! The three cell types map to three modules:
//!
//! * [`dsl`] — load a spec file into a native [`RdfDataset`](purrdf::RdfDataset)
//!   and SPARQL-introspect its cells into typed Rust structs.
//! * [`stores`] — the merged ontology graph competency questions run over: the
//!   asserted graph (default) and its RDFS closure (opt-in via
//!   `gmeow:cqReasoning`). See `docs/TESTING.md` for the design.
//! * [`exec`] — the three cell executors and their per-file aggregators.
//! * [`native_query`] — the oxigraph-free native SPARQL substrate: the
//!   dataset builder, the `NativeSparqlEngine` wrapper, and the canonical term
//!   renderer the other modules share.
//! * [`paths`] — `CARGO_MANIFEST_DIR`-anchored path resolution.
//! * [`repository`] — exact-input discovery plus the cached producer/read-only
//!   verifier boundary for the complete declarative slice verdict.

/// Exact implementation identity for declarative slice-spec action receipts.
pub const BUILD_FINGERPRINT: &str = env!("GMEOW_SLICETEST_BUILD_FINGERPRINT");

pub mod dsl;
pub mod error;
pub mod exec;
pub mod flagship;
pub mod native_query;
pub mod paths;
pub mod repository;
pub mod stores;
