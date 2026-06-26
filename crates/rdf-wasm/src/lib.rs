// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # purrdf — a wasm32, in-memory RDF 1.2 engine with an idiomatic RDF/JS API
//!
//! Parcel **P10** of the purrdf program (EPIC #832, `docs/design/PURRDF-PLAN.md`).
//! This crate compiles the oxigraph-free, PyO3-free [`gmeow-rdf`](gmeow_rdf) kernel to
//! `wasm32-unknown-unknown` and exposes it to JavaScript/TypeScript through the
//! [RDF/JS](https://rdf.js.org/) community spec — `DataFactory`, `DatasetCore`, and
//! `Stream`/`Sink` — packaged for npm/ESM as **`purrdf`**.
//!
//! ## Scope (by charter)
//!
//! - **In-memory only.** The oxigraph `Store` (RocksDB) and `crates/logic` do not
//!   compile to wasm and are deliberately excluded — this is the
//!   value-interned IR + the COW [`MutableDataset`](gmeow_rdf::ir::MutableDataset),
//!   not a persistent quad store. SPARQL query (oxigraph-backed, native-only by
//!   charter) is out of scope.
//! - **Separate from the C-ABI (P8 #842).** WASM has its own ownership model,
//!   packaging, and async I/O; it is not a C-ABI consumer and does not depend on the
//!   `no_std` track.
//!
//! ## The RDF-1.2 wedge
//!
//! No incumbent RDF/JS library carries RDF-1.2 quoted-triple terms or directional
//! literals. purrdf's `DataFactory` accepts a quoted triple anywhere a term is
//! expected (`termType: "Quad"` as subject/object) and round-trips base direction on
//! literals — the deliberate "overcome, don't inherit" extension to stock RDF/JS
//! (`.goals`: SUBSUME, EXTEND, ENHANCE).
//!
//! ## Architecture
//!
//! The `#[wasm_bindgen]` surface is a thin shim over `gmeow-rdf` seams that already
//! exist: `TermFactory` (the DataFactory 1:1 map), `DatasetMut`/`MutableDataset` (the
//! mutable `DatasetCore`), `native_codecs` (parse/serialize), and the
//! `gmeow-rdf-events` protocol (the `Stream`/`Sink`). Mapping logic lives in plain
//! Rust so it unit-tests on the native workspace gate; the wasm-bindgen wrappers are
//! exercised as real wasm under `wasm-pack test --node`.

use wasm_bindgen::prelude::*;

// The idiomatic RDF/JS surface, built up parcel by parcel (issue #846):
//   * `term`    — RDF/JS Term types (NamedNode/BlankNode/Literal/Variable/DefaultGraph
//                 + the RDF-1.2 Quad-as-term wedge)
//   * `factory` — the RDF/JS DataFactory over the engine's owned term model
//   * `codec`   — format-name resolution for the native codecs
//   * `dataset` — the mutable RDF/JS DatasetCore over `MutableDataset`/`DatasetMut`
//                 (parse/serialize/size here; add/delete/has/match next commit)
//   * `stream`  — the RDF/JS Stream/Sink over the `gmeow-rdf-events` protocol  [Task 4]
mod codec;
mod dataset;
mod factory;
mod term;

pub use dataset::Dataset;
pub use factory::DataFactory;
pub use term::{Quad, Term};

/// The purrdf engine version (the crate's SemVer), exposed to JS as `version()`.
///
/// A liveness probe for the wasm build + the npm package: importing `purrdf` and
/// calling `version()` proves the module instantiated and the engine linked.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_the_crate_semver() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
