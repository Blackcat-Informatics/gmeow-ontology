// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic static-site renderers for the documentation model (PyO3-free).
//!
//! This is the redesigned replacement for the legacy Python `ontology_docs`
//! HTML/markdown surface — *not* a byte-for-byte port. The renderers are pure
//! functions of [`DocsModel`]: they never read the filesystem. Two layers:
//!
//! 1. [`to_markdown`] produces a Markdown document for a [`Page`].
//! 2. [`to_html`] converts that Markdown body to HTML via `pulldown-cmark` and
//!    injects it into the minijinja [`shell`](SHELL) template (doctype, head,
//!    site nav, `<main>`, footer), yielding fully self-contained HTML with an
//!    embedded local CSS theme — no network assets.
//!
//! [`render_site`] walks the model into the full page set and emits a
//! [`Site`] — a sorted tree of relative-path → bytes holding the `.md` and
//! `.html` for every page plus `assets/gmeow.css`. Determinism is structural:
//! `Site.files` is a [`BTreeMap`], every rendered list is sorted by a stable
//! key, and no `HashMap` iteration reaches the output. Rendering the same model
//! twice is byte-identical.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use gmeow_docs_model::slug::short_iri_digest;
use gmeow_errors::{Diag, Result};
use minijinja::{Environment, Value as TemplateValue};
use pulldown_cmark::{Options, Parser, html as cmark_html};
use purrdf_lex::json::record::ToJson;
use purrdf_lex::json::{self, Object, Value};

use crate::badge;
use crate::exec::ExecutableDocsData;
use crate::i18n::{self, ENGLISH};
use crate::llms::{self, LlmsBullet, LlmsSection};
use crate::model::{DocChangelogEntry, DocConcern, DocSlice, DocTerm, DocTermCategory, DocsModel};
use crate::source_map::{
    DocLinkResolution, LinkResolution, SLICE_PAGE_SOURCE, SourceToPageMap, fence_open,
};
use crate::svg;

/// Admit the source-link authority for a caller-supplied model without a panic.
pub(crate) fn source_page_map(model: &DocsModel) -> Result<SourceToPageMap> {
    SourceToPageMap::build(model).map_err(|cause| {
        Diag::of_kind(crate::render_error::SourceMap::caused_by(
            "admit documentation source-to-page map".into(),
            cause,
        ))
    })
}

/// The GMEOW vocabulary namespace (mirrors `model.rs`).
use gmeow_ns::GMEOW_NS;

// Full predicate IRIs used to resolve translated label / definition / title
// values (the `.po` msgctxt predicate, CURIE-expanded). Mirror `model.rs`.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const DCTERMS_TITLE: &str = "http://purl.org/dc/terms/title";

/// The `lang:` grounding slice's IRI. Every term this slice owns cross-links
/// to the notation-grammars index from its term page (see `md_term`'s
/// "Notation grammars" section) — a whole-slice link rather than a per-field
/// heuristic match against logic stereotypes/frameworks: every term owned by
/// this slice genuinely belongs to the sign-system/notation vocabulary the
/// grammars describe, so the whole-slice link is honest and always correct,
/// never a fragile keyword guess.
const GMEOW_LANG_SLICE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/lang";

/// The embedded minijinja HTML shell (doctype + head + nav + body + footer).
const SHELL: &str = include_str!("../templates/shell.html");

/// The embedded, self-contained CSS theme, emitted to `assets/gmeow.css`.
const CSS: &str = include_str!("../assets/gmeow.css");

/// The site-relative path the CSS asset is emitted to.
const CSS_PATH: &str = "assets/gmeow.css";

/// The site-relative path the offline SPARQL playground's bundled RDF asset (TriG)
/// is emitted to. Language-neutral: the RDF is language-independent.
const PLAYGROUND_TRIG_PATH: &str = "assets/playground.trig";

/// The site-relative path of the **conjecture demo library** — the curated
/// `logic:Conjecture` corpus as Turtle ([`ExecutableDocsData::conjectures_ttl`]). The W4
/// conjecture playground fetches + byte-verifies it (via [`BUNDLE_MANIFEST_PATH`]) and
/// presents its curated conjectures alongside the live wasm symmetric engine.
const CONJECTURES_PATH: &str = "assets/conjectures.ttl";

/// The site-relative path of the FULL `gmeow.gts` bundle
/// ([`ExecutableDocsData::full_bundle_gts`]) — the in-browser Tier-1 validate surface
/// reads its `shapes-archive`. An EXTERNAL site asset (never re-embedded in the bundle).
const FULL_BUNDLE_GTS_PATH: &str = "assets/gmeow.gts";

/// The site-relative path of the browser-bundle integrity manifest: a JSON map from
/// each bundle asset path to its `blake3:<hex>` content address and byte length, so
/// the client loader can record/verify which exact bytes it fetched. A deterministic
/// function of the emitted asset bytes.
const BUNDLE_MANIFEST_PATH: &str = "assets/bundle-manifest.json";

/// The site-relative path of the docs controller module (SPARQL playground query
/// execution + result transcoding). A self-contained ES module.
const DOCS_JS_PATH: &str = "assets/gmeow-docs.js";

/// The embedded docs controller module, emitted to [`DOCS_JS_PATH`] when the
/// playground is present.
const DOCS_JS: &str = include_str!("../assets/gmeow-docs.js");

/// The site-relative path of the shared MCP transport — the engine boot, the JSON-RPC
/// frame shape and the reasoning-segment demand loading, in one place.
///
/// The standalone console's engine worker imports it. The documentation site does NOT:
/// the site dispatches per capability to the four in-repo wasm engines, and this is the
/// console's protocol seam, emitted here because both surfaces pack from one asset set.
pub(crate) const MCP_TRANSPORT_PATH: &str = "assets/mcp-transport.mjs";

/// The embedded shared MCP transport, emitted to [`MCP_TRANSPORT_PATH`].
const MCP_TRANSPORT: &str = include_str!("../assets/mcp-transport.mjs");
const REASONING_ARTIFACT_PATH: &str = "assets/reasoning-artifact.mjs";
const REASONING_ARTIFACT: &str = include_str!("../assets/reasoning-artifact.mjs");

/// The site-relative path of the client-side BLAKE3 the transport verifies every
/// integrity-pinned asset with. The browser has no BLAKE3 primitive and `crypto.subtle`
/// offers none, so the project's own content-address function is reimplemented rather
/// than a weaker check substituted.
pub(crate) const BLAKE3_PATH: &str = "assets/blake3.mjs";

/// The embedded client-side BLAKE3, emitted to [`BLAKE3_PATH`].
const BLAKE3_MODULE: &str = include_str!("../assets/blake3.mjs");

/// The wasm engines emitted under `assets/<name>/` when the playground is present: the
/// offline SPARQL/bundle-explorer runtime, the repo-free Tier-1 validator, the
/// structured-DL reasoner and the GMN codec. Every one is built in this repository; one
/// descriptor per asset lives in [`crate::vendored_asset`], and this is an alias for that
/// module's single registry rather than a second copy of the list.
use crate::vendored_asset::ALL_ASSETS as VENDORED_WASM_ASSETS;

// ── Pages ──────────────────────────────────────────────────────────────────

/// A single logical page in the site. Each page renders to both a `.md` and a
/// `.html` file under [`Page::dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    /// The landing page (`index`).
    Landing,
    /// `getting-started/index`.
    GettingStarted,
    /// `about/index`.
    About,
    /// The documentation-health dashboard (`health/index`) — per-dimension
    /// coverage of the vocabulary surface and a completeness distribution.
    Health,
    /// `changelog/index`.
    Changelog,
    /// A vocabulary category index (`classes/`, `properties/`, …).
    Category(DocTermCategory),
    /// A single term detail page (`terms/<slug>/index`).
    Term(String),
    /// The slice index (`slices/index`).
    SliceIndex,
    /// A single slice page (`slices/<slug>/index`).
    Slice(String),
    /// A slice child-document page (`slices/<slug>/documents/<stem>/index`) — one
    /// non-slice-page `text/markdown` source (a `design/*.md`, or any markdown
    /// other than the top-level `docs.md`) rendered as its own page. `slice` is
    /// the owning slice IRI and `path` is the document's normalized slice-relative
    /// source path (e.g. `design/ARCHITECTURE.md`); the page path is minted through
    /// the single [`crate::source_map::page_for`] authority.
    SliceDocument {
        /// The owning slice IRI.
        slice: String,
        /// The document's normalized slice-relative source path.
        path: String,
    },
    /// The linkages (term equivalences) index (`linkages/index`).
    LinkageIndex,
    /// The worked-examples index (`examples/index`).
    ExampleIndex,
    /// The conformance Do/Don't fixtures index (`fixtures/index`) — well-formed
    /// instances and deliberately malformed counter-examples, grouped by slice,
    /// joined to their expected outcome / violation code / rationale when a
    /// slice authors an `example-conformance.ttl` binding.
    FixtureIndex,
    /// The competency-questions index (`competency/index`) — every declarative
    /// SPARQL competency question, grouped by slice, with its rationale, the
    /// terms it exercises, the full copy-paste-runnable query, and its expected
    /// result rows/count.
    CompetencyIndex,
    /// The notation-grammars index (`notation/index`) — every first-class
    /// `lang:Grammar` rendering (GMN / GTS / Turtle) authored under
    /// `slices/grounding/lang/grammars/*.ebnf` as plain W3C EBNF text, with its
    /// title and license, linking to the full source on its own
    /// [`Page::Grammar`] page.
    NotationIndex,
    /// A single notation-grammar detail page (`notation/<slug>/index`) — the
    /// full W3C EBNF source for one grammar, plus its title and license.
    Grammar(String),
    /// The concerns index (`concerns/index`).
    ConcernIndex,
    /// A single concern page (`concerns/<slug>/index`).
    Concern(String),
    /// The external-ontologies index (`external-ontologies/index`).
    ExternalIndex,
    /// The integrity-constraints (verify queries) index
    /// (`integrity-constraints/index`).
    IntegrityIndex,
    /// The constraint catalog (`enforced-constraints/index`) — every
    /// `gmeow:ValidationRule` the validator enforces, grouped by category and
    /// anchored by the same slug the validator's `helpUri` uses.
    ConstraintCatalog,
    /// The logic-stereotypes index (`logic/index`) — terms grouped by their
    /// lowered OntoUML/UFO stereotype. Resolves the `nav_logic` chrome string.
    Logic,
    /// The canonical-IR compiler-product page (`logic/canonical-ir/index`) — the
    /// one AST / RDF 1.2 identity serialization the `logic:` compiler produces.
    LogicCanonicalIr,
    /// The preservation loss-ledger compiler-product page
    /// (`logic/loss-ledger/index`) — per-target preservation kind, complexity
    /// class, and structural lossy drops.
    LogicLossLedger,
    /// The derivation-graph compiler-product page
    /// (`logic/derivation-graph/index`) — the reasoning provenance (per-axiom
    /// proof skeletons).
    LogicDerivationGraph,
    /// The compiler-diagnostics product page (`logic/diagnostics/index`) — parse
    /// findings + lossy-drop notes, surfaced as SARIF.
    LogicDiagnostics,
    /// The adoption-recipes index (`recipes/index`).
    RecipeIndex,
    /// A single recipe page (`recipes/<slug>/index`).
    Recipe(String),
    /// The learning-paths index (`learning-paths/index`).
    LearningPathIndex,
    /// A single learning-path page (`learning-paths/<slug>/index`).
    LearningPath(String),
    /// The "four boxes" doctrine page (`four-boxes/index`).
    FourBoxes,
    /// The build-pipeline DAG page (`pipeline/index`) — the dogfooded
    /// `gmeow:Pipeline` build graph rendered as a deterministic SVG plus a stage
    /// table (impl, capabilities, resources, dataflow). Emitted only when the
    /// model carries a discovered pipeline (`model.pipeline.is_some()`).
    PipelineDag,
    /// The RDF-to-developer glossary (`glossary/index`) — a fixed mapping from RDF
    /// / OWL / SHACL vocabulary to the everyday software concepts they correspond
    /// to, a bridge for readers who do not know RDF.
    Glossary,
    /// The grounding seam registry (`seams/index`) — every sanctioned
    /// cross-grounding `gmeow:Seam` (direction, carrying terms, owning design
    /// doc), projected from the canonical data authored in the grounding
    /// slices' manifests (never a hand-authored table). See
    /// [`crate::model::DocSeam`].
    SeamRegistry,
    /// The offline SPARQL playground (`sparql/index`). Emitted only when the pipeline
    /// supplies a bundled query asset (never in a model-only render).
    SparqlPlayground,
    /// The bundle explorer (`explorer/index`) — browser `gmeow info`/`describe` over
    /// the object-level core bundle. Emitted only when the bundle assets ship
    /// (`has_bundle()`).
    BundleExplorer,
    /// The conjecture playground (`conjectures/index`, the WASM-interactive docs W4 deliverable) — the browser
    /// symmetric conjecture / anti-conjecture engine over the curated demo library, run
    /// client-side by the SAME native `logic:` reasoner via the vendored wasm
    /// `conjecture` export. Emitted only when the conjecture demo asset + bundle ship
    /// (`has_conjectures()`).
    ConjecturePlayground,
}

impl Page {
    /// The site-relative directory this page lives in (no trailing slash, empty
    /// for the root landing page).
    pub fn dir(&self) -> String {
        match self {
            Page::Landing => String::new(),
            Page::GettingStarted => "getting-started".to_string(),
            Page::About => "about".to_string(),
            Page::Health => "health".to_string(),
            Page::Changelog => "changelog".to_string(),
            Page::Category(category) => category_dir(*category).to_string(),
            Page::Term(slug) => format!("terms/{slug}"),
            Page::SliceIndex => "slices".to_string(),
            Page::Slice(slug) => format!("slices/{slug}"),
            Page::SliceDocument { slice, path } => {
                // The child page path is minted by the single `source_map` authority
                // (`slices/{slug}/documents/{stem}/`); `dir()` is that path without its
                // trailing slash, so the site's `join(dir, "index.{md,html}")` matches
                // the `Page::Slice` convention.
                let page = crate::source_map::page_for(&slice_slug_of_iri(slice), path);
                page.strip_suffix('/').unwrap_or(&page).to_string()
            }
            Page::LinkageIndex => "linkages".to_string(),
            Page::ExampleIndex => "examples".to_string(),
            Page::FixtureIndex => "fixtures".to_string(),
            Page::CompetencyIndex => "competency".to_string(),
            Page::NotationIndex => "notation".to_string(),
            Page::Grammar(slug) => format!("notation/{slug}"),
            Page::ConcernIndex => "concerns".to_string(),
            Page::Concern(slug) => format!("concerns/{slug}"),
            Page::ExternalIndex => "external-ontologies".to_string(),
            Page::IntegrityIndex => "integrity-constraints".to_string(),
            Page::ConstraintCatalog => "enforced-constraints".to_string(),
            Page::Logic => "logic".to_string(),
            Page::LogicCanonicalIr => "logic/canonical-ir".to_string(),
            Page::LogicLossLedger => "logic/loss-ledger".to_string(),
            Page::LogicDerivationGraph => "logic/derivation-graph".to_string(),
            Page::LogicDiagnostics => "logic/diagnostics".to_string(),
            Page::RecipeIndex => "recipes".to_string(),
            Page::Recipe(slug) => format!("recipes/{slug}"),
            Page::LearningPathIndex => "learning-paths".to_string(),
            Page::LearningPath(slug) => format!("learning-paths/{slug}"),
            Page::FourBoxes => "four-boxes".to_string(),
            Page::PipelineDag => "pipeline".to_string(),
            Page::Glossary => "glossary".to_string(),
            Page::SeamRegistry => "seams".to_string(),
            Page::SparqlPlayground => "sparql".to_string(),
            Page::BundleExplorer => "explorer".to_string(),
            Page::ConjecturePlayground => "conjectures".to_string(),
        }
    }

    /// The site-relative `.md` path for this page.
    pub fn md_path(&self) -> String {
        join(&self.dir(), "index.md")
    }

    /// The site-relative `.html` path for this page.
    pub fn html_path(&self) -> String {
        join(&self.dir(), "index.html")
    }

