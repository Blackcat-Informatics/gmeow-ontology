// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native slice-test harness (#784, T3 of the Rust-first testing epic #781).
//!
//! GMEOW's doctrine is "one canonical source, everything else a generated or
//! checked projection". T2 (#783) minted a declarative test-DSL
//! ([`dsl/tests/vocabulary.ttl`]) and authored each slice's tests AS ontology
//! data under `slices/<group>/<name>/tests/*.ttl`: a competency question is a
//! SPARQL ASK/SELECT plus an expected outcome; a structural assertion is a
//! MUST / MUST-NOT triple pattern over the module; an example-conformance
//! fixture binds an example file to its expected validation outcome.
//!
//! T2 deferred *execution* of those specs to T3. This crate supplies it: a
//! native Rust harness (run under `cargo-nextest` via `datatest-stable`) that
//! discovers every `tests/*.ttl` spec and executes its cells — fast, in
//! parallel, and entirely off rdflib. The three cell types map to three modules:
//!
//! * [`dsl`] — load a spec file into an oxigraph store and SPARQL-introspect its
//!   cells into typed Rust structs.
//! * [`reasoned`] — the merged ontology closed under OWL 2 RL, built once and
//!   shared, against which competency questions run (the same reasoned-graph
//!   lane `tests/test_competency.py` uses).
//! * [`exec`] — the three cell executors and their per-file aggregators.
//! * [`paths`] — `CARGO_MANIFEST_DIR`-anchored path resolution.

// NOTE: `reasoned` and `exec` are introduced in the next commit (Task 3).
pub mod dsl;
pub mod paths;
