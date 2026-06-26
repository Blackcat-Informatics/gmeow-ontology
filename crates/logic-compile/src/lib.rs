// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMEOW logic **compiler** (#664/#732): the pure, wasm-able half of the logic
//! stack — parse RDF 1.2 → [`ir::LogicProgram`] → project the eight committed
//! artifacts (OWL DL/EL, Datalog, N3, gUFO, canonical RDF-1.2, Nemo, projection
//! report). It carries **no reasoning-runtime dependencies**: no Nemo, Scryer,
//! tokio, PyO3, or oxigraph. The RDF parse/serialize path rides the wasm-clean
//! `gmeow-rdf` `gts` surface (the same surface `crates/rdf-wasm` uses), so the
//! whole compiler builds for `wasm32-unknown-unknown`.
//!
//! The reasoning runtime (worlds, the Nemo/Scryer chase, certify, the PyO3
//! `compile_logic` entrypoint) lives in the sibling `gmeow-logic` crate, which
//! depends on this one. `lower.rs` (compiler-IR → runtime `EvalRule`) and
//! `diagnostics_report` (a PyO3-tainted `gmeow_diagnostics::Report`) stay in
//! `gmeow-logic` by design — they are runtime concerns, not pure compilation.
//!
//! Modules are populated by the split in #732; this scaffold establishes the
//! crate and its wasm-clean dependency contract.

#![forbid(unsafe_code)]
