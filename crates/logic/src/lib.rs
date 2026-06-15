// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! `gmeow-logic` — the Rust core of the gmeow reasoning engine.
//!
//! This crate is the Rust counterpart of the Python reference oracle; it models
//! worlds as oxigraph named graphs and provides world-indexed entailment queries
//! gated against the same language-neutral conformance corpus as `gmeow-gts`.
//!
//! Nemo-based rule evaluation and PyO3 bindings arrive in later tasks.

pub mod seam;
pub mod store;
pub mod versioning;

// PyO3 Python bindings — native targets only.
// pyo3 physically cannot link into a wasm binary (the CPython C extension ABI
// is unavailable on wasm32); this cfg is platform-correct, not optionality.
#[cfg(not(target_arch = "wasm32"))]
pub mod py;

// Nemo reasoner bridge — native targets only.
// Nemo's transitive deps (reqwest, tower-lsp) require OS networking unavailable
// on wasm32; this cfg is platform-correct, not an optionality toggle.
#[cfg(not(target_arch = "wasm32"))]
pub mod nemo_engine;

// WebAssembly entry points — wasm32 target only.
// wasm-bindgen binds to the JS host runtime which does not exist on native.
#[cfg(target_arch = "wasm32")]
pub mod wasm;
