// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMEOW Logic **compiler** — the native Rust port of the Python compiler
//! (`logic_ir.py` / `logic_frontend.py` / `logic_adapter.py` /
//! `logic_projections.py`), issue #664.
//!
//! Layered exactly like its Python ancestor:
//!
//! * [`ir`] — the canonical intermediate representation (a faithful port of
//!   `logic_ir.py`): the frozen dataclass hierarchy with the order-independent
//!   canonicalization contract.  This is the **one AST** the issue calls for; the
//!   evaluable IR ([`crate::rule_ir`]), the query IR ([`crate::query_ir`]), and the
//!   certifier ([`crate::certify`]) all lower from / are views of it.
//! * [`frontend`] — RDF 1.2 → [`ir::LogicProgram`] parser (port of
//!   `logic_frontend.py`).
//! * [`adapter`] — owl/gUFO → [`ir::LogicProgram`] normalization plus the
//!   IR-isomorphism gate (port of `logic_adapter.py`).
//! * [`projections`] — the seven projection back-ends + the overclaim gate +
//!   the projection report (port of `logic_projections.py`).
//!
//! # Parity contract
//!
//! The compiler is gated against the Python compiler by committed parity goldens
//! (trust-anchor doctrine, the #622/#636/#641 pattern).  Text targets (Datalog,
//! N3, Nemo) are **byte-identical**; RDF targets (OWL-DL, OWL-EL, gUFO,
//! canonical-RDF12, the report) are **RDF-isomorphic**.  Every ordering and hash
//! the artifacts depend on bottoms out in [`ir`]'s `sort_key` helpers, so those
//! are ported character-for-character (null-byte separators, Python `bool`
//! `Display` of `True`/`False`, corpus-safety append-only `negated`/`distinct`).

pub mod adapter;
pub mod frontend;
pub mod graphutil;
pub mod ir;
pub mod lower;
pub mod projections;