    /// The human page title used in `<title>` and the shell brand line.
    pub fn title(&self, model: &DocsModel) -> String {
        match self {
            Page::Landing => model.title.clone(),
            Page::GettingStarted => "Getting started".to_string(),
            Page::About => "About".to_string(),
            Page::Health => "Documentation health".to_string(),
            Page::Changelog => "Changelog".to_string(),
            Page::Category(category) => category_plural(*category).to_string(),
            Page::Term(slug) => model
                .terms
                .iter()
                .find(|t| term_slug(t) == *slug)
                .map(|t| t.curie.clone())
                .unwrap_or_else(|| slug.clone()),
            Page::SliceIndex => "Slices".to_string(),
            Page::Slice(slug) => model
                .slices
                .iter()
                .find(|s| slice_slug(s) == *slug)
                .map(slice_display)
                .unwrap_or_else(|| slug.clone()),
            Page::SliceDocument { slice, path } => model
                .slices
                .iter()
                .find(|s| &s.iri == slice)
                .and_then(|s| s.documents.iter().find(|d| &d.source_path == path))
                .map(|d| d.title.clone())
                .unwrap_or_else(|| path.clone()),
            Page::LinkageIndex => "Linkages".to_string(),
            Page::ExampleIndex => "Examples".to_string(),
            Page::FixtureIndex => "Conformance fixtures".to_string(),
            Page::CompetencyIndex => "Competency questions".to_string(),
            Page::NotationIndex => "Notation grammars".to_string(),
            Page::Grammar(slug) => model
                .grammars
                .iter()
                .find(|g| g.slug == *slug)
                .map(|g| g.title.clone())
                .unwrap_or_else(|| slug.clone()),
            Page::ConcernIndex => "Concerns".to_string(),
            Page::Concern(slug) => model
                .concerns
                .iter()
                .find(|c| concern_slug(c) == *slug)
                .map(concern_display)
                .unwrap_or_else(|| slug.clone()),
            Page::ExternalIndex => "External ontologies".to_string(),
            Page::IntegrityIndex => "Integrity constraints".to_string(),
            Page::ConstraintCatalog => "What GMEOW enforces".to_string(),
            Page::Logic => "Logic & Reasoning".to_string(),
            Page::LogicCanonicalIr => "Canonical IR".to_string(),
            Page::LogicLossLedger => "Preservation loss ledger".to_string(),
            Page::LogicDerivationGraph => "Derivation graph".to_string(),
            Page::LogicDiagnostics => "Compiler diagnostics".to_string(),
            Page::RecipeIndex => "Recipes".to_string(),
            Page::Recipe(slug) => model
                .recipes
                .iter()
                .find(|r| r.slug == *slug)
                .map(|r| r.title.clone())
                .unwrap_or_else(|| slug.clone()),
            Page::LearningPathIndex => "Learning paths".to_string(),
            Page::LearningPath(slug) => model
                .learning_paths
                .iter()
                .find(|p| p.slug == *slug)
                .map(|p| p.title.clone())
                .unwrap_or_else(|| slug.clone()),
            Page::FourBoxes => "What is this?".to_string(),
            Page::PipelineDag => "Build pipeline".to_string(),
            Page::Glossary => "Glossary".to_string(),
            Page::SeamRegistry => "Grounding seams".to_string(),
            Page::SparqlPlayground => "SPARQL playground".to_string(),
            Page::BundleExplorer => "Bundle explorer".to_string(),
            Page::ConjecturePlayground => "Conjecture playground".to_string(),
        }
    }
}

// ── Site ─────────────────────────────────────────────────────────────────────

/// The complete rendered static site: a deterministic map of relative path →
/// file bytes (sorted, so serialization is byte-reproducible).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// All emitted files keyed by site-relative path.
    pub files: BTreeMap<String, Vec<u8>>,
}

/// Render the full English static-site tree from the model.
///
/// Emits, for every page, both `<dir>/index.md` and `<dir>/index.html`, plus the
/// CSS asset at `assets/gmeow.css`. The output is byte-identical across calls.
/// This is exactly [`render_site_lang`] for the English carrier language.
///
/// # Errors
/// Refuses ambiguous source pages, dangling internal links, heading overflow or failure of a selected syntax provider.
pub fn render_site(model: &DocsModel) -> Result<Site> {
    render_site_lang(model, ENGLISH)
}

/// Render the full static-site tree for a target language.
///
/// `lang` is the English carrier (`"english"`) or a BCP-47 code (`"fr"`, `"zh"`)
/// present in [`DocsModel::available_languages`]. Every localizable string — term
/// and slice labels / definitions, concern / recipe / learning-path text, and the
/// UI-chrome nav — is resolved to its translation via
/// [`Translations::lookup`](crate::i18n::Translations::lookup) /
/// [`ui_string`](crate::i18n::ui_string), falling back to the English value the
/// model carries. The per-language tree is deterministic and preserves the
/// no-dangling-link invariant (slugs / IRIs are language-independent).
///
/// # Errors
/// Refuses ambiguous source pages, dangling internal links, heading overflow or failure of a selected syntax provider.
pub fn render_site_lang(model: &DocsModel, lang: &str) -> Result<Site> {
    render_site_lang_exec(model, lang, &ExecutableDocsData::default())
}

/// Render the full static-site tree for a target language, **with** the build-time
/// executable-docs data.
///
/// This is the render the pipeline uses: `exec` carries the reasoned "try it" diffs
/// and the offline SPARQL playground asset (which also backs term/slice export via
/// `DESCRIBE`). The executable surfaces are rendered **only when `exec` supplies data
/// AND only in the English carrier tree** (their RDF/JS/wasm are language-independent,
/// so duplicating them per locale would bloat the bundle) — an
/// [`ExecutableDocsData::default`] (the model-only render used by unit tests / the
/// PyO3 preview) produces the complete base site without them. See [`render_site_lang`]
/// for the model-only convenience.
///
/// # Errors
/// Refuses ambiguous source pages, dangling internal links, heading overflow or failure of a selected syntax provider.
pub fn render_site_lang_exec(
    model: &DocsModel,
    lang: &str,
    exec: &ExecutableDocsData,
) -> Result<Site> {
    // The language-invariant purrdf graph diagrams are the dominant render cost;
    // render them once here (a multi-language archive shares a single render — see
    // [`render_site_lang_exec_with_diagrams`] and the pipeline's `build_docs_archive`).
    let diagrams = render_purrdf_diagrams(model);
    render_site_lang_exec_with_diagrams(model, lang, exec, &diagrams)
}

/// The language-invariant purrdf-backed graph diagrams: the slice-dependency graph,
/// the per-slice local dependency views, and the per-term neighbourhoods. Their node
/// labels are IRI local names and their edges are structural IRIs — none of which the
/// localizer rewrites — so they are byte-identical across every language tree.
/// Rendering these thousands of purrdf graphs is the dominant snapshot-render cost, so
/// a multi-language archive renders them ONCE via this function and shares the result
/// (see [`render_site_lang_exec_with_diagrams`]). The language-DEPENDENT diagrams
/// (`concerns.svg`, whose bars carry translated concern labels) and the cheap
/// hand-emitted `coverage-heatmap.svg` / `pipeline.svg` are rendered per language.
pub fn render_purrdf_diagrams(model: &DocsModel) -> BTreeMap<String, Vec<u8>> {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert(
        "diagrams/slices.svg".to_string(),
        svg::slice_dependency_svg(model).into_bytes(),
    );
    for slice in &model.slices {
        files.insert(
            format!("diagrams/slices/{}.svg", slice_slug(slice)),
            svg::slice_local_svg(model, &slice.iri).into_bytes(),
        );
    }
    // Per-term neighbourhood diagrams — only for terms that actually have a
    // neighbourhood, gated on the same predicate as the page embed so the two never
    // disagree (no dangling image paths).
    for term in &model.terms {
        if svg::term_has_neighbourhood(term) {
            files.insert(
                format!("diagrams/terms/{}.svg", term_slug(term)),
                svg::term_neighbourhood_svg(term).into_bytes(),
            );
        }
    }
    files
}

/// [`render_site_lang_exec`] with the language-invariant purrdf graph diagrams
/// supplied pre-rendered, so a multi-language render produces those thousands of
/// graphs ONCE and splices the identical bytes into every language tree. `diagrams`
/// MUST be [`render_purrdf_diagrams`] of the same `model`.
///
/// # Errors
/// Refuses ambiguous source pages, dangling internal links, heading overflow or failure of a selected syntax provider.
pub fn render_site_lang_exec_with_diagrams(
    model: &DocsModel,
    lang: &str,
    exec: &ExecutableDocsData,
    diagrams: &BTreeMap<String, Vec<u8>>,
) -> Result<Site> {
    // The executable surfaces (playground, export links, wasm engine, query asset) are
    // language-INDEPENDENT: their RDF/JS/wasm are identical across locales. Emitting
    // them in every translated tree would triple the bundled asset. They therefore
    // live ONLY in the English carrier tree; a non-English render behaves exactly like
    // a model-only render (byte-identical base site).
    let empty = ExecutableDocsData::default();
    let exec: &ExecutableDocsData = if lang == ENGLISH { exec } else { &empty };

    // Build a localized copy of the model so the existing renderers (which read
    // label / definition / title directly) emit translated content with English
    // fallback. The English carrier needs no rewrite.
    let localized;
    let model: &DocsModel = if lang == ENGLISH {
        model
    } else {
        localized = localize_model(model, lang);
        &localized
    };

    // The single link-rewrite authority, built ONCE for this whole site render and threaded
    // through every page/index renderer below (a pure function of the — possibly localized —
    // model). Previously each of md_slice / md_slice_document / search_index_json / llms_txt /
    // llms_full_txt rebuilt it, re-deriving the same document anchors N+M+3 times per render.
    let page_map = source_page_map(model)?;

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    for page in pages(model) {
        files.insert(
            page.md_path(),
            to_markdown_exec_with_map(model, &page, exec, &page_map)?.into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec_with_map(model, &page, lang, exec, &page_map)?.into_bytes(),
        );
    }
    files.insert(CSS_PATH.to_string(), CSS.as_bytes().to_vec());

    // The interactive asset set (the controller module, the vendored wasm engines, and
    // the SPARQL-playground / browser-bundle data) — the ONE authority both the site
    // here and the packed mdbook ([`crate::mdbook`]) draw from, so the book ships the
    // byte-identical engines the site's witness lanes prove. Language-independent,
    // emitted only in the English tree (see the gate above).
    files.extend(interactive_asset_files(exec));

    // The standalone console tree: its shell, service worker and offline manifest.
    // ADDITIVE — it emits under its own prefix and replaces nothing the site emitted.
    files.extend(crate::console::console_files(exec));

    // The offline SPARQL playground page. Term/slice export runs through this same
    // engine + asset via `DESCRIBE`, so no static export files are needed.
    if exec.has_playground() {
        let page = Page::SparqlPlayground;
        files.insert(
            page.md_path(),
            to_markdown_exec_with_map(model, &page, exec, &page_map)?.into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec_with_map(model, &page, lang, exec, &page_map)?.into_bytes(),
        );
    }

    // The bundle explorer page (browser gmeow info/describe + live reasoning + GMN
    // transcode over the core bundle). Its assets are shipped by
    // `interactive_asset_files` above.
    if exec.has_bundle() {
        let page = Page::BundleExplorer;
        files.insert(
            page.md_path(),
            to_markdown_exec_with_map(model, &page, exec, &page_map)?.into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec_with_map(model, &page, lang, exec, &page_map)?.into_bytes(),
        );
    }

    // The conjecture playground page (browser symmetric proof / counterproof over the
    // curated demo library + the live wasm conjecture engine). Its assets (the demo
    // library + the vendored reason engine) are shipped by `interactive_asset_files`.
    if exec.has_conjectures() {
        let page = Page::ConjecturePlayground;
        files.insert(
            page.md_path(),
            to_markdown_exec_with_map(model, &page, exec, &page_map)?.into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec_with_map(model, &page, lang, exec, &page_map)?.into_bytes(),
        );
    }

    // Diagram SVGs. The node-link graph diagrams (slice dependency graph, per-slice
    // local dependencies, per-term neighbourhoods) are rendered by gmeow's shipped
    // RDF-graph renderer (`purrdf::viz`) and supplied pre-rendered via `diagrams`
    // (they are language-invariant, so a multi-language archive renders them once);
    // the chart diagrams (concern bar chart, coverage heatmap) and the
    // capability-coloured pipeline DAG stay hand-emitted here, as those are not
    // node-link graphs. `concerns.svg` in particular is language-DEPENDENT (its bars
    // carry translated concern labels), so it must render per language. Every emitted
    // path also has a page embed, so no image link dangles.
    for (path, bytes) in diagrams {
        files.insert(path.clone(), bytes.clone());
    }
    files.insert(
        "diagrams/concerns.svg".to_string(),
        svg::concern_overview_svg(model).into_bytes(),
    );
    // The per-slice documentation-coverage heatmap embedded on the health page.
    files.insert(
        "diagrams/coverage-heatmap.svg".to_string(),
        svg::coverage_heatmap_svg(model).into_bytes(),
    );
    // The dogfooded build-pipeline DAG, embedded on `Page::PipelineDag` (emitted
    // only when a pipeline was discovered, so the page's embed never dangles).
    if model.pipeline.is_some() {
        files.insert(
            "diagrams/pipeline.svg".to_string(),
            svg::pipeline_dag_svg(model).into_bytes(),
        );
    }

    // Shared color-coded badge SVGs (deduped; one per distinct (family, value)).
    // Emitted from the same `badge::term_badges` source the term pages reference,
    // so the referenced and emitted asset sets are identical (no dangling image).
    for (path, svg) in badge::site_badge_assets(model) {
        files.insert(path, svg.into_bytes());
    }

    // Static indexes (deterministic, pure functions of the model).
    files.insert(
        "search-index.json".to_string(),
        search_index_json_with_map(model, &page_map).into_bytes(),
    );
    // Standard llmstxt.org surfaces: a links-only index and a complete
    // inlined form, both at the site root, superseding the ad-hoc `llms-docs.txt`.
    files.insert(
        "llms.txt".to_string(),
        llms_txt_with_map(model, &page_map).into_bytes(),
    );
    files.insert(
        "llms-full.txt".to_string(),
        llms_full_txt_with_map(model, &page_map)?.into_bytes(),
    );

    // Prompt-ready per-term cards: a compact, link-free Markdown card per
    // term at `terms/{slug}/card.md`, for context-window injection. The alignment
    // facets are precomputed once so emitting every card stays O(N), not O(N²).
    //
    // Alongside the human-oriented `card.md` two machine surfaces ride:
    // `card.json` — the STANDARD-tier `Card` serialized through the SAME
    // native JSON record the live MCP `doc_card format=json detail=standard` uses
    // (the same record shape, values, and tier, with distinct member ordering) — and
    // `card-full.md` — the FULL-tier oracle card: the compact card enriched with
    // the rich panels (entailments, Do / Don't fixtures, diagnostics, projection
    // loss) drawn from `model` + `exec`, rendered by the ONE canonical
    // `render_card` at `CardDetail::Full` (no second full-card renderer).
    {
        let alignment_facets = precompute_alignment_facets(model);
        let fixtures_by_curie = precompute_fixtures_by_curie(model);
        for term in &model.terms {
            let slug = term_slug(term);
            files.insert(
                format!("terms/{slug}/card.md"),
                term_card_md_inner(term, &alignment_facets, model).into_bytes(),
            );
            // card.json — the standard-tier neutral card, deterministically
            // serialized in its native declaration order; every internal collection
            // is already ordered.
            let standard = doc_term_card(term, &alignment_facets, model)
                .projected(crate::card::CardDetail::Standard);
            let json = json::write_compact(&standard.to_json()).into_bytes();
            files.insert(format!("terms/{slug}/card.json"), json);
            // card-full.md — the full-tier oracle card.
            let full = full_card_for(model, exec, term, &alignment_facets, &fixtures_by_curie);
            let title = format!("{}{}", term.curie, term_signature(term));
            files.insert(
                format!("terms/{slug}/card-full.md"),
                crate::card::render_card(&title, &full, crate::card::CardDetail::Full).into_bytes(),
            );
        }
    }

    Ok(Site { files })
}

/// Write a rendered [`Site`] tree under `directory`, creating parent directories
/// as needed, in the engine's fixed sorted `BTreeMap` order. Returns the written
/// paths. Pure Rust (no Python GIL) so it is directly unit-testable; the PyO3
/// `DocSet::write_artifacts` method is a thin wrapper over this.
pub fn write_site(
    site: &Site,
    directory: &std::path::Path,
) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut written = Vec::with_capacity(site.files.len());
    for (rel, data) in &site.files {
        let path = directory.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        written.push(path);
    }
    Ok(written)
}

/// The deterministically ordered set of pages that constitute the mdbook.
///
/// This is exactly the site's [`pages`] set (same order, same inclusion rules),
/// exposed so the mdbook renderer in [`crate::mdbook`] can walk it without
/// duplicating the ordering logic. It deliberately EXCLUDES
/// [`Page::SparqlPlayground`] and every other exec-only interactive surface —
/// those are never in [`pages`], so the book cannot accidentally reference a
/// page it does not emit.
pub fn book_pages(model: &DocsModel) -> Vec<Page> {
    pages(model)
}

/// The browser-bundle integrity manifest JSON — a stable, sorted, byte-deterministic
/// map from each bundle asset path to its `blake3:<hex>` content address and length.
/// A pure function of the emitted bundle bytes; the client loader records/verifies it.
fn bundle_manifest_json(exec: &ExecutableDocsData) -> String {
    // One entry per shipped bundle asset, in a fixed deterministic order (full bundle,
    // then the playground corpus, then the optional conjecture demo library). EVERY
    // client-fetched RDF asset gets an entry: the browser loader verifies each fetch
    // against its recorded content address, and an asset with no entry could only be
    // accepted unverified — the bypass the manifest exists to make impossible.
    let entry = |path: &str, bytes: &[u8]| {
        format!(
            "  \"{path}\": {{ \"blake3\": \"blake3:{d}\", \"bytes\": {n} }}",
            d = purrdf_hash::hex::encode(purrdf_hash::blake3::hash(bytes).as_bytes()),
            n = bytes.len(),
        )
    };
    let mut entries = vec![entry(FULL_BUNDLE_GTS_PATH, &exec.full_bundle_gts)];
    if !exec.playground_trig.is_empty() {
        entries.push(entry(PLAYGROUND_TRIG_PATH, &exec.playground_trig));
    }
    // The conjecture demo library ships iff the W4 playground surface is rendered.
    if exec.has_conjectures() {
        entries.push(entry(CONJECTURES_PATH, &exec.conjectures_ttl));
    }
    format!("{{\n{}\n}}\n", entries.join(",\n"))
}

