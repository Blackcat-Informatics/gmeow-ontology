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
pub mod describe;
pub mod error;
pub mod exec;
pub mod formats;
pub mod gmn1_primer;
pub mod i18n;
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
pub mod svg;
