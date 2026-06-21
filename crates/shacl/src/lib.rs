// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-shacl` — the Rust SHACL Core validator for gmeow.
//!
//! Validates an oxigraph RDF 1.2 data graph against a SHACL shapes graph with
//! NO inference (parity with pySHACL `inference="none"`). The engine core is
//! PyO3-free so the rlib links into the future Rust compiler over its own Store.
//! SHACL-AF SPARQL-based constraints (`sh:sparql`/`sh:SPARQLConstraint`) and
//! targets (`sh:SPARQLTarget`) are implemented in the [`sparql`] module,
//! delegated to oxigraph's SPARQL 1.1 engine (#577).

pub mod constraints;
pub mod engine;
pub mod model;
pub mod path;
pub mod report;
pub mod shapes;
pub mod sparql;

/// Crate version string for cache/toolchain salt parity with Python package
/// versions (`metadata.version("gmeow-shacl")`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// PyO3 bindings — native targets only (pyo3 cannot link into wasm32).
#[cfg(not(target_arch = "wasm32"))]
pub mod py;

// Re-export the module-registration entrypoint so the unified `gmeow_native`
// cdylib can populate the `gmeow_native.shacl` submodule (#630).
#[cfg(not(target_arch = "wasm32"))]
pub use py::register;
