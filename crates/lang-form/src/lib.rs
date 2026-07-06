// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-lang-form` — the `lang:` form AST, its content keys, and interning.
//!
//! This crate is the Rust realization of the `lang:` form-core charter: a stratified
//! form AST whose identity is a deterministic **content key** computed over structural
//! content alone — sign system, stratum, typed morphology, and slot structure with
//! constituent keys — and **never** over any surface string, encoding, script, casing,
//! normalization, or rendering. Two consequences are load-bearing:
//!
//! * Re-encoding, re-normalizing, or re-transliterating a text creates new
//!   [`SurfaceForm`]s but no new [`Form`]s: the realization fan-out widens, the form
//!   stands. Surface material is kept on [`SurfaceForm`] with its own [`SurfaceForm::surface_key`]
//!   so it is excluded from form identity *by construction*.
//! * Word order and grammatical function are identity-bearing: a [`Composed`](Form::Composed)
//!   form's key includes its slot indexes, roles, and dependency edges, so swapping two
//!   constituents is a different form.
//!
//! Identity always flows through [`Form::content_key`]; the AST deliberately derives no
//! `Ord`/`PartialEq` for canonical use, so the derived-`Ord`-versus-lexical-sort trap
//! cannot arise — collections are ordered by their content key via
//! [`slice::sort_by_cached_key`], never by variant-declaration order.
//!
//! The runtime lifting of external material (plain text, CoNLL-U, EBNF, OntoLex) into
//! this AST is a separate concern (the runtime charter); this crate provides the AST,
//! the key, and the interner that lifting targets.

mod ast;
mod intern;
mod key;

pub use ast::{AnalysisLevel, Form, MorphFeature, Slot, SurfaceForm};
pub use intern::{Interner, dedup_by_content_key};
