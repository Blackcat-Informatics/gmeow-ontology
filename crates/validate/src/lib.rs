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
//! Only [`py`] imports pyo3. The engine modules ([`store`], [`model`]) are
//! PyO3-free so the rlib links into the future Rust compiler without any Python
//! dependency.

pub mod coverage;
pub mod dsl;
pub mod gufo;
pub mod lint;
pub mod model;
pub mod store;

// PyO3 bindings — the only module that imports pyo3.
pub mod py;