/// The interactive asset FILES (no pages) an exec-backed render ships, keyed by their
/// site-relative path (`assets/…`): the shared controller module, the vendored wasm
/// engines, and — when present — the SPARQL playground data and the browser bundle +
/// its integrity manifest.
///
/// This is the SINGLE authority for "which interactive assets exist": the static site
/// emits them at the site root and the mdbook packer ([`crate::mdbook`]) copies the
/// same map under the book `src/` tree, so both surfaces carry byte-identical engines —
/// the ones the native↔wasm witness lanes prove. Empty when the render is neither
/// playground- nor bundle-backed.
pub fn interactive_asset_files(exec: &ExecutableDocsData) -> BTreeMap<String, Vec<u8>> {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    if !exec.has_playground() && !exec.has_bundle() && !exec.has_conjectures() {
        return files;
    }
    files.insert(DOCS_JS_PATH.to_string(), DOCS_JS.as_bytes().to_vec());
    // The console's protocol seam and the client-side content-address function. Emitted
    // with the interactive set rather than from `console_files`, because `console_files`
    // folds THIS map in and derives the service worker's shell from the result — an asset
    // the console fetches but the shell never lists is the offline hole that derivation
    // exists to close.
    files.insert(
        MCP_TRANSPORT_PATH.to_string(),
        MCP_TRANSPORT.as_bytes().to_vec(),
    );
    files.insert(BLAKE3_PATH.to_string(), BLAKE3_MODULE.as_bytes().to_vec());
    files.insert(
        REASONING_ARTIFACT_PATH.to_string(),
        REASONING_ARTIFACT.as_bytes().to_vec(),
    );
    for asset in VENDORED_WASM_ASSETS {
        asset.emit_into(&mut files);
    }
    if exec.has_playground() {
        files.insert(
            PLAYGROUND_TRIG_PATH.to_string(),
            exec.playground_trig.clone(),
        );
    }
    if exec.has_bundle() {
        files.insert(
            FULL_BUNDLE_GTS_PATH.to_string(),
            exec.full_bundle_gts.clone(),
        );
        files.insert(
            BUNDLE_MANIFEST_PATH.to_string(),
            bundle_manifest_json(exec).into_bytes(),
        );
    }
    // The conjecture demo library: an UNCONDITIONAL site sub-asset whenever the W4
    // playground surface renders (the release path hard-fails on an empty declared
    // sub-asset, so it must always be present when interactive). Its integrity entry
    // rides in the bundle manifest emitted just above (`has_conjectures()` ⟹ `has_bundle()`).
    if exec.has_conjectures() {
        files.insert(CONJECTURES_PATH.to_string(), exec.conjectures_ttl.clone());
    }
    files
}

/// The DOM hooks the documentation controller binds to.
///
/// This is the single inventory of "a page carries an interactive control", read by the
/// script-injection gate and by the console producer. It mirrors the elements
/// `assets/gmeow-docs.js` binds a handler to; a widget that added a hook without adding
/// it here would render a control with no controller, which is exactly the defect the
/// derived gate exists to prevent.
///
/// The mirror is itself gated: `controller_hooks_match_the_shipped_controller_selectors`
/// scrapes the shipped controller module for every element it looks up
/// (`getElementById` / `querySelectorAll`) AND registers a listener on, and asserts
/// set-EQUALITY with this list in both directions. A hook added to the controller and not
/// to this list — or removed from the controller and left here — fails that test, so this
/// list cannot drift out of agreement with the module it mirrors.
pub(crate) const CONTROLLER_HOOKS: &[&str] = &[
    "gmeow-run-validation",
    "id=\"gmeow-explorer-form\"",
    "id=\"gmeow-reason-form\"",
    "id=\"gmeow-conjecture-form\"",
    "id=\"gmeow-gmn-form\"",
    "id=\"gmeow-sparql\"",
];

/// Whether a rendered body carries any control the documentation controller drives.
#[must_use]
pub fn body_carries_interactive_control(body: &str) -> bool {
    CONTROLLER_HOOKS.iter().any(|hook| body.contains(hook))
}

/// The full, deterministically ordered page set for the model.
fn pages(model: &DocsModel) -> Vec<Page> {
    let mut pages = vec![
        Page::Landing,
        Page::GettingStarted,
        Page::About,
        Page::Health,
        Page::Changelog,
        Page::SliceIndex,
        Page::LinkageIndex,
        Page::ExampleIndex,
        Page::FixtureIndex,
        Page::CompetencyIndex,
        Page::NotationIndex,
        Page::ConcernIndex,
        Page::ExternalIndex,
        Page::IntegrityIndex,
        Page::ConstraintCatalog,
        Page::Logic,
        Page::LogicCanonicalIr,
        Page::LogicLossLedger,
        Page::LogicDerivationGraph,
        Page::LogicDiagnostics,
        Page::RecipeIndex,
        Page::LearningPathIndex,
        Page::Glossary,
        Page::SeamRegistry,
    ];
    // The curated "four boxes" doctrine page only when its prose is present.
    if model.four_boxes.is_some() {
        pages.push(Page::FourBoxes);
    }
    // The build-pipeline DAG page only when a pipeline was discovered (a bare
    // unit-test model without the pipeline module omits it — honest absence).
    if model.pipeline.is_some() {
        pages.push(Page::PipelineDag);
    }
    // Per-recipe and per-learning-path pages (slugs are deterministic).
    for recipe in &model.recipes {
        pages.push(Page::Recipe(recipe.slug.clone()));
    }
    for path in &model.learning_paths {
        pages.push(Page::LearningPath(path.slug.clone()));
    }
    // Per-grammar detail pages (model.grammars is already sorted by slug).
    for grammar in &model.grammars {
        pages.push(Page::Grammar(grammar.slug.clone()));
    }
    // Category indexes only for categories that have at least one term, in a
    // fixed order.
    for category in [
        DocTermCategory::Class,
        DocTermCategory::Property,
        DocTermCategory::Individual,
        DocTermCategory::Datatype,
        DocTermCategory::Other,
    ] {
        if model.terms.iter().any(|t| t.category == category) {
            pages.push(Page::Category(category));
        }
    }
    // Terms are already sorted by IRI in the model; slug is derived from the
    // local name and is deterministic.
    for term in &model.terms {
        pages.push(Page::Term(term_slug(term)));
    }
    for slice in &model.slices {
        pages.push(Page::Slice(slice_slug(slice)));
        // Each of the slice's NON-slice-page markdown sources gets its own child
        // page, right after the slice page. `slice.documents` is path-sorted, so
        // the child pages are emitted in logical-path order (matching
        // `SourceToPageMap::slice_children`). The top-level `docs.md` IS the slice
        // page (grafted into it), so it is not a child.
        for doc in &slice.documents {
            if doc.source_path != SLICE_PAGE_SOURCE {
                pages.push(Page::SliceDocument {
                    slice: slice.iri.clone(),
                    path: doc.source_path.clone(),
                });
            }
        }
    }
    for concern in &model.concerns {
        pages.push(Page::Concern(concern_slug(concern)));
    }
    pages
}

// ── Markdown layer ────────────────────────────────────────────────────────────

/// Render a page to a Markdown document (the page body, no HTML shell).
///
/// # Errors
/// Refuses ambiguous source pages, dangling internal links, heading overflow or failure of a selected syntax provider.
pub fn to_markdown(model: &DocsModel, page: &Page) -> Result<String> {
    to_markdown_exec(model, page, &ExecutableDocsData::default())
}

/// Render a page's Markdown body, appending the build-time executable-docs sections
/// (per-slice export links, reasoner "try it" diffs) when `exec` supplies them. With
/// an empty `exec` this is byte-identical to the base render (the executable surfaces
/// simply do not appear), so the model-only goldens are unaffected.
///
/// # Errors
/// Refuses ambiguous source pages, dangling internal links, heading overflow or failure of a selected syntax provider.
pub fn to_markdown_exec(
    model: &DocsModel,
    page: &Page,
    exec: &ExecutableDocsData,
) -> Result<String> {
    // The single link-rewrite authority is a pure function of the model; a standalone
    // page render builds it here. A full site/book render instead builds it ONCE and
    // calls [`to_markdown_exec_with_map`] per page, so the map is not rebuilt for every
    // slice/child page (the anchors were otherwise re-derived N+M times per render).
    let page_map = source_page_map(model)?;
    to_markdown_exec_with_map(model, page, exec, &page_map)
}

/// [`to_markdown_exec`] with the shared [`SourceToPageMap`] threaded in, so a full
/// site/book render builds it once. Byte-identical to `to_markdown_exec` (the map is a
/// deterministic function of the model).
pub(crate) fn to_markdown_exec_with_map(
    model: &DocsModel,
    page: &Page,
    exec: &ExecutableDocsData,
    page_map: &SourceToPageMap,
) -> Result<String> {
    let mut md = match page {
        Page::Term(slug) => {
            let mut md = md_term(model, slug, exec)?;
            append_term_export_section(&mut md, model, slug, exec);
            md
        }
        Page::Slice(slug) => {
            let mut md = md_slice(model, slug, page_map)?;
            append_slice_executable_sections(&mut md, model, slug, exec);
            md
        }
        Page::Landing => {
            let mut md = to_markdown_base(model, page, page_map)?;
            append_landing_interactive_nav(&mut md, model, exec);
            md
        }
        Page::SparqlPlayground => md_playground(model, exec),
        Page::BundleExplorer => md_bundle_explorer(model, exec),
        Page::ConjecturePlayground => md_conjecture_playground(model, exec),
        _ => to_markdown_base(model, page, page_map)?,
    };
    // The generalized page-level cite-this-surface on every durable NON-term page
    // (the term page carries its own richer content-addressed Citation section).
    // Distinct from the coarse-grain provenance footer below.
    append_cite_this_surface(&mut md, model, page);
    // Every durable page carries the coarse-grain provenance footer: the
    // producing-stage chain of the docs render, walked BACKWARD over
    // `gmeow:dataflowConsumes` from `stage-docs-render` — the build-grain
    // projection of the single provenance relation. A no-op on a bare model whose
    // catalog carries no pipeline (honest absence).
    append_provenance_footer(&mut md, model);
    Ok(md)
}

fn to_markdown_base(model: &DocsModel, page: &Page, page_map: &SourceToPageMap) -> Result<String> {
    Ok(match page {
        Page::Landing => md_landing(model),
        Page::GettingStarted => md_getting_started(model),
        Page::About => md_about(model),
        Page::Health => md_health(model),
        Page::Changelog => md_changelog(model),
        Page::Category(category) => md_category(model, *category),
        Page::Term(slug) => md_term(model, slug, &ExecutableDocsData::default())?,
        Page::SliceIndex => md_slice_index(model),
        Page::Slice(slug) => md_slice(model, slug, page_map)?,
        Page::SliceDocument { slice, path } => md_slice_document(model, slice, path, page_map)?,
        Page::LinkageIndex => md_linkage_index(model),
        Page::ExampleIndex => md_example_index(model),
        Page::FixtureIndex => md_fixture_index(model),
        Page::CompetencyIndex => md_competency_index(model),
        Page::NotationIndex => md_notation_index(model),
        Page::Grammar(slug) => md_grammar(model, slug),
        Page::ConcernIndex => md_concern_index(model),
        Page::Concern(slug) => md_concern(model, slug),
        Page::ExternalIndex => md_external_index(model),
        Page::IntegrityIndex => md_integrity_index(model),
        Page::ConstraintCatalog => md_constraint_catalog(model),
        Page::Logic => md_logic_index(model),
        Page::LogicCanonicalIr => md_logic_canonical_ir(model),
        Page::LogicLossLedger => md_logic_loss_ledger(model),
        Page::LogicDerivationGraph => md_logic_derivation_graph(model),
        Page::LogicDiagnostics => md_logic_diagnostics(model),
        Page::RecipeIndex => md_recipe_index(model),
        Page::Recipe(slug) => md_recipe(model, slug),
        Page::LearningPathIndex => md_learning_path_index(model),
        Page::LearningPath(slug) => md_learning_path(model, slug),
        Page::FourBoxes => md_four_boxes(model),
        Page::PipelineDag => md_pipeline_dag(model),
        Page::Glossary => md_glossary(model),
        Page::SeamRegistry => md_seam_registry(model),
        // Routed through `to_markdown_exec`; this arm keeps the match exhaustive.
        Page::SparqlPlayground => md_playground(model, &ExecutableDocsData::default()),
        Page::BundleExplorer => md_bundle_explorer(model, &ExecutableDocsData::default()),
        Page::ConjecturePlayground => {
            md_conjecture_playground(model, &ExecutableDocsData::default())
        }
    })
}

// ── Executable-docs surfaces (rendered only when `exec` supplies data) ─────────

/// Append the per-slice export affordance (a `DESCRIBE` in the playground) and the
/// reasoner "try it" inference blocks to a slice page's Markdown. No-op when `exec`
/// carries no data.
fn append_slice_executable_sections(
    out: &mut String,
    model: &DocsModel,
    slug: &str,
    exec: &ExecutableDocsData,
) {
    let Some(slice) = model.slices.iter().find(|s| slice_slug(s) == slug) else {
        return;
    };
    let root = root_href(&Page::Slice(slug.to_string()).dir());

    // Export: query this slice's vocabulary in the offline playground and copy the
    // result in any RDF format (client-side, no static per-format files).
    if exec.has_playground() {
        heading(out, 2, "Export");
        let query = format!("DESCRIBE <{}>", slice.iri);
        let encoded = url_query_encode(&query);
        line(
            out,
            &format!(
                "[Explore this slice in the SPARQL playground]({root}sparql/index.html?q={encoded}) \
                 — query its vocabulary offline and copy the result as Turtle / N-Triples / \
                 N-Quads / TriG / RDF-XML / JSON-LD."
            ),
        );
        // OKF is a structural per-concept Markdown bundle with no per-slice document;
        // point at its root index (the entry point to every term's OKF projection).
        line(
            out,
            "Each of this slice's terms also ships an OKF Markdown projection under the \
             `gmeow-okf/` bundle (see `gmeow-okf/index.md`).",
        );
    }

    // Try it: the reasoner's inferences over each worked example.
    let mut wrote_heading = false;
    for example in model.examples.iter().filter(|e| e.slice == slice.iri) {
        let Some(diff) = exec.inference_for(&slice.iri, &example.logical_path) else {
            continue;
        };
        if diff.inferred.is_empty() {
            continue;
        }
        if !wrote_heading {
            heading(out, 2, "Try it — reasoner inferences");
            line(
                out,
                "Fed through the native reasoner, each worked example yields these \
                 additional triples beyond what it asserts:",
            );
            wrote_heading = true;
        }
        heading(out, 3, &example.title);
        // Show both columns of the diff — the example's asserted ABox and, beneath it,
        // what the reasoner derived on top of it — so the surface is genuinely
        // asserted-vs-inferred rather than inferred-only.
        if !diff.asserted.is_empty() {
            line(out, "**Asserted**");
            fenced(out, "turtle", &diff.asserted.join("\n"));
        }
        line(out, "**Inferred**");
        fenced(out, "turtle", &diff.inferred.join("\n"));
    }
}

/// The `gmeow-okf/` bundle-relative path of a term's OKF (Ontology Knowledge
/// Format) Markdown document, or `None` for a category the OKF bundle does not
/// emit a per-concept document for (datatypes / other). The `{category-dir}/
/// {local-name}.md` scheme MUST match the OKF projection in the pipeline
/// (`crates/pipeline/src/stages/okf.rs` — `category_dir` + `slug`); a mismatch
/// would produce a dangling reference, so only the three covered categories emit
/// one and the datatype/other arms deliberately return `None`.
pub fn okf_doc_reference(term: &DocTerm) -> Option<String> {
    let dir = match term.category {
        DocTermCategory::Class => "classes",
        DocTermCategory::Property => "properties",
        DocTermCategory::Individual => "individuals",
        DocTermCategory::Datatype | DocTermCategory::Other => return None,
    };
    // The OKF projection is derived from the COMPOSED ontology (the carrier term surface),
    // whose term collector covers only the `gmeow:` core vocabulary (see
    // `crate::stages::export::collect_terms` — namespace-gated). A term in another
    // namespace — a grounding-language term (`lang:`/`logic:`/`math:`), an external IRI, or
    // a docs-site-only nested-example term whose curie is a full IRI — is NOT in the OKF
    // export universe, so it emits no OKF link (and the OKF-coverage gate, which pairs this
    // reference against the emitted OKF docs, correctly skips it rather than flagging a
    // dangling link the OKF bundle never promised to render).
    let (prefix, local) = term.curie.split_once(':')?;
    if prefix != "gmeow" || local.contains(['/', '#']) {
        return None;
    }
    Some(format!("gmeow-okf/{dir}/{local}.md"))
}

/// Append the per-term export affordance (a `DESCRIBE` in the playground + the
/// prompt-ready card + the OKF projection reference) to a term page's Markdown.
/// No-op without a playground.
fn append_term_export_section(
    out: &mut String,
    model: &DocsModel,
    slug: &str,
    exec: &ExecutableDocsData,
) {
    if !exec.has_playground() {
        return;
    }
    let Some(term) = model.terms.iter().find(|t| term_slug(t) == slug) else {
        return;
    };
    let root = root_href(&Page::Term(slug.to_string()).dir());
    heading(out, 2, "Export");
    // A DESCRIBE prefilled into the offline playground: the reader runs it in-browser
    // and copies the result in any RDF format. The card is the prompt-ready projection.
    let query = format!("DESCRIBE <{}>", term.iri);
    let encoded = url_query_encode(&query);
    line(
        out,
        &format!(
            "[Describe this term in the SPARQL playground]({root}sparql/index.html?q={encoded}) \
             (run offline, copy as Turtle / N-Triples / N-Quads / TriG / RDF-XML / JSON-LD) · \
             [prompt-ready card]({root}terms/{slug}/card.md)"
        ),
    );
    // OKF is a structural multi-file Markdown bundle (not a serialize codec the
    // playground can transcode), so its per-concept document is referenced by its
    // path in the sibling `gmeow-okf/` bundle rather than transcoded inline.
    if let Some(okf) = okf_doc_reference(term) {
        line(
            out,
            &format!("The OKF Markdown projection of this term ships at `{okf}`."),
        );
    }
}

