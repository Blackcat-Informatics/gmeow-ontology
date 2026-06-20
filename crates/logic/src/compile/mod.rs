// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMEOW Logic **compiler** (issue #664) — the sole authority for compiling
//! `logic:` source into the eight committed artifacts.  The Python duplicate
//! (`logic_ir.py` / `logic_frontend.py` / `logic_adapter.py` /
//! `logic_projections.py`) was retired in #727; this crate is now the source of
//! truth, not a mirror of it.
//!
//! Layered into four phases:
//!
//! * [`ir`] — the canonical intermediate representation: the frozen value
//!   hierarchy with the order-independent canonicalization contract.  This is the
//!   **one AST** the issue calls for; the evaluable IR ([`crate::rule_ir`]), the
//!   query IR ([`crate::query_ir`]), and the certifier ([`crate::certify`]) all
//!   lower from / are views of it.
//! * [`frontend`] — RDF 1.2 → [`ir::LogicProgram`] parser.
//! * [`adapter`] — owl/gUFO → [`ir::LogicProgram`] normalization plus the
//!   IR-isomorphism gate.
//! * [`projections`] — the seven projection back-ends + the overclaim gate +
//!   the projection report.
//!
//! # Conformance contract
//!
//! The compiler's output is pinned by committed conformance goldens
//! (trust-anchor doctrine, the #622/#636/#641 pattern).  Text targets (Datalog,
//! N3, Nemo) are **byte-identical** to those goldens; RDF targets (OWL-DL,
//! OWL-EL, gUFO, canonical-RDF12, the report) are **RDF-isomorphic**.  Every
//! ordering and hash the artifacts depend on bottoms out in [`ir`]'s `sort_key`
//! helpers (null-byte separators, capitalized boolean `Display` of `True`/`False`
//! preserved for golden stability, corpus-safety append-only
//! `negated`/`distinct`).

pub mod adapter;
pub mod frontend;
pub mod graphutil;
pub mod ir;
pub mod lower;
pub mod projections;
