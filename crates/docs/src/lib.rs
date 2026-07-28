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

pub mod fixture;
pub mod lint;
pub mod mdbook;
// The documentation model now lives in `gmeow-docs-model`, re-exported here at
// its original paths so every `gmeow_docs::<module>` caller is unchanged.
pub use gmeow_docs_model::{
    badge, card, coverage, describe, error, exec, formats, gmn1_primer, i18n, i18n_compile, llms,
    maturity, model, rdf, slug, source_map, surface_lattice, svg,
};

pub mod render;
// The pure naming layer (slugs, display names, alignment facets), hoisted out of
// `render` so the model half of this crate can use it without the renderer.
pub mod vendored_asset;

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
    DocMarkdownDocument, DocPipeline, DocRecipe, DocSeam, DocSeamDirection, DocShape, DocSlice,
    DocStage, DocTerm, DocTermCategory, DocsError, DocsModel, TermLossDigest, TermLossRow,
};
pub use rdf::to_gmeow_rdf;
pub use render::{
    Page, Site, okf_doc_reference, render_purrdf_diagrams, render_site, render_site_lang,
    render_site_lang_exec, render_site_lang_exec_with_diagrams, to_html, to_markdown,
};
pub use source_map::{
    DocLinkResolution, DocumentEntry, HeadingAnchor, LinkResolution, SourceToPageMap,
    TargetLocation,
};
