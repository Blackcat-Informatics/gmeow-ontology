// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow_native` — the single unified PyO3 extension module (#630).
//!
//! # Why one cdylib
//!
//! `gmeow-diagnostics` defines the `Report` / `Finding` pyclasses. When that
//! crate is statically linked into several *separate* cdylibs (the old
//! `gmeow_rdf` / `gmeow_validate` / `gmeow_logic` / `gmeow_shacl` /
//! `gmeow_diagnostics` extensions), each cdylib gets its **own** copy of the
//! `Report` type. A `Report` produced by one extension is then "not an instance
//! of" the `Report` of another — PyO3 raises
//! `TypeError: 'Report' object is not an instance of 'Report'` across the seam.
//!
//! Folding all five engine crates into ONE cdylib makes `PyReport` (and every
//! other shared pyclass) a single type the whole extension agrees on.
//!
//! # Layout
//!
//! Each engine is registered into its own submodule:
//!
//! * `gmeow_native.rdf` — the RDF 1.2 kernel: statement codec + oxigraph
//!   Store/SPARQL/parse/canonicalize surface (#667).
//! * `gmeow_native.diagnostics` — the Finding/Report model + renderers (#654).
//! * `gmeow_native.shacl` — the SHACL Core validator.
//! * `gmeow_native.validate` — the validation-path lints + orchestration.
//! * `gmeow_native.logic` — the reasoning engine surface.
//! * `gmeow_native.slice` — the native slice catalog + ownership/dependency
//!   analyzer (#820 S8): the authoritative slice machinery.
//! * `gmeow_native.docs` — the typed documentation model built from the slice
//!   catalog (#853): slices, terms, and dependency edges.
//!
//! Each submodule is also registered in `sys.modules` under its dotted name so
//! `import gmeow_native.validate` (and friends) resolves. The legacy import
//! names (`import gmeow_rdf`, `import gmeow_validate`, …) are thin Python shims
//! (see `crates/native/python/`) that alias themselves to the matching submodule
//! object, so the ~60 existing call sites keep working unchanged — and crucially
//! resolve to the SAME submodule object, hence the SAME pyclass types.

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Build one engine submodule, register it on the parent, and expose it in
/// `sys.modules` under `gmeow_native.<name>` so `import gmeow_native.<name>`
/// works.
fn add_engine_submodule(
    py: Python<'_>,
    parent: &Bound<'_, PyModule>,
    sys_modules: &Bound<'_, pyo3::types::PyAny>,
    name: &str,
    register: impl FnOnce(&Bound<'_, PyModule>) -> PyResult<()>,
) -> PyResult<()> {
    let sub = PyModule::new(py, name)?;
    // `PyModule::new` sets no `__file__`. The legacy import-name shims
    // (`gmeow_logic`, `gmeow_rdf`, …) swap themselves in `sys.modules` for this
    // submodule object, so it must satisfy `module.__file__` introspection — a CI
    // import smoke test exercises exactly `gmeow_logic.__file__`. Borrow the
    // parent cdylib's `__file__` (the installed `.so`).
    if let Ok(file) = parent.getattr("__file__") {
        sub.setattr("__file__", file)?;
    }
    register(&sub)?;
    parent.add_submodule(&sub)?;
    sys_modules.set_item(format!("gmeow_native.{name}"), &sub)?;
    Ok(())
}

/// The unified `gmeow_native` extension module.
#[pymodule]
fn gmeow_native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let sys_modules = py.import("sys")?.getattr("modules")?;

    add_engine_submodule(py, m, &sys_modules, "rdf", gmeow_rdf::register)?;
    add_engine_submodule(
        py,
        m,
        &sys_modules,
        "diagnostics",
        gmeow_diagnostics::register,
    )?;
    add_engine_submodule(py, m, &sys_modules, "shacl", gmeow_shacl::register)?;
    add_engine_submodule(py, m, &sys_modules, "validate", gmeow_validate::register)?;
    add_engine_submodule(py, m, &sys_modules, "logic", gmeow_logic::register)?;
    add_engine_submodule(py, m, &sys_modules, "slice", gmeow_slice::register)?;
    add_engine_submodule(py, m, &sys_modules, "docs", gmeow_docs::register)?;

    Ok(())
}