/// The offline SPARQL playground page.
/// The bundle explorer page: browser `gmeow info`/`describe` over the object-level
/// core bundle. The controller (`gmeow-docs.js`) loads the FULL `gmeow.gts` bundle
/// via the shared loader (`Dataset.fromGts`, byte-verified against the manifest),
/// shows the bundle's `info` summary on boot, and runs a client-side `DESCRIBE` for
/// the entered term IRI — the same describe the native `gmeow describe` produces,
/// proven byte-identical by the F2 witness lane.
fn md_bundle_explorer(model: &DocsModel, _exec: &ExecutableDocsData) -> String {
    let mut out = String::new();
    heading(&mut out, 1, "Bundle explorer");
    line(
        &mut out,
        "Explore the shipped ontology **entirely in your browser** — no server, no \
         network. This loads the full `gmeow.gts` bundle — every named graph and the \
         RDF 1.2 statement layer — and answers `info` (a summary of the loaded \
         dataset) and `describe <iri>` (every triple mentioning a term) via the \
         repository's own query engine compiled to WebAssembly — the same answers \
         the `gmeow` CLI gives.",
    );
    // Raw HTML passes through the Markdown → HTML step; the controller script is
    // injected per page by the HTML shell (gated on `has_bundle()`).
    out.push_str(
        "<div id=\"gmeow-explorer\" class=\"gmeow-explorer\">\n\
         <p id=\"gmeow-explorer-info\" class=\"gmeow-explorer-info\">Loading the bundle…</p>\n\
         <form id=\"gmeow-explorer-form\">\n\
         <label for=\"gmeow-explorer-iri\">Describe a term (IRI or CURIE)</label>\n\
         <input id=\"gmeow-explorer-iri\" type=\"text\" spellcheck=\"false\" \
         placeholder=\"https://blackcatinformatics.ca/gmeow/Cat\">\n\
         <button type=\"submit\">Describe</button>\n\
         </form>\n\
         <div id=\"gmeow-explorer-results\" class=\"gmeow-explorer-results\"></div>\n\
         </div>\n",
    );
    // ── Live entailment panel (W4b) ─────────────────────────────────────────
    heading(&mut out, 2, "Live entailment");
    line(
        &mut out,
        "Paste RDF (Turtle) and run the **native GMEOW structured-DL reasoner** over \
         it in your browser — the SAME chase the on-gate authority runs (serial on \
         wasm, byte-identical to the parallel native path). The panel shows the \
         inference diff: the entailed triples the reasoner derived.",
    );
    out.push_str(
        "<div id=\"gmeow-reason\" class=\"gmeow-reason\">\n\
         <form id=\"gmeow-reason-form\">\n\
         <label for=\"gmeow-reason-input\">RDF (Turtle)</label>\n\
         <textarea id=\"gmeow-reason-input\" rows=\"6\" spellcheck=\"false\">\
@prefix rdfs: &lt;http://www.w3.org/2000/01/rdf-schema#&gt; .\n\
@prefix rdf: &lt;http://www.w3.org/1999/02/22-rdf-syntax-ns#&gt; .\n\
@prefix ex: &lt;https://example.org/&gt; .\n\
ex:Cat rdfs:subClassOf ex:Animal .\n\
ex:felix rdf:type ex:Cat .</textarea>\n\
         <button type=\"submit\">Reason</button>\n\
         </form>\n\
         <div id=\"gmeow-reason-results\" class=\"gmeow-reason-results\"></div>\n\
         </div>\n",
    );
    // ── GMN transcode widget (W4c) ──────────────────────────────────────────
    heading(&mut out, 2, "GMN transcode");
    line(
        &mut out,
        "Transcode authored RDF into the token-compact **GMN-1** surface — and back — \
         in your browser, using the SAME codec + glyph symbology the on-gate authority \
         ships. GMN-1 is a source code over the LLM token channel: reference-position \
         terms resolve through the shipped `lang:` codebook, so a codebook-covered \
         surface reads back to the identical RDF. The panel shows the round-trip: your \
         RDF → GMN-1 → the canonical N-Quads it reads back to. Hover a glyph in the \
         legend to see its real token cost.",
    );
    out.push_str(
        "<div id=\"gmeow-gmn\" class=\"gmeow-gmn\">\n\
         <form id=\"gmeow-gmn-form\">\n\
         <label for=\"gmeow-gmn-input\">RDF (Turtle)</label>\n\
         <textarea id=\"gmeow-gmn-input\" rows=\"6\" spellcheck=\"false\">\
@prefix gmeow: &lt;https://blackcatinformatics.ca/gmeow/&gt; .\n\
gmeow:gate1 gmeow:hasState gmeow:doorGate1 .\n\
gmeow:gate1 gmeow:statusLabel &quot;open&quot; .</textarea>\n\
         <button type=\"submit\">Transcode</button>\n\
         </form>\n\
         <div id=\"gmeow-gmn-legend\" class=\"gmeow-gmn-legend\"></div>\n\
         <div id=\"gmeow-gmn-results\" class=\"gmeow-gmn-results\"></div>\n\
         </div>\n",
    );
    let _ = model;
    out
}

/// The conjecture playground page (the WASM-interactive docs W4 deliverable): the browser SYMMETRIC conjecture /
/// anti-conjecture engine. The controller (`gmeow-docs.js`) fetches + byte-verifies the
/// curated demo library, loads the core bundle as the KB, and — on submit — runs the
/// vendored wasm `conjecture` export (the SAME native `logic:` reasoner, proven
/// byte-identical by the W4 conjecture witness lane), then renders BOTH legs of the test:
/// the proof leg (`KB ⊨ φ`), the counterproof leg (`KB ∪ {φ} ⊨ ⊥`) with its contradiction
/// witness, and the Belnap classification.
fn md_conjecture_playground(model: &DocsModel, exec: &ExecutableDocsData) -> String {
    let mut out = String::new();
    heading(&mut out, 1, "Conjecture playground");
    line(
        &mut out,
        "Test a candidate `logic:` formula against a knowledge base with the **native \
         GMEOW symmetric conjecture engine** — entirely in your browser, no server, no \
         network. The engine runs TWO independent legs: a **proof** leg (does the KB \
         *entail* the formula? `KB ⊨ φ`) and a **counterproof** leg (does asserting the \
         formula make the KB *inconsistent*? `KB ∪ {φ} ⊨ ⊥`, yielding a concrete \
         contradiction witness). The Belnap classification of the two legs decides the \
         conjecture's epistemic lifecycle: corroborated, refuted-in-standpoint, or open. \
         This is the SAME chase the on-gate authority runs (serial on wasm, byte-identical \
         to native — proven by the conjecture witness lane).",
    );
    // The interactive form (raw HTML passes through the Markdown → HTML step). The
    // controller script is injected per page by the HTML shell (gated on
    // `has_conjectures()`); it populates the selector from the curated demo library and
    // renders the symmetric verdict.
    out.push_str(
        "<div id=\"gmeow-conjecture\" class=\"gmeow-conjecture\">\n\
         <p id=\"gmeow-conjecture-status\" class=\"gmeow-conjecture-status\">Loading the \
         conjecture engine…</p>\n\
         <form id=\"gmeow-conjecture-form\">\n\
         <label for=\"gmeow-conjecture-select\">Curated conjecture demo</label>\n\
         <select id=\"gmeow-conjecture-select\"></select>\n\
         <button type=\"submit\">Test</button>\n\
         </form>\n\
         <div id=\"gmeow-conjecture-results\" class=\"gmeow-conjecture-results\"></div>\n\
         </div>\n",
    );
    // The curated conjecture demo library, shipped verbatim as a site sub-asset and
    // rendered here for reference: six `logic:Conjecture`s exercising every branch of the
    // Belnap-to-lifecycle projection (open, corroborated, refuted-in-standpoint with a
    // contradiction witness + symmetric anti-conjecture leg, a Lakatos refinement
    // successor, and a phi-entails-psi propagation pair). The controller fetches this exact
    // asset (`assets/conjectures.ttl`) and byte-verifies it against the bundle manifest.
    heading(&mut out, 2, "The curated conjecture library");
    line(
        &mut out,
        "The playground ships this curated `logic:Conjecture` corpus as an integrity-pinned \
         site asset. Each conjecture carries its formula, its reified standpoint, its \
         engine-produced verdict lifecycle, and — for a refutation — a concrete \
         `logic:ContradictionWitness` and the symmetric anti-conjecture \
         `logic:NonEntailmentObligation` it proposes.",
    );
    let library = String::from_utf8_lossy(&exec.conjectures_ttl);
    fenced(&mut out, "turtle", library.trim_end());
    let _ = model;
    out
}

fn md_playground(model: &DocsModel, exec: &ExecutableDocsData) -> String {
    let mut out = String::new();
    heading(&mut out, 1, "SPARQL playground");
    line(
        &mut out,
        "Query the bundled ontology and its documentation **entirely in your browser** — \
         no server, no network. The query runs against a self-contained RDF asset via the \
         native `purrdf` engine compiled to WebAssembly.",
    );
    // The interactive form (raw HTML passes through the Markdown → HTML step). The
    // controller script is injected per page by the HTML shell, not embedded here.
    out.push_str(
        "<form id=\"gmeow-sparql\" class=\"gmeow-sparql\">\n\
         <label for=\"gmeow-sparql-query\">SPARQL query</label>\n\
         <textarea id=\"gmeow-sparql-query\" rows=\"8\" spellcheck=\"false\">\
         SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 20</textarea>\n\
         <div class=\"gmeow-sparql-controls\">\n\
         <button type=\"submit\">Run</button>\n\
         <span id=\"gmeow-sparql-status\" role=\"status\"></span>\n\
         </div>\n\
         </form>\n\
         <div id=\"gmeow-sparql-results\"></div>\n\n",
    );
    line(
        &mut out,
        "SELECT and ASK return a results table; CONSTRUCT and DESCRIBE return a graph you \
         can copy in any RDF serialization. A `SERVICE` or `LOAD` clause fails offline — \
         there is no remote endpoint to reach.",
    );

    // Surface any reasoner inferences that could not be attributed to a single worked
    // example (shared / Skolem witnesses) — never silently dropped.
    if !exec.cross_example.is_empty() {
        heading(&mut out, 2, "Cross-example inferences");
        line(
            &mut out,
            "The reasoner derived these triples from the union of all worked examples; they \
             are not attributable to any single example:",
        );
        fenced(&mut out, "turtle", &exec.cross_example.join("\n"));
    }

    // Explain a chase-invented existential null (Skolem witness). The reason stage
    // projects each invented null into `graph/reasoning` as a `gmeow:InventedWitness`
    // typed node plus a standard-RDF-reification head quad, so the playground can
    // decompose it — with NO new vocabulary — back into the rule that fired, its
    // existential ordinal, and the frontier binding that satisfied the antecedent.
    heading(&mut out, 2, "Explain a chase-invented witness");
    line(
        &mut out,
        "When the reasoner satisfies an existential obligation it *invents* a fresh null — a \
         Skolem witness with a content-addressed IRI. That witness ships in the queryable asset \
         alongside the closure that references it, so you can decompose any null into its firing \
         rule, existential ordinal, and frontier binding entirely in your browser:",
    );
    let witness_query = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n\
         PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>\n\
         SELECT ?witness ?rule ?ordinal ?frontier WHERE {\n\
         \x20 ?witness a gmeow:InventedWitness ;\n\
         \x20          gmeow:existentialOrdinal ?ordinal .\n\
         \x20 ?r rdf:object ?witness ;\n\
         \x20    rdf:subject ?frontier ;\n\
         \x20    gmeow:viaRule ?rule .\n\
         }";
    let root = root_href(&Page::SparqlPlayground.dir());
    let encoded = url_query_encode(witness_query);
    line(
        &mut out,
        &format!(
            "[Explain a chase-invented witness in the SPARQL playground]\
             ({root}sparql/index.html?q={encoded}) (runs offline, no server)."
        ),
    );
    fenced(&mut out, "sparql", witness_query);
    line(
        &mut out,
        "To decompose one specific null, pin it by its Skolem IRI: add \
         `FILTER(?witness = <skolem-iri>)` to the query above, or run `DESCRIBE <skolem-iri>` \
         to dump every triple that mentions it.",
    );

    let _ = model;
    out
}

/// Percent-encode a string for use in a URL query value (RFC 3986 unreserved kept).
fn url_query_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                purrdf_hash::hex::encode_upper_into(&[b], &mut out);
            }
        }
    }
    out
}

/// Link the interactive surfaces from the landing page, for the ones this render carries.
///
/// The explorer and the conjecture playground were RENDERED and unreachable: only the SPARQL
/// playground was ever linked, and only from slice and term pages. A page nothing navigates to
/// is a page nobody opens, and the shipped site is the deliverable — so the nav is derived from
/// the same `exec` predicates that decide whether the page is emitted at all, rather than from a
/// hand-kept list that can fall out of step with what was built.
fn append_landing_interactive_nav(out: &mut String, model: &DocsModel, exec: &ExecutableDocsData) {
    let surfaces: Vec<(String, &str, &str)> = [(
        Page::SparqlPlayground.dir(),
        "SPARQL playground",
        "query the shipped bundle in your browser, offline.",
    )]
    .into_iter()
    .filter(|_| exec.has_playground())
    .chain(
        [(
            Page::BundleExplorer.dir(),
            "Bundle explorer",
            "read any term's full description straight out of `gmeow.gts`.",
        )]
        .into_iter()
        .filter(|_| exec.has_bundle()),
    )
    .chain(
        [(
            Page::ConjecturePlayground.dir(),
            "Conjecture playground",
            "put a conjecture to the reasoner and read the verdict.",
        )]
        .into_iter()
        .filter(|_| exec.has_conjectures()),
    )
    .chain(
        [(
            crate::console::CONSOLE_PREFIX
                .trim_end_matches('/')
                .to_string(),
            "Standalone console",
            "the same engine in its own shell, installable and offline-first.",
        )]
        .into_iter()
        .filter(|_| exec.has_bundle()),
    )
    .collect();
    if surfaces.is_empty() {
        return;
    }
    let from = Page::Landing.dir();
    heading(out, 2, model.ui("body_interactive"));
    for (dir, title, blurb) in surfaces {
        push_line(
            out,
            &format!("- [{title}]({}index.md) — {blurb}", rel(&from, &dir)),
        );
    }
    blank(out);
}

// ── Synthesized quickstart (a pure function of domain/range — no new model
// state) ─────────────────────────────────────────────────────────────────────

/// The maximum number of properties rendered as explicit predicate lines in a
/// synthesized class skeleton before the remainder is summarized as a single
/// `+N more` comment line — keeps a richly-propertied class's skeleton
/// readable instead of dumping its entire reverse-domain set.
const QUICKSTART_PROPERTY_CAP: usize = 8;

