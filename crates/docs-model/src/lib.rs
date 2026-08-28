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
// The once-per-run, content-addressed disk cache for the documentation MODEL, plus
// the cache key / payload digest / atomic writer every fixture artifact shares. It
// lives HERE rather than in `gmeow-docs` because every model consumer must be able to
// reach it: `gmeow-slice-quality`'s DocMaturity axis reads the model once per repo
// root, and an edge from that crate to `gmeow-docs` would close a first-party cycle
// (`gmeow-docs` dev-depends on `gmeow-mcp`, which depends on `gmeow-slice-quality`).
// The renderer-only site/book caches layer on top of this key in `gmeow_docs::fixture`.
#[cfg(not(target_arch = "wasm32"))]
pub mod fixture;
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
// `llms.rs` is source-byte frozen by the model-facing invariance gate because it is
// the one shared llmstxt.org shape emitter. Its pre-extraction module docs still name
// the renderer through the old crate-local path, which cannot resolve from this leaf
// crate without creating the renderer/model dependency cycle this crate exists to
// break. Keep the lint exception at the module boundary so the frozen emitter remains
// byte-identical; every other intra-doc link in the crate remains denied by rustdoc.
#[allow(rustdoc::broken_intra_doc_links)]
pub mod llms;
pub mod maturity;
pub mod model;
/// The prose-quality PREDICATES (boundary statements, worked triples) the coverage
/// dimensions score against. They live beside the model rather than in the renderer
/// because `coverage` is the only caller and both moved together.
pub mod prose;
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
