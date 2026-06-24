// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-xsd` — the native XSD **value space** for the RDF 1.2 query stack.
//!
//! This is a pure-Rust, **zero-runtime-dependency**, wasm-clean leaf crate. It is
//! the drop-in replacement for the oxigraph-family `oxsdatatypes`, and the first
//! foundation slice of the native SPARQL engine (purrdf S1, EPIC #906): the SPARQL
//! evaluator evaluates `FILTER`/`ORDER BY` over *typed values*, which this crate
//! supplies. It is deliberately decoupled from `gmeow-rdf-core` (no dependency in
//! either direction yet); the IR keeps literals **lexical-verbatim** (Constitution
//! C0.1) and this crate is the value layer that complements it.
//!
//! # Two distinct identities (load-bearing — do not conflate)
//!
//! A typed literal has TWO different notions of "equal", and mixing them silently
//! corrupts behavior:
//!
//! * **Term identity** — structural equality on the value's *representation*
//!   (`XsdValue: Eq + Hash`, added with the value type in a later task). This is the
//!   interner / cache key: a consumer builds a `HashMap<TermId, XsdValue>` keyed by
//!   representation. `"1"^^xsd:integer` and `"1.0"^^xsd:decimal` are **distinct**
//!   term identities.
//! * **Value-space identity** — SPARQL `=` / `<` over the *value* (the free fns
//!   `value_eq` / `value_cmp`, added with the operator surface). Here
//!   `"1"^^xsd:integer` and `"1.0"^^xsd:decimal` are **equal** (numeric promotion).
//!
//! `value_cmp` returns `Option<Ordering>`: `None` means the values are genuinely
//! **incomparable** (NaN, indeterminate-timezone dateTime, the two-component partial
//! order of `xsd:duration`, or non-comparable cross-types) — a spec-mandated outcome,
//! never a degraded fallback. `XsdValue` therefore does NOT implement `PartialOrd`
//! (that would re-introduce the conflation for `BTreeMap`); ordering is the free fn.
//!
//! # Hard-fail
//!
//! Malformed lexical input is a hard error ([`XsdError`], added with the value type),
//! never a silent default. Out-of-range integer/decimal lexicals fail rather than
//! saturate (this crate is `i128`-bounded — already exceeding `oxsdatatypes`' `i64`).

#![forbid(unsafe_code)]

pub mod datatype;

pub use datatype::{XsdDatatype, XSD_NS};