/// Well-known external namespace prefixes recognized ONLY for the compact,
/// human-readable comment annotations in a synthesized quickstart skeleton —
/// never used to resolve a link (`term_link`/`to_curie` own that).
const QUICKSTART_WELL_KNOWN_NS: &[(&str, &str)] = &[
    ("http://www.w3.org/2001/XMLSchema#", "xsd"),
    ("http://www.w3.org/2000/01/rdf-schema#", "rdfs"),
    ("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "rdf"),
    ("http://www.w3.org/2002/07/owl#", "owl"),
    ("http://www.w3.org/2004/02/skos/core#", "skos"),
];

/// A short, honest label for a domain/range IRI inside a synthesized
/// quickstart's placeholder comment: the resolved term's own CURIE when it
/// names a documented term, else the CURIE form for the two GMEOW/`logic:`
/// namespaces, else a well-known external prefix (`xsd:`/`rdfs:`/…), else the
/// bare local name after the final `#`/`/` — never the full IRI (keeps the
/// comment compact) and never a fabricated claim about what the IRI names.
fn quickstart_label(model: &DocsModel, iri: &str) -> String {
    if let Some(term) = model.terms.iter().find(|t| t.iri == iri) {
        return term.curie.clone();
    }
    let curie = to_curie(iri);
    if curie != iri {
        return curie;
    }
    for (ns, prefix) in QUICKSTART_WELL_KNOWN_NS {
        if let Some(local) = iri.strip_prefix(ns)
            && !local.is_empty()
        {
            return format!("{prefix}:{local}");
        }
    }
    iri.rsplit(['#', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or(iri)
        .to_string()
}

/// A placeholder object for one property's example triple, plus the trailing
/// `#`-comment naming what the placeholder stands in for. Never fabricates an
/// instance: an empty `range` renders a bare placeholder literal (nothing is
/// known about the expected type). A non-empty `range` renders an honest
/// `<...>` IRI placeholder annotated with the range's label WHEN the first
/// range entry resolves to a documented non-`Datatype` term (an object-valued
/// property) — but a placeholder LITERAL, still annotated, when it resolves to
/// a `Datatype` term or does not resolve at all (almost always an external
/// literal type such as `xsd:string`/`rdfs:Literal`, never a modeled class),
/// so the skeleton never shows an IRI placeholder next to a datatype comment.
fn quickstart_placeholder(model: &DocsModel, range: &[String]) -> (&'static str, String) {
    let Some(first) = range.first() else {
        return ("\"...\"", String::new());
    };
    let comment = format!("  # {}", quickstart_label(model, first));
    let is_object_valued = model
        .terms
        .iter()
        .find(|t| &t.iri == first)
        .is_some_and(|t| t.category != DocTermCategory::Datatype);
    if is_object_valued {
        ("<...>", comment)
    } else {
        ("\"...\"", comment)
    }
}

/// The synthesized, copy-paste Turtle skeleton for a CLASS term: `<subject> a
/// <this class>`, followed by one predicate line per property term whose
/// `domain` names this class (a direct reverse-domain lookup over
/// `model.terms` — no subclass-inheritance walk, so the skeleton only shows
/// what THIS class itself asserts). A class with no such properties still
/// renders the honest one-triple skeleton (never an empty block).
fn synthesize_class_quickstart(model: &DocsModel, term: &DocTerm) -> String {
    let mut props: Vec<&DocTerm> = model
        .terms
        .iter()
        .filter(|p| {
            p.category == DocTermCategory::Property && p.domain.iter().any(|d| d == &term.iri)
        })
        .collect();
    props.sort_by(|a, b| a.curie.cmp(&b.curie));
    let class = purrdf::emit_term(&purrdf::RdfTerm::iri(&term.iri));

    if props.is_empty() {
        return format!("<subject> a {class} .\n");
    }

    let cap = props.len().min(QUICKSTART_PROPERTY_CAP);
    let mut out = format!("<subject> a {class} ;\n");
    for (i, prop) in props.iter().take(cap).enumerate() {
        let terminator = if i + 1 == cap { '.' } else { ';' };
        let (object, comment) = quickstart_placeholder(model, &prop.range);
        out.push_str(&format!(
            "    {} {object} {terminator}{comment}\n",
            purrdf::emit_term(&purrdf::RdfTerm::iri(&prop.iri))
        ));
    }
    if props.len() > QUICKSTART_PROPERTY_CAP {
        let remaining = props.len() - QUICKSTART_PROPERTY_CAP;
        let noun = if remaining == 1 {
            "property"
        } else {
            "properties"
        };
        out.push_str(&format!(
            "# … +{remaining} more {noun} carried by this class (see the class page for the full list)\n"
        ));
    }
    out
}

/// The synthesized, copy-paste Turtle skeleton for a PROPERTY term:
/// `<subject> a <first domain class, or owl:Thing when the property declares
/// none>`, then one triple applying the property itself, with an honest
/// placeholder object (an IRI placeholder annotated with the first `range`
/// term when one is declared, else a placeholder literal).
fn synthesize_property_quickstart(model: &DocsModel, term: &DocTerm) -> String {
    let subject_type = match term.domain.first() {
        Some(d) => purrdf::emit_term(&purrdf::RdfTerm::iri(d)),
        None => "owl:Thing".to_string(),
    };
    let (object, comment) = quickstart_placeholder(model, &term.range);
    format!(
        "<subject> a {subject_type} ;\n    {} {object} .{comment}\n",
        purrdf::emit_term(&purrdf::RdfTerm::iri(&term.iri))
    )
}

/// The synthesized quickstart Turtle skeleton for one term — a PURE function
/// of the already-discovered `domain`/`range` shape (no new model state, no
/// SHACL-constraint fabrication): a class gets an example instantiation of
/// every property it is the domain of; a property gets a one-triple example
/// application; any other category (individual, datatype, or the catch-all
/// `Other`) carries no domain/range shape to synthesize from, so it gets an
/// honest one-line comment rather than a fabricated triple.
fn synthesize_quickstart(model: &DocsModel, term: &DocTerm) -> String {
    match term.category {
        DocTermCategory::Class => synthesize_class_quickstart(model, term),
        DocTermCategory::Property => synthesize_property_quickstart(model, term),
        DocTermCategory::Individual | DocTermCategory::Datatype | DocTermCategory::Other => {
            format!(
                "# No example skeleton could be synthesized for `{}` — {} terms carry no \
                 domain/range shape to derive a triple pattern from.\n",
                term.curie,
                category_singular(term.category)
            )
        }
    }
}

/// Resolve a term by its CURIE (`gmeow:Foo` / `logic:Bar`) — the same lookup
/// [`curie_link`] performs when it renders a cross-reference. `None` for a
/// CURIE that does not name any documented term.
fn find_term_by_curie<'a>(model: &'a DocsModel, curie: &str) -> Option<&'a DocTerm> {
    model.terms.iter().find(|t| t.curie == curie)
}

/// The composed, copy-paste-runnable quickstart Turtle block for a term SET
/// (a recipe's or learning path's `term_curies`) — the concatenation of each
/// resolved member term's own [`synthesize_quickstart`] skeleton, in the
/// set's own (already sorted/deduped, per `DocRecipe`/`DocLearningPath`)
/// CURIE order, so promoted recipes are copy-paste-runnable as a whole. A
/// `term_curie` that fails to resolve to a documented term is surfaced as a
/// visible `# UNRESOLVED` comment rather than silently dropped or a panic —
/// `term_curies` is expected to already be validated upstream, so a genuine
/// miss here is a modeling bug worth surfacing on the page, not hiding.
fn synthesize_composed_quickstart(model: &DocsModel, term_curies: &[String]) -> String {
    let mut out = String::new();
    for curie in term_curies {
        match find_term_by_curie(model, curie) {
            Some(term) => {
                out.push_str(&format!("# {}\n", term.curie));
                out.push_str(&synthesize_quickstart(model, term));
                out.push('\n');
            }
            None => {
                out.push_str(&format!(
                    "# UNRESOLVED term_curie `{curie}` — no documented term matches this CURIE.\n\n"
                ));
            }
        }
    }
    out
}

mod overview;
use overview::{md_about, md_category, md_changelog, md_getting_started, md_health, md_landing};
mod term_pages;
use term_pages::{append_cite_this_surface, md_term};
#[cfg(test)]
use term_pages::{python_str_escape, rust_raw_fence_width, rust_raw_string_literal};
mod catalog_pages;
#[cfg(test)]
use catalog_pages::finding_category_display;
use catalog_pages::{
    HeadingDemotion, append_provenance_footer, append_stage_section, box_role_label,
    md_competency_index, md_concern, md_concern_index, md_constraint_catalog, md_example_index,
    md_external_index, md_fixture_index, md_four_boxes, md_glossary, md_grammar,
    md_integrity_index, md_learning_path, md_learning_path_index, md_linkage_index,
    md_logic_canonical_ir, md_logic_derivation_graph, md_logic_diagnostics, md_logic_index,
    md_logic_loss_ledger, md_notation_index, md_pipeline_dag, md_recipe, md_recipe_index,
    md_seam_registry, md_slice, md_slice_document, md_slice_index, push_fixture_binding_bullets,
    rewrite_doc_body,
};

// ── HTML layer ────────────────────────────────────────────────────────────────

/// Render a page to a complete, self-contained HTML document (English chrome).
///
/// Equivalent to [`to_html_lang`] with the English carrier language.
///
/// # Errors
/// Refuses ambiguous source pages, dangling internal links, heading overflow or failure of a selected syntax provider.
pub fn to_html(model: &DocsModel, page: &Page) -> Result<String> {
    to_html_lang(model, page, ENGLISH)
}

/// Lazily-initialized minijinja [`Environment`] with the [`SHELL`] template
/// compiled once for the lifetime of the process. Avoids rebuilding and
/// recompiling the template on every [`to_html_lang`] call.
static SHELL_ENV: OnceLock<Environment<'static>> = OnceLock::new();

fn shell_env() -> &'static Environment<'static> {
    SHELL_ENV.get_or_init(|| {
        let mut env = Environment::new();
        env.add_template("shell", SHELL)
            .expect("embedded shell template is valid");
        env
    })
}

/// Render a page to a complete, self-contained HTML document: the page's
/// Markdown body converted to HTML and injected into the minijinja shell, with
/// the UI-chrome nav labels resolved for `lang` (English fallback). The `model`
/// passed in is already localized by [`render_site_lang`], so the body content
/// is in the target language.
///
/// # Errors
/// Refuses ambiguous source pages, dangling internal links, heading overflow or failure of a selected syntax provider.
pub fn to_html_lang(model: &DocsModel, page: &Page, lang: &str) -> Result<String> {
    to_html_lang_exec(model, page, lang, &ExecutableDocsData::default())
}

/// Render a page to a complete HTML document with the executable-docs data: appends
/// the reasoner/export sections to the body, a SPARQL nav entry, and the playground
/// controller script — all only when `exec` supplies data (byte-identical to
/// [`to_html_lang`] otherwise).
///
/// # Errors
/// Refuses ambiguous source pages, dangling internal links, heading overflow or failure of a selected syntax provider.
pub fn to_html_lang_exec(
    model: &DocsModel,
    page: &Page,
    lang: &str,
    exec: &ExecutableDocsData,
) -> Result<String> {
    // Standalone render builds the map here; a full site render threads one shared map
    // through [`to_html_lang_exec_with_map`] (see [`to_markdown_exec`]).
    let page_map = source_page_map(model)?;
    to_html_lang_exec_with_map(model, page, lang, exec, &page_map)
}

/// [`to_html_lang_exec`] with the shared [`SourceToPageMap`] threaded in. Byte-identical
/// to `to_html_lang_exec` (the map is a deterministic function of the model).
pub(crate) fn to_html_lang_exec_with_map(
    model: &DocsModel,
    page: &Page,
    lang: &str,
    exec: &ExecutableDocsData,
    page_map: &SourceToPageMap,
) -> Result<String> {
    let body_html = rewrite_internal_links(&markdown_to_html(&to_markdown_exec_with_map(
        model, page, exec, page_map,
    )?));
    let root = root_href(&page.dir());

    let ui = &model.ui_catalog;
    // Resolve a nav label: for the English carrier keep the exact historical
    // sentence-case string (so the English golden is byte-stable); for any other
    // language use the per-language UI-chrome override when present, else the
    // historical English string (the table's Title-Case default is only a POT
    // template surface, never the live English nav).
    let label = |key: &str, english: &str| -> String {
        if lang == ENGLISH {
            return english.to_string();
        }
        let resolved = i18n::ui_string(key, lang, ui);
        // `ui_string` returns the English *default* when no override exists; that
        // default may differ in casing from the live nav, so prefer the caller's
        // historical English when the catalog had no real override.
        if resolved == i18n::ui_default(key) {
            english.to_string()
        } else {
            resolved.to_string()
        }
    };

    // Nav items are a fixed, pre-sorted Vec (never a map) for determinism. Labels
    // resolve through the UI-chrome table (English fallback).
    let mut nav = vec![
        nav_item(&root, &Page::Landing.dir(), &label("nav_home", "Home")),
        nav_item(
            &root,
            &Page::SliceIndex.dir(),
            &label("nav_slices", "Slices"),
        ),
        nav_item(
            &root,
            &Page::Category(DocTermCategory::Class).dir(),
            &label("category_class", "Classes"),
        ),
        nav_item(
            &root,
            &Page::Category(DocTermCategory::Property).dir(),
            &label("category_property", "Properties"),
        ),
        nav_item(
            &root,
            &Page::ConcernIndex.dir(),
            &label("nav_concerns", "Concerns"),
        ),
        nav_item(
            &root,
            &Page::LinkageIndex.dir(),
            &label("nav_linkages", "Linkages"),
        ),
        nav_item(
            &root,
            &Page::Logic.dir(),
            &label("nav_logic", "Logic & Reasoning"),
        ),
        nav_item(
            &root,
            &Page::ExampleIndex.dir(),
            &label("nav_examples", "Examples"),
        ),
        nav_item(
            &root,
            &Page::ExternalIndex.dir(),
            &label("nav_external", "External"),
        ),
        nav_item(
            &root,
            &Page::RecipeIndex.dir(),
            &label("nav_recipes", "Recipes"),
        ),
        nav_item(
            &root,
            &Page::LearningPathIndex.dir(),
            &label("nav_learning_paths", "Learning paths"),
        ),
        nav_item(
            &root,
            &Page::GettingStarted.dir(),
            &label("nav_getting_started", "Getting started"),
        ),
        nav_item(&root, &Page::About.dir(), &label("page_about", "About")),
    ];
    // The offline SPARQL playground joins the nav only when the pipeline shipped its
    // bundled query asset (never in a model-only render, so the goldens are stable).
    if exec.has_playground() {
        nav.push(nav_item(
            &root,
            &Page::SparqlPlayground.dir(),
            &label("nav_sparql", "SPARQL"),
        ));
    }

    let page_lang = if lang == ENGLISH { "en" } else { lang };

    // The playground page loads the controller module (query execution + result
    // transcoding). Empty for every other page and every model-only render, so the
    // shell's `body_scripts` slot is byte-neutral there.
    // ANY page whose body carries a control the controller drives loads it — not a
    // hand-listed three. Term pages render `.gmeow-run-validation` controls, so the
    // enumeration shipped a validation button on every term page with nothing bound to
    // it, while the mdbook shell's boot shim imported the controller globally and wired
    // the identical markup. Two shells, same controls, different behaviour. Derived from
    // CONTROLLER_HOOKS, whose agreement with the shipped module is itself gated, so a new
    // widget cannot render a dead control by forgetting to extend a list here.
    let body_scripts = if body_carries_interactive_control(&body_html)
        || (matches!(page, Page::SparqlPlayground) && exec.has_playground())
        || (matches!(page, Page::BundleExplorer) && exec.has_bundle())
        || (matches!(page, Page::ConjecturePlayground) && exec.has_conjectures())
    {
        format!("<script type=\"module\" src=\"{root}{DOCS_JS_PATH}\"></script>\n")
    } else {
        String::new()
    };

    let tmpl = shell_env()
        .get_template("shell")
        .expect("shell template registered");
    tmpl.render(TemplateValue::from_iter([
        ("site_title", TemplateValue::from(&model.title)),
        ("site_version", TemplateValue::from(&model.version)),
        ("page_title", TemplateValue::from(page.title(model))),
        ("css_href", TemplateValue::from(format!("{root}{CSS_PATH}"))),
        ("root_href", TemplateValue::from(root)),
        ("nav", TemplateValue::from_iter(nav)),
        ("body", TemplateValue::from(body_html)),
        ("page_lang", TemplateValue::from(page_lang)),
        ("body_scripts", TemplateValue::from(body_scripts)),
    ]))
    .map_err(|cause| {
        Diag::of_kind(crate::render_error::Template::caused_by(
            format!("render HTML shell for {page:?}"),
            cause,
        ))
    })
}

/// Convert a Markdown document to an HTML fragment via pulldown-cmark with a
/// fixed, deterministic option set (tables enabled; raw HTML in the source is
/// inert because we author all Markdown and escape inserted values ourselves).
fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(markdown, options);
    let mut html = String::new();
    cmark_html::push_html(&mut html, parser);
    html
}

/// Rewrite intra-site link targets in a converted HTML body from the Markdown
/// tree's `.md` extension to `.html`, so the HTML site navigates between HTML
/// pages. Every internal link the renderers emit ends in `index.md` inside an
/// `href="…"` attribute; external links are absolute `http(s)://` URLs that
/// never contain `index.md`, so this targeted swap touches only internal links.
fn rewrite_internal_links(html: &str) -> String {
    // `index.md"` closes a fragment-less internal link; `index.md#` precedes a
    // heading fragment (the grafted `docs.md` / child-document cross-links carry
    // `…index.md#anchor`). Both forms map to the `.html` page.
    html.replace("index.md\"", "index.html\"")
        .replace("index.md#", "index.html#")
}

/// Native template map used in the navigation's fixed sequence order.
fn nav_item(root: &str, dir: &str, label: &str) -> TemplateValue {
    TemplateValue::from_iter([
        ("label", TemplateValue::from(label)),
        (
            "href",
            TemplateValue::from(format!("{root}{}", join(dir, "index.html"))),
        ),
    ])
}

// ── Localization ────────────────────────────────────────────────────────────────

const GMEOW_GUIDE_TITLE: &str = "https://blackcatinformatics.ca/gmeow/guideTitle";
const GMEOW_GUIDE_GOAL: &str = "https://blackcatinformatics.ca/gmeow/guideGoal";
const GMEOW_LEARNING_AUDIENCE: &str = "https://blackcatinformatics.ca/gmeow/learningAudience";

/// Build a copy of `model` with every localizable string replaced by its `lang`
/// translation where one exists, falling back to the English carrier value the
/// element already holds.
///
/// IRIs, CURIEs, slugs, digests, paths, and structural collections are
/// language-independent and are left untouched, so the page graph (and the
/// no-dangling-link invariant) is identical across languages — only the human
/// prose changes.
fn localize_model(model: &DocsModel, lang: &str) -> DocsModel {
    let tr = &model.translations;
    let mut out = model.clone();

    // Translate a value for (iri, predicate); definition tries skos:definition
    // then rdfs:comment, mirroring the model's English fallback chain.
    let tr_one = |iri: &str, predicate: &str| -> Option<String> {
        tr.lookup(iri, predicate, lang).map(str::to_string)
    };
    let tr_def = |iri: &str| -> Option<String> {
        tr_one(iri, SKOS_DEFINITION).or_else(|| tr_one(iri, RDFS_COMMENT))
    };

    for term in &mut out.terms {
        // Stash the authored ENGLISH label/definition before overwriting the display
        // fields with translations. Documentation-COMPLETENESS is a property of the
        // canonical source, not the render language (see `DocTerm::coverage_label`):
        // the completeness badge and its scored dimensions must be identical across
        // languages, so coverage keeps reading the English text via these fields even
        // as the page prose is localized. Unconditional so the canonical text is
        // preserved even when only one of the two carriers has a translation.
        term.canonical_label = term.label.clone();
        term.canonical_definition = term.definition.clone();
        if let Some(v) = tr_one(&term.iri, RDFS_LABEL) {
            term.label = Some(v);
        }
        if let Some(v) = tr_def(&term.iri) {
            term.definition = Some(v);
        }
    }

    for slice in &mut out.slices {
        if let Some(v) = tr_one(&slice.iri, RDFS_LABEL) {
            slice.label = Some(v);
        }
        // Slice display prefers `title`; translate it from dcterms:title, else
        // promote a translated label / skos:definition is not a title source.
        if let Some(v) = tr_one(&slice.iri, DCTERMS_TITLE) {
            slice.title = Some(v);
        }
    }

    for concern in &mut out.concerns {
        if let Some(v) = tr_one(&concern.iri, RDFS_LABEL) {
            concern.label = Some(v);
        }
        if let Some(v) = tr_def(&concern.iri) {
            concern.definition = Some(v);
        }
    }

    // Recipes / learning paths are keyed by slug; their localizable prose lives
    // on the guide individual's IRI. The model does not retain that IRI, but the
    // guide's GMEOW IRI is `gmeow:<localname>` derived from the slug is not
    // reliable, so we look up by the canonical guide predicates on the term IRI
    // reconstructed from the slug only when a translation is actually present.
    // (No guide translations exist yet — this is a forward-compatible hook that
    // is a no-op until a catalog provides them.)
    let guide_iri = |slug: &str| format!("{GMEOW_NS}{slug}");
    for recipe in &mut out.recipes {
        let iri = guide_iri(&recipe.slug);
        if let Some(v) = tr_one(&iri, GMEOW_GUIDE_TITLE).or_else(|| tr_one(&iri, RDFS_LABEL)) {
            recipe.title = v;
        }
        if let Some(v) = tr_one(&iri, GMEOW_GUIDE_GOAL) {
            recipe.goal = v;
        }
    }
    for path in &mut out.learning_paths {
        let iri = guide_iri(&path.slug);
        if let Some(v) = tr_one(&iri, GMEOW_GUIDE_TITLE).or_else(|| tr_one(&iri, RDFS_LABEL)) {
            path.title = v;
        }
        if let Some(v) = tr_one(&iri, GMEOW_GUIDE_GOAL) {
            path.goal = v;
        }
        if let Some(v) = tr_one(&iri, GMEOW_LEARNING_AUDIENCE) {
            path.audience = v;
        }
    }

    // Record the target language so the body renderers resolve UI-chrome strings
    // through the override catalog (English fallback). The English carrier keeps
    // its default empty `lang`, which also resolves to the English defaults.
    out.lang = lang.to_string();

    out
}

