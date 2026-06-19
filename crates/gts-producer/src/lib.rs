// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-gts-producer` — Rust-native GTS producer for GMEOW.
//!
//! This crate will build GTS files directly from Oxigraph stores using the
//! public `gmeow_gts::writer::Writer` API. It is exposed to Python as the
//! `gmeow_gts_producer` extension module.

use pyo3::prelude::*;

/// Placeholder bootstrap function returning an empty byte vector.
#[pyfunction]
fn hello() -> Vec<u8> {
    Vec::new()
}

/// Python module entry point.
#[pymodule]
fn gmeow_gts_producer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hello, m)?)?;
    Ok(())
}
