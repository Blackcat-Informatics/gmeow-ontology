// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMEOW logic **compiler** (#664/#732): the pure, wasm-able half of the logic
//! stack — the sole authority for compiling `logic:` source into the eight committed
//! artifacts. The Python duplicate (`logic_ir.py` / `logic_frontend.py` /
//! `logic_adapter.py` / `logic_projections.py`) was retired in #727; this crate is
//! the source of truth, not a mirror of it.
//!
//! It carries **no reasoning-runtime dependencies**: no Nemo, Scryer, tokio, PyO3,
//! or oxigraph. The RDF parse/serialize path rides the wasm-clean `gmeow-rdf` `gts`
//! surface (the same surface `crates/rdf-wasm` uses), so the whole compiler builds
//! for `wasm32-unknown-unknown`. The reasoning runtime (worlds, the Nemo/Scryer
//! chase, certify, the PyO3 `compile_logic` entrypoint) lives in the sibling
//! `gmeow-logic` crate, which depends on this one. The compiler-IR → runtime
//! `EvalRule` bridge (`lower.rs`) and the PyO3-tainted `diagnostics_report` stay in
//! `gmeow-logic` by design — they are runtime concerns, not pure compilation.
//!
//! Layered into four phases:
//!
//! * [`ir`] — the canonical intermediate representation: the frozen value hierarchy
//!   with the order-independent canonicalization contract. This is the **one AST**
//!   the issue calls for.
//! * [`frontend`] — RDF 1.2 → [`ir::LogicProgram`] parser.
//! * [`adapter`] — owl/gUFO → [`ir::LogicProgram`] normalization plus the
//!   IR-isomorphism gate.
//! * [`projections`] — the seven projection back-ends + the overclaim gate + the
//!   projection report.
//!
//! # Conformance contract
//!
//! The compiler's output is pinned by committed conformance goldens (trust-anchor
//! doctrine, the #622/#636/#641 pattern). Text targets (Datalog, N3, Nemo) are
//! **byte-identical** to those goldens; RDF targets (OWL-DL, OWL-EL, gUFO,
//! canonical-RDF12, the report) are **RDF-isomorphic**. Every ordering and hash the
//! artifacts depend on bottoms out in [`ir`]'s `sort_key` helpers.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod compat;
pub mod frontend;
pub mod graphutil;
// Wasm-clean ingestion of the alignment DSL + ontology (the oxigraph-free read layer
// the correspondence lowerings consume; file I/O + parsing live in the caller).
pub mod ingest;
pub mod ir;
pub mod projections;
pub mod result_shape;
