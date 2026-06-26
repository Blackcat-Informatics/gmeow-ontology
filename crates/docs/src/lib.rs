// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-docs` — the Rust-owned documentation model for GMEOW.
//!
//! The crate projects the native slice catalog (`gmeow-slice`) into a single
//! typed, deterministic [`DocsModel`]: slices + manifest metadata, the artifact
//! inventory (by digest, never embedded bytes — blobs are by-reference per
//! project doctrine), the vocabulary terms parsed from each slice's
//! `module.ttl`, and the cross-slice dependency edges from the ownership
//! analyzer.
//!
//! The model in [`model`] is PyO3-free so every consumer (renderers, lint,
//! diagram, bundle) shares one source of truth. The [`render`] module turns the
//! model into a deterministic static-site tree (Markdown + self-contained HTML),
//! and [`svg`] hand-emits deterministic SVG diagrams folded into that tree.
//! Python bindings are kept in [`py`]; lint/i18n arrive in later tasks of #853.

pub mod i18n;
pub mod i18n_compile;
pub mod lint;
pub mod llms;
pub mod model;
pub mod rdf;
pub mod render;
pub mod svg;

// PyO3 bindings — the only module that imports pyo3.
pub mod py;

pub use i18n::{available_languages, ui_string, Translations, UiCatalog};
pub use lint::lint;
pub use llms::{GMEOW_SUMMARY, LLMS_NOTE_CAP};
pub use model::{
    DocArtifact, DocCompetency, DocConcern, DocDependencyEdge, DocExample, DocExternalTerm,
    DocLearningPath, DocLinkage, DocMappingSet, DocRecipe, DocShape, DocSlice, DocTerm,
    DocTermCategory, DocsError, DocsModel,
};
pub use rdf::to_gmeow_rdf;
pub use render::{render_site, render_site_lang, to_html, to_markdown, Page, Site};
// Re-export the module-registration entrypoint so the unified `gmeow_native`
// cdylib can populate the `gmeow_native.docs` submodule (#630).
pub use py::register;