// ── Slugging ──────────────────────────────────────────────────────────────────

/// The INJECTIVE documentation-entry slug of a term — the single source of the
/// `documentation/term/{slug}` doc-entry IRI, the page URL, and every cross-page
/// link. Returns the term's resolved [`DocTerm::slug`] (assigned once from the
/// whole term set by [`resolve_term_slugs`] at model build), so the doc-entry
/// subject is collision-free and its coverage incidence can never be conflated.
///
/// A hand-built term (a unit-test fixture that never went through model
/// resolution) carries an empty `slug`; it then falls back to the base slug —
/// safe because such tiny models never collide, and the real model always carries
/// a resolved slug, so this is one function with one answer, never two that can
/// disagree.
pub fn term_slug(term: &DocTerm) -> String {
    if term.slug.is_empty() {
        slugify(local_name(&term.iri))
    } else {
        term.slug.clone()
    }
}

/// The category discriminator segment appended to a contended base slug.
fn category_slug(category: DocTermCategory) -> &'static str {
    match category {
        DocTermCategory::Class => "class",
        DocTermCategory::Property => "property",
        DocTermCategory::Individual => "individual",
        DocTermCategory::Datatype => "datatype",
        DocTermCategory::Other => "other",
    }
}

/// Resolve the disambiguated `documentation/term/{slug}` slug for every term whose
/// base slug COLLIDES — a deterministic pure function of the term set, keyed by
/// term IRI. Terms whose base slug is already unique are ABSENT from the map (they
/// keep the base slug via [`term_slug`]'s fallback), so the returned entries are
/// exactly the colliders — the minority that must change.
///
/// # Scheme (minimal churn, no blank nodes)
///
/// 1. **Base slug** = [`slugify`] of the IRI's local name (the historical slug).
///    A base slug carried by exactly ONE term is kept verbatim — the non-colliding
///    terms' IRIs / URLs / links are unchanged (and they are not in the map).
/// 2. **Category disambiguation** — a base slug shared by ≥2 distinct terms (the
///    `slugify` case/punctuation fold is lossy, e.g. class `AcceptanceStatus` and
///    property `acceptanceStatus` both fold to `acceptancestatus`) gets its
///    category appended (`-class` / `-property` / `-individual` / `-datatype` /
///    `-other`).
/// 3. **Digest tiebreak** — a residual collision (same base AND category, or a
///    disambiguated slug that would clash with a reserved base) appends
///    [`short_iri_digest`] of the full IRI; a further clash appends an incrementing
///    suffix. The full slug set (unique bases ∪ resolved) is asserted injective — a
///    HARD FAIL otherwise, never silent conflation.
///
/// Contended terms are processed in IRI-sorted order, so the assignment is a total
/// function of the (unordered) term set: the same terms always yield the same map.
pub fn resolve_term_slugs(terms: &[DocTerm]) -> BTreeMap<String, String> {
    use std::collections::{HashMap, HashSet};

    // Distinct terms by IRI (first occurrence in IRI-sorted order). A term IRI that
    // appears more than once in the list (e.g. lifted by two scans) is ONE doc-entry
    // subject, so it resolves to ONE slug — the injectivity target is distinct IRIs,
    // not list positions.
    let mut order: Vec<&DocTerm> = terms.iter().collect();
    order.sort_by(|a, b| a.iri.cmp(&b.iri));
    let mut seen: HashSet<&str> = HashSet::new();
    let distinct: Vec<&DocTerm> = order
        .into_iter()
        .filter(|t| seen.insert(t.iri.as_str()))
        .collect();

    // Base slug per distinct term IRI + how many distinct terms share each base.
    let base_of: HashMap<&str, String> = distinct
        .iter()
        .map(|t| (t.iri.as_str(), slugify(local_name(&t.iri))))
        .collect();
    let mut base_count: HashMap<&str, usize> = HashMap::new();
    for base in base_of.values() {
        *base_count.entry(base.as_str()).or_default() += 1;
    }

    // Every uncontended base is reserved (kept verbatim, absent from the map).
    let mut used: HashSet<String> = HashSet::new();
    for term in &distinct {
        let base = &base_of[term.iri.as_str()];
        if base_count[base.as_str()] == 1 {
            used.insert(base.clone());
        }
    }

    // Disambiguate the contended terms (already in IRI-sorted order → determinism).
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for term in &distinct {
        let base = &base_of[term.iri.as_str()];
        if base_count[base.as_str()] == 1 {
            continue;
        }
        let cat = category_slug(term.category);
        let mut cand = format!("{base}-{cat}");
        if used.contains(&cand) {
            cand = format!("{base}-{cat}-{}", short_iri_digest(&term.iri));
        }
        let mut n = 2;
        while used.contains(&cand) {
            cand = format!("{base}-{cat}-{}-{n}", short_iri_digest(&term.iri));
            n += 1;
        }
        used.insert(cand.clone());
        out.insert(term.iri.clone(), cand);
    }

    // Injectivity is the whole point: distinct IRIs → distinct slugs across the
    // WHOLE surface (unique bases ∪ resolved). `used` grew by exactly one per
    // reserved base and per resolved slug, so its size must equal the distinct-IRI
    // count — a HARD FAIL otherwise, never silent conflation.
    assert_eq!(
        used.len(),
        distinct.len(),
        "resolve_term_slugs produced a non-injective slug surface"
    );
    out
}

/// A filesystem-safe slug from a slice IRI's last path segment.
pub fn slice_slug(slice: &DocSlice) -> String {
    slice_slug_of_iri(&slice.iri)
}

/// The slice slug derived directly from a slice IRI — the same slug
/// [`slice_slug`] yields, without needing a materialized [`DocSlice`]. Used by
/// [`crate::model::DocMarkdownDocument`] collection during model build, before the
/// owning `DocSlice` is fully assembled.
pub fn slice_slug_of_iri(iri: &str) -> String {
    slugify(local_name(iri))
}

/// A filesystem-safe slug from a concern IRI's last path segment.
pub fn concern_slug(concern: &DocConcern) -> String {
    slugify(local_name(&concern.iri))
}

/// The local name of an IRI: the tail after the last `/` or `#`.
fn local_name(iri: &str) -> &str {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    &iri[cut..]
}

/// Lowercase + collapse to `[a-z0-9-]`, with non-alphanumerics becoming `-`,
/// runs collapsed, and leading/trailing dashes trimmed. Empty input → `unnamed`.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

// ── Link resolution ───────────────────────────────────────────────────────────

/// A link to a term: an intra-site relative link when the IRI names a known
/// term, else a plain `code` rendering of the CURIE (GMEOW IRIs not in the
/// model) or an external Markdown link (non-GMEOW IRIs).
fn term_link(model: &DocsModel, from: &str, iri: &str) -> String {
    if let Some(term) = model.terms.iter().find(|t| t.iri == iri) {
        let href = rel(from, &Page::Term(term_slug(term)).dir());
        return format!("[`{}`]({}index.md)", code_escape(&term.curie), href);
    }
    if let Some(local) = iri.strip_prefix(GMEOW_NS) {
        // A GMEOW IRI we do not document (e.g. an external gUFO alignment target
        // referenced by a parent edge): show the CURIE without a dangling link.
        return format!("`gmeow:{}`", code_escape(local));
    }
    // A non-GMEOW IRI: a plain external link.
    format!("[{}]({})", md_escape(iri), md_escape(iri))
}

/// A link to a slice page given its IRI, or a `code` rendering when the slice is
/// not in the model.
/// Render a consumer-profile CURIE (e.g. `gmeow:ClaimsProfile`) as a doc link
/// when it expands to a documented GMEOW term, else as a plain code span.
fn consumer_link(model: &DocsModel, from: &str, curie: &str) -> String {
    if let Some(local) = curie.strip_prefix("gmeow:") {
        return term_link(model, from, &format!("{GMEOW_NS}{local}"));
    }
    format!("`{}`", code_escape(curie))
}

/// Short relation tag for an alignment predicate IRI — the local name after the
/// final `#`/`/` (`skos:closeMatch` → `closeMatch`, `owl:equivalentClass` →
/// `equivalentClass`), mirroring the SSSOM-style tags the Python projection used.
fn align_tag(predicate: &str) -> String {
    predicate
        .rsplit(['#', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or(predicate)
        .to_string()
}

/// The inline projection-loss caveat for a per-term alignment predicate, or `None`
/// for `skos:exactMatch` (a lossless equivalence).
///
/// The note DISCLOSES the approximation the SKOS mapping predicate ALREADY
/// declares — it asserts nothing new (epistemic-shape honesty, Principle 17). A
/// close / broad / narrow / related match carries the term into an external
/// vocabulary lossily, and the term page says so inline rather than letting the
/// crosswalk read as an equivalence. An unrecognized predicate (e.g. an exact
/// `owl:equivalentClass`) carries no caveat — only the declared-approximate SKOS
/// predicates do.
fn approximate_match_note(model: &DocsModel, predicate: &str) -> Option<String> {
    let key = match align_tag(predicate).as_str() {
        "closeMatch" => "body_caveat_close",
        "broadMatch" => "body_caveat_broad",
        "narrowMatch" => "body_caveat_narrow",
        "relatedMatch" => "body_caveat_related",
        _ => return None,
    };
    Some(model.ui(key).to_string())
}

/// Whether the EDOAL / FnO correspondence lowerings are declared lossy in the
/// canonical projection loss ledger. The alignment section discloses that any
/// crosswalk — even an exact SKOS/OWL match — is a lossy projection once lowered
/// to those alignment formats; sourcing the verdict from the ledger (rather than
/// hardcoding it) means an EDOAL/FnO row that ever became exact would suppress the
/// note automatically.
fn edoal_fno_lowering_is_lossy() -> bool {
    gmeow_logic_compile::projections::projection_ledger_rows()
        .iter()
        .any(|row| (row.target == "edoal" || row.target == "fno") && !row.lossy_drops.is_empty())
}

fn slice_link(model: &DocsModel, from: &str, iri: &str) -> String {
    if let Some(slice) = model.slices.iter().find(|s| s.iri == iri) {
        let href = rel(from, &Page::Slice(slice_slug(slice)).dir());
        return format!("[{}]({}index.md)", md_escape(&slice_display(slice)), href);
    }
    format!("`{}`", code_escape(iri))
}

/// A link from a term CURIE to its term page, or a plain `code` CURIE when the
/// term is not documented.
/// The compact CURIE for an IRI: a `gmeow:` / `logic:` / `math:` / `lang:` CURIE
/// for the GMEOW-family namespaces, otherwise the IRI unchanged. Used by the
/// constraint catalog to abbreviate a rule's applies-to terms and formalized
/// axiom.
fn to_curie(iri: &str) -> String {
    const FAMILY: &[(&str, &str)] = &[
        ("https://blackcatinformatics.ca/gmeow/", "gmeow"),
        ("https://blackcatinformatics.ca/logic/", "logic"),
        ("https://blackcatinformatics.ca/math/", "math"),
        ("https://blackcatinformatics.ca/lang/", "lang"),
    ];
    for (ns, prefix) in FAMILY {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{prefix}:{local}");
        }
    }
    iri.to_string()
}

fn curie_link(model: &DocsModel, from: &str, curie: &str) -> String {
    if let Some(term) = model.terms.iter().find(|t| t.curie == curie) {
        let href = rel(from, &Page::Term(term_slug(term)).dir());
        return format!("[`{}`]({}index.md)", code_escape(curie), href);
    }
    format!("`{}`", code_escape(curie))
}

/// A link from a linkage's subject (its `subject_curie`/`subject` IRI) to the
/// term page, falling back to the CURIE in a code span.
fn subject_link(model: &DocsModel, from: &str, link: &crate::model::DocLinkage) -> String {
    if let Some(term) = model.terms.iter().find(|t| t.iri == link.subject) {
        let href = rel(from, &Page::Term(term_slug(term)).dir());
        return format!("[`{}`]({}index.md)", code_escape(&link.subject_curie), href);
    }
    format!("`{}`", code_escape(&link.subject_curie))
}

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Join a directory and a leaf into a site path (`""`+`index.md` → `index.md`).
fn join(dir: &str, leaf: &str) -> String {
    if dir.is_empty() {
        leaf.to_string()
    } else {
        format!("{dir}/{leaf}")
    }
}

/// A `../`-prefixed href from page-dir `from` back to the site root.
fn root_href(from: &str) -> String {
    if from.is_empty() {
        String::new()
    } else {
        "../".repeat(from.split('/').count())
    }
}

/// A relative path from one page directory to another (both site-relative,
/// no trailing slash). The result ends with `/` (or is empty for the same dir),
/// so callers append the leaf (e.g. `index.md`).
fn rel(from: &str, to: &str) -> String {
    let from_parts: Vec<&str> = if from.is_empty() {
        Vec::new()
    } else {
        from.split('/').collect()
    };
    let to_parts: Vec<&str> = if to.is_empty() {
        Vec::new()
    } else {
        to.split('/').collect()
    };
    // Common prefix length.
    let common = from_parts
        .iter()
        .zip(to_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let ups = from_parts.len() - common;
    let mut out = "../".repeat(ups);
    for part in &to_parts[common..] {
        out.push_str(part);
        out.push('/');
    }
    out
}

// ── Display helpers ───────────────────────────────────────────────────────────

/// The display name for a slice: its title, then label, then IRI local name.
pub(crate) fn slice_display(slice: &DocSlice) -> String {
    slice
        .title
        .clone()
        .or_else(|| slice.label.clone())
        .unwrap_or_else(|| local_name(&slice.iri).to_string())
}

/// The display name for a concern: its label, else its IRI local name.
pub(crate) fn concern_display(concern: &DocConcern) -> String {
    concern
        .label
        .clone()
        .unwrap_or_else(|| local_name(&concern.iri).to_string())
}

/// The display name for a mapping set: its set-id tail / label, else local name.
fn set_display(set: &crate::model::DocMappingSet) -> String {
    local_name(&set.iri).to_string()
}

/// The display name for a slice IRI (looked up in the model), else its local name.
fn slice_name(model: &DocsModel, iri: &str) -> String {
    model
        .slices
        .iter()
        .find(|s| s.iri == iri)
        .map(slice_display)
        .unwrap_or_else(|| local_name(iri).to_string())
}

/// A table cell for an optional confidence value (one decimal, or em-dash).
fn confidence_cell(confidence: Option<f64>) -> String {
    match confidence {
        Some(c) => format!("{c:.2}"),
        None => "—".to_string(),
    }
}

/// The number of distinct external namespaces in the model.
fn namespace_count(model: &DocsModel) -> usize {
    let mut set: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for term in &model.external_terms {
        set.insert(term.namespace.as_str());
    }
    set.len()
}

/// The slice tier as a stable name.
fn tier_name(slice: &DocSlice) -> String {
    use purrdf::slice::SliceTier;
    match &slice.tier {
        Some(SliceTier::Core) => "Core".to_string(),
        Some(SliceTier::Extension) => "Extension".to_string(),
        Some(SliceTier::Domain) => "Domain".to_string(),
        Some(SliceTier::Unknown(iri)) => local_name(iri).to_string(),
        None => "—".to_string(),
    }
}

/// A stable, human role name for an artifact role.
fn role_name(role: &purrdf::slice::ArtifactRole) -> String {
    use purrdf::slice::ArtifactRole;
    match role {
        ArtifactRole::Manifest => "Manifest".to_string(),
        ArtifactRole::Module => "Module".to_string(),
        ArtifactRole::Shapes => "Shapes".to_string(),
        ArtifactRole::Mapping => "Mappings".to_string(),
        ArtifactRole::CompetencyQuery => "Competency queries".to_string(),
        ArtifactRole::VerifyQuery => "Verification queries".to_string(),
        ArtifactRole::TestDsl => "Tests".to_string(),
        ArtifactRole::Example => "Examples".to_string(),
        ArtifactRole::CounterExample => "Counter-examples".to_string(),
        ArtifactRole::Documentation => "Documentation".to_string(),
        ArtifactRole::TranslationCatalog => "Translations".to_string(),
        ArtifactRole::Citation => "Citation".to_string(),
        ArtifactRole::Other(name) => format!("Other ({name})"),
    }
}

/// The first 12 hex chars of a digest, for compact by-reference display.
fn short_digest(digest: &str) -> String {
    digest.chars().take(12).collect()
}

/// The category directory segment (`classes`, `properties`, …).
/// The fixed category order used by the category-index pages and the `llms.txt`
/// Vocabulary section (deterministic).
const CATEGORY_ORDER: [DocTermCategory; 5] = [
    DocTermCategory::Class,
    DocTermCategory::Property,
    DocTermCategory::Individual,
    DocTermCategory::Datatype,
    DocTermCategory::Other,
];

fn category_dir(category: DocTermCategory) -> &'static str {
    match category {
        DocTermCategory::Class => "classes",
        DocTermCategory::Property => "properties",
        DocTermCategory::Individual => "individuals",
        DocTermCategory::Datatype => "datatypes",
        DocTermCategory::Other => "other",
    }
}

/// The plural category title.
fn category_plural(category: DocTermCategory) -> &'static str {
    match category {
        DocTermCategory::Class => "Classes",
        DocTermCategory::Property => "Properties",
        DocTermCategory::Individual => "Individuals",
        DocTermCategory::Datatype => "Datatypes",
        DocTermCategory::Other => "Other terms",
    }
}

