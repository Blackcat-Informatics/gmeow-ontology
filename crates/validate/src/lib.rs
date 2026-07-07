// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-validate` — the Rust host for the GMEOW validation-path lints.
//!
//! This crate carries the two lowest-risk lints —
//! per-file Turtle syntax checking ([`store::parse_file`]) and the Principle 5
//! `owl:sameAs`-to-external-entity ban (store-scan consumers) —
//! routing `src/gmeow_tools/validate.py` through the Rust path and proving the
//! Rust↔Python validation seam end-to-end.
//!
//! # Platform posture
//!
//! The repo-free **Tier-1** validator ([`data_validate`]) compiles for
//! `wasm32-unknown-unknown`: it runs SHACL + the OntoUML disciplines over an
//! in-memory RDF data graph and a `gmeow.gts` byte blob, with no reasoner,
//! filesystem, threading, or PyO3 coupling. The wasm-clean core modules
//! ([`model`], [`codes`], [`store`], [`gufo`], [`findings`], [`data_validate`],
//! [`report_bridge`]) are compiled on every target.
//!
//! Everything else — the slice-authoring dev gate ([`validate_all`], with the
//! native DL reasoner + rayon), the repo-lint guards, the DSL phases, and the
//! Wikidata/HTTP lanes — is **native-only** (`#[cfg(not(target_arch = "wasm32"))]`).
//! The Tier-2 `--deep` semantic pass is excluded from the wasm surface by contract,
//! not degraded: the wasm boundary reaches validation solely through
//! [`data_validate::run_tier1`].
//!
//! # Engine core separation
//!
//! Only [`py`] and [`py_dsl`] import pyo3. The engine modules are PyO3-free so the
//! rlib links into a future Rust compiler (and the wasm target) without Python.

// Wasm-clean Tier-1 core: compiled on every target.
pub mod codes;
pub mod data_validate;
pub mod findings;
pub mod gufo;
pub mod model;
pub mod projection_profiles;
pub mod report_bridge;
pub mod store;

// Native-only: the slice-authoring dev gate, repo-lint guards, DSL phases, and the
// Tier-2 reasoner path all pull native-only crates (gmeow-logic, rayon, ureq,
// gmeow-slice) and cannot cross-compile to wasm.
#[cfg(not(target_arch = "wasm32"))]
pub mod advisory;
#[cfg(not(target_arch = "wasm32"))]
pub mod box_roles;
#[cfg(not(target_arch = "wasm32"))]
pub mod cache;
#[cfg(not(target_arch = "wasm32"))]
pub mod compliance;
#[cfg(not(target_arch = "wasm32"))]
pub mod constitution;
#[cfg(not(target_arch = "wasm32"))]
pub mod coverage;
#[cfg(not(target_arch = "wasm32"))]
pub mod crate_layering;
#[cfg(not(target_arch = "wasm32"))]
pub mod crossref;
#[cfg(not(target_arch = "wasm32"))]
pub mod dsl;
#[cfg(not(target_arch = "wasm32"))]
pub mod dsl_shacl;
#[cfg(not(target_arch = "wasm32"))]
pub mod instance;
#[cfg(not(target_arch = "wasm32"))]
pub mod language_tags;
#[cfg(not(target_arch = "wasm32"))]
pub mod lint;
#[cfg(not(target_arch = "wasm32"))]
pub mod mapping_eval;
#[cfg(not(target_arch = "wasm32"))]
pub mod repo_static;
#[cfg(not(target_arch = "wasm32"))]
pub mod rule_catalog;
#[cfg(not(target_arch = "wasm32"))]
pub mod self_desc;
#[cfg(not(target_arch = "wasm32"))]
pub mod signature;
#[cfg(not(target_arch = "wasm32"))]
pub mod slice_ownership;
#[cfg(not(target_arch = "wasm32"))]
pub mod statement;
#[cfg(not(target_arch = "wasm32"))]
pub mod time_util;
#[cfg(not(target_arch = "wasm32"))]
pub mod validate_all;
#[cfg(not(target_arch = "wasm32"))]
pub mod wikidata_audit;

// PyO3 bindings — enabled only for the unified native extension, never on wasm.
#[cfg(all(feature = "python", not(target_arch = "wasm32")))]
pub mod py;
#[cfg(all(feature = "python", not(target_arch = "wasm32")))]
pub mod py_dsl;

// Re-export the module-registration entrypoint so the unified `gmeow_native`
// cdylib can populate the `gmeow_native.validate` submodule.
#[cfg(all(feature = "python", not(target_arch = "wasm32")))]
pub use py::register;
