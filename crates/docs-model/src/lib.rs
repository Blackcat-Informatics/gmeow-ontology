// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMEOW documentation MODEL — everything the docs surface needs that is
//! not the renderer itself.
//!
//! Split out of `gmeow-docs` so consumers can use the documentation model
//! without linking the renderer. `render.rs` and `vendored_asset.rs` are the
//! only two files in `gmeow-docs` that carry `include_bytes!`/`include_str!`,
//! and between them they embed 13.6 MB of vendored wasm engines. Anything that
//! depends on `gmeow-docs` inherits those bytes; the MCP tool surface reaches
//! the docs model through `card`, `gmn1_primer` and `llms`, and
//! `gmeow-slice-quality` reaches it through `i18n`, `i18n_compile`, `maturity`,
//! `model` and `rdf` — none of which needs a single embedded byte.
//!
//! This crate is therefore a LEAF with respect to the renderer: nothing here may
//! reference `gmeow_docs`.

pub mod badge;
pub mod card;
pub mod coverage;
// `describe` reaches into `gmeow_validate`'s native-only half (its Wikidata/HTTP-adjacent
// surface). Mirror that gate here rather than widening validate's wasm surface: nothing in
// the browser engine's path needs it.
#[cfg(not(target_arch = "wasm32"))]
pub mod describe;
pub mod error;
pub mod exec;
pub mod formats;
pub mod gmn1_primer;
pub mod i18n;
// The developer i18n authoring/compile family. Compiled on EVERY target: its only
// non-wasm-clean-looking import was `gmeow_validate::distinctiveness`, which is itself
// wasm-clean (pure `std::collections` computation) and no longer gated. The `std::fs`
// entry points here are inert on wasm rather than absent, and the browser-side
// slice-quality translation axis reads the PURE half (`parse_po`,
// `counts_as_reviewed_coverage`, `LOCALIZABLE_PREDICATES`, `expand_predicate`).
pub mod i18n_compile;
pub mod llms;
pub mod maturity;
pub mod model;
pub mod rdf;
/// The pure naming layer (slugs, display names, alignment facets) the renderer
/// re-exports at its original path.
pub mod slug;
pub mod source_map;
mod store;
/// The formal-concept lattice DERIVED from the `Surface × Capability` incidence in
/// [`formats`] — its order, bounds, Hasse edges, and Duquenne–Guigues implication basis.
pub mod surface_lattice;
pub mod svg;