/// The singular category label.
fn category_singular(category: DocTermCategory) -> &'static str {
    match category {
        DocTermCategory::Class => "Class",
        DocTermCategory::Property => "Property",
        DocTermCategory::Individual => "Individual",
        DocTermCategory::Datatype => "Datatype",
        DocTermCategory::Other => "Term",
    }
}

/// A trailing `— label` suffix for a term when it has a label distinct from its
/// CURIE, else empty.
fn label_suffix(term: &DocTerm) -> String {
    match &term.label {
        Some(label) if label != &term.curie => format!(" — {}", md_escape(label)),
        _ => String::new(),
    }
}

// ── Markdown emission helpers ─────────────────────────────────────────────────

/// Push a heading at `level` followed by a blank line.
fn heading(out: &mut String, level: usize, text: &str) {
    out.push_str(&"#".repeat(level));
    out.push(' ');
    out.push_str(&md_escape(text));
    out.push_str("\n\n");
}

/// Push a paragraph (a line) followed by a blank line. The text is assumed
/// already-escaped by the caller where it contains model data.
fn line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push_str("\n\n");
}

/// Push a single line followed by a newline (for table rows / list items).
fn push_line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push('\n');
}

/// Ensure the buffer ends in a blank line.
fn blank(out: &mut String) {
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

/// Push a fenced code block. The fence length adapts so a body that itself
/// contains backtick runs cannot break out (CommonMark info-string rule). The
/// body is emitted verbatim — inside a fenced block no escaping is needed.
fn fenced(out: &mut String, lang: &str, body: &str) {
    // Find the longest run of backticks in the body and use one more.
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in body.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    let fence = "`".repeat(longest.max(2) + 1);
    out.push_str(&fence);
    out.push_str(lang);
    out.push('\n');
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&fence);
    out.push_str("\n\n");
}

/// Collapse a definition to a single line: newlines/tabs → spaces, runs
/// collapsed. Pipes are NOT touched here ([`md_escape`] handles table-cell
/// escaping at emission).
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// Escape model-derived text destined for a Markdown **code span** (between
/// backticks). Inside a code span Markdown does not interpret backslash escapes
/// or inline metacharacters, so only the backtick (which would close the span),
/// the table-cell delimiter `|`, and newlines need handling. Backticks are
/// space-neutralized rather than escaped since CURIEs/IRIs/paths never contain
/// them legitimately.
fn code_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '`' => out.push(' '),
            '|' => out.push_str("\\|"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Whether a B3 entailment's display-form conclusion (`s p o`, CURIE-compacted) is a
/// class-unsatisfiability witness — `<class> rdfs:subClassOf owl:Nothing` — the same
/// signal [`crate::model::ReasoningVerdict::unsatisfiable`] keys on, so the
/// "unsatisfiable because" derivation lines never surface an unrelated entailment.
fn is_unsatisfiable_conclusion(conclusion: &str) -> bool {
    conclusion.contains(" rdfs:subClassOf ") && conclusion.ends_with(" owl:Nothing")
}

/// Escape model-derived text for safe Markdown emission: backslash-escape the
/// inline metacharacters that would otherwise be interpreted, and replace `|`
/// (table-cell delimiter) and newlines so cell content cannot break the grid.
fn md_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '<' | '>' => {
                out.push('\\');
                out.push(ch);
            }
            '|' => out.push_str("\\|"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

// ── Static indexes (search-index.json, llms.txt / llms-full.txt) ───────────────

/// The flattened advice facet for a term — its English advisory carriers in a
/// stable field order (scope, use-when, avoid-when, how-to-use). Empty when the
/// term carries no advice. Lets search match on advisory prose, not just label.
pub(crate) fn term_advice_facet(term: &DocTerm) -> Vec<String> {
    term.scope_notes
        .iter()
        .chain(term.use_when.iter())
        .chain(term.avoid_when.iter())
        .chain(term.how_to_use.iter())
        .cloned()
        .collect()
}

/// Maps each subject IRI to its sorted+deduped `tag:object` alignment tokens.
/// Borrows the subject IRIs from the model, so it is lifetime-bound to it.
pub(crate) type AlignmentFacets<'a> = std::collections::HashMap<&'a str, Vec<String>>;

/// Precompute alignment facets for all terms in one pass: maps each subject IRI
/// to a sorted+deduped `tag:object` token list. Avoids the O(N×M) per-term
/// linear scan of `model.linkages` when rendering the search and llms surfaces.
pub(crate) fn precompute_alignment_facets(model: &DocsModel) -> AlignmentFacets<'_> {
    let mut map: std::collections::HashMap<&str, Vec<String>> = std::collections::HashMap::new();
    for l in &model.linkages {
        map.entry(l.subject.as_str()).or_default().push(format!(
            "{}:{}",
            align_tag(&l.predicate),
            local_name(&l.object)
        ));
    }
    for tags in map.values_mut() {
        tags.sort_unstable();
        tags.dedup();
    }
    map
}

/// Maps each term CURIE to the [`crate::model::DocFixture`]s that reference it
/// (via `terms_referenced`). Borrows both the CURIE keys and the fixtures from
/// the model, so it is lifetime-bound to it.
pub(crate) type FixturesByCurie<'a> =
    std::collections::HashMap<&'a str, Vec<&'a crate::model::DocFixture>>;

/// Precompute the fixture index for all terms in one pass: maps each term CURIE
/// to the fixtures referencing it. Avoids the O(terms × fixtures) per-term
/// linear scan of `model.fixtures` in [`full_card_for`]. The index itself
/// carries no ordering guarantee — callers re-sort the looked-up `Vec` (see
/// `full_card_for`'s `(slice, logical_path)` sort) so output order is
/// unaffected by build order here.
pub(crate) fn precompute_fixtures_by_curie(model: &DocsModel) -> FixturesByCurie<'_> {
    let mut map: FixturesByCurie<'_> = std::collections::HashMap::new();
    for fixture in &model.fixtures {
        for curie in &fixture.terms_referenced {
            map.entry(curie.as_str()).or_default().push(fixture);
        }
    }
    map
}

/// A single search record. Serialized as a deterministic JSON array element.
struct SearchRecord {
    /// The record kind (`term`, `slice`, `concern`, `mappingSet`, `document`).
    kind: &'static str,
    /// The CURIE for terms / mapping sets, else the IRI.
    id: String,
    /// The display label.
    label: String,
    /// The definition / comment, if any.
    definition: Option<String>,
    /// The site-relative URL of the record's HTML page.
    url: String,
    /// Advisory prose facet (terms only); omitted from JSON when empty.
    advice: Vec<String>,
    /// Crosswalk facet — `tag:object` tokens (terms only); omitted when empty.
    alignments: Vec<String>,
    /// Documentation-coverage facet — machine keys of the dimensions a term is
    /// MISSING (terms only), so a client can filter under-documented terms; omitted
    /// from JSON when the term is fully covered.
    missing_coverage: Vec<&'static str>,
}

impl ToJson for SearchRecord {
    /// Preserve declaration order, explicit nullable definition and omitted empty facets.
    fn to_json(&self) -> Value {
        let mut object = Object::new()
            .with("kind", self.kind)
            .with("id", self.id.to_json())
            .with("label", self.label.to_json())
            .with("definition", self.definition.to_json())
            .with("url", self.url.to_json());
        if !self.advice.is_empty() {
            object.insert("advice", self.advice.to_json());
        }
        if !self.alignments.is_empty() {
            object.insert("alignments", self.alignments.to_json());
        }
        if !self.missing_coverage.is_empty() {
            object.insert("missing_coverage", self.missing_coverage.to_json());
        }
        Value::from(object)
    }
}

/// Sort the complete record roster by URL/kind/ID and emit pretty JSON without a newline.
fn search_records_json(mut records: Vec<SearchRecord>) -> String {
    records.sort_by(|a, b| {
        a.url
            .cmp(&b.url)
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });
    json::write_pretty(&records.to_json())
}

/// Build the deterministic `search-index.json`: one record per term, slice,
/// concern, and mapping set, sorted by URL. A pure function of the model.
///
/// # Errors
/// Refuses an ambiguous source-to-page map.
pub fn search_index_json(model: &DocsModel) -> Result<String> {
    let page_map = source_page_map(model)?;
    Ok(search_index_json_with_map(model, &page_map))
}

/// [`search_index_json`] with the shared [`SourceToPageMap`] threaded in (built once per
/// site render). Byte-identical to `search_index_json`.
pub(crate) fn search_index_json_with_map(model: &DocsModel, doc_map: &SourceToPageMap) -> String {
    let mut records: Vec<SearchRecord> = Vec::new();
    let alignment_facets = precompute_alignment_facets(model);
    let ctx = crate::coverage::CoverageContext::new(model);

    for term in &model.terms {
        records.push(SearchRecord {
            kind: "term",
            id: term.curie.clone(),
            label: term.label.clone().unwrap_or_else(|| term.curie.clone()),
            definition: term.definition.clone(),
            url: format!("{}/index.html", Page::Term(term_slug(term)).dir()),
            advice: term_advice_facet(term),
            alignments: alignment_facets
                .get(term.iri.as_str())
                .cloned()
                .unwrap_or_default(),
            missing_coverage: crate::coverage::term_coverage(term, &ctx).missing_keys(),
        });
    }
    for slice in &model.slices {
        records.push(SearchRecord {
            kind: "slice",
            id: slice.iri.clone(),
            label: slice_display(slice),
            definition: None,
            url: format!("{}/index.html", Page::Slice(slice_slug(slice)).dir()),
            advice: Vec::new(),
            alignments: Vec::new(),
            missing_coverage: Vec::new(),
        });
    }
    for concern in &model.concerns {
        records.push(SearchRecord {
            kind: "concern",
            id: concern.curie.clone(),
            label: concern_display(concern),
            definition: concern.definition.clone(),
            url: format!("{}/index.html", Page::Concern(concern_slug(concern)).dir()),
            advice: Vec::new(),
            alignments: Vec::new(),
            missing_coverage: Vec::new(),
        });
    }
    for set in &model.mapping_sets {
        records.push(SearchRecord {
            kind: "mappingSet",
            id: set.curie.clone(),
            label: set_display(set),
            definition: set.comment.clone(),
            url: format!("{}/index.html", Page::LinkageIndex.dir()),
            advice: Vec::new(),
            alignments: Vec::new(),
            missing_coverage: Vec::new(),
        });
    }
    // One record per slice CHILD document (every non-`docs.md` markdown), indexed by
    // its title (label) and its full prose (definition), so search matches document
    // bodies. The top-level `docs.md` prose is grafted onto — and searched as — its
    // slice page, so it is not a separate record. `slice_children` is path-sorted.
    for slice in &model.slices {
        for entry in doc_map.slice_children(&slice.iri) {
            let full_prose = slice
                .documents
                .iter()
                .find(|d| d.source_path == entry.source_path)
                .map(|d| d.source_text.clone());
            records.push(SearchRecord {
                kind: "document",
                id: entry.source_path.clone(),
                label: entry.title.clone(),
                definition: full_prose,
                url: format!("{}index.html", entry.page),
                advice: Vec::new(),
                alignments: Vec::new(),
                missing_coverage: Vec::new(),
            });
        }
    }

    search_records_json(records)
}

