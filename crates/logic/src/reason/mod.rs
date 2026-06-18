// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, ontology-independent OWL-2 reasoning over the Nemo chase.
//!
//! This module hosts fixed entailment rule sets — like ELK's built-in
//! calculus — that run over an arbitrary TBox/ABox through the world-scoped
//! ternary gmeow encoding. Unlike the user-authored `logic:` programs the
//! [`crate::compile`] pipeline projects, these rule sets are intrinsic to the
//! reasoner: they encode the OWL semantics themselves, not a domain ontology.
//!
//! Currently provides the EL subsumption closure ([`el`]); DL consistency and
//! the divergence ledger land in sibling modules.

pub mod el;

pub use el::{el_closure, ElClosure, InferredAxiom};
