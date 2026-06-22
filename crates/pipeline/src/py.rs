// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 bindings for the pipeline — the `gmeow_native.pipeline` submodule (#861).
//!
//! Only this module imports `pyo3`, gated by the `python` feature, so the engine
//! core stays PyO3-free. The `run_pipeline` entrypoint (the single Python surface
//! that replaces the Python orchestrator) lands in P6; P1 ships the registration
//! shell so `crates/native` can wire the 8th submodule.

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Register the `gmeow_native.pipeline` submodule. Called by the unified
/// `gmeow_native` cdylib (#630). P1 registers nothing yet; `run_pipeline`
/// arrives in P6.
pub fn register(_m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
