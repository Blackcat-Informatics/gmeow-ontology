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
//! The crate is native Rust throughout; lint and i18n consumers share the same
//! typed model directly.

pub mod badge;
pub mod card;
pub mod coverage;
pub mod describe;
pub mod error;
pub mod exec;
pub mod fixture;
pub mod formats;
pub mod i18n;
pub mod i18n_compile;
pub mod lint;
pub mod llms;
pub mod maturity;
pub mod mdbook;
pub mod model;
pub mod rdf;
pub mod render;
mod store;
pub mod svg;

pub use describe::{
    DescribeGraph, DescribeStatus, Resolution, build_card, describe, describe_dataset, resolve_term,
};
pub use exec::{Entailment, ExecutableDocsData, InferenceDiff, example_key};
pub use i18n::{Translations, UiCatalog, available_languages, ui_string};
pub use lint::lint;
pub use llms::{GMEOW_SUMMARY, LLMS_NOTE_CAP};
pub use model::{
    ConstraintRule, DiagnosticsDigest, DocArtifact, DocCompetency, DocConcern, DocDependencyEdge,
    DocDiagFinding, DocExample, DocExpectedCell, DocExpectedRow, DocExternalTerm, DocFixture,
    DocFixtureKind, DocFlowEdge, DocLearningPath, DocLinkage, DocLossTarget, DocMappingSet,
    DocPipeline, DocRecipe, DocShape, DocSlice, DocStage, DocTerm, DocTermCategory, DocsError,
    DocsModel, TermLossDigest, TermLossRow,
};
pub use rdf::to_gmeow_rdf;
pub use render::{
    Page, Site, okf_doc_reference, render_purrdf_diagrams, render_site, render_site_lang,
    render_site_lang_exec, render_site_lang_exec_with_diagrams, to_html, to_markdown,
};