/// Local names of a list of IRIs, joined with `, ` — for compact one-line
/// relational segments in the `llms.txt`/term-card surfaces.
fn join_local_names(iris: &[String]) -> String {
    iris.iter()
        .map(|i| local_name(i))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The compact llmstxt.org signature suffix for a term: ` (⊑ parents)` for a
/// class, ` [domain → range]` for a property (each side `?` when absent), empty
/// otherwise. Local names only, so the line stays compact.
fn term_signature(term: &DocTerm) -> String {
    match term.category {
        DocTermCategory::Property => {
            if term.domain.is_empty() && term.range.is_empty() {
                String::new()
            } else {
                let d = if term.domain.is_empty() {
                    "?".to_string()
                } else {
                    join_local_names(&term.domain)
                };
                let r = if term.range.is_empty() {
                    "?".to_string()
                } else {
                    join_local_names(&term.range)
                };
                format!(" [{d} → {r}]")
            }
        }
        _ => {
            if term.parents.is_empty() {
                String::new()
            } else {
                format!(" (⊑ {})", join_local_names(&term.parents))
            }
        }
    }
}

/// The one-lined definition (falling back to the label, else empty) for a term —
/// the bullet note source shared by the index and search surfaces.
fn term_note(term: &DocTerm) -> String {
    term.definition
        .as_deref()
        .or(term.label.as_deref())
        .map(one_line)
        .unwrap_or_default()
}

/// Build the standard llmstxt.org **index** (`llms.txt`) at the site root: an H1
/// title, the canonical summary blockquote, then `## Section`s of markdown-link
/// bullets covering the full vocabulary plus the slice/concern/guide/reference
/// pages. The links are site-root-relative (`from == ""`) and target the
/// published `.../index.html` pages — the SAME convention the MCP index recovers
/// from `gmeow:docUrl`, so the two never disagree. Notes are capped
/// ([`llms::LLMS_NOTE_CAP`]); the complete inlined form is [`llms_full_txt`].
///
/// # Errors
/// Refuses an ambiguous source-to-page map.
pub fn llms_txt(model: &DocsModel) -> Result<String> {
    let page_map = source_page_map(model)?;
    Ok(llms_txt_with_map(model, &page_map))
}

/// [`llms_txt`] with the shared [`SourceToPageMap`] threaded in (built once per site
/// render). Byte-identical to `llms_txt`.
pub(crate) fn llms_txt_with_map(model: &DocsModel, doc_map: &SourceToPageMap) -> String {
    let prose = vec![format!(
        "Vocabulary {}. Namespace: {GMEOW_NS}. The RDF 1.2 grounding slices are canonical; this index links into the published documentation.",
        model.version
    )];
    let mut sections: Vec<LlmsSection> = Vec::new();

    // ── Vocabulary: the category index pages, with term counts. ──
    let mut vocab: Vec<LlmsBullet> = Vec::new();
    for category in CATEGORY_ORDER {
        let count = model
            .terms
            .iter()
            .filter(|t| t.category == category)
            .count();
        if count == 0 {
            continue;
        }
        vocab.push(LlmsBullet {
            text: category_plural(category).to_string(),
            url: Some(format!("{}/index.html", Page::Category(category).dir())),
            signature: String::new(),
            note: format!("{count} terms"),
        });
    }
    if !vocab.is_empty() {
        sections.push(LlmsSection {
            heading: "Vocabulary".to_string(),
            bullets: vocab,
        });
    }

    // ── Terms: one linked bullet per term (full vocabulary coverage). ──
    let term_bullets: Vec<LlmsBullet> = model
        .terms
        .iter()
        .map(|term| LlmsBullet {
            text: term.curie.clone(),
            url: Some(format!("{}/index.html", Page::Term(term_slug(term)).dir())),
            signature: term_signature(term),
            note: llms::cap_note(&term_note(term)),
        })
        .collect();
    if !term_bullets.is_empty() {
        sections.push(LlmsSection {
            heading: "Terms".to_string(),
            bullets: term_bullets,
        });
    }

    // ── Slices. ──
    let slice_bullets: Vec<LlmsBullet> = model
        .slices
        .iter()
        .map(|slice| LlmsBullet {
            text: slice_display(slice),
            url: Some(format!(
                "{}/index.html",
                Page::Slice(slice_slug(slice)).dir()
            )),
            signature: String::new(),
            note: String::new(),
        })
        .collect();
    if !slice_bullets.is_empty() {
        sections.push(LlmsSection {
            heading: "Slices".to_string(),
            bullets: slice_bullets,
        });
    }

    // ── Documents: every slice CHILD document page (non-`docs.md` markdown),
    // grouped by slice via the single page-map authority; the top-level `docs.md`
    // is grafted onto its slice page and covered by the Slices section above. ──
    let mut document_bullets: Vec<LlmsBullet> = Vec::new();
    for slice in &model.slices {
        for entry in doc_map.slice_children(&slice.iri) {
            document_bullets.push(LlmsBullet {
                text: entry.title.clone(),
                url: Some(format!("{}index.html", entry.page)),
                signature: String::new(),
                note: llms::cap_note(&format!(
                    "{} — `{}`",
                    slice_display(slice),
                    entry.source_path
                )),
            });
        }
    }
    if !document_bullets.is_empty() {
        sections.push(LlmsSection {
            heading: "Documents".to_string(),
            bullets: document_bullets,
        });
    }

    // ── Concerns. ──
    let concern_bullets: Vec<LlmsBullet> = model
        .concerns
        .iter()
        .map(|concern| LlmsBullet {
            text: concern_display(concern),
            url: Some(format!(
                "{}/index.html",
                Page::Concern(concern_slug(concern)).dir()
            )),
            signature: String::new(),
            note: concern
                .definition
                .as_deref()
                .map(|d| llms::cap_note(&one_line(d)))
                .unwrap_or_default(),
        })
        .collect();
    if !concern_bullets.is_empty() {
        sections.push(LlmsSection {
            heading: "Concerns".to_string(),
            bullets: concern_bullets,
        });
    }

    // ── Guides: recipes then learning paths. ──
    let mut guide_bullets: Vec<LlmsBullet> = Vec::new();
    for recipe in &model.recipes {
        guide_bullets.push(LlmsBullet {
            text: recipe.title.clone(),
            url: Some(format!(
                "{}/index.html",
                Page::Recipe(recipe.slug.clone()).dir()
            )),
            signature: String::new(),
            note: llms::cap_note(&one_line(&recipe.goal)),
        });
    }
    for path in &model.learning_paths {
        guide_bullets.push(LlmsBullet {
            text: path.title.clone(),
            url: Some(format!(
                "{}/index.html",
                Page::LearningPath(path.slug.clone()).dir()
            )),
            signature: String::new(),
            note: llms::cap_note(&one_line(&path.goal)),
        });
    }
    if !guide_bullets.is_empty() {
        sections.push(LlmsSection {
            heading: "Guides".to_string(),
            bullets: guide_bullets,
        });
    }

    // ── Reference: the standing index pages (always present) plus the
    // offline-corpus affordance. The five titled by
    // `llms::STANDING_REFERENCE_PAGES` are the SAME ones the MCP/consumer
    // surfaces name (linkless there, via `llms::standing_reference_section`);
    // this site rendering links each into its published page so wording/order
    // cannot drift between the two while the site keeps real URLs. ──
    let standing = standing_reference_site_pages();
    let mut reference_bullets = vec![
        reference_bullet("Slice index", &Page::SliceIndex),
        reference_bullet("Linkages", &Page::LinkageIndex),
    ];
    for (title, page) in &standing[..4] {
        reference_bullets.push(reference_bullet(title, page));
    }
    reference_bullets.extend([
        reference_bullet("External ontologies", &Page::ExternalIndex),
        reference_bullet("Integrity constraints", &Page::IntegrityIndex),
        reference_bullet("Logic & reasoning", &Page::Logic),
    ]);
    // The build-pipeline DAG page is emitted only when a pipeline was discovered
    // (honest absence for a bare model); link it only when it is actually rendered.
    if model.pipeline.is_some() {
        let (title, page) = &standing[4];
        reference_bullets.push(reference_bullet(title, page));
    }
    // The offline, agent-ingestible corpus: a flattened per-term card projection
    // written by the docs-export snippets affordance (linkless — it names a
    // capability, not a published page).
    reference_bullets.push(LlmsBullet {
        text: "Offline snippet corpus".to_string(),
        url: None,
        signature: String::new(),
        note: llms::SNIPPETS_CORPUS_NOTE.to_string(),
    });
    sections.push(LlmsSection {
        heading: "Reference".to_string(),
        bullets: reference_bullets,
    });

    llms::render_index(&model.title, &prose, &sections)
}

/// The one-line description of the offline snippet corpus. Re-exported from the
/// shared [`llms`] module (the single definition both the docs site and the
/// MCP/consumer surfaces reference) so this crate's existing
/// `render::SNIPPETS_CORPUS_NOTE` path keeps working.
pub use crate::llms::SNIPPETS_CORPUS_NOTE;

/// Map each of `llms::STANDING_REFERENCE_PAGES` (title, same order) to its
/// docs-site page. The docs-site rendering of the shared standing-page list;
/// the MCP/consumer surfaces render the same titles linkless via
/// `llms::standing_reference_section` instead.
fn standing_reference_site_pages() -> [(&'static str, Page); 5] {
    let titles = llms::STANDING_REFERENCE_PAGES;
    [
        (titles[0], Page::CompetencyIndex),
        (titles[1], Page::FixtureIndex),
        (titles[2], Page::NotationIndex),
        (titles[3], Page::Glossary),
        (titles[4], Page::PipelineDag),
    ]
}

/// A single Reference-section bullet linking to a standing index page.
fn reference_bullet(text: &str, page: &Page) -> LlmsBullet {
    LlmsBullet {
        text: text.to_string(),
        url: Some(format!("{}/index.html", page.dir())),
        signature: String::new(),
        note: String::new(),
    }
}

/// Build the standard llmstxt.org **complete** form (`llms-full.txt`): the same
/// header as [`llms_txt`], then the full inlined content of every term (no
/// truncation), followed by the concern definitions and the slice inventory. This
/// is the single-file, link-free surface an agent can ingest whole.
///
/// # Errors
/// Refuses an ambiguous source-to-page map or a dangling internal document link.
pub fn llms_full_txt(model: &DocsModel) -> Result<String> {
    let page_map = source_page_map(model)?;
    llms_full_txt_with_map(model, &page_map)
}

/// [`llms_full_txt`] with the shared [`SourceToPageMap`] threaded in (built once per site
/// render). Byte-identical to `llms_full_txt`.
pub(crate) fn llms_full_txt_with_map(
    model: &DocsModel,
    doc_map: &SourceToPageMap,
) -> Result<String> {
    let prose = vec![format!(
        "Vocabulary {}. Namespace: {GMEOW_NS}. Complete inlined form — every term, its definition, and its usage advice in full.",
        model.version
    )];
    let mut out = llms::llms_header(&model.title, &prose);

    let alignment_facets = precompute_alignment_facets(model);
    out.push_str("## Terms\n\n");
    for term in &model.terms {
        out.push_str(&term_full_block(term, &alignment_facets, model));
    }

    if !model.concerns.is_empty() {
        out.push_str("## Concerns\n\n");
        for concern in &model.concerns {
            out.push_str(&format!("### {}\n\n", concern_display(concern)));
            if let Some(def) = &concern.definition {
                out.push_str(&one_line(def));
                out.push_str("\n\n");
            }
        }
    }

    if !model.slices.is_empty() {
        out.push_str("## Slices\n\n");
        for slice in &model.slices {
            let tier = slice
                .tier
                .as_ref()
                .map(|t| format!(" (tier: {t:?})"))
                .unwrap_or_default();
            out.push_str(&format!("- {}{tier}\n", slice_display(slice)));
        }
        out.push('\n');
    }

    // ── Documents: the COMPLETE first-class Markdown corpus inlined in full —
    // every slice document (the top-level `docs.md` AND every child `*.md`), full
    // prose, with intra-corpus links resolved through the SAME `SourceToPageMap`
    // authority (this self-contained corpus is a T3 link domain rooted at the site
    // root). `model.slices` is IRI-sorted and each `documents` is path-sorted. ──
    let has_documents = model.slices.iter().any(|s| !s.documents.is_empty());
    if has_documents {
        out.push_str("## Documents\n\n");
        for slice in &model.slices {
            for doc in &slice.documents {
                out.push_str(&format!("### {} — {}\n\n", slice_display(slice), doc.title));
                let body = rewrite_doc_body(
                    &doc.source_text,
                    doc_map,
                    &slice.iri,
                    &doc.source_path,
                    "",
                    HeadingDemotion::CorpusFloor,
                    false,
                )?;
                out.push_str(&body);
                if !body.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
        }
    }

    // ── Reference: the standing index pages an agent should know exist, plus the
    // offline-corpus affordance. This surface is self-contained (linkless), so the
    // pages are named rather than linked; built from the SAME
    // `llms::standing_reference_section` the MCP/consumer surfaces render, so the
    // two genuinely cannot drift (this was previously a hand-duplicated list). ──
    let mut reference_section = llms::standing_reference_section();
    if model.pipeline.is_none() {
        // Honest absence for a bare model: the build-pipeline DAG page is only
        // ever rendered when a pipeline was actually discovered.
        let pipeline_title = *llms::STANDING_REFERENCE_PAGES
            .last()
            .expect("standing reference pages is non-empty");
        reference_section
            .bullets
            .retain(|b| b.text != pipeline_title);
    }
    out.push_str(&llms::render_section(&reference_section));

    Ok(out)
}

/// The metadata + definition + advisory-field body of a term (NO heading). The
/// shared core of both the per-term card and the `llms-full.txt` inlined block.
/// Pure markdown text (no links), so it is safe to inline anywhere. Takes the
/// precomputed alignment facets so a caller emitting every term pays the linkage
/// scan ONCE (not O(N²)), and `model` so the term→model link can be gated on the
/// schema-fragment digest (see [`doc_term_card`]).
fn term_body(term: &DocTerm, alignment_facets: &AlignmentFacets, model: &DocsModel) -> String {
    crate::card::render_card_body(
        &doc_term_card(term, alignment_facets, model),
        crate::card::CardDetail::Standard,
    )
}

/// Build the neutral [`crate::card::Card`] from a docs-site [`DocTerm`], resolving
/// every IRI-bearing field to its display (local-name) form. The shared
/// [`crate::card::render_card_body`] then renders it — the SAME renderer the
/// folded-snapshot MCP card uses, so the two never diverge (§19 one-path).
fn doc_term_card(
    term: &DocTerm,
    alignment_facets: &AlignmentFacets,
    model: &DocsModel,
) -> crate::card::Card {
    let label = match &term.label {
        Some(l) if l != &term.curie => Some(l.clone()),
        _ => None,
    };
    // The explicit term→model link (§19): a modeled class carries the importable
    // dotted path of its generated Pydantic model plus a compact construct/validate
    // snippet, from the SAME emitter routing the Python example tab uses (never
    // duplicated). Gated on the SAME schema-fragment digest as
    // [`python_syntax_tab`] — only a class with an actually-generated model has a
    // schema fragment to route to.
    let is_modeled = term.category == DocTermCategory::Class
        && model
            .schema_fragments
            .as_ref()
            .is_some_and(|digest| digest.schema_by_term.contains_key(&term.iri));
    let (python_model, python_snippet) = if is_modeled {
        (
            Some(crate::card::python_model_path(&term.owner_slice, &term.iri)),
            Some(crate::card::python_model_snippet(
                &term.owner_slice,
                &term.iri,
                &term.curie,
            )),
        )
    } else {
        (None, None)
    };
    crate::card::Card {
        category: category_singular(term.category).to_string(),
        iri: term.iri.clone(),
        label,
        // The docs side ALWAYS carries slice provenance (the owning module).
        slice: Some(local_name(&term.owner_slice).to_string()),
        // Box roles stay CURIEs (matching the folded side's `gmeow:boxTBox`).
        box_roles: term.box_roles.clone(),
        definition: term.definition.as_deref().map(one_line),
        parents: local_name_vec(&term.parents),
        domain: local_name_vec(&term.domain),
        range: local_name_vec(&term.range),
        use_when: term.use_when.clone(),
        avoid_when: term.avoid_when.clone(),
        how_to_use: term.how_to_use.clone(),
        scope_notes: term.scope_notes.clone(),
        examples: term.examples.clone(),
        logic_stereotypes: term.logic_stereotypes.clone(),
        related_terms: local_name_vec(&term.related_terms),
        use_for_consumer: term.use_for_consumer.clone(),
        avoid_for_consumer: term.avoid_for_consumer.clone(),
        aligns: alignment_facets
            .get(term.iri.as_str())
            .cloned()
            .unwrap_or_default(),
        python_model,
        python_snippet,
        // Full-tier rich panels: never populated on the docs-site path.
        ..crate::card::Card::default()
    }
}

/// Build the FULL-tier [`crate::card::Card`] for a site term: the compact
/// [`doc_term_card`] enriched with the rich oracle panels drawn DIRECTLY from the
/// site model + executable-docs data (`model` + `exec`) — the site twin of the MCP
/// `doc_card` full tier. It carries the SAME `Card` field types and is rendered by
/// the SAME [`crate::card::render_card`], so the two full-card surfaces never fork a
/// second renderer (§19 one-path).
///
/// Panel provenance mirrors the term page's own sections:
/// * entailments — `exec.term_entailments[term.iri]` (the reasoned B3 derivations);
/// * Do / Don't — `fixtures_by_curie[term.curie]` (`model.fixtures` referencing
///   the term's CURIE, precomputed once by [`precompute_fixtures_by_curie`]),
///   split by kind;
/// * diagnostics — `model.diagnostics.by_term[term.iri]`;
/// * loss — `model.term_loss.by_term[term.iri]`.
///
/// Every panel is an honest projection: a term with no data for a panel simply
/// carries an empty `Vec`, which the renderer omits — never a fabricated section.
/// In a model-only render (`ExecutableDocsData::default`, non-English tree) the
/// entailments panel is empty by construction, exactly like the term page.
fn full_card_for(
    model: &DocsModel,
    exec: &ExecutableDocsData,
    term: &DocTerm,
    alignment_facets: &AlignmentFacets,
    fixtures_by_curie: &FixturesByCurie<'_>,
) -> crate::card::Card {
    let mut card = doc_term_card(term, alignment_facets, model);

    // Entailments — the reasoner "why" derivations documenting the term.
    if let Some(entailments) = exec.term_entailments.get(&term.iri) {
        card.entailments = entailments
            .iter()
            .map(|e| crate::card::CardEntailment {
                rule: e.rule.clone(),
                conclusion: e.conclusion.clone(),
                premises: e.premises.clone(),
            })
            .collect();
    }

    // Do / Don't conformance fixtures referencing this term, in the same
    // (slice, logical_path) order the term page lists them. Each body is one-lined
    // and capped to a short snippet (the full Turtle stays available on the
    // fixtures index / `counter_examples` tool). Looked up from the precomputed
    // CURIE index (O(1)) rather than rescanning `model.fixtures` per term.
    let mut term_fixtures: Vec<&crate::model::DocFixture> = fixtures_by_curie
        .get(term.curie.as_str())
        .cloned()
        .unwrap_or_default();
    term_fixtures.sort_by(|a, b| {
        a.slice
            .cmp(&b.slice)
            .then_with(|| a.logical_path.cmp(&b.logical_path))
    });
    for fixture in term_fixtures {
        let entry = crate::card::CardFixture {
            title: fixture.title.clone(),
            body: crate::llms::cap_note(&one_line(&fixture.text)),
        };
        match fixture.kind {
            crate::model::DocFixtureKind::Wellformed => card.fixtures_do.push(entry),
            crate::model::DocFixtureKind::CounterExample => card.fixtures_dont.push(entry),
        }
    }

    // Diagnostics the term may hit — the per-term join from `stage-validate` +
    // `stage-compile-logic`, in the digest's stable node order.
    if let Some(findings) = model
        .diagnostics
        .as_ref()
        .and_then(|d| d.by_term.get(&term.iri))
    {
        card.diagnostics = findings
            .iter()
            .map(|f| crate::card::CardDiagnostic {
                code: f.code.clone(),
                note: f.message.clone(),
            })
            .collect();
    }

    // Projection loss — the dynamic per-term degradation rows, in the digest's
    // by-target order.
    if let Some(rows) = model
        .term_loss
        .as_ref()
        .and_then(|d| d.by_term.get(&term.iri))
    {
        card.loss = rows
            .iter()
            .map(|r| crate::card::CardLoss {
                target: r.target.clone(),
                preservation: r.preservation_kind.clone(),
            })
            .collect();
    }

    card
}

/// The full inlined block for one term in `llms-full.txt`: a `### {curie}{signature}`
/// heading followed by the shared [`term_body`].
fn term_full_block(
    term: &DocTerm,
    alignment_facets: &AlignmentFacets,
    model: &DocsModel,
) -> String {
    format!(
        "### {}{}\n\n{}",
        term.curie,
        term_signature(term),
        term_body(term, alignment_facets, model)
    )
}

/// A prompt-ready, standalone Markdown card for one term: a `# {curie}{signature}`
/// title followed by the shared [`term_body`] (metadata + definition + every
/// advisory field). Compact, link-free, and self-contained for context-window
/// injection. Emitted at `terms/{slug}/card.md` and served live over MCP.
pub fn term_card_md(model: &DocsModel, term: &DocTerm) -> String {
    let alignment_facets = precompute_alignment_facets(model);
    term_card_md_inner(term, &alignment_facets, model)
}

/// The `card.json` machine surface for ONE term — the STANDARD-tier [`Card`](crate::card::Card)
/// serialized byte-for-byte as `render_site_lang` emits `terms/{slug}/card.json`
/// (and as the live MCP `doc_card format=json detail=standard` renders). The
/// single-term counterpart of [`term_card_md`]: lets a caller obtain one term's
/// card payload without rendering the whole site.
pub fn term_card_json(model: &DocsModel, term: &DocTerm) -> Vec<u8> {
    let alignment_facets = precompute_alignment_facets(model);
    let standard =
        doc_term_card(term, &alignment_facets, model).projected(crate::card::CardDetail::Standard);
    json::write_compact(&standard.to_json()).into_bytes()
}

/// [`term_card_md`] with the alignment facets supplied — lets `render_site_lang`
/// emit every card while paying the linkage scan once.
fn term_card_md_inner(
    term: &DocTerm,
    alignment_facets: &AlignmentFacets,
    model: &DocsModel,
) -> String {
    format!(
        "# {}{}\n\n{}",
        term.curie,
        term_signature(term),
        term_body(term, alignment_facets, model)
    )
}

/// The local names of a list of IRIs as an owned `Vec` (for the advisory-field
/// helper that takes `&[String]`).
fn local_name_vec(iris: &[String]) -> Vec<String> {
    iris.iter().map(|i| local_name(i).to_string()).collect()
}

#[path = "render.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "render.cq_rationale.tests.rs"]
mod cq_rationale_tests;

#[path = "render.search_index.tests.rs"]
#[cfg(test)]
mod search_index_tests;

#[path = "render.card_json.tests.rs"]
#[cfg(test)]
mod card_json_tests;
