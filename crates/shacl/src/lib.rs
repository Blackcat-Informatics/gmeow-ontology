// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! `gmeow-shacl` — the Rust SHACL Core validator for gmeow.
//!
//! Validates an oxigraph RDF 1.2 data graph against a SHACL shapes graph with
//! NO inference (parity with pySHACL `inference="none"`). The engine core is
//! PyO3-free so the rlib links into the future Rust compiler over its own Store;
//! SPARQL-based constraints/targets arrive in a later task (#577).

pub mod constraints;
pub mod engine;
pub mod model;
pub mod path;
pub mod report;
pub mod shapes;
pub mod sparql;

// PyO3 bindings — native targets only (pyo3 cannot link into wasm32).
#[cfg(not(target_arch = "wasm32"))]
pub mod py;
