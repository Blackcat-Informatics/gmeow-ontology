// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMEOW logic **compiler**: the pure, wasm-able half of the logic
//! stack — the sole authority for compiling `logic:` source into the seven committed
//! artifacts. The Python duplicate (`logic_ir.py` / `logic_frontend.py` /
//! `logic_adapter.py` / `logic_projections.py`) has since been retired; this crate is
//! the source of truth, not a mirror of it.
//!
//! It carries **no reasoning-runtime dependencies**. The RDF parse/serialize path rides
//! the wasm-clean `gmeow-rdf` `gts`
//! surface (the same surface the purrdf wasm bindings use), so the whole compiler builds for
//! `wasm32-unknown-unknown`. The reasoning runtime (worlds, native forward/backward
//! evaluation, and certification) lives in the sibling `gmeow-logic` crate, which
//! depends on this one. The compiler-IR → runtime
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
//! doctrine). Text targets (Datalog and N3) are
//! **byte-identical** to those goldens; RDF targets (OWL-DL, OWL-EL, gUFO,
//! canonical-RDF12, the report) are **RDF-isomorphic**. Every ordering and hash the
//! artifacts depend on bottoms out in [`ir`]'s `sort_key` helpers.

#![forbid(unsafe_code)]

pub mod adapter;
// The logic-compiler diagnostic-code catalog: every hard compile/parse/lowering
// failure surfaces as a typed diagnostic on the shared substrate.
pub mod error;
// The CGIF (Conceptual Graph Interchange Format) text dialect: a bidirectional,
// PreservationKind::Exact conceptual-graph FOL surface (writer + reader).
pub mod cgif;
// The CLIF (Common Logic Interchange Format) text dialect: a bidirectional,
// PreservationKind::Exact s-expression FOL surface (writer + reader).
pub mod clif;
// The Common Logic round-trip isomorphism authority: proves a program round-trips
// through every CL dialect (clif/cgif/xcl) with IR isomorphism and that the three
// reconstructions are cross-dialect equivalent. Reused by the conformance harness.
pub mod cl_roundtrip;
pub mod compat;
pub mod frontend;
pub mod graphutil;
// Wasm-clean ingestion of the alignment DSL + ontology (the oxigraph-free read layer
// the correspondence lowerings consume; file I/O + parsing live in the caller).
pub mod ingest;
pub mod ir;
// The single loss store: one substrate DiagLedger every loss serialization
// (transcode / coherence certificate / F2 projection report) projects from.
pub mod loss_ledger;
// Shared N-Triples term codecs (escape only, no bracket/quote wrapping) used by the
// xcl/clif/cgif dialects' embedded canonical N-Triples RDF channel.
mod nt;
pub mod openehr_opt;
pub mod opt_lift;
pub mod projections;
pub mod relational_core;
pub mod restriction;
pub mod result_shape;
pub mod typing_vocab;
// The XCL (eXtended Common Logic Markup Language) XML dialect: a bidirectional,
// PreservationKind::Exact XML FOL surface (writer + reader), sibling of clif/cgif.
pub mod xcl;
