// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-validate` — the Rust host for the GMEOW validation-path lints.
//!
//! As of EPIC #575 / issue #579 this crate carries the two lowest-risk lints —
//! per-file Turtle syntax checking ([`store::parse_file`]) and the Principle 5
//! `owl:sameAs`-to-external-entity ban ([`store::scan_quads`] consumers) —
//! routing `src/gmeow_tools/validate.py` through the Rust path and proving the
//! Rust↔Python validation seam end-to-end.
//!
//! # Platform posture
//!
//! This crate is **native-only** and carries **NO architecture cfg guards
//! anywhere**. A capability cfg would be optionality, not compliance (the
//! no-optionality / hard-fail doctrine, #579). pyo3 is a plain unconditional
//! dependency, never behind a target table.
//!
//! # Engine core separation
//!
//! Only [`py`] and [`py_dsl`] import pyo3. The engine modules ([`store`],
//! [`model`]) are PyO3-free so the rlib links into the future Rust compiler
//! without any Python dependency.

pub mod cache;
pub mod constitution;
pub mod coverage;
pub mod dsl;
pub mod findings;
pub mod gufo;
pub mod instance;
pub mod language_tags;
pub mod lint;
pub mod model;
pub mod signature;
pub mod slice_ownership;
pub mod statement;
pub mod store;
pub mod validate_all;

pub mod crossref;
pub mod dsl_shacl;

// PyO3 bindings — the only modules that import pyo3.
pub mod py;
pub mod py_dsl;

// Re-export the module-registration entrypoint so the unified `gmeow_native`
// cdylib can populate the `gmeow_native.validate` submodule (#630).
pub use py::register;
