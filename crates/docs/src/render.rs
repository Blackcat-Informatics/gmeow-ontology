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

use minijinja::{Environment, context};
use pulldown_cmark::{Options, Parser, html as cmark_html};

use crate::badge;
use crate::exec::ExecutableDocsData;
use crate::i18n::{self, ENGLISH};
use crate::llms::{self, LlmsBullet, LlmsSection};
use crate::model::{DocConcern, DocSlice, DocTerm, DocTermCategory, DocsModel};
use crate::source_map::{
    DocLinkResolution, LinkResolution, SLICE_PAGE_SOURCE, SourceToPageMap, fence_open,
};
use crate::svg;

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

/// The site-relative path of the **core browser bundle** — the object-level ontology
/// as N-Quads text ([`ExecutableDocsData::core_bundle_nquads`]). The bundle explorer
/// parses it client-side (purrdf `Dataset.parse`) to answer `info`/`describe`.
const CORE_BUNDLE_NQ_PATH: &str = "assets/gmeow-core.nq";

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

/// The vendored wasm engines emitted under `assets/<name>/` when the playground is
/// present: the offline SPARQL runtime (purrdf) and the repo-free Tier-1 validator
/// (gmeow-validate-wasm). Pinned build inputs — one descriptor per asset lives in
/// [`crate::vendored_asset`]; see each `PROVENANCE.md`.
const VENDORED_WASM_ASSETS: &[&crate::vendored_asset::VendoredWasmAsset] = &[
    &crate::vendored_asset::PURRDF_ASSET,
    &crate::vendored_asset::VALIDATE_ASSET,
    &crate::vendored_asset::REASON_ASSET,
    &crate::vendored_asset::GMN_ASSET,
];

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
pub fn render_site(model: &DocsModel) -> Site {
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
pub fn render_site_lang(model: &DocsModel, lang: &str) -> Site {
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
pub fn render_site_lang_exec(model: &DocsModel, lang: &str, exec: &ExecutableDocsData) -> Site {
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
pub fn render_site_lang_exec_with_diagrams(
    model: &DocsModel,
    lang: &str,
    exec: &ExecutableDocsData,
    diagrams: &BTreeMap<String, Vec<u8>>,
) -> Site {
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
    let page_map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    for page in pages(model) {
        files.insert(
            page.md_path(),
            to_markdown_exec_with_map(model, &page, exec, &page_map).into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec_with_map(model, &page, lang, exec, &page_map).into_bytes(),
        );
    }
    files.insert(CSS_PATH.to_string(), CSS.as_bytes().to_vec());

    // The interactive asset set (the controller module, the vendored wasm engines, and
    // the SPARQL-playground / browser-bundle data) — the ONE authority both the site
    // here and the packed mdbook ([`crate::mdbook`]) draw from, so the book ships the
    // byte-identical engines the site's witness lanes prove. Language-independent,
    // emitted only in the English tree (see the gate above).
    files.extend(interactive_asset_files(exec));

    // The offline SPARQL playground page. Term/slice export runs through this same
    // engine + asset via `DESCRIBE`, so no static export files are needed.
    if exec.has_playground() {
        let page = Page::SparqlPlayground;
        files.insert(
            page.md_path(),
            to_markdown_exec_with_map(model, &page, exec, &page_map).into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec_with_map(model, &page, lang, exec, &page_map).into_bytes(),
        );
    }

    // The bundle explorer page (browser gmeow info/describe + live reasoning + GMN
    // transcode over the core bundle). Its assets are shipped by
    // `interactive_asset_files` above.
    if exec.has_bundle() {
        let page = Page::BundleExplorer;
        files.insert(
            page.md_path(),
            to_markdown_exec_with_map(model, &page, exec, &page_map).into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec_with_map(model, &page, lang, exec, &page_map).into_bytes(),
        );
    }

    // The conjecture playground page (browser symmetric proof / counterproof over the
    // curated demo library + the live wasm conjecture engine). Its assets (the demo
    // library + the vendored reason engine) are shipped by `interactive_asset_files`.
    if exec.has_conjectures() {
        let page = Page::ConjecturePlayground;
        files.insert(
            page.md_path(),
            to_markdown_exec_with_map(model, &page, exec, &page_map).into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec_with_map(model, &page, lang, exec, &page_map).into_bytes(),
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
        llms_full_txt_with_map(model, &page_map).into_bytes(),
    );

    // Prompt-ready per-term cards: a compact, link-free Markdown card per
    // term at `terms/{slug}/card.md`, for context-window injection. The alignment
    // facets are precomputed once so emitting every card stays O(N), not O(N²).
    //
    // Alongside the human-oriented `card.md` two machine surfaces ride:
    // `card.json` — the STANDARD-tier `Card` serialized through the SAME
    // `serde_json` path the live MCP `doc_card format=json detail=standard` uses
    // (byte-identical to that tool's card payload for the term) — and
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
            // serialized (the `Card` derives `Serialize` with a fixed field
            // order and every internal collection is already ordered).
            let standard = doc_term_card(term, &alignment_facets, model)
                .projected(crate::card::CardDetail::Standard);
            let json = serde_json::to_vec(&standard).unwrap_or_else(|e| {
                // A pure-data `Card` of `String`/`Vec`/`Option` fields cannot fail
                // to serialize; a failure here is a genuine invariant break.
                panic!("card.json serialize for {slug}: {e}")
            });
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

    Site { files }
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
    // One entry per shipped bundle asset, in a fixed deterministic order (core, full, then
    // the optional conjecture demo library). Each entry is byte-identical to the others' shape,
    // so the 2-entry (bundle-only) case is byte-for-byte unchanged from the fixed format.
    let entry = |path: &str, bytes: &[u8]| {
        format!(
            "  \"{path}\": {{ \"blake3\": \"blake3:{d}\", \"bytes\": {n} }}",
            d = blake3::hash(bytes).to_hex(),
            n = bytes.len(),
        )
    };
    let mut entries = vec![
        entry(CORE_BUNDLE_NQ_PATH, &exec.core_bundle_nquads),
        entry(FULL_BUNDLE_GTS_PATH, &exec.full_bundle_gts),
    ];
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
pub(crate) fn interactive_asset_files(exec: &ExecutableDocsData) -> BTreeMap<String, Vec<u8>> {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    if !exec.has_playground() && !exec.has_bundle() && !exec.has_conjectures() {
        return files;
    }
    files.insert(DOCS_JS_PATH.to_string(), DOCS_JS.as_bytes().to_vec());
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
            CORE_BUNDLE_NQ_PATH.to_string(),
            exec.core_bundle_nquads.clone(),
        );
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
pub fn to_markdown(model: &DocsModel, page: &Page) -> String {
    to_markdown_exec(model, page, &ExecutableDocsData::default())
}

/// Render a page's Markdown body, appending the build-time executable-docs sections
/// (per-slice export links, reasoner "try it" diffs) when `exec` supplies them. With
/// an empty `exec` this is byte-identical to the base render (the executable surfaces
/// simply do not appear), so the model-only goldens are unaffected.
pub fn to_markdown_exec(model: &DocsModel, page: &Page, exec: &ExecutableDocsData) -> String {
    // The single link-rewrite authority is a pure function of the model; a standalone
    // page render builds it here. A full site/book render instead builds it ONCE and
    // calls [`to_markdown_exec_with_map`] per page, so the map is not rebuilt for every
    // slice/child page (the anchors were otherwise re-derived N+M times per render).
    let page_map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");
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
) -> String {
    let mut md = match page {
        Page::Term(slug) => {
            let mut md = md_term(model, slug, exec);
            append_term_export_section(&mut md, model, slug, exec);
            md
        }
        Page::Slice(slug) => {
            let mut md = md_slice(model, slug, page_map);
            append_slice_executable_sections(&mut md, model, slug, exec);
            md
        }
        Page::SparqlPlayground => md_playground(model, exec),
        Page::BundleExplorer => md_bundle_explorer(model, exec),
        Page::ConjecturePlayground => md_conjecture_playground(model, exec),
        _ => to_markdown_base(model, page, page_map),
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
    md
}

fn to_markdown_base(model: &DocsModel, page: &Page, page_map: &SourceToPageMap) -> String {
    match page {
        Page::Landing => md_landing(model),
        Page::GettingStarted => md_getting_started(model),
        Page::About => md_about(model),
        Page::Health => md_health(model),
        Page::Changelog => md_changelog(model),
        Page::Category(category) => md_category(model, *category),
        Page::Term(slug) => md_term(model, slug, &ExecutableDocsData::default()),
        Page::SliceIndex => md_slice_index(model),
        Page::Slice(slug) => md_slice(model, slug, page_map),
        Page::SliceDocument { slice, path } => md_slice_document(model, slice, path, page_map),
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
    }
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
/// core bundle. The controller (`gmeow-docs.js`) loads the core N-Quads via the
/// shared loader, shows the bundle's `info` summary on boot, and runs a client-side
/// `DESCRIBE` for the entered term IRI — the same describe the native `gmeow describe`
/// produces, proven byte-identical by the F2 witness lane.
fn md_bundle_explorer(model: &DocsModel, _exec: &ExecutableDocsData) -> String {
    let mut out = String::new();
    heading(&mut out, 1, "Bundle explorer");
    line(
        &mut out,
        "Explore the shipped ontology **entirely in your browser** — no server, no \
         network. This loads the object-level core bundle and answers `info` (a summary \
         of the loaded graph) and `describe <iri>` (every triple mentioning a term) via \
         the native `purrdf` engine compiled to WebAssembly — the same answers the \
         `gmeow` CLI gives.",
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
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn md_landing(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, &model.title);
    line(
        &mut out,
        &format!("Version **{}**.", md_escape(&model.version)),
    );
    line(
        &mut out,
        &format!(
            "This site documents **{}** vocabulary terms across **{}** slices.",
            model.terms.len(),
            model.slices.len()
        ),
    );

    // ── Task-first navigation: promote the authored recipes and learning paths
    // to a prominent "I want to…" block so a goal-oriented reader starts from a
    // task rather than the vocabulary, plus the RDF-to-developer glossary bridge.
    // Rendered only when the guides slice authored at least one recipe / learning
    // path (honest absence otherwise); the glossary link is always offered.
    {
        let from = Page::Landing.dir();
        heading(&mut out, 2, model.ui("body_i_want_to"));
        for recipe in &model.recipes {
            push_line(
                &mut out,
                &format!(
                    "- [{}]({}index.md) — {}",
                    md_escape(&recipe.title),
                    rel(&from, &Page::Recipe(recipe.slug.clone()).dir()),
                    md_escape(&one_line(&recipe.goal)),
                ),
            );
        }
        for path in &model.learning_paths {
            push_line(
                &mut out,
                &format!(
                    "- [{}]({}index.md) — {}",
                    md_escape(&path.title),
                    rel(&from, &Page::LearningPath(path.slug.clone()).dir()),
                    md_escape(&one_line(&path.goal)),
                ),
            );
        }
        push_line(
            &mut out,
            &format!(
                "- [{}]({}index.md) — a bridge for readers new to RDF.",
                md_escape(model.ui("body_glossary")),
                rel(&from, &Page::Glossary.dir()),
            ),
        );
        blank(&mut out);
    }

    heading(&mut out, 2, model.ui("body_vocabulary_by_category"));
    push_line(&mut out, "| Category | Terms |");
    push_line(&mut out, "| --- | --- |");
    for category in [
        DocTermCategory::Class,
        DocTermCategory::Property,
        DocTermCategory::Individual,
        DocTermCategory::Datatype,
        DocTermCategory::Other,
    ] {
        let count = model
            .terms
            .iter()
            .filter(|t| t.category == category)
            .count();
        if count == 0 {
            continue;
        }
        let href = rel(&Page::Landing.dir(), &Page::Category(category).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {count} |",
                md_escape(category_plural(category)),
                href
            ),
        );
    }
    blank(&mut out);

    heading(&mut out, 2, model.ui("body_browse"));
    let from = Page::Landing.dir();
    push_line(
        &mut out,
        &format!(
            "- [All slices]({}index.md) — the {} compilation units.",
            rel(&from, &Page::SliceIndex.dir()),
            model.slices.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Concerns]({}index.md) — {} cross-cutting design concerns.",
            rel(&from, &Page::ConcernIndex.dir()),
            model.concerns.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Linkages]({}index.md) — {} term equivalences to external vocabularies.",
            rel(&from, &Page::LinkageIndex.dir()),
            model.linkages.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Examples]({}index.md) — {} worked examples.",
            rel(&from, &Page::ExampleIndex.dir()),
            model.examples.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [External ontologies]({}index.md) — {} external terms referenced.",
            rel(&from, &Page::ExternalIndex.dir()),
            model.external_terms.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Integrity constraints]({}index.md)",
            rel(&from, &Page::IntegrityIndex.dir())
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [What GMEOW enforces]({}index.md) — {} validation rules the toolchain enforces.",
            rel(&from, &Page::ConstraintCatalog.dir()),
            model.constraint_rules.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Recipes]({}index.md) — {} task-oriented adoption recipes.",
            rel(&from, &Page::RecipeIndex.dir()),
            model.recipes.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Learning paths]({}index.md) — {} curated adoption journeys.",
            rel(&from, &Page::LearningPathIndex.dir()),
            model.learning_paths.len()
        ),
    );
    if model.four_boxes.is_some() {
        push_line(
            &mut out,
            &format!(
                "- [What is this?]({}index.md) — the ABox/TBox/RBox/CBox/ConfigBox doctrine.",
                rel(&from, &Page::FourBoxes.dir())
            ),
        );
    }
    push_line(
        &mut out,
        &format!(
            "- [Documentation health]({}index.md) — per-dimension coverage of the vocabulary surface.",
            rel(&from, &Page::Health.dir())
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Getting started]({}index.md)",
            rel(&from, &Page::GettingStarted.dir())
        ),
    );
    push_line(
        &mut out,
        &format!("- [About]({}index.md)", rel(&from, &Page::About.dir())),
    );
    blank(&mut out);
    out
}

/// The number of PER-TERM coverage dimensions a projected term record covers —
/// counted over the canonical [`crate::coverage::DIMENSIONS`] set so a stray
/// local name in the read-back can never inflate the completeness distribution.
fn present_dimension_count(term: &crate::rdf::DocTermFacts) -> usize {
    crate::coverage::DIMENSIONS
        .iter()
        .filter(|dim| term.covers.contains(dim.dimension.local_name()))
        .count()
}

/// The short human title of a maturity anchor (`Minimal` / `Basic` / `Full` /
/// `Maximal`) for the health dashboard.
fn anchor_title(anchor: crate::maturity::MaturityAnchor) -> String {
    use crate::maturity::MaturityAnchor::*;
    match anchor {
        Minimal => "Minimal",
        Basic => "Basic",
        Full => "Full",
        Maximal => "Maximal",
    }
    .to_owned()
}

/// The gap-to-next-tier burn-down for a slice: the next anchor up the derived
/// ladder and the dimensions its intent requires that the slice's covered set
/// (`covers`, read back from `gmeow:docCoversDimension`) does not yet carry. At
/// the ceiling (earns `Maximal`) there is no next tier.
fn maturity_gap(
    earned: Option<crate::maturity::MaturityAnchor>,
    covers: &BTreeSet<String>,
) -> String {
    use crate::maturity::MaturityAnchor;
    // The next tier above the earned floor; when nothing is earned yet the first
    // rung (Minimal) is the target.
    let next = match earned {
        Some(a) => a.next(),
        None => Some(MaturityAnchor::Minimal),
    };
    let Some(next) = next else {
        return "at ceiling (Maximal)".to_owned();
    };
    let missing: Vec<String> = next
        .intent()
        .iter()
        .filter(|dim| !covers.contains(dim.local_name()))
        .map(|dim| dimension_label(*dim))
        .collect();
    if missing.is_empty() {
        // The intent is already covered — the slice is one closure step from the
        // next tier (the emitter reports the largest satisfied anchor as earned).
        format!("→ {}", anchor_title(next))
    } else {
        format!("→ {}: missing {}", anchor_title(next), missing.join(", "))
    }
}

/// The human display label for a coverage dimension, resolved from the single
/// [`crate::coverage`] label authority (per-term and slice-scoped dimensions).
fn dimension_label(dim: crate::maturity::Dimension) -> String {
    crate::coverage::DIMENSIONS
        .iter()
        .chain(crate::coverage::SLICE_DIMENSIONS.iter())
        .find(|d| d.dimension == dim)
        .map_or_else(|| dim.local_name().to_owned(), |d| d.label.to_owned())
}

/// The documentation-health dashboard, a PURE projection of the emitted
/// `graph/documentation` incidence: per-dimension coverage of the vocabulary
/// surface, a completeness distribution, and the per-slice earned-maturity floor
/// with a gap-to-next-tier burn-down. Every coverage number is read back from
/// `gmeow:docCoversDimension` / `gmeow:coverageFraction` / `gmeow:docEarnedMaturity`
/// (never a second recompute from `crate::coverage`), so the dashboard and the
/// reasoned graph cannot silently disagree.
fn md_health(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_documentation_health"));
    line(
        &mut out,
        &format!(
            "Coverage of the {} documentation dimensions across **{}** vocabulary terms. Each \
             dimension mirrors a `docs/missing-*` lint code; the covered counts grow as source \
             prose, examples, scope notes, and external alignments land. Per-term detail lives \
             on each term page's *Documentation coverage* section.",
            crate::coverage::TermCoverage::TOTAL,
            model.terms.len()
        ),
    );

    let aligned = crate::coverage::alignment_subjects(model);

    // PURE PROJECTION: the health dashboard reads its coverage numbers back from
    // the emitted `graph/documentation` incidence (`gmeow:docCoversDimension` per
    // record), NEVER a second recompute from `crate::coverage`. The page and the
    // reasoned graph are therefore the same bytes read two ways — they cannot
    // silently diverge.
    let graph = crate::rdf::documentation_graph(model);
    let total = graph.terms.len();

    // Per-dimension coverage — covered count = the number of documented terms whose
    // projected incidence COVERS the dimension (the complement of the
    // `docs/missing-*` gap).
    heading(&mut out, 2, model.ui("body_coverage_by_dimension"));
    push_line(&mut out, "| Dimension | Covered | Total | % |");
    push_line(&mut out, "| --- | --- | --- | --- |");
    for dim in crate::coverage::DIMENSIONS.iter() {
        let local = dim.dimension.local_name();
        let covered = graph
            .terms
            .iter()
            .filter(|t| t.covers.contains(local))
            .count();
        let pct = (covered * 100).checked_div(total).unwrap_or(0);
        push_line(
            &mut out,
            &format!("| {} | {covered} | {total} | {pct}% |", dim.label),
        );
    }
    blank(&mut out);

    // Completeness distribution: how many terms carry exactly k of the dimensions,
    // counted from the projected per-term covered set.
    heading(&mut out, 2, model.ui("body_completeness_distribution"));
    push_line(&mut out, "| Dimensions present | Terms |");
    push_line(&mut out, "| --- | --- |");
    let dims_total = crate::coverage::TermCoverage::TOTAL;
    for k in (0..=dims_total).rev() {
        let count = graph
            .terms
            .iter()
            .filter(|t| present_dimension_count(t) == k)
            .count();
        push_line(&mut out, &format!("| {k} / {dims_total} | {count} |"));
    }
    blank(&mut out);

    // ── Maturity by slice (earned floor + gap-to-next-tier burn-down) ────────────
    // Projected from the per-slice incidence: `gmeow:docEarnedMaturity` (the FCA
    // floor), `gmeow:coverageFraction`, and any asserted `gmeow:sliceDocMaturity`.
    // The gap column names the exact dimensions standing between the slice and its
    // next tier — the burn-down a slice author reads straight off the dashboard.
    if !graph.slices.is_empty() {
        heading(&mut out, 2, model.ui("body_maturity_by_slice"));
        line(&mut out, model.ui("body_maturity_legend"));
        push_line(
            &mut out,
            "| Slice | Earns | Coverage | Claims | Gap to next tier |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- |");
        for slice in &graph.slices {
            let earned = slice
                .earned
                .as_deref()
                .and_then(crate::maturity::MaturityAnchor::from_local);
            let asserted = slice
                .asserted
                .as_deref()
                .and_then(crate::maturity::MaturityAnchor::from_local);
            let earned_label = earned.map_or("—".to_owned(), anchor_title);
            // The claim column flags an over-claim inline: a slice that asserts a
            // tier above the earned floor trips the `asserted ⊄ earned` gate.
            let claim_label = match asserted {
                None => "—".to_owned(),
                Some(a) => {
                    let over = crate::maturity::asserted_exceeds_earned(a, earned);
                    if over {
                        format!("⚠ {} (unsupported)", anchor_title(a))
                    } else {
                        anchor_title(a)
                    }
                }
            };
            let gap = maturity_gap(earned, &slice.covers);
            push_line(
                &mut out,
                &format!(
                    "| `{}` | {earned_label} | {}% | {claim_label} | {gap} |",
                    code_escape(local_name(&slice.documents)),
                    (slice.coverage_fraction * 100.0).round() as i64,
                ),
            );
        }
        blank(&mut out);
    }

    // ── Reasoning (present only when the native-reasoner verdict is attached) ────
    if let Some(verdict) = &model.reasoning {
        heading(&mut out, 2, model.ui("body_reasoning"));
        let classes = model
            .terms
            .iter()
            .filter(|t| t.category == DocTermCategory::Class)
            .count();
        let unsat = model
            .terms
            .iter()
            .filter(|t| {
                t.category == DocTermCategory::Class && verdict.unsatisfiable.contains(&t.iri)
            })
            .count();
        let consistency = if verdict.is_consistent {
            model.ui("body_reasoning_consistent")
        } else {
            model.ui("body_reasoning_inconsistent")
        };
        push_line(
            &mut out,
            &format!(
                "- Native DL reasoning: the ontology is {consistency}. **{unsat}** of {classes} \
                 documented classes are unsatisfiable; the rest are satisfiable.",
            ),
        );
        blank(&mut out);
    }

    // ── Coverage heatmap by slice (deterministic SVG, the shared color scale) ────
    heading(&mut out, 2, model.ui("body_coverage_by_slice"));
    line(&mut out, model.ui("body_health_heatmap_legend"));
    push_line(
        &mut out,
        &format!(
            "![Documentation coverage by slice]({}diagrams/coverage-heatmap.svg)",
            root_href(&Page::Health.dir())
        ),
    );
    blank(&mut out);

    // ── Linkage: alignment density + orphan terms ───────────────────────────────
    heading(&mut out, 2, model.ui("body_linkage"));
    let aligned_count = aligned.len();
    let orphan_count = model
        .terms
        .iter()
        .filter(|t| {
            t.parents.is_empty() && t.related_terms.is_empty() && !aligned.contains(t.iri.as_str())
        })
        .count();
    push_line(
        &mut out,
        &format!(
            "- **{}:** {aligned_count} of {total} terms ({}%) are the subject of at \
             least one external alignment.",
            model.ui("body_label_alignment_density"),
            (aligned_count * 100).checked_div(total).unwrap_or(0)
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- **{}:** {orphan_count} term(s) carry no parent, related term, or alignment.",
            model.ui("body_label_orphan_terms")
        ),
    );
    blank(&mut out);

    // ── Framework distribution (term count per logic:LogicalFramework) ──────────
    let mut framework_counts: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for term in &model.terms {
        for framework in &term.frameworks {
            *framework_counts.entry(framework.as_str()).or_default() += 1;
        }
    }
    if !framework_counts.is_empty() {
        heading(&mut out, 2, model.ui("body_framework_distribution"));
        push_line(&mut out, "| Framework | Terms |");
        push_line(&mut out, "| --- | --- |");
        for (framework, count) in &framework_counts {
            push_line(
                &mut out,
                &format!("| `{}` | {count} |", code_escape(framework)),
            );
        }
        blank(&mut out);
    }

    // ── Badge legend (rendered from the single badge color authority) ───────────
    heading(&mut out, 2, model.ui("body_badge_legend"));
    push_line(&mut out, "| Family | What it encodes |");
    push_line(&mut out, "| --- | --- |");
    for family in &crate::badge::FAMILIES {
        push_line(
            &mut out,
            &format!("| **{}** | {} |", family.label, family.description),
        );
    }
    blank(&mut out);

    out
}

fn md_getting_started(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_getting_started"));
    line(
        &mut out,
        "The GMEOW ontology is organized into self-contained *slices*. Each slice owns a \
         vocabulary module, optional SHACL shapes, mappings, queries, and tests, and declares \
         its manifest identity and dependencies.",
    );
    heading(&mut out, 2, model.ui("body_where_to_go_next"));
    let from = Page::GettingStarted.dir();
    push_line(
        &mut out,
        &format!(
            "- Browse the [slice index]({}index.md) to see every compilation unit.",
            rel(&from, &Page::SliceIndex.dir())
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- Browse [classes]({}index.md) and [properties]({}index.md) to explore the vocabulary.",
            rel(&from, &Page::Category(DocTermCategory::Class).dir()),
            rel(&from, &Page::Category(DocTermCategory::Property).dir())
        ),
    );
    blank(&mut out);
    out
}

fn md_about(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_about"));
    line(
        &mut out,
        &format!(
            "**{}** is generated directly from the slice catalog by the Rust `gmeow-docs` \
             renderer. Every page is a deterministic projection of the typed documentation \
             model — there is no hand-authored HTML.",
            md_escape(&model.title)
        ),
    );
    heading(&mut out, 2, model.ui("body_at_a_glance"));
    push_line(
        &mut out,
        &format!(
            "- Documentation model version: **{}**",
            md_escape(&model.version)
        ),
    );
    push_line(&mut out, &format!("- Slices: **{}**", model.slices.len()));
    push_line(
        &mut out,
        &format!("- Vocabulary terms: **{}**", model.terms.len()),
    );
    push_line(
        &mut out,
        &format!(
            "- Cross-slice dependency edges: **{}**",
            model.dependency_edges.len()
        ),
    );
    blank(&mut out);
    out
}

/// A total-order sort key for a version string: the parsed `(major, minor,
/// patch)` numeric triple when the string is dotted-numeric, else a sentinel that
/// sorts unparsable versions last; the original string is the final tiebreak so
/// the order is total and deterministic (never a branchy comparator).
fn version_sort_key(version: &str) -> ((u64, u64, u64, u8), String) {
    let mut parts = version.split('.');
    let mut next = || parts.next().and_then(|p| p.parse::<u64>().ok());
    match (next(), next(), next()) {
        (Some(major), Some(minor), Some(patch)) if parts.next().is_none() => {
            ((major, minor, patch, 0), version.to_string())
        }
        // Unparsable / non-triple versions sort after every semver triple (the
        // trailing sentinel byte = 1), ordered among themselves by their string.
        _ => ((u64::MAX, u64::MAX, u64::MAX, 1), version.to_string()),
    }
}

/// The changelog surface, derived from the per-term content-address provenance:
/// every term's `added_in_version` seeds an "Added" row under that release, and
/// every reified changelog entry (a content-digest divergence) a "Changed" row.
/// Releases are listed newest-first; within a release, terms are CURIE-sorted.
fn md_changelog(model: &DocsModel) -> String {
    let from = Page::Changelog.dir();
    // release version → (added terms, changed (term, note) rows). BTreeMap keeps
    // the collection deterministic; the explicit version_sort_key drives display.
    let mut added: BTreeMap<String, Vec<&DocTerm>> = BTreeMap::new();
    let mut changed: BTreeMap<String, Vec<(&DocTerm, Option<String>)>> = BTreeMap::new();
    for term in &model.terms {
        if let Some(version) = &term.added_in_version {
            added.entry(version.clone()).or_default().push(term);
        }
        for entry in &term.changelog {
            changed
                .entry(entry.version.clone())
                .or_default()
                .push((term, entry.note.clone()));
        }
    }

    // Every release that carries either an addition or a change, newest-first.
    let mut versions: Vec<String> = added.keys().chain(changed.keys()).cloned().collect();
    versions.sort_by_key(|v| std::cmp::Reverse(version_sort_key(v)));
    versions.dedup();

    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_changelog"));
    line(
        &mut out,
        "Each release below is derived from the per-term content-address manifest: a term is \
         listed under the release it was first seen in, and again whenever its canonical \
         definition digest changed.",
    );

    let term_link = |from: &str, term: &DocTerm| {
        format!(
            "[`{}`]({}index.md){}",
            code_escape(&term.curie),
            rel(from, &Page::Term(term_slug(term)).dir()),
            label_suffix(term)
        )
    };

    for version in &versions {
        heading(&mut out, 2, &md_escape(version));
        if let Some(terms) = added.get(version) {
            let mut terms = terms.clone();
            terms.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
            heading(&mut out, 3, model.ui("body_changelog_added"));
            for term in terms {
                push_line(&mut out, &format!("- {}", term_link(&from, term)));
            }
            blank(&mut out);
        }
        if let Some(rows) = changed.get(version) {
            let mut rows = rows.clone();
            rows.sort_by(|a, b| {
                a.0.curie
                    .cmp(&b.0.curie)
                    .then_with(|| a.0.iri.cmp(&b.0.iri))
            });
            heading(&mut out, 3, model.ui("body_changelog_changed"));
            for (term, note) in rows {
                match note {
                    Some(note) => push_line(
                        &mut out,
                        &format!("- {} — {}", term_link(&from, term), md_escape(&note)),
                    ),
                    None => push_line(&mut out, &format!("- {}", term_link(&from, term))),
                }
            }
            blank(&mut out);
        }
    }
    blank(&mut out);
    out
}

fn md_category(model: &DocsModel, category: DocTermCategory) -> String {
    let from = Page::Category(category).dir();
    let mut terms: Vec<&DocTerm> = model
        .terms
        .iter()
        .filter(|t| t.category == category)
        .collect();
    terms.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));

    let mut out = String::new();
    heading(&mut out, 1, category_plural(category));
    line(&mut out, &format!("{} term(s).", terms.len()));

    push_line(&mut out, "| Term | Definition |");
    push_line(&mut out, "| --- | --- |");
    for term in &terms {
        let href = rel(&from, &Page::Term(term_slug(term)).dir());
        let def = term.definition.as_deref().map(one_line).unwrap_or_default();
        push_line(
            &mut out,
            &format!(
                "| [`{}`]({}index.md){} | {} |",
                code_escape(&term.curie),
                href,
                label_suffix(term),
                md_escape(&def)
            ),
        );
    }
    blank(&mut out);
    out
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

    if props.is_empty() {
        return format!("<subject> a {} .\n", term.curie);
    }

    let cap = props.len().min(QUICKSTART_PROPERTY_CAP);
    let mut out = format!("<subject> a {} ;\n", term.curie);
    for (i, prop) in props.iter().take(cap).enumerate() {
        let terminator = if i + 1 == cap { '.' } else { ';' };
        let (object, comment) = quickstart_placeholder(model, &prop.range);
        out.push_str(&format!(
            "    {} {object} {terminator}{comment}\n",
            prop.curie
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
        Some(d) => quickstart_label(model, d),
        None => "owl:Thing".to_string(),
    };
    let (object, comment) = quickstart_placeholder(model, &term.range);
    format!(
        "<subject> a {subject_type} ;\n    {} {object} .{comment}\n",
        term.curie
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

/// Standard Base64 (RFC 4648, `+`/`/`, `=` padding) — used to carry a fixture's
/// Turtle verbatim inside a `data-` attribute for the in-browser "run validation"
/// control, so newlines/quotes/`<` in the RDF never break the HTML. Small, dep-free,
/// and deterministic.
fn base64_encode(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            A[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn md_term(model: &DocsModel, slug: &str, exec: &ExecutableDocsData) -> String {
    let Some(term) = model.terms.iter().find(|t| term_slug(t) == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_term_not_found"));
        return out;
    };
    let from = Page::Term(slug.to_string()).dir();

    let mut out = String::new();
    let title = term.label.clone().unwrap_or_else(|| term.curie.clone());
    heading(&mut out, 1, &title);
    line(
        &mut out,
        &format!(
            "`{}` · {}",
            code_escape(&term.curie),
            category_singular(term.category)
        ),
    );

    push_line(&mut out, "| Field | Value |");
    push_line(&mut out, "| --- | --- |");
    push_line(
        &mut out,
        &format!("| CURIE | `{}` |", code_escape(&term.curie)),
    );
    push_line(&mut out, &format!("| IRI | `{}` |", code_escape(&term.iri)));
    push_line(
        &mut out,
        &format!(
            "| Category | {} |",
            md_escape(category_singular(term.category))
        ),
    );
    push_line(
        &mut out,
        &format!(
            "| Slice | {} |",
            slice_link(model, &from, &term.owner_slice)
        ),
    );
    blank(&mut out);

    // ── Badges (the term's small-cardinality categories as color-coded SVGs) ─────
    // A visual summary row: completeness, stability, category, then every box
    // role, logic stereotype, and framework. Each image points at a shared asset
    // emitted in `render_site_lang` from this same source (no dangling path); the
    // detailed, linkable surfaces follow in their own sections below.
    {
        let ctx = crate::coverage::CoverageContext::new(model);
        let badges = crate::badge::term_badges(term, &ctx, model.reasoning.as_ref());
        let row = badges
            .iter()
            .map(|b| {
                format!(
                    "![{}]({}{})",
                    md_escape(&format!("{}: {}", b.family, b.label)),
                    root_href(&from),
                    crate::badge::badge_path(b)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        push_line(&mut out, &row);
        blank(&mut out);
    }

    if let Some(def) = &term.definition {
        heading(&mut out, 2, model.ui("body_definition"));
        line(&mut out, &md_escape(def));
    }

    // ── Progressive disclosure: the DEVELOPER surface first (how to USE the
    // term — quickstart, multi-syntax examples, conformance Do/Don't, diagnostics),
    // then the ACADEMIC surface below (the formal axioms, derivation provenance,
    // projection fidelity, and citation). Every prior section is preserved; the two
    // tiers only reorder them and add the multi-syntax tabs + DL axioms.
    out.push_str(&term_developer_surface(model, term, &from, exec));
    out.push_str(&term_academic_surface(model, term, &from, exec));

    // Enriched build-pipeline stage section (only when the term IS a stage).
    append_stage_section(&mut out, model, term, &from);

    out
}

/// The DEVELOPER-facing tier of the term page: everything a consumer needs to
/// pick up and USE the term. Leads with the synthesized quickstart and the
/// multi-syntax example tabs (Turtle / JSON-LD / JSON Schema / OpenAPI), then the
/// shape (super-classes, domain, range, SHACL constraints), the conformance
/// Do/Don't pairs, the diagnostics the term might trip, and the advisory / usage
/// context. Returns a Markdown fragment; `from` is the term page's site-relative
/// directory (rebound to an owned `String` so every moved block reads `&from`
/// unchanged).
fn term_developer_surface(
    model: &DocsModel,
    term: &DocTerm,
    from: &str,
    exec: &ExecutableDocsData,
) -> String {
    let from = from.to_string();
    let mut out = String::new();
    heading(&mut out, 2, model.ui("body_developer_surface"));

    // ── Quickstart (synthesized Turtle skeleton + playground link) ─────────────
    // A pure render of the term's OWN domain/range shape — always present (every
    // term renders some skeleton, or an honest "no skeleton" comment), so a
    // reader gets a copy-paste starting point before reading anything else.
    {
        heading(&mut out, 2, model.ui("body_quickstart"));
        let skeleton = synthesize_quickstart(model, term);
        fenced(&mut out, "turtle", skeleton.trim_end());
        // The `Page::SparqlPlayground` page is emitted only when the pipeline
        // attaches a playground asset (`exec.has_playground()`) — a model-only
        // render never emits `sparql/index.md`, so the link is gated exactly like
        // the sibling "Export" affordance (`append_term_export_section`) to keep
        // the no-dangling-internal-link invariant (`lint::clean_site_has_zero_errors`).
        if exec.has_playground() {
            let playground_href = rel(&from, &Page::SparqlPlayground.dir());
            push_line(
                &mut out,
                &format!(
                    "[Try it in the SPARQL playground]({playground_href}index.md) with \
                     `DESCRIBE <{}>`.",
                    code_escape(&term.iri)
                ),
            );
        }
        blank(&mut out);
    }

    // ── The same example, in several concrete syntaxes (the SyntaxTabProvider
    // seam): Turtle, JSON-LD, and — when the schema digest is attached — the
    // per-term JSON Schema + OpenAPI fragments ("use this term without RDF").
    append_syntax_tabs(&mut out, model, term);

    // ── Neighbourhood diagram (the term and its 1-hop relations) ────────────────
    // Gated on the identical predicate as the emission loop in `render_site_lang`
    // so the embedded path always resolves (preserves no-dangling-link).
    if svg::term_has_neighbourhood(term) {
        heading(&mut out, 2, model.ui("body_neighborhood"));
        push_line(
            &mut out,
            &format!(
                "![Term neighborhood]({}diagrams/terms/{}.svg)",
                root_href(&from),
                term_slug(term)
            ),
        );
        blank(&mut out);
    }

    if !term.parents.is_empty() {
        let label = match term.category {
            DocTermCategory::Property => model.ui("body_super_properties"),
            _ => model.ui("body_super_classes"),
        };
        heading(&mut out, 2, label);
        for parent in &term.parents {
            push_line(&mut out, &format!("- {}", term_link(model, &from, parent)));
        }
        blank(&mut out);
    }

    if !term.domain.is_empty() {
        heading(&mut out, 2, model.ui("body_domain"));
        for d in &term.domain {
            push_line(&mut out, &format!("- {}", term_link(model, &from, d)));
        }
        blank(&mut out);
    }

    if !term.range.is_empty() {
        heading(&mut out, 2, model.ui("body_range"));
        for r in &term.range {
            push_line(&mut out, &format!("- {}", term_link(model, &from, r)));
        }
        blank(&mut out);
    }

    // ── Constraints (SHACL node shapes — DISTINCT from the verify-query index) ───
    let constraints: Vec<&str> = model
        .shapes
        .iter()
        .filter(|s| s.target_term == term.iri)
        .flat_map(|s| s.messages.iter().map(String::as_str))
        .collect();
    let mut constraint_msgs: Vec<&str> = constraints;
    constraint_msgs.sort_unstable();
    constraint_msgs.dedup();
    if !constraint_msgs.is_empty() {
        heading(&mut out, 2, model.ui("body_constraints"));
        for msg in constraint_msgs {
            push_line(&mut out, &format!("- {}", md_escape(msg)));
        }
        blank(&mut out);
    }

    // ── Conformance examples (Do/Don't fixtures referencing this term) ──────────
    let mut term_fixtures: Vec<&crate::model::DocFixture> = model
        .fixtures
        .iter()
        .filter(|f| f.terms_referenced.iter().any(|c| c == &term.curie))
        .collect();
    term_fixtures.sort_by(|a, b| {
        a.slice
            .cmp(&b.slice)
            .then_with(|| a.logical_path.cmp(&b.logical_path))
    });
    if !term_fixtures.is_empty() {
        heading(&mut out, 2, model.ui("body_conformance_examples"));
        for fixture in &term_fixtures {
            let label = match fixture.kind {
                crate::model::DocFixtureKind::Wellformed => model.ui("body_label_do"),
                crate::model::DocFixtureKind::CounterExample => model.ui("body_label_dont"),
            };
            push_line(
                &mut out,
                &format!("- **{label}:** `{}`", code_escape(&fixture.logical_path)),
            );
            push_fixture_binding_bullets(&mut out, model, &from, fixture);
            // Live "run validation" control (W1): a counter-example is meant to
            // VIOLATE, so the reader can run the REAL Tier-1 validator in-browser
            // over it and see the actual unified-diagnostics findings (each linking
            // through its helpUri into the constraint catalog). Only when the browser
            // validate asset + bundle ship (`has_bundle()`); the fixture Turtle rides
            // base64 in a data-attribute so its RDF never breaks the HTML.
            if exec.has_bundle()
                && matches!(fixture.kind, crate::model::DocFixtureKind::CounterExample)
            {
                blank(&mut out);
                push_line(
                    &mut out,
                    &format!(
                        "<div class=\"gmeow-validation\"><button class=\"gmeow-run-validation\" \
                         data-turtle=\"{}\" data-origin=\"{}\" data-catalog-href=\"{}index.html\">{}</button>\
                         <div class=\"gmeow-validation-results\"></div></div>",
                        base64_encode(fixture.text.as_bytes()),
                        fixture
                            .logical_path
                            .replace('&', "&amp;")
                            .replace('"', "&quot;"),
                        rel(&from, &Page::ConstraintCatalog.dir()),
                        model.ui("body_run_validation"),
                    ),
                );
                blank(&mut out);
            }
        }
        let fixture_index_href = rel(&from, &Page::FixtureIndex.dir());
        push_line(
            &mut out,
            &format!("- See the [conformance fixtures index]({fixture_index_href}index.md)."),
        );
        blank(&mut out);
    }

    // ── Diagnostics you might hit (present only when a diagnostics digest is attached)
    // Live counts + code list joined from `stage-validate` + `stage-compile-logic` (B1);
    // a term with a genuine `by_term` entry lists each finding (deep-linked into the
    // constraint catalog ONLY when a real `help_uri` resolved); an attached-but-empty
    // digest renders the honest "no diagnostics" line rather than omitting the section —
    // never conflated with the section being absent because no digest was attached at all.
    if let Some(digest) = &model.diagnostics {
        heading(&mut out, 2, model.ui("body_diagnostics_you_might_hit"));
        match digest.by_term.get(&term.iri) {
            Some(findings) if !findings.is_empty() => {
                let mut by_severity: BTreeMap<&str, usize> = BTreeMap::new();
                for finding in findings {
                    *by_severity.entry(finding.severity.as_str()).or_default() += 1;
                }
                let counts = by_severity
                    .iter()
                    .map(|(severity, count)| format!("{count} {severity}"))
                    .collect::<Vec<_>>()
                    .join(" · ");
                push_line(
                    &mut out,
                    &format!("- **{}:** {counts}", model.ui("body_label_severity")),
                );
                for finding in findings {
                    let code_display = match &finding.help_uri {
                        // The display text is md-escaped; the link *target* is the raw
                        // URL (escaping a target corrupts the href).
                        Some(uri) => format!("[`{}`]({uri})", code_escape(&finding.code)),
                        None => format!("`{}`", code_escape(&finding.code)),
                    };
                    push_line(
                        &mut out,
                        &format!(
                            "- {code_display} ({}): {}",
                            md_escape(&finding.category),
                            md_escape(&finding.message)
                        ),
                    );
                }
            }
            _ => {
                push_line(
                    &mut out,
                    &format!("- {}", model.ui("body_diagnostics_none")),
                );
            }
        }
        blank(&mut out);
    }

    // ── Usage Advice (per-term advisory metadata) ───────────────────────────────
    // Field order mirrors the retired Python `_append_usage_advice`: prose first,
    // then consumer-profile guidance. The whole section is suppressed when empty.
    let advice_text: [(&str, &[String]); 5] = [
        (model.ui("body_advice_scope"), &term.scope_notes),
        (model.ui("body_advice_example"), &term.examples),
        (model.ui("body_advice_use_when"), &term.use_when),
        (model.ui("body_advice_avoid_when"), &term.avoid_when),
        (model.ui("body_advice_how_to_use"), &term.how_to_use),
    ];
    let advice_consumer: [(&str, &[String]); 2] = [
        (
            model.ui("body_advice_use_for_consumers"),
            &term.use_for_consumer,
        ),
        (
            model.ui("body_advice_avoid_for_consumers"),
            &term.avoid_for_consumer,
        ),
    ];
    let has_advice = advice_text.iter().any(|(_, v)| !v.is_empty())
        || advice_consumer.iter().any(|(_, v)| !v.is_empty());
    if has_advice {
        heading(&mut out, 2, model.ui("body_usage_advice"));
        for (label, values) in advice_text {
            if !values.is_empty() {
                let joined = values
                    .iter()
                    .map(|v| md_escape(v))
                    .collect::<Vec<_>>()
                    .join(" ");
                push_line(&mut out, &format!("- **{label}:** {joined}"));
            }
        }
        for (label, values) in advice_consumer {
            if !values.is_empty() {
                let joined = values
                    .iter()
                    .map(|c| consumer_link(model, &from, c))
                    .collect::<Vec<_>>()
                    .join(", ");
                push_line(&mut out, &format!("- **{label}:** {joined}"));
            }
        }
        blank(&mut out);
    }

    // ── Related terms (bidirectional: skos:related / pairsWith / seeAlso) ────────
    if !term.related_terms.is_empty() {
        heading(&mut out, 2, model.ui("body_related_terms"));
        for related in &term.related_terms {
            push_line(&mut out, &format!("- {}", term_link(model, &from, related)));
        }
        blank(&mut out);
    }

    // ── Examples using this term (cross-links to the full source on slice pages) ─
    let mut term_examples: Vec<&crate::model::DocExample> = model
        .examples
        .iter()
        .filter(|e| e.terms_referenced.iter().any(|c| c == &term.curie))
        .collect();
    term_examples.sort_by(|a, b| {
        a.slice
            .cmp(&b.slice)
            .then_with(|| a.logical_path.cmp(&b.logical_path))
    });
    if !term_examples.is_empty() {
        heading(&mut out, 2, model.ui("body_examples_using_this_term"));
        for example in term_examples {
            let slice_href = model
                .slices
                .iter()
                .find(|s| s.iri == example.slice)
                .map(|s| rel(&from, &Page::Slice(slice_slug(s)).dir()));
            match slice_href {
                Some(href) => push_line(
                    &mut out,
                    &format!(
                        "- [{}]({}index.md) — `{}`",
                        md_escape(&example.title),
                        href,
                        code_escape(&example.logical_path)
                    ),
                ),
                None => push_line(
                    &mut out,
                    &format!(
                        "- {} — `{}`",
                        md_escape(&example.title),
                        code_escape(&example.logical_path)
                    ),
                ),
            }
        }
        blank(&mut out);
    }

    // ── Notation grammars (whole-slice cross-link, see GMEOW_LANG_SLICE_IRI) ────
    if term.owner_slice == GMEOW_LANG_SLICE_IRI && !model.grammars.is_empty() {
        heading(&mut out, 2, model.ui("body_notation_grammars"));
        let notation_index_href = rel(&from, &Page::NotationIndex.dir());
        push_line(
            &mut out,
            &format!(
                "- See the [notation grammars index]({notation_index_href}index.md) for the \
                 GMN / GTS / Turtle W3C EBNF surface-syntax exhibits."
            ),
        );
        blank(&mut out);
    }

    // ── Profiles (named profiles whose membership includes this term) ─────────────
    if !term.profiles.is_empty() {
        heading(&mut out, 2, model.ui("body_profiles"));
        let chips = term
            .profiles
            .iter()
            .map(|p| format!("`{}`", code_escape(p)))
            .collect::<Vec<_>>()
            .join(" · ");
        push_line(&mut out, &format!("- {chips}"));
        blank(&mut out);
    }

    out
}

/// The ACADEMIC-facing tier of the term page: the formal structure and its
/// provenance. Leads with the Description-Logic axioms the term genuinely
/// carries, then the cross-vocabulary alignments and their preservation caveats,
/// the lowered-logic disciplines, the reasoning verdict and the reasoner-derived
/// "inferred facts" / "unsatisfiable because" panels, the per-term
/// projection-fidelity table, coverage/stability/changelog metadata, and finally
/// the content-addressed cite-this-term citation. Returns a Markdown fragment.
fn term_academic_surface(
    model: &DocsModel,
    term: &DocTerm,
    from: &str,
    exec: &ExecutableDocsData,
) -> String {
    let from = from.to_string();
    let mut out = String::new();
    heading(&mut out, 2, model.ui("body_academic_surface"));

    // ── Description-Logic axioms (the formal reading of the asserted structure) ──
    // Only the axioms the model GENUINELY carries: subsumption from each parent,
    // and — for a property — the standard domain/range axioms. Never a fabricated
    // value restriction or intersection (see `dl_axioms`).
    {
        let axioms = dl_axioms(model, term);
        if !axioms.is_empty() {
            heading(&mut out, 2, model.ui("body_dl_axioms"));
            fenced(&mut out, "text", &axioms.join("\n"));
        }
    }

    // ── Alignments (per-term cross-walks projected from the slice mappings) ──────
    let mut aligns: Vec<&crate::model::DocLinkage> = model
        .linkages
        .iter()
        .filter(|l| l.subject == term.iri)
        .collect();
    aligns.sort_by(|a, b| {
        a.predicate
            .cmp(&b.predicate)
            .then_with(|| a.object.cmp(&b.object))
    });
    if !aligns.is_empty() {
        heading(&mut out, 2, model.ui("body_alignments"));
        let mut any_lossy = false;
        for link in aligns {
            let tag = code_escape(&align_tag(&link.predicate));
            let target = term_link(model, &from, &link.object);
            match approximate_match_note(model, &link.predicate) {
                Some(note) => {
                    any_lossy = true;
                    push_line(&mut out, &format!("- `{tag}` → {target} — *{note}.*"));
                }
                None => push_line(&mut out, &format!("- `{tag}` → {target}")),
            }
        }
        let ledger_href = rel(&from, &Page::LogicLossLedger.dir());
        // One section-level disclosure when any crosswalk is an approximate
        // (lossy) SKOS match, cross-linking the preservation loss ledger that
        // records the per-target structural drops. The prose halves resolve through
        // the UI-chrome catalog; the link target is the language-independent path.
        if any_lossy {
            push_line(
                &mut out,
                &format!(
                    "- *{}({ledger_href}index.md) {}*",
                    model.ui("body_caveat_disclosure_pre"),
                    model.ui("body_caveat_disclosure_post"),
                ),
            );
        }
        // Even an EXACT SKOS/OWL match is a lossy projection once it is LOWERED to
        // the EDOAL / FnO alignment formats — those targets under-approximate the
        // canonical correspondence (they drop the SOL caveats + preservation
        // judgment). Disclose that per-term whenever the term has any crosswalk and
        // the loss ledger declares those lowerings lossy (sourced from the ledger,
        // never hardcoded — an exact EDOAL/FnO row would suppress this note).
        if edoal_fno_lowering_is_lossy() {
            push_line(
                &mut out,
                &format!(
                    "- *{}({ledger_href}index.md) {}*",
                    model.ui("body_caveat_edoal_fno_pre"),
                    model.ui("body_caveat_disclosure_post"),
                ),
            );
        }
        blank(&mut out);
    }

    // ── Logic stereotypes (lowered OntoUML/UFO discipline) ──────────────────────
    if !term.logic_stereotypes.is_empty() {
        heading(&mut out, 2, model.ui("body_logic_stereotypes"));
        let badges = term
            .logic_stereotypes
            .iter()
            .map(|s| format!("`{}`", code_escape(s)))
            .collect::<Vec<_>>()
            .join(" · ");
        let logic_href = rel(&from, &Page::Logic.dir());
        line(
            &mut out,
            &format!("{badges} — see the [Logic & Reasoning]({logic_href}index.md) index."),
        );
    }

    // ── Frameworks (the logical disciplines the term traffics in) ───────────────
    // Rendered as `logic:`-prefixed chips linking the Logic & Reasoning index — the
    // per-term counterpart of the logic-stereotype chips (the framework individuals
    // live in the logic: vocabulary, which is documented via that index).
    if !term.frameworks.is_empty() {
        heading(&mut out, 2, model.ui("body_frameworks"));
        let chips = term
            .frameworks
            .iter()
            .map(|f| format!("`{}`", code_escape(f)))
            .collect::<Vec<_>>()
            .join(" · ");
        let logic_href = rel(&from, &Page::Logic.dir());
        line(
            &mut out,
            &format!("{chips} — see the [Logic & Reasoning]({logic_href}index.md) index."),
        );
    }

    // ── Box role badge (links the four-boxes doctrine when that page exists) ─────
    if let Some(role) = &term.box_role {
        heading(&mut out, 2, model.ui("body_box_role"));
        let label = box_role_label(role);
        if model.four_boxes.is_some() {
            let href = rel(&from, &Page::FourBoxes.dir());
            line(
                &mut out,
                &format!(
                    "[{} (`{}`)]({}index.md)",
                    md_escape(&label),
                    code_escape(role),
                    href
                ),
            );
        } else {
            line(
                &mut out,
                &format!("{} (`{}`)", md_escape(&label), code_escape(role)),
            );
        }
    }

    // ── Formalized by (reverse logic:formalizes back-refs) ──────────────────────
    if !term.formalized_by.is_empty() {
        heading(&mut out, 2, model.ui("body_formalized_by"));
        for subject in &term.formalized_by {
            push_line(&mut out, &format!("- {}", term_link(model, &from, subject)));
        }
        blank(&mut out);
    }

    // ── Tested by (competency questions that exercise this term) ─────────────────
    let mut tested_by: Vec<&crate::model::DocCompetency> = model
        .competencies
        .iter()
        .filter(|c| c.exercises.iter().any(|t| t == &term.iri))
        .collect();
    tested_by.sort_by(|a, b| a.iri.cmp(&b.iri));
    if !tested_by.is_empty() {
        heading(&mut out, 2, model.ui("body_tested_by"));
        for cq in tested_by {
            let rationale = cq
                .rationale
                .as_deref()
                .map(one_line)
                .unwrap_or_else(|| local_name(&cq.iri).to_string());
            match &cq.query_file {
                Some(qf) => push_line(
                    &mut out,
                    &format!("- {} (`{}`)", md_escape(&rationale), code_escape(qf)),
                ),
                None => push_line(&mut out, &format!("- {}", md_escape(&rationale))),
            }
        }
        let competency_index_href = rel(&from, &Page::CompetencyIndex.dir());
        push_line(
            &mut out,
            &format!("- See the [competency questions index]({competency_index_href}index.md)."),
        );
        blank(&mut out);
    }

    // ── Reasoning status (present only when the native reasoner verdict is attached)
    // The textual, accessible counterpart of the reasoning badge: a class is
    // satisfiable unless the native DL reasoner proved it unsatisfiable; a
    // non-class term is not-evaluated (satisfiability is a class notion). Never
    // rendered for a source-only model, so no satisfiability claim is fabricated.
    if let Some(verdict) = &model.reasoning {
        heading(&mut out, 2, model.ui("body_reasoning_status"));
        let unsatisfiable =
            term.category == DocTermCategory::Class && verdict.unsatisfiable.contains(&term.iri);
        let status = if term.category == DocTermCategory::Class {
            if unsatisfiable {
                model.ui("body_reasoning_unsatisfiable")
            } else {
                model.ui("body_reasoning_satisfiable")
            }
        } else {
            model.ui("body_reasoning_not_evaluated")
        };
        push_line(&mut out, &format!("- {status}"));
        // ── Unsatisfiable because (B3): the derivation chain(s) proving this
        // class necessarily empty, read off the SAME per-term entailment panel
        // "Inferred facts" reads below — never a second join. A term the
        // reasoner proved unsatisfiable but whose witnessing derivation is not
        // (yet) attached renders no extra lines here, rather than a fabricated
        // "because" claim.
        if unsatisfiable {
            let because: Vec<&crate::exec::Entailment> = exec
                .term_entailments
                .get(&term.iri)
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|e| is_unsatisfiable_conclusion(&e.conclusion))
                        .collect()
                })
                .unwrap_or_default();
            if !because.is_empty() {
                push_line(
                    &mut out,
                    &format!(
                        "- **{}:**",
                        model.ui("body_reasoning_unsatisfiable_because")
                    ),
                );
                for entailment in because {
                    push_line(
                        &mut out,
                        &format!(
                            "  - `{}` (via `{}`)",
                            code_escape(&entailment.conclusion),
                            code_escape(&entailment.rule)
                        ),
                    );
                }
            }
        }
        blank(&mut out);
    }

    // ── Inferred facts (present only when the pipeline attached B3 entailment data
    // AND this term has a matching derivation) ─────────────────────────────────
    // The reasoner-derived "why" panel: for a documented term appearing in the
    // subject/predicate/object position of a materialized derivation's conclusion
    // or premises, list the firing rule, the concluded axiom, and its premises.
    // `exec.term_entailments` is empty in a model-only render (the genuine
    // `ExecutableDocsData` layering seam — see `exec.rs`), so the panel is simply
    // absent there, never a fabricated "no entailments" claim.
    if let Some(entailments) = exec.term_entailments.get(&term.iri) {
        heading(&mut out, 2, model.ui("body_term_entailments"));
        for entailment in entailments {
            push_line(
                &mut out,
                &format!(
                    "- **{}** ⟹ `{}`",
                    code_escape(&entailment.rule),
                    code_escape(&entailment.conclusion)
                ),
            );
            for premise in &entailment.premises {
                push_line(&mut out, &format!("  - via `{}`", code_escape(premise)));
            }
        }
        blank(&mut out);
    }

    // ── How this term degrades under projection (present only when the dynamic
    // per-term loss digest is attached) ──────────────────────────────────────────
    // The dynamic per-shape join (B2): DISTINCT from the static whole-program rows
    // on `Page::LogicLossLedger` (`owl-dl`, `datalog`, …) — this surfaces ONLY the
    // `property-path:<shape-iri>` rows the live `stage-mappings` projection ledger
    // emits for a `logic:PathShape` this term owns. An attached-but-empty join
    // renders the honest "carried exactly by every projection" text rather than
    // omitting the section — never conflated with the section being absent
    // because no digest was attached at all.
    if let Some(digest) = &model.term_loss {
        heading(&mut out, 2, model.ui("body_term_projection_degradation"));
        match digest.by_term.get(&term.iri) {
            Some(rows) if !rows.is_empty() => {
                push_line(
                    &mut out,
                    "| Target | Preservation kind | Complexity class | Lossy drops |",
                );
                push_line(&mut out, "| --- | --- | --- | --- |");
                for row in rows {
                    let drops = if row.lossy_drops.is_empty() {
                        "—".to_string()
                    } else {
                        row.lossy_drops
                            .iter()
                            .map(|d| md_escape(&one_line(d)))
                            .collect::<Vec<_>>()
                            .join("<br>")
                    };
                    push_line(
                        &mut out,
                        &format!(
                            "| `{}` | `{}` | `{}` | {} |",
                            code_escape(&row.target),
                            code_escape(&row.preservation_kind),
                            code_escape(&row.complexity_class),
                            drops,
                        ),
                    );
                }
            }
            _ => {
                push_line(
                    &mut out,
                    &format!("- {}", model.ui("body_term_projection_degradation_none")),
                );
            }
        }
        blank(&mut out);
    }

    // ── Documentation coverage (always present) ──────────────────────────────────
    // The six richness dimensions this term carries, read from the shared coverage
    // source — exactly the predicates behind the `docs/missing-*` lint, so the page
    // and the gate can never disagree about what a term is missing.
    {
        let ctx = crate::coverage::CoverageContext::new(model);
        let cov = crate::coverage::term_coverage(term, &ctx);
        heading(&mut out, 2, model.ui("body_documentation_coverage"));
        let badges = crate::coverage::DIMENSIONS
            .iter()
            .zip(cov.flags())
            .map(|(dim, present)| format!("{} {}", if present { "✓" } else { "✗" }, dim.label))
            .collect::<Vec<_>>()
            .join(" · ");
        push_line(
            &mut out,
            &format!(
                "- **{} of {} dimensions present.** {}",
                cov.present_count(),
                crate::coverage::TermCoverage::TOTAL,
                badges
            ),
        );
        push_line(
            &mut out,
            &format!(
                "- See [Documentation health]({}index.md) for coverage across the whole vocabulary.",
                rel(&Page::Term(term_slug(term)).dir(), &Page::Health.dir())
            ),
        );
        blank(&mut out);
    }

    // ── Stability (always present; tier-derived default or explicit) ──────────────
    heading(&mut out, 2, model.ui("body_stability"));
    push_line(
        &mut out,
        &format!(
            "- **{}:** {}",
            model.ui("body_label_status"),
            term.stability.label()
        ),
    );
    blank(&mut out);

    // ── Changelog (added-in version + reified per-release entries) ──────────────
    if term.added_in_version.is_some() || !term.changelog.is_empty() {
        heading(&mut out, 2, model.ui("body_changelog"));
        if let Some(version) = &term.added_in_version {
            push_line(
                &mut out,
                &format!(
                    "- **{}:** {}",
                    model.ui("body_label_added_in"),
                    md_escape(version)
                ),
            );
        }
        for entry in &term.changelog {
            match &entry.note {
                Some(note) => push_line(
                    &mut out,
                    &format!("- **{}** — {}", md_escape(&entry.version), md_escape(note)),
                ),
                None => push_line(&mut out, &format!("- **{}**", md_escape(&entry.version))),
            }
        }
        blank(&mut out);
    }

    // ── Citation (permalink + genuine content address + cite-this affordance) ───
    // The term IRI is the dereferenceable permalink. The content address is the
    // RDFC-1.0 canonical digest of the term's defining triples (gmeow:definitionDigest),
    // so `<iri>@<digest>` pins the exact definition this page describes. The concept
    // DOI (read from metadata/gmeow-self.ttl) cites the whole ontology; the owner
    // slice's identifier cites the slice when one is registered. This is the
    // per-term instance of the page-level cite-this-surface (`append_cite_this_surface`).
    heading(&mut out, 2, model.ui("body_citation"));
    push_line(
        &mut out,
        &format!("- **{}:** <{}>", model.ui("body_label_permalink"), term.iri),
    );
    if !term.content_digest.is_empty() {
        push_line(
            &mut out,
            &format!(
                "- **{}:** `{}@{}`",
                model.ui("body_label_content_address"),
                term.iri,
                code_escape(&term.content_digest)
            ),
        );
    }
    if let Some(doi) = &model.concept_doi {
        push_line(
            &mut out,
            &format!(
                "- **{}:** [{}](https://doi.org/{})",
                model.ui("body_label_cite_ontology"),
                md_escape(doi),
                doi
            ),
        );
    }
    if let Some(identifier) = model
        .slices
        .iter()
        .find(|s| s.iri == term.owner_slice)
        .and_then(|s| s.identifier.as_ref())
    {
        push_line(
            &mut out,
            &format!(
                "- **{}:** {}",
                model.ui("body_label_cite_slice"),
                md_escape(identifier)
            ),
        );
    }
    blank(&mut out);

    out
}

// ── Multi-syntax example tabs (the concrete `SyntaxTabProvider` OCP seam) ─────
//
// A term's example is rendered in several concrete serializations. Each is
// produced by a `SyntaxTabProvider`: a PURE `fn(&DocTerm, &DocsModel) ->
// Option<DocSyntaxTab>` yielding one labeled, language-tagged code block, or
// `None` when this term carries no honest content for that syntax (the tab is
// then simply absent — the seam's honest layering, never a fabricated
// placeholder). The providers run in the fixed order of `SYNTAX_TAB_PROVIDERS`.
//
// EXTENSION POINT (append-only): a sibling issue adding a GMN-compact / Python /
// Rust tab (the dialects and Pydantic siblings) plugs in by APPENDING ONE
// provider fn to `SYNTAX_TAB_PROVIDERS` and implementing it — the term-page
// renderer (`append_syntax_tabs`) and the `DocSyntaxTab` container never change.

/// One rendered example-syntax tab: a language-tagged code block with a label.
struct DocSyntaxTab {
    /// Stable slug id for the tab (part of the tab identity a future interactive
    /// renderer keys on; the static markdown render uses `label`/`lang`/`body`).
    #[allow(dead_code)]
    id: String,
    /// The human tab label (a format proper noun, e.g. `Turtle`, or a localized
    /// UI-chrome string for the schema panels).
    label: String,
    /// The fenced-code language tag (e.g. `turtle`, `json`).
    lang: String,
    /// The code-block body.
    body: String,
}

/// A pure example-syntax tab producer. `None` = this term has no honest content
/// for the syntax (absent tab).
type SyntaxTabProvider = fn(&DocTerm, &DocsModel) -> Option<DocSyntaxTab>;

/// The ordered example-syntax providers. APPEND-ONLY extension point: a sibling
/// issue adding a GMN-compact / Python / Rust tab appends its provider fn here.
const SYNTAX_TAB_PROVIDERS: &[SyntaxTabProvider] = &[
    turtle_syntax_tab,
    jsonld_syntax_tab,
    json_schema_syntax_tab,
    openapi_syntax_tab,
    python_syntax_tab,
    rust_syntax_tab,
];

/// The generated `gmeow_models` module slug for a slice IRI (the last IRI segment,
/// lowercased, non-identifier chars → `_`) — the same routing the Pydantic emitter
/// uses, so `gmeow_models.<slice>` resolves to the term's model module.
pub(crate) fn pydantic_module_slug(slice_iri: &str) -> String {
    let local = slice_iri.rsplit(['#', '/']).next().unwrap_or(slice_iri);
    let mut out = String::new();
    for ch in local.chars() {
        out.push(if ch == '_' || ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        });
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

/// The generated Pydantic class name for a class IRI: the CamelCase of its local
/// name (mirroring the emitter's `sanitize_type`), guarded against a leading digit.
pub(crate) fn pydantic_class_name(iri: &str) -> String {
    let local = iri.rsplit(['#', '/']).next().unwrap_or(iri);
    let mut ident = String::new();
    for ch in local.chars() {
        ident.push(if ch == '_' || ch.is_ascii_alphanumeric() {
            ch
        } else {
            '_'
        });
    }
    while ident.contains("__") {
        ident = ident.replace("__", "_");
    }
    ident = ident.trim_matches('_').to_string();
    let mut chars = ident.chars();
    let name = match chars.next() {
        Some(c) => format!("{}{}", c.to_ascii_uppercase(), chars.as_str()),
        None => "GmeowModel".to_string(),
    };
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("N{name}")
    } else {
        name
    }
}

/// Escape `text` for embedding as a non-raw, double-quoted Python string
/// literal: backslash and `"` are the only two characters a Python string
/// escape needs to neutralize, and an embedded literal newline (the
/// pretty-printer's structural whitespace, not a JSON string's *own*
/// characters — JSON already `\n`-escapes those) must become `\n` too, since a
/// non-raw, non-triple-quoted literal cannot contain one. Unlike a raw
/// triple-quoted literal (`r'''...'''`), this form has no forbidden
/// subsequence: `json.loads` decoding the result always reproduces `text`
/// exactly, regardless of its content.
fn python_str_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// The Python (Pydantic) example tab: construct-and-validate the term's worked
/// instance against its generated `gmeow_models` model — importing the model IS
/// reading the term. Present only for a modeled class (one carrying a schema
/// fragment); the payload is the SAME quickstart instance the Turtle / JSON-LD
/// tabs render, so the code never drifts from the RDF example.
fn python_syntax_tab(term: &DocTerm, model: &DocsModel) -> Option<DocSyntaxTab> {
    // Only a modeled class has a Pydantic model + a JSON-Schema fragment.
    model
        .schema_fragments
        .as_ref()?
        .schema_by_term
        .get(&term.iri)?;
    let skeleton = synthesize_quickstart(model, term);
    if !skeleton_has_triple(&skeleton) {
        return None;
    }
    let payload = quickstart_turtle_to_jsonld(&skeleton)?;
    // A raw triple-quoted literal (`r'''...'''`) cannot escape a `'''` run, so
    // an unlucky payload would terminate the string early and emit broken
    // Python; a non-raw double-quoted literal has no such forbidden
    // subsequence.
    let payload_literal = python_str_escape(&payload);
    let module = pydantic_module_slug(&term.owner_slice);
    let class = pydantic_class_name(&term.iri);
    let body = format!(
        "import json\n\n\
         from gmeow_models.{module} import {class}\n\n\
         # The same worked instance shown in the Turtle / JSON-LD tabs, validated\n\
         # against the ontology-derived model:\n\
         payload = json.loads(\"{payload_literal}\")\n\
         obj = {class}.model_validate(payload)\n\
         print(obj.model_dump(by_alias=True, exclude_none=True))"
    );
    Some(DocSyntaxTab {
        id: "python".to_string(),
        label: "Python".to_string(),
        lang: "python".to_string(),
        body,
    })
}

/// The narrowest Rust raw-string hash-fence width that is safe for `text`: one
/// more `#` than the longest run of `#` immediately following any `"` in
/// `text`. A raw string `r###"..."###` terminates at the first `"` followed by
/// AT LEAST as many `#` as the fence width, so a fixed one-`#` fence
/// (`r#"..."#`) is unsafe whenever `text` itself contains a `"#` sequence;
/// widening past every such run in `text` makes early termination impossible.
fn rust_raw_fence_width(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut max_run = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut run = 0usize;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b'#' {
                run += 1;
                j += 1;
            }
            max_run = max_run.max(run);
        }
        i += 1;
    }
    max_run + 1
}

/// Render `text` as a Rust raw-string literal (delimiters included) using a
/// hash fence wide enough that `text` cannot terminate it early — see
/// [`rust_raw_fence_width`]. For the common case (no `"#`-like run in `text`)
/// this is the familiar `r#"..."#`.
fn rust_raw_string_literal(text: &str) -> String {
    let fence = "#".repeat(rust_raw_fence_width(text));
    format!("r{fence}\"{text}\"{fence}")
}

/// The Rust example tab: parse the SAME quickstart instance and validate it with
/// the native GMEOW validator — the shipped Rust surface paired with the Python
/// tab, from the one worked instance. Present under the same modeled-class
/// condition as [`python_syntax_tab`].
fn rust_syntax_tab(term: &DocTerm, model: &DocsModel) -> Option<DocSyntaxTab> {
    model
        .schema_fragments
        .as_ref()?
        .schema_by_term
        .get(&term.iri)?;
    let skeleton = synthesize_quickstart(model, term);
    if !skeleton_has_triple(&skeleton) {
        return None;
    }
    let turtle = format!("{QUICKSTART_TURTLE_PREAMBLE}{skeleton}");
    // A fixed one-`#` fence breaks if `turtle` ever contains `"#`; widen the
    // fence to whatever this particular worked instance needs.
    let turtle_literal = rust_raw_string_literal(&turtle);
    let body = format!(
        "// Parse the same worked instance shown in the Turtle tab and validate it\n\
         // with the native GMEOW validator (the shipped Rust surface).\n\
         let turtle = {turtle_literal};\n\
         let dataset = purrdf::parse_turtle(turtle)?;\n\
         let report = gmeow_validate::validate(&dataset)?;\n\
         assert!(report.conforms());"
    );
    Some(DocSyntaxTab {
        id: "rust".to_string(),
        label: "Rust".to_string(),
        lang: "rust".to_string(),
        body: body.trim_end().to_string(),
    })
}

/// Whether a synthesized quickstart skeleton carries at least one Turtle triple
/// line (a non-comment, non-blank line) — the class/property skeletons do; the
/// individual/datatype/other skeletons are comment-only, so their example tabs
/// are honestly absent.
fn skeleton_has_triple(skeleton: &str) -> bool {
    skeleton
        .lines()
        .any(|l| !l.trim_start().is_empty() && !l.trim_start().starts_with('#'))
}

/// The Turtle example tab: the term's synthesized quickstart skeleton verbatim.
/// Present for a class/property (they synthesize a triple skeleton); absent for
/// an individual/datatype/other whose skeleton is comment-only.
fn turtle_syntax_tab(term: &DocTerm, model: &DocsModel) -> Option<DocSyntaxTab> {
    let body = synthesize_quickstart(model, term);
    if !skeleton_has_triple(&body) {
        return None;
    }
    Some(DocSyntaxTab {
        id: "turtle".to_string(),
        label: "Turtle".to_string(),
        lang: "turtle".to_string(),
        body: body.trim_end().to_string(),
    })
}

/// The JSON-LD example tab: the SAME quickstart Turtle, transcoded to a
/// deterministic JSON-LD document via the native `purrdf` codec. `None` when the
/// skeleton carries no triple or fails to parse/transcode (honest absence — no
/// fabricated document).
fn jsonld_syntax_tab(term: &DocTerm, model: &DocsModel) -> Option<DocSyntaxTab> {
    let skeleton = synthesize_quickstart(model, term);
    if !skeleton_has_triple(&skeleton) {
        return None;
    }
    let body = quickstart_turtle_to_jsonld(&skeleton)?;
    Some(DocSyntaxTab {
        id: "jsonld".to_string(),
        label: "JSON-LD".to_string(),
        lang: "json".to_string(),
        body,
    })
}

/// The JSON Schema fragment tab ("use this term without RDF"): the class's
/// `$defs` fragment from the attached schema digest. `None` when no digest is
/// attached (source-only render) or the term has no fragment — honest absence.
fn json_schema_syntax_tab(term: &DocTerm, model: &DocsModel) -> Option<DocSyntaxTab> {
    let body = model
        .schema_fragments
        .as_ref()?
        .schema_by_term
        .get(&term.iri)?
        .clone();
    Some(DocSyntaxTab {
        id: "json-schema".to_string(),
        label: model.ui("body_schema_fragment").to_string(),
        lang: "json".to_string(),
        body,
    })
}

/// The OpenAPI fragment tab: the class's `components/schemas` fragment from the
/// attached schema digest. `None` under the same honest-absence conditions as
/// [`json_schema_syntax_tab`].
fn openapi_syntax_tab(term: &DocTerm, model: &DocsModel) -> Option<DocSyntaxTab> {
    let body = model
        .schema_fragments
        .as_ref()?
        .openapi_by_term
        .get(&term.iri)?
        .clone();
    Some(DocSyntaxTab {
        id: "openapi".to_string(),
        label: model.ui("body_openapi_fragment").to_string(),
        lang: "json".to_string(),
        body,
    })
}

/// The canonical `@prefix`/`@base` preamble under which a synthesized quickstart
/// skeleton (which uses bare `gmeow:`/`logic:`/`math:`/`owl:`… CURIEs and the
/// relative `<subject>`/`<...>` placeholders) parses as a self-contained Turtle
/// document: a base resolves the relative placeholders, and every prefix a
/// `quickstart_label`/`term.curie` can emit is declared.
const QUICKSTART_TURTLE_PREAMBLE: &str = concat!(
    "@base <https://blackcatinformatics.ca/gmeow/example/> .\n",
    "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
    "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n",
    "@prefix math: <https://blackcatinformatics.ca/math/> .\n",
    "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n",
    "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n",
    "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
    "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n",
    "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n",
);

/// Transcode one synthesized quickstart Turtle skeleton to a deterministic
/// JSON-LD document using the native `purrdf` codecs: parse the skeleton (under
/// [`QUICKSTART_TURTLE_PREAMBLE`]) into an in-memory dataset, then serialize with
/// `serialize_dataset_to_jsonld` (sorted keys / `@graph`, byte-stable). `None`
/// when the skeleton fails to parse or yields no quads (honest absence). A pure
/// in-memory transcode — no `generated/` disk read.
fn quickstart_turtle_to_jsonld(skeleton: &str) -> Option<String> {
    let doc = format!("{QUICKSTART_TURTLE_PREAMBLE}{skeleton}");
    let dataset = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None).ok()?;
    if dataset.quad_count() == 0 {
        return None;
    }
    purrdf::native_codecs::jsonld::serialize_dataset_to_jsonld(&dataset).ok()
}

/// Render the ordered example-syntax tabs for a term under the "Example in
/// multiple syntaxes" heading. Since the site is static markdown/HTML, "tabs" are
/// sequential labeled fenced code blocks (the house style — cf. the loss-ledger's
/// labeled blocks). Emits nothing when no provider yields a tab (a source-only
/// render of an individual/datatype term produces no tabs — honest absence).
fn append_syntax_tabs(out: &mut String, model: &DocsModel, term: &DocTerm) {
    let tabs: Vec<DocSyntaxTab> = SYNTAX_TAB_PROVIDERS
        .iter()
        .filter_map(|provider| provider(term, model))
        .collect();
    if tabs.is_empty() {
        return;
    }
    heading(out, 2, model.ui("body_example_syntaxes"));
    for tab in &tabs {
        push_line(out, &format!("**{}**", md_escape(&tab.label)));
        blank(out);
        fenced(out, &tab.lang, &tab.body);
    }
}

// ── Description-Logic axiom rendering (academic surface) ─────────────────────
//
// A small, HONEST DL-notation formatter over the model data a term genuinely
// carries: subsumption (`⊑`) from each parent, and — for a property — the
// standard domain (`∃R.⊤ ⊑ D`) and range (`⊤ ⊑ ∀R.C`) axioms that `rdfs:domain`
// / `rdfs:range` denote. It never invents a value restriction the model does not
// carry (no fabricated `∃r.C` filler, no `⊓` intersection), so every line is a
// faithful DL reading of an asserted triple. Uses the standard glyphs (`⊑ ∃ ∀
// ⊤`); `lang-bridge::gmn_symbology` is a glyph-cost table, not a DL renderer, so
// this formatter owns the notation.

/// The compact DL name for an IRI: its CURIE when it names a documented term or a
/// GMEOW-family IRI, else the bare local name — never the full IRI.
fn dl_name(model: &DocsModel, iri: &str) -> String {
    if let Some(term) = model.terms.iter().find(|t| t.iri == iri) {
        return term.curie.clone();
    }
    let curie = to_curie(iri);
    if curie != iri {
        return curie;
    }
    local_name(iri).to_string()
}

/// The Description-Logic axioms a term GENUINELY carries, one per line (empty
/// when none): `C ⊑ P` per parent (class subsumption / property sub-role), plus,
/// for a property, the standard domain (`∃R.⊤ ⊑ D`) and range (`⊤ ⊑ ∀R.C`) axioms.
fn dl_axioms(model: &DocsModel, term: &DocTerm) -> Vec<String> {
    let self_name = term.curie.clone();
    let mut lines: Vec<String> = Vec::new();
    for parent in &term.parents {
        lines.push(format!("{self_name} ⊑ {}", dl_name(model, parent)));
    }
    if term.category == DocTermCategory::Property {
        for d in &term.domain {
            lines.push(format!("∃{self_name}.⊤ ⊑ {}", dl_name(model, d)));
        }
        for r in &term.range {
            lines.push(format!("⊤ ⊑ ∀{self_name}.{}", dl_name(model, r)));
        }
    }
    lines
}

// ── Page-level cite-this-surface (the generalized citation affordance) ────────
//
// The per-term Citation section (blake3 content digest + slice DOI + ontology
// concept DOI) generalized to a page-level "cite this page" block on every OTHER
// durable surface (fixtures index, competency index, notation index / grammar,
// pipeline DAG, glossary). The term page keeps its richer content-addressed form
// inline (`term_academic_surface`); this block is distinct from the
// per-page provenance footer.

/// Whether `page` is a durable, citable surface carrying the generalized
/// "cite this page" block. The term page is excluded here — its cite-this-term
/// surface is the richer inline Citation section.
fn page_is_citable(page: &Page) -> bool {
    matches!(
        page,
        Page::FixtureIndex
            | Page::CompetencyIndex
            | Page::NotationIndex
            | Page::Grammar(_)
            | Page::PipelineDag
            | Page::Glossary
    )
}

/// Append the generalized page-level "cite this page" block: the whole-ontology
/// concept DOI (when the model carries one) and the page's stable site locator.
/// A no-op on a non-citable page (see [`page_is_citable`]).
fn append_cite_this_surface(out: &mut String, model: &DocsModel, page: &Page) {
    if !page_is_citable(page) {
        return;
    }
    blank(out);
    heading(out, 2, model.ui("body_cite_this_page"));
    if let Some(doi) = &model.concept_doi {
        push_line(
            out,
            &format!(
                "- **{}:** [{}](https://doi.org/{})",
                model.ui("body_label_cite_ontology"),
                md_escape(doi),
                doi
            ),
        );
    }
    push_line(
        out,
        &format!(
            "- **{}:** `{}`",
            model.ui("body_label_permalink"),
            code_escape(&page.html_path())
        ),
    );
    blank(out);
}

/// The RDF-to-developer glossary: a fixed mapping from the RDF / OWL / SHACL
/// vocabulary this site uses to the everyday software concepts they correspond
/// to, so a reader who does not know RDF has a bridge into the rest of the site.
/// A pure, authored table (no model data), deterministic.
fn md_glossary(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_glossary"));
    line(
        &mut out,
        "If you build software but have not worked with RDF, these are the \
         everyday concepts behind the vocabulary this site uses. The mapping is \
         approximate — a bridge into the model, not an exact equivalence.",
    );
    push_line(
        &mut out,
        "| RDF / OWL / SHACL | Developer concept | Notes |",
    );
    push_line(&mut out, "| --- | --- | --- |");
    // (rdf-term, developer-concept, note) — authored, sorted-by-intent.
    const ROWS: &[(&str, &str, &str)] = &[
        (
            "Class",
            "Type",
            "A named category of thing — like a struct/record type or an interface.",
        ),
        (
            "Property",
            "Field",
            "A typed attribute or relation on a thing — like a struct field or a foreign key.",
        ),
        (
            "Individual",
            "Instance / value",
            "A concrete thing of some class — like a row or an object instance.",
        ),
        (
            "IRI",
            "Typed global ID",
            "A globally-unique identifier for a thing — like a URL-shaped primary key.",
        ),
        (
            "Triple",
            "Row (subject, predicate, object)",
            "One fact: `subject predicate object` — like a single (id, column, value) cell.",
        ),
        (
            "SHACL shape",
            "Schema validation",
            "Constraints a graph must satisfy — like a JSON Schema or a DB `CHECK`/`NOT NULL`.",
        ),
        (
            "Ontology",
            "Schema + rules",
            "The class/property vocabulary plus its axioms — a schema that also entails new facts.",
        ),
        (
            "rdfs:subClassOf",
            "Subtype / `extends`",
            "One class specialises another — like inheritance between types.",
        ),
        (
            "Domain / range",
            "Field's owner type / value type",
            "Which class a property applies to, and what type its values take.",
        ),
        (
            "Reasoner",
            "Inference engine",
            "Derives entailed facts and detects contradictions from the axioms.",
        ),
    ];
    for (rdf, dev, note) in ROWS {
        push_line(
            &mut out,
            &format!(
                "| `{}` | {} | {} |",
                code_escape(rdf),
                md_escape(dev),
                md_escape(note)
            ),
        );
    }
    blank(&mut out);
    out
}

/// A link from a grounding-slice IRI (a `gmeow:seamFromSlice`/`seamToSlice`
/// object) to its slice page, falling back to the IRI's local name in a code
/// span when the slice is unresolvable (never happens for a well-formed
/// registry, but this is a documentation READ, not a re-validation).
fn seam_slice_link(model: &DocsModel, from: &str, slice_iri: &str) -> String {
    if let Some(slice) = model.slices.iter().find(|s| s.iri == slice_iri) {
        let href = rel(from, &Page::Slice(slice_slug(slice)).dir());
        return format!("[{}]({}index.md)", md_escape(&slice_display(slice)), href);
    }
    md_escape(local_name(slice_iri))
}

/// The grounding seam registry: every sanctioned cross-grounding `gmeow:Seam`
/// — the closed channel set that authorizes a term reference to cross a
/// peered grounding-slice pair (Principle 19). Rendered directly from
/// [`crate::model::DocsModel::seams`] — the canonical governance data authored
/// in the grounding slices' `manifest.ttl` files — so this page is always a
/// faithful projection of that data, never a hand-maintained duplicate (see
/// `gmeow_validate`'s seam-registry drift gate). Deterministic: `model.seams`
/// is already IRI-sorted, and every per-seam collection is sorted/deduped.
fn md_seam_registry(model: &DocsModel) -> String {
    let from = Page::SeamRegistry.dir();
    let mut out = String::new();
    heading(&mut out, 1, "Grounding seams");
    line(
        &mut out,
        &format!(
            "The closed set of **{}** sanctioned cross-grounding reference channels among the \
             `logic:`, `lang:`, and `math:` grounding slices (Principle 19). Every peered \
             cross-slice term reference must land on one of these seams rather than riding free \
             on the `gmeow:sliceCoFoundationalWith` peerage grant. Each row is a `gmeow:Seam` \
             individual authored as canonical data in a grounding slice's `manifest.ttl`.",
            model.seams.len(),
        ),
    );

    if model.seams.is_empty() {
        line(&mut out, "No grounding seams are declared in this model.");
        return out;
    }

    push_line(
        &mut out,
        "| Seam | Direction | Carrying terms | Owning doc |",
    );
    push_line(&mut out, "| --- | --- | --- | --- |");
    for seam in &model.seams {
        let name = seam.label.clone().unwrap_or_else(|| to_curie(&seam.iri));
        let directions = seam
            .directions
            .iter()
            .map(|d| {
                format!(
                    "{} → {}",
                    seam_slice_link(model, &from, &d.from),
                    seam_slice_link(model, &from, &d.to)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let carrying = seam
            .carrying_terms
            .iter()
            .map(|iri| curie_link(model, &from, &to_curie(iri)))
            .collect::<Vec<_>>()
            .join(", ");
        let owning = seam
            .owning_docs
            .iter()
            .map(|d| format!("`{}`", code_escape(d)))
            .collect::<Vec<_>>()
            .join(", ");
        push_line(
            &mut out,
            &format!(
                "| **{}** | {} | {} | {} |",
                md_escape(&name),
                directions,
                carrying,
                owning,
            ),
        );
    }
    blank(&mut out);

    // A definitions section: the full prose each seam's `skos:definition`
    // carries, keyed to the same seam name the table above uses.
    heading(&mut out, 2, "Definitions");
    for seam in &model.seams {
        let name = seam.label.clone().unwrap_or_else(|| to_curie(&seam.iri));
        heading(&mut out, 3, &name);
        if let Some(definition) = &seam.definition {
            line(&mut out, &md_escape(definition));
        }
    }
    out
}

/// The short four-boxes label for a `gmeow:box*` role CURIE (`gmeow:boxTBox` →
/// `TBox`); the CURIE unchanged when it does not match the expected shape.
fn box_role_label(role: &str) -> String {
    role.strip_prefix("gmeow:box")
        .filter(|s| !s.is_empty())
        .unwrap_or(role)
        .to_string()
}

/// The logic-stereotypes index: every term grouped by its `logic:` stereotype.
/// Resolves the `nav_logic` chrome string to a real page.
fn md_logic_index(model: &DocsModel) -> String {
    let from = Page::Logic.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_logic_and_reasoning"));
    line(
        &mut out,
        "Terms grouped by their lowered OntoUML/UFO stereotype (the `logic:` discipline \
         each term carries). See the slice doctrine under `slices/grounding/logic` for the \
         stereotype semantics.",
    );

    // Cross-link to the four `logic:` compiler-product pages. The compiler turns
    // one canonical IR into every projection target; these pages document the four
    // information products that fall out of that compile — the IR itself, the
    // preservation loss ledger, the derivation-graph explanations, and the
    // diagnostics. Links use the same `rel` helper the rest of the page does, so
    // they resolve cleanly under the anchor gate.
    heading(&mut out, 2, model.ui("body_compiler_products"));
    line(
        &mut out,
        "The `logic:` compiler emits four information products from one canonical \
         program. Each has a dedicated page:",
    );
    let ir_href = rel(&from, &Page::LogicCanonicalIr.dir());
    let ledger_href = rel(&from, &Page::LogicLossLedger.dir());
    let deriv_href = rel(&from, &Page::LogicDerivationGraph.dir());
    let diag_href = rel(&from, &Page::LogicDiagnostics.dir());
    push_line(
        &mut out,
        &format!(
            "- [Canonical IR]({ir_href}index.md) — the one AST / RDF 1.2 identity \
             serialization the compiler produces."
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Preservation loss ledger]({ledger_href}index.md) — per projection \
             target: preservation kind, complexity class, and structural lossy drops."
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Derivation graph]({deriv_href}index.md) — the reasoning provenance: \
             per-axiom proof skeletons."
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Compiler diagnostics]({diag_href}index.md) — parse findings plus \
             lossy-drop notes, surfaced as SARIF."
        ),
    );
    blank(&mut out);

    // Cross-link to the grounding seam registry — the closed set of sanctioned
    // cross-grounding reference channels among `logic:`/`lang:`/`math:`.
    let seams_href = rel(&from, &Page::SeamRegistry.dir());
    push_line(
        &mut out,
        &format!(
            "See also the [grounding seam registry]({seams_href}index.md) — the sanctioned \
             cross-grounding reference channels among the `logic:`/`lang:`/`math:` grounding \
             slices."
        ),
    );
    blank(&mut out);

    // Group terms by each stereotype they carry (a term may carry several).
    let mut by_stereotype: BTreeMap<String, Vec<&DocTerm>> = BTreeMap::new();
    for term in &model.terms {
        for stereotype in &term.logic_stereotypes {
            by_stereotype
                .entry(stereotype.clone())
                .or_default()
                .push(term);
        }
    }

    if by_stereotype.is_empty() {
        line(&mut out, model.ui("body_no_logic_stereotypes"));
        return out;
    }

    for (stereotype, mut terms) in by_stereotype {
        terms.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
        heading(
            &mut out,
            2,
            &format!("{stereotype} ({} term(s))", terms.len()),
        );
        for term in terms {
            push_line(
                &mut out,
                &format!("- {}", term_link(model, &from, &term.iri)),
            );
        }
        blank(&mut out);
    }
    out
}

// ── Logic compiler products ───────────────────────────────────────────────────
//
// The `logic:` compiler turns one canonical program into every projection
// target; four information products fall out of that compile. Each gets a
// dedicated page under the logic area. The loss-ledger page reads the public
// `gmeow_logic_compile::projections::projection_ledger_rows` accessor (the
// crate-private `target_meta` table is the source of truth); the other three are
// high-level prose pages — they describe products, not fabricated APIs.

/// The canonical-IR product page: the one AST / RDF 1.2 identity serialization
/// the compiler produces, and the projection targets it feeds.
fn md_logic_canonical_ir(model: &DocsModel) -> String {
    let from = Page::LogicCanonicalIr.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_canonical_ir"));
    line(
        &mut out,
        "The `logic:` compiler parses every source surface into a single canonical \
         intermediate representation — **the one AST**. The IR is a frozen value \
         hierarchy with an order-independent canonicalization contract: every \
         ordering and hash the downstream artifacts depend on bottoms out in the \
         IR's stable sort keys, so the same program always compiles byte-for-byte \
         identically.",
    );
    line(
        &mut out,
        "The IR's own faithful serialization is the **canonical RDF 1.2** projection: \
         a lossless, identity round-trip (`ExactPreservation`). Re-parsing those \
         triples reconstructs the IR exactly — it is the AST written down, not a \
         lossy view of it.",
    );

    heading(&mut out, 2, model.ui("body_projection_surface"));
    line(
        &mut out,
        "From the one IR the compiler runs every projection back-end. The standard \
         whole-program targets are:",
    );
    for row in gmeow_logic_compile::projections::projection_ledger_rows() {
        push_line(&mut out, &format!("- `{}`", code_escape(&row.target)));
    }
    blank(&mut out);
    let ledger_href = rel(&from, &Page::LogicLossLedger.dir());
    line(
        &mut out,
        &format!(
            "Each target declares how faithfully it preserves the IR; the \
             [preservation loss ledger]({ledger_href}index.md) records the \
             preservation kind, complexity class, and structural drops per target."
        ),
    );
    out
}

/// The preservation loss-ledger product page: a table built from the public
/// `projection_ledger_rows` accessor — one row per standard projection target.
fn md_logic_loss_ledger(model: &DocsModel) -> String {
    let from = Page::LogicLossLedger.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_preservation_loss_ledger"));
    line(
        &mut out,
        "Every projection of the canonical IR declares **how much it preserves**. A \
         lossy down-projection (OWL, gUFO, Datalog, …) is sound but drops structure \
         the target format cannot carry; exact targets such as canonical RDF 1.2 \
         drop nothing. This ledger is the per-target record — the compiler's \
         overclaim gate turns the build red if a target claims exact preservation \
         yet drops anything.",
    );

    let ir_href = rel(&from, &Page::LogicCanonicalIr.dir());
    line(
        &mut out,
        &format!(
            "The rows below are the static, whole-program targets the \
             [canonical IR]({ir_href}index.md) feeds; a concrete program may add a \
             per-shape property-path row for each declared path shape."
        ),
    );

    push_line(
        &mut out,
        "| Target | Preservation kind | Complexity class | Lossy drops |",
    );
    push_line(&mut out, "| --- | --- | --- | --- |");
    for row in gmeow_logic_compile::projections::projection_ledger_rows() {
        // The drops are a structured list; render them as a `<br>`-separated cell so
        // the four-column grid stays intact (a multi-line cell would break it). An
        // exact target carries no drops — show an em dash rather than an empty cell.
        let drops = if row.lossy_drops.is_empty() {
            "—".to_string()
        } else {
            row.lossy_drops
                .iter()
                .map(|d| md_escape(&one_line(d)))
                .collect::<Vec<_>>()
                .join("<br>")
        };
        push_line(
            &mut out,
            &format!(
                "| `{}` | `{}` | `{}` | {} |",
                code_escape(&row.target),
                code_escape(&row.preservation_kind),
                code_escape(&row.complexity),
                drops,
            ),
        );
    }
    blank(&mut out);

    // ── Worked, authored examples ────────────────────────────────────────────
    // Distinct from the static whole-program rows above: these come from
    // concrete AUTHORED artifacts (`examples/*.ttl`, any slice — not just the
    // logic slice's `projection-loss-ledger.ttl`) applying the SAME
    // preservation-kind vocabulary to a specific report, not a whole target
    // class. Discovered generically by `gmeow_docs::model::extract_loss_targets`:
    // any example subject carrying both `logic:preservationKind` and
    // `logic:complexityClass` becomes a row here.
    heading(&mut out, 2, model.ui("body_worked_preservation_examples"));
    line(
        &mut out,
        "The compiler ledger above covers the static, whole-program targets; \
         these are worked, authored examples of the SAME preservation-kind \
         vocabulary applied to concrete artifacts — a projection report, a \
         bridge view, a closed-world SHACL-to-JSON-Schema compile — each one an \
         instance, not a target class.",
    );
    if model.loss_targets.is_empty() {
        line(&mut out, model.ui("body_no_worked_preservation_examples"));
    } else {
        push_line(
            &mut out,
            "| Target | Label | Preservation kind | Complexity class |",
        );
        push_line(&mut out, "| --- | --- | --- | --- |");
        for row in &model.loss_targets {
            let label = row
                .label
                .as_deref()
                .map(|l| md_escape(&one_line(l)))
                .unwrap_or_else(|| "—".to_string());
            push_line(
                &mut out,
                &format!(
                    "| `{}` | {} | `{}` | `{}` |",
                    code_escape(&row.target),
                    label,
                    code_escape(&row.preservation_kind),
                    code_escape(&row.complexity_class),
                ),
            );
        }
        blank(&mut out);
    }
    out
}

/// The derivation-graph product page: the reasoning-provenance explanation
/// skeletons (per-axiom proof skeletons) the reasoning channel ships.
fn md_logic_derivation_graph(model: &DocsModel) -> String {
    let from = Page::LogicDerivationGraph.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_derivation_graph"));
    line(
        &mut out,
        "Beyond the projected programs, the reasoning channel produces a **derivation \
         graph** — the provenance of every entailed fact. Each derived statement \
         carries a proof skeleton: the axiom(s) and the body bindings that justified \
         it, chained back to the source facts. The graph is the content-addressed \
         explanation surface — it is what makes a conclusion auditable rather than \
         opaque.",
    );
    line(
        &mut out,
        "The skeletons double as the truth-maintenance justification: when a premise \
         is retracted, the derivation graph identifies exactly which conclusions \
         lose their support. Because the explanation is keyed by content, two runs \
         over the same program yield the same justification graph — the basis for \
         the explanation goldens.",
    );
    let ir_href = rel(&from, &Page::LogicCanonicalIr.dir());
    line(
        &mut out,
        &format!(
            "This product ships in the bundle's `reasoning-archive` channel as \
             `generated/logic/reasoning-explanations.rdf12.ttl`, folded alongside the \
             compiled programs the [canonical IR]({ir_href}index.md) projects, so a \
             consumer can carry the conclusions and their proofs together."
        ),
    );
    out
}

/// The compiler-diagnostics product page: parse findings + lossy-drop notes,
/// surfaced as SARIF.
fn md_logic_diagnostics(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_compiler_diagnostics"));
    line(
        &mut out,
        "Compiling a `logic:` program is also a **diagnostic pass**. The compiler \
         emits two kinds of finding:",
    );
    push_line(
        &mut out,
        "- **Parse findings** — malformed or ill-typed source surfaces flagged at \
         the point they are read.",
    );
    push_line(
        &mut out,
        "- **`preservation.rung.structural` / `preservation.rung.actual` notes** — one \
         finding per structural limitation a lossy projection carries and per concrete \
         item it had to drop, so the loss is never silent. Each actual drop carries its \
         causing structural limitation as an antecedent, and these line up with the \
         per-target rows in the preservation loss ledger.",
    );
    blank(&mut out);
    line(
        &mut out,
        "The findings are surfaced as **SARIF**, the standard static-analysis result \
         format, so they flow into the same code-scanning surface as the rest of the \
         repository's diagnostics — no bespoke reader required.",
    );
    out
}

fn md_slice_index(model: &DocsModel) -> String {
    let from = Page::SliceIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_slices"));
    line(
        &mut out,
        &format!("{} compilation unit(s).", model.slices.len()),
    );

    heading(&mut out, 2, model.ui("body_dependency_graph"));
    push_line(
        &mut out,
        &format!(
            "![Slice dependency graph]({}diagrams/slices.svg)",
            root_href(&from)
        ),
    );
    blank(&mut out);

    push_line(&mut out, "| Slice | Tier | IRI |");
    push_line(&mut out, "| --- | --- | --- |");
    // model.slices is already sorted by IRI.
    for slice in &model.slices {
        let href = rel(&from, &Page::Slice(slice_slug(slice)).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} | `{}` |",
                md_escape(&slice_display(slice)),
                href,
                md_escape(&tier_name(slice)),
                code_escape(&slice.iri)
            ),
        );
    }
    blank(&mut out);
    out
}

fn md_slice(model: &DocsModel, slug: &str, page_map: &SourceToPageMap) -> String {
    let Some(slice) = model.slices.iter().find(|s| slice_slug(s) == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_slice_not_found"));
        return out;
    };
    let from = Page::Slice(slug.to_string()).dir();

    let mut out = String::new();
    heading(&mut out, 1, &slice_display(slice));
    line(&mut out, &format!("`{}`", code_escape(&slice.iri)));

    // Manifest identity.
    push_line(&mut out, "| Field | Value |");
    push_line(&mut out, "| --- | --- |");
    push_line(
        &mut out,
        &format!("| Tier | {} |", md_escape(&tier_name(slice))),
    );
    if let Some(id) = &slice.identifier {
        push_line(&mut out, &format!("| Identifier | `{}` |", code_escape(id)));
    }
    if !slice.creators.is_empty() {
        push_line(
            &mut out,
            &format!("| Creators | {} |", md_escape(&slice.creators.join(", "))),
        );
    }
    if !slice.consumers.is_empty() {
        push_line(
            &mut out,
            &format!("| Consumers | {} |", md_escape(&slice.consumers.join(", "))),
        );
    }
    blank(&mut out);

    // `page_map` is the single link-rewrite authority (a pure function of the model's
    // already-validated document set), built ONCE per render pass and threaded in — it
    // drives the grafted `docs.md` prose, the child-document index, and every internal
    // link those surfaces rewrite.

    // Graft the slice's top-level `docs.md` narrative directly onto the slice page,
    // between the manifest metadata and the generated artifact/term/linkage sections.
    // Every heading is demoted one level (H1→H2 … H5→H6) so the generated slice H1
    // stays the unique page H1, and every internal link is rewritten through the
    // page map. The thesis / realized-state FACTS are still read separately from
    // `docs.md` (see `DocSlice::from_record`); this additionally RENDERS its prose.
    if let Some(docs_md) = slice
        .documents
        .iter()
        .find(|d| d.source_path == SLICE_PAGE_SOURCE)
    {
        let grafted = rewrite_doc_body(
            &docs_md.source_text,
            page_map,
            &slice.iri,
            SLICE_PAGE_SOURCE,
            &from,
            HeadingDemotion::GraftOne,
            true,
        );
        out.push_str(&grafted);
        blank(&mut out);
    }

    // The slice's child documents (every non-`docs.md` markdown), by title, sorted
    // by logical path, each linking to its own generated page and retaining its
    // source path + raw digest as visible provenance.
    let children = page_map.slice_children(&slice.iri);
    if !children.is_empty() {
        heading(&mut out, 2, "Documents");
        for entry in &children {
            let target_dir = entry.page.strip_suffix('/').unwrap_or(&entry.page);
            let href = rel(&from, target_dir);
            push_line(
                &mut out,
                &format!(
                    "- [{}]({href}index.md) — `{}` (`{}`)",
                    md_escape(&entry.title),
                    code_escape(&entry.source_path),
                    code_escape(&short_digest(&entry.raw_digest)),
                ),
            );
        }
        blank(&mut out);
    }

    // Artifact inventory grouped by role (by reference: path + media type + digest).
    if !slice.artifacts.is_empty() {
        heading(&mut out, 2, model.ui("body_artifacts"));
        // Group by role-name; both the group order and the within-group order are
        // sorted for determinism.
        let mut by_role: BTreeMap<String, Vec<&crate::model::DocArtifact>> = BTreeMap::new();
        for artifact in &slice.artifacts {
            by_role
                .entry(role_name(&artifact.role))
                .or_default()
                .push(artifact);
        }
        for (role, mut artifacts) in by_role {
            artifacts.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
            heading(&mut out, 3, &role);
            push_line(&mut out, "| Path | Media type | Digest |");
            push_line(&mut out, "| --- | --- | --- |");
            for artifact in artifacts {
                push_line(
                    &mut out,
                    &format!(
                        "| `{}` | `{}` | `{}` |",
                        code_escape(&artifact.logical_path),
                        code_escape(&artifact.media_type),
                        code_escape(&short_digest(&artifact.raw_digest))
                    ),
                );
            }
            blank(&mut out);
        }
    }

    // Terms owned by this slice.
    let mut owned: Vec<&DocTerm> = model
        .terms
        .iter()
        .filter(|t| t.owner_slice == slice.iri)
        .collect();
    owned.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    if !owned.is_empty() {
        heading(&mut out, 2, &format!("Terms ({})", owned.len()));
        for term in owned {
            let href = rel(&from, &Page::Term(term_slug(term)).dir());
            push_line(
                &mut out,
                &format!(
                    "- [`{}`]({}index.md) — {}",
                    code_escape(&term.curie),
                    href,
                    md_escape(category_singular(term.category))
                ),
            );
        }
        blank(&mut out);
    }

    // Linkages whose subject is owned by this slice.
    let mut slice_links: Vec<&crate::model::DocLinkage> = model
        .linkages
        .iter()
        .filter(|l| l.owner_slice == slice.iri)
        .collect();
    slice_links.sort_by(|a, b| {
        a.subject_curie
            .cmp(&b.subject_curie)
            .then_with(|| a.object.cmp(&b.object))
    });
    if !slice_links.is_empty() {
        heading(&mut out, 2, &format!("Linkages ({})", slice_links.len()));
        push_line(
            &mut out,
            "| Subject | Predicate | External object | Conf. |",
        );
        push_line(&mut out, "| --- | --- | --- | --- |");
        for link in slice_links {
            push_line(
                &mut out,
                &format!(
                    "| {} | `{}` | [{}]({}) | {} |",
                    subject_link(model, &from, link),
                    code_escape(&link.predicate),
                    md_escape(&link.object),
                    md_escape(&link.object),
                    confidence_cell(link.confidence),
                ),
            );
        }
        blank(&mut out);
    }

    // Worked examples owned by this slice — rendered IN FULL.
    let mut slice_examples: Vec<&crate::model::DocExample> = model
        .examples
        .iter()
        .filter(|e| e.slice == slice.iri)
        .collect();
    slice_examples.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
    if !slice_examples.is_empty() {
        heading(&mut out, 2, &format!("Examples ({})", slice_examples.len()));
        for example in slice_examples {
            heading(&mut out, 3, &example.title);
            line(
                &mut out,
                &format!("`{}`", code_escape(&example.logical_path)),
            );
            fenced(&mut out, "turtle", &example.text);
        }
    }

    out
}

/// Render a slice child document (`Page::SliceDocument`) as its own page: the
/// document's `source_text` Markdown IS the page body — its authored H1 stays the
/// page H1 (NO heading demotion) — with every internal markdown→markdown link and
/// fragment rewritten through the single [`SourceToPageMap`] authority. A dangling
/// WITHIN-slice link hard-fails (never renders silently); off-corpus references
/// (another slice, a repo `docs/` file, a non-markdown asset) are absolutized to
/// the published site. List provenance (source path + digest) lives on the SLICE
/// page's document index, not on the child body itself.
fn md_slice_document(
    model: &DocsModel,
    slice_iri: &str,
    path: &str,
    page_map: &SourceToPageMap,
) -> String {
    let page_dir = Page::SliceDocument {
        slice: slice_iri.to_string(),
        path: path.to_string(),
    }
    .dir();
    let doc = model
        .slices
        .iter()
        .find(|s| s.iri == slice_iri)
        .and_then(|s| s.documents.iter().find(|d| d.source_path == path));
    let Some(doc) = doc else {
        let mut out = String::new();
        heading(&mut out, 1, path);
        line(&mut out, model.ui("body_slice_not_found"));
        return out;
    };
    rewrite_doc_body(
        &doc.source_text,
        page_map,
        slice_iri,
        path,
        &page_dir,
        HeadingDemotion::Keep,
        true,
    )
}

// ── First-class Markdown document rewrite (graft + child page + llms corpus) ──

/// Re-emit an authored slice Markdown document (`docs.md` or a child `*.md`) with
/// every internal link resolved through the single [`SourceToPageMap`] authority
/// and, when `demote` is set, every ATX heading demoted one level.
///
/// The body is processed line by line so every Markdown construct is preserved
/// verbatim except the two deliberate rewrites. Fenced code blocks (```` ``` ````
/// / `~~~`) and inline code spans are passed through untouched, so a `#` or a
/// `](…)` inside a code sample is never mistaken for a heading or a link.
///
/// * `from_slice` / `from_path` locate the document in source space (the resolver
///   joins relative links against `from_path`).
/// * `page_dir` is the site directory of the page the body is rendered ONTO (the
///   slice page for a graft, the child page for a child render, `""` for the
///   root-level llms corpus) — relative hrefs are computed from it.
/// * `demote` selects the heading transform (see [`HeadingDemotion`]).
/// * `inject_anchors` — when set, emit an explicit `<a id="{slug}"></a>` before
///   each heading using the map's page-scoped, text-derived slugs (matched in
///   source order). pulldown-cmark emits no heading `id`, so these anchors are what
///   the rewritten `#slug` / `…index.md#slug` cross-links resolve to on the HTML
///   page. Off for the plain-text `llms-full` corpus (no HTML anchors there).
fn rewrite_doc_body(
    source: &str,
    map: &SourceToPageMap,
    from_slice: &str,
    from_path: &str,
    page_dir: &str,
    demote: HeadingDemotion,
    inject_anchors: bool,
) -> String {
    // The page-scoped heading anchors, in source order, for the id injection. Empty
    // when not injecting or when the document has no known page.
    let anchors: &[crate::source_map::HeadingAnchor] = if inject_anchors {
        map.page_of(from_slice, from_path)
            .map(|page| map.heading_anchors(page))
            .unwrap_or(&[])
    } else {
        &[]
    };
    let mut anchor_idx = 0usize;

    let mut out = String::with_capacity(source.len() + 64);
    let mut in_fence = false;
    let mut fence_marker = "";
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        // Toggle fenced-code state; fenced lines pass through verbatim.
        if let Some(marker) = fence_open(trimmed) {
            if in_fence {
                if trimmed.starts_with(fence_marker) {
                    in_fence = false;
                    fence_marker = "";
                }
            } else {
                in_fence = true;
                fence_marker = marker;
            }
            out.push_str(line);
            continue;
        }
        if in_fence {
            // Fenced code is emitted verbatim and its links are NOT rewritten (a
            // `](x.md)` in a code sample is not a document link). The one exception
            // is the `CorpusFloor` flatten, which additionally demotes any leading
            // `#`-run so an inlined Turtle/shell comment (`# note`) never surfaces
            // as a spurious H1/section in the flat `llms-full` corpus — harmless,
            // since `#`-to-end-of-line comment syntaxes stay comments at any depth.
            if demote == HeadingDemotion::CorpusFloor {
                out.push_str(&demote_heading_line(line, demote, from_slice, from_path));
            } else {
                out.push_str(line);
            }
            continue;
        }
        // Before an ATX heading (in source order), emit its explicit HTML anchor so
        // the resolved `#slug` cross-links have a matching `id` on the page.
        if inject_anchors && atx_level(line).is_some() {
            if let Some(anchor) = anchors.get(anchor_idx) {
                out.push_str(&format!("<a id=\"{}\"></a>\n\n", anchor.slug));
            }
            anchor_idx += 1;
        }
        // Heading demotion is applied to the raw line before link rewriting; the
        // added `#`s never affect link scanning.
        let demoted = demote_heading_line(line, demote, from_slice, from_path);
        rewrite_doc_line(&demoted, map, from_slice, from_path, page_dir, &mut out);
    }
    out
}

/// The heading transform [`rewrite_doc_body`] applies to a re-emitted document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadingDemotion {
    /// Keep every heading level (a child page: its authored H1 is the page H1).
    Keep,
    /// Demote one level (H1→H2 … H5→H6) so a grafted `docs.md` sits under the
    /// generated slice H1. A source H6 is the H6-overflow guard: H6→H7 is illegal,
    /// so it is a hard failure naming the document.
    GraftOne,
    /// Floor headings two levels down, clamped at H6, so the inlined `llms-full`
    /// document corpus sits below both the single `# ` document H1 and the
    /// `## Documents` section header (every heading becomes H3+, never a bare `## `
    /// that the flat llmstxt structure would read as a new section). Clamps rather
    /// than overflowing — the corpus is a lossy flattened surface, not the graft.
    CorpusFloor,
}

/// Apply the [`HeadingDemotion`] transform to one line, preserving leading
/// whitespace and the trailing newline. Non-heading lines are returned unchanged.
fn demote_heading_line(
    line: &str,
    demote: HeadingDemotion,
    from_slice: &str,
    from_path: &str,
) -> String {
    if demote == HeadingDemotion::Keep {
        return line.to_string();
    }
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let hashes = rest.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return line.to_string();
    }
    let after = &rest[hashes..];
    // A heading needs whitespace (or end of line) after the hash run; otherwise
    // it is not an ATX heading (e.g. `#hashtag`) and must not be demoted.
    if !after.is_empty() && !after.starts_with([' ', '\t', '\n']) {
        return line.to_string();
    }
    let extra = match demote {
        HeadingDemotion::Keep => unreachable!("handled above"),
        HeadingDemotion::GraftOne => {
            if hashes == 6 {
                panic!(
                    "markdown source `{from_path}` in slice {from_slice} carries a level-6 heading \
                     that cannot be demoted (H6→H7 is illegal): {:?}",
                    line.trim_end()
                );
            }
            1
        }
        // Floor at H6: every heading lands at level+2 but never past 6.
        HeadingDemotion::CorpusFloor => (6 - hashes).min(2),
    };
    let prefix = "#".repeat(hashes + extra);
    format!("{indent}{prefix}{after}")
}

/// Rewrite the link targets on a single (non-fenced) source line, appending the
/// result to `out`. Scans for inline-link / image destinations `](target)`,
/// skipping inline code spans so a `](…)` inside backticks is left verbatim.
fn rewrite_doc_line(
    line: &str,
    map: &SourceToPageMap,
    from_slice: &str,
    from_path: &str,
    page_dir: &str,
    out: &mut String,
) {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut span_start = 0usize;
    while i < bytes.len() {
        // Skip an inline code span: a backtick run of length N is closed by the
        // next run of exactly N backticks (CommonMark). Everything between is
        // literal and never rewritten.
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            let close_from = i + run;
            if let Some(rel_close) = find_backtick_run(&bytes[close_from..], run) {
                i = close_from + rel_close + run;
                continue;
            }
            // No closing run on this line: the rest is not a code span, keep scanning.
            i += run;
            continue;
        }
        // An inline link/image destination opens at `](`.
        if bytes[i] == b']' && i + 1 < bytes.len() && bytes[i + 1] == b'(' {
            let open = i + 2;
            // Destinations in authored slice markdown never contain a raw `)`.
            if let Some(rel_close) = line[open..].find(')') {
                let close = open + rel_close;
                out.push_str(&line[span_start..open]);
                out.push_str(&rewrite_doc_target(
                    &line[open..close],
                    map,
                    from_slice,
                    from_path,
                    page_dir,
                ));
                span_start = close;
                i = close;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&line[span_start..]);
}

/// The ATX heading level (1–6) of a non-fenced line, or `None` when it is not an
/// ATX heading. Mirrors the `source_map` heading detector so this walk's heading
/// order aligns with the map's page-scoped anchor order.
fn atx_level(line: &str) -> Option<u8> {
    let rest = line.trim_start();
    let hashes = rest.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = &rest[hashes..];
    if after.is_empty() || after.starts_with([' ', '\t', '\n']) {
        Some(hashes as u8)
    } else {
        None
    }
}

/// The fence marker (` ``` ` or `~~~`) a trimmed line opens/closes a fenced code
/// block with (three or more of one marker char), else `None`.
/// Find the byte offset of the next run of EXACTLY `run` backticks in `bytes`,
/// else `None`. Used to skip inline code spans in [`rewrite_doc_line`].
fn find_backtick_run(bytes: &[u8], run: usize) -> Option<usize> {
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let here = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            if here == run {
                return Some(i);
            }
            i += here;
        } else {
            i += 1;
        }
    }
    None
}

/// Rewrite a single authored link/image destination (the text between `](` and
/// `)`), routing every internal reference through the single [`SourceToPageMap`]
/// authority.
///
/// * Absolute / scheme-qualified / `mailto:` / site-absolute targets are external
///   and pass through verbatim.
/// * A pure `#fragment` addresses a heading in THIS document; a missing anchor is
///   a hard failure (a broken same-document link must not render silently).
/// * A relative `*.md` reference that stays WITHIN the slice is resolved to its
///   generated page (relative href, with any `#anchor`); a dangling within-slice
///   reference is a hard failure the slice's markdown must fix.
/// * A reference that escapes the slice root (another slice, a repo `docs/` file)
///   or targets a non-markdown asset is off-corpus and is absolutized to the
///   published site, so the rendered page never dangles.
fn rewrite_doc_target(
    target: &str,
    map: &SourceToPageMap,
    from_slice: &str,
    from_path: &str,
    page_dir: &str,
) -> String {
    if target.is_empty() {
        return target.to_string();
    }
    // The path portion (before any `#fragment`) classifies the target; the resolver
    // re-splits and validates the fragment itself.
    let path_part = target.split_once('#').map_or(target, |(p, _)| p);

    // A pure fragment: a heading in this same document.
    if path_part.is_empty() {
        return match map.resolve_link(from_slice, from_path, target) {
            LinkResolution::Resolved(loc) => match loc.anchor {
                Some(anchor) => format!("#{anchor}"),
                // A bare `#` with no anchor — leave as authored.
                None => target.to_string(),
            },
            LinkResolution::Dangling { .. } => panic!(
                "markdown source `{from_path}` in slice {from_slice} has a dangling same-document \
                 anchor link `{target}` (no such heading)"
            ),
        };
    }

    // Everything with a path portion is CLASSIFIED by the single authority
    // (`SourceToPageMap::classify_doc_link`) — the site renderer never re-derives
    // "internal vs. off-corpus vs. external"; it only formats the classification for
    // its own (site-relative) output.
    match map.classify_doc_link(from_slice, from_path, target) {
        DocLinkResolution::External => target.to_string(),
        DocLinkResolution::OffCorpus => absolutize_offsite(target),
        DocLinkResolution::Corpus(loc) => {
            let target_dir = loc.page.strip_suffix('/').unwrap_or(&loc.page);
            let href = rel(page_dir, target_dir);
            match loc.anchor {
                Some(anchor) => format!("{href}index.md#{anchor}"),
                None => format!("{href}index.md"),
            }
        }
        DocLinkResolution::Dangling { .. } => panic!(
            "markdown source `{from_path}` in slice {from_slice} has a dangling internal document \
             link `{target}` — fix the link in the slice's markdown (it names no document in the \
             slice)"
        ),
    }
}

/// Absolutize an off-corpus relative reference to the published documentation site,
/// so a rendered page never carries a dangling relative link into a document this
/// render does not emit. Leading `./` / `../` segments are dropped and the
/// remainder is appended to [`crate::mdbook::PUBLISHED_SITE_BASE`] — the SAME
/// published-site base the mdbook renderer externalizes dropped surfaces to.
fn absolutize_offsite(target: &str) -> String {
    let mut rest = target;
    loop {
        if let Some(r) = rest.strip_prefix("../") {
            rest = r;
        } else if let Some(r) = rest.strip_prefix("./") {
            rest = r;
        } else {
            break;
        }
    }
    format!("{}{rest}", crate::mdbook::PUBLISHED_SITE_BASE)
}

// ── Linkage / example / concern / external / integrity pages ──────────────────

fn md_linkage_index(model: &DocsModel) -> String {
    let from = Page::LinkageIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_linkages"));
    line(
        &mut out,
        &format!(
            "**{}** term equivalences across **{}** mapping set(s), cross-walking GMEOW terms to \
             external vocabularies.",
            model.linkages.len(),
            model.mapping_sets.len()
        ),
    );

    for set in &model.mapping_sets {
        heading(&mut out, 2, &set_display(set));
        push_line(&mut out, "| Field | Value |");
        push_line(&mut out, "| --- | --- |");
        push_line(
            &mut out,
            &format!("| CURIE | `{}` |", code_escape(&set.curie)),
        );
        if let Some(id) = &set.set_id {
            push_line(&mut out, &format!("| Set ID | `{}` |", code_escape(id)));
        }
        if let Some(file) = &set.sssom_file {
            push_line(
                &mut out,
                &format!("| SSSOM file | `{}` |", code_escape(file)),
            );
        }
        if let Some(license) = &set.license {
            push_line(
                &mut out,
                &format!(
                    "| License | [{}]({}) |",
                    md_escape(license),
                    md_escape(license)
                ),
            );
        }
        push_line(
            &mut out,
            &format!(
                "| Owner slice | {} |",
                slice_link(model, &from, &set.owner_slice)
            ),
        );
        push_line(
            &mut out,
            &format!("| Equivalences | {} |", set.equivalence_count),
        );
        blank(&mut out);

        if let Some(comment) = &set.comment {
            line(&mut out, &md_escape(&one_line(comment)));
        }

        let mut links: Vec<&crate::model::DocLinkage> = model
            .linkages
            .iter()
            .filter(|l| l.mapping_set.as_deref() == Some(set.iri.as_str()))
            .collect();
        links.sort_by(|a, b| {
            a.subject_curie
                .cmp(&b.subject_curie)
                .then_with(|| a.object.cmp(&b.object))
        });
        if !links.is_empty() {
            push_line(
                &mut out,
                "| Subject | Predicate | External object | Justification | Conf. |",
            );
            push_line(&mut out, "| --- | --- | --- | --- | --- |");
            for link in links {
                push_line(
                    &mut out,
                    &format!(
                        "| {} | `{}` | [{}]({}) | {} | {} |",
                        subject_link(model, &from, link),
                        code_escape(&link.predicate),
                        md_escape(&link.object),
                        md_escape(&link.object),
                        link.justification
                            .as_deref()
                            .map(|j| format!("`{}`", code_escape(j)))
                            .unwrap_or_default(),
                        confidence_cell(link.confidence),
                    ),
                );
            }
            blank(&mut out);
        }
    }

    // Any linkages not attached to a known mapping set (defensive completeness).
    let mut orphans: Vec<&crate::model::DocLinkage> = model
        .linkages
        .iter()
        .filter(|l| {
            l.mapping_set
                .as_deref()
                .map(|m| !model.mapping_sets.iter().any(|s| s.iri == m))
                .unwrap_or(true)
        })
        .collect();
    orphans.sort_by(|a, b| {
        a.subject_curie
            .cmp(&b.subject_curie)
            .then_with(|| a.object.cmp(&b.object))
    });
    if !orphans.is_empty() {
        heading(&mut out, 2, model.ui("body_other_equivalences"));
        push_line(
            &mut out,
            "| Subject | Predicate | External object | Conf. |",
        );
        push_line(&mut out, "| --- | --- | --- | --- |");
        for link in orphans {
            push_line(
                &mut out,
                &format!(
                    "| {} | `{}` | [{}]({}) | {} |",
                    subject_link(model, &from, link),
                    code_escape(&link.predicate),
                    md_escape(&link.object),
                    md_escape(&link.object),
                    confidence_cell(link.confidence),
                ),
            );
        }
        blank(&mut out);
    }

    out
}

fn md_example_index(model: &DocsModel) -> String {
    let from = Page::ExampleIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_examples"));
    line(
        &mut out,
        &format!(
            "**{}** worked example(s). Each example's Turtle source is shown in full on its \
             owning slice page.",
            model.examples.len()
        ),
    );

    // Group examples by slice (model.examples is slice/path-sorted).
    let mut by_slice: BTreeMap<String, Vec<&crate::model::DocExample>> = BTreeMap::new();
    for example in &model.examples {
        by_slice
            .entry(example.slice.clone())
            .or_default()
            .push(example);
    }
    for (slice_iri, mut examples) in by_slice {
        examples.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
        heading(&mut out, 2, &slice_name(model, &slice_iri));
        let slice_href = model
            .slices
            .iter()
            .find(|s| s.iri == slice_iri)
            .map(|s| rel(&from, &Page::Slice(slice_slug(s)).dir()));
        for example in examples {
            match &slice_href {
                Some(href) => push_line(
                    &mut out,
                    &format!(
                        "- [{}]({}index.md) — `{}`",
                        md_escape(&example.title),
                        href,
                        code_escape(&example.logical_path)
                    ),
                ),
                None => push_line(
                    &mut out,
                    &format!(
                        "- {} — `{}`",
                        md_escape(&example.title),
                        code_escape(&example.logical_path)
                    ),
                ),
            }
        }
        blank(&mut out);
    }

    // ── Worked math instances (ℚ⁷ SI-dimension worked examples) ─────────────
    // Distinct from the example listing above: every example subject (in any
    // slice) carrying `math:hasDimension`, resolved generically by
    // `gmeow_docs::model::extract_worked_instances` — not special-cased to
    // `measure-and-dimension.ttl` (today's only author). Placed on THIS page
    // (rather than a new dedicated `Page` variant) because a worked instance
    // IS a worked example — the same `examples/*.ttl` scan the loss-row
    // section reuses, and `Page::ExampleIndex` is already the page readers
    // reach for "show me a concrete instance", so it stays their next stop
    // rather than a fourth example-adjacent page.
    heading(&mut out, 2, model.ui("body_worked_instances"));
    line(
        &mut out,
        "Concrete quantities/measures/functions carrying `math:hasDimension`, resolved to their \
         ℚ⁷ SI base-dimension exponent vector (mass · length · time · electric current · \
         temperature · amount of substance · luminous intensity) when the dimension object \
         breaks one down — a dimensionless subject (e.g. `math:dimensionless`) honestly renders \
         with no exponent rows, not a fabricated breakdown.",
    );
    if model.worked_instances.is_empty() {
        line(&mut out, model.ui("body_no_worked_instances"));
    } else {
        for instance in &model.worked_instances {
            let type_suffix = if instance.types.is_empty() {
                String::new()
            } else {
                format!(" ({})", instance.types.join(", "))
            };
            heading(
                &mut out,
                3,
                &format!(
                    "{}{}",
                    md_escape(&instance.subject),
                    md_escape(&type_suffix)
                ),
            );
            line(
                &mut out,
                &format!(
                    "`{}` — `{}`",
                    code_escape(&slice_name(model, &instance.slice)),
                    code_escape(&instance.logical_path)
                ),
            );
            if let Some(label) = &instance.label {
                line(&mut out, &md_escape(label));
            }
            if let Some(dimension_label) = &instance.dimension_label {
                line(
                    &mut out,
                    &format!("Dimension: {}", md_escape(dimension_label)),
                );
            }
            if instance.dimension_exponents.is_empty() {
                line(
                    &mut out,
                    "Dimensionless — no base-dimension exponent breakdown.",
                );
            } else {
                push_line(&mut out, "| Base dimension | Numerator | Denominator |");
                push_line(&mut out, "| --- | --- | --- |");
                for exponent in &instance.dimension_exponents {
                    push_line(
                        &mut out,
                        &format!(
                            "| `{}` | {} | {} |",
                            code_escape(&exponent.base_dimension),
                            exponent.numerator,
                            exponent.denominator
                        ),
                    );
                }
                blank(&mut out);
            }
            fenced(&mut out, "turtle", &instance.turtle);
        }
    }

    out
}

/// The filename stem of a fixture's slice-relative logical path (no directory,
/// no `.ttl` extension) — the basis for the best-effort Do/Don't pairing.
fn fixture_stem(logical_path: &str) -> &str {
    logical_path
        .rsplit('/')
        .next()
        .unwrap_or(logical_path)
        .trim_end_matches(".ttl")
}

/// The number of leading `-`-separated tokens two fixture stems share (e.g.
/// `plan-wellformed` / `plan-missing-successmode` share 1: `plan`). Used to
/// pick, for a counter-example, the best-matching well-formed fixture in the
/// same slice — the repo's fixture-naming convention puts the shared concept
/// stem first and the wellformed/violating descriptor last.
fn shared_prefix_tokens(a: &str, b: &str) -> usize {
    a.split('-')
        .zip(b.split('-'))
        .take_while(|(x, y)| x == y)
        .count()
}

/// Render a fixture's expected outcome as a Markdown link into the constraint
/// catalog when [`DocFixture::catalog_slug`] resolved a genuine match, else a
/// plain code span (never a dead link).
fn fixture_violation_code_display(
    from: &str,
    fixture: &crate::model::DocFixture,
) -> Option<String> {
    let code = fixture.violation_code.as_ref()?;
    Some(match &fixture.catalog_slug {
        Some(slug) => {
            let catalog_href = rel(from, &Page::ConstraintCatalog.dir());
            format!("[`{}`]({catalog_href}index.md#{slug})", code_escape(code))
        }
        None => format!("`{}`", code_escape(code)),
    })
}

/// Push the Do/Don't detail bullets shared by the fixture index and the
/// per-term conformance-examples section: the violation code (catalog-linked
/// when resolved) and the rationale, when the fixture carries a binding.
fn push_fixture_binding_bullets(
    out: &mut String,
    model: &DocsModel,
    from: &str,
    fixture: &crate::model::DocFixture,
) {
    if let Some(code_display) = fixture_violation_code_display(from, fixture) {
        push_line(
            out,
            &format!(
                "  - **{}:** {code_display}",
                model.ui("body_label_violation_code")
            ),
        );
    }
    if let Some(rationale) = &fixture.rationale {
        push_line(out, &format!("  - {}", md_escape(&one_line(rationale))));
    }
}

/// The conformance Do/Don't fixtures index, grouped by slice. Each
/// counter-example is paired with its best-matching well-formed fixture in the
/// same slice (by shared filename-stem prefix tokens) when one exists; a
/// well-formed fixture never picked as a pair partner is listed once on its
/// own so none are lost.
fn md_fixture_index(model: &DocsModel) -> String {
    let from = Page::FixtureIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_conformance_fixtures"));
    line(
        &mut out,
        &format!(
            "**{}** conformance fixture(s) — well-formed instances and deliberately \
             malformed counter-examples used to pin per-slice validation behaviour. Each \
             counter-example isolates ONE violation; its \"{}\" entry names the expected \
             `sh:*ConstraintComponent` code and rationale from the owning slice's \
             `tests/example-conformance.ttl` binding, when one is authored.",
            model.fixtures.len(),
            model.ui("body_label_dont"),
        ),
    );
    if model.fixtures.is_empty() {
        line(&mut out, model.ui("body_no_conformance_fixtures"));
        return out;
    }

    let mut by_slice: BTreeMap<&str, Vec<&crate::model::DocFixture>> = BTreeMap::new();
    for fixture in &model.fixtures {
        by_slice
            .entry(fixture.slice.as_str())
            .or_default()
            .push(fixture);
    }

    for (slice_iri, fixtures) in by_slice {
        heading(&mut out, 2, &slice_name(model, slice_iri));

        let wellformed: Vec<&crate::model::DocFixture> = fixtures
            .iter()
            .copied()
            .filter(|f| f.kind == crate::model::DocFixtureKind::Wellformed)
            .collect();
        let counter_examples: Vec<&crate::model::DocFixture> = fixtures
            .iter()
            .copied()
            .filter(|f| f.kind == crate::model::DocFixtureKind::CounterExample)
            .collect();

        let mut paired_wellformed: std::collections::BTreeSet<&str> =
            std::collections::BTreeSet::new();

        for ce in &counter_examples {
            let ce_stem = fixture_stem(&ce.logical_path);
            let best = wellformed
                .iter()
                .copied()
                .filter_map(|wf| {
                    let n = shared_prefix_tokens(fixture_stem(&wf.logical_path), ce_stem);
                    (n > 0).then_some((n, wf))
                })
                .max_by(|(n1, wf1), (n2, wf2)| {
                    n1.cmp(n2)
                        .then_with(|| wf2.logical_path.cmp(&wf1.logical_path))
                });

            heading(&mut out, 3, &ce.title);
            if let Some((_, wf)) = best {
                paired_wellformed.insert(wf.logical_path.as_str());
                push_line(
                    &mut out,
                    &format!(
                        "- **{}:** `{}`",
                        model.ui("body_label_do"),
                        code_escape(&wf.logical_path)
                    ),
                );
            }
            push_line(
                &mut out,
                &format!(
                    "- **{}:** `{}`",
                    model.ui("body_label_dont"),
                    code_escape(&ce.logical_path)
                ),
            );
            push_fixture_binding_bullets(&mut out, model, &from, ce);
            blank(&mut out);
        }

        // Well-formed fixtures never picked as a pair partner (e.g. a standalone
        // flagship-scenario fixture with no counter-example twin) — listed once
        // so none are silently dropped from the index.
        for wf in &wellformed {
            if paired_wellformed.contains(wf.logical_path.as_str()) {
                continue;
            }
            heading(&mut out, 3, &wf.title);
            push_line(
                &mut out,
                &format!(
                    "- **{}:** `{}`",
                    model.ui("body_label_do"),
                    code_escape(&wf.logical_path)
                ),
            );
            push_fixture_binding_bullets(&mut out, model, &from, wf);
            blank(&mut out);
        }
    }
    out
}

/// The competency-questions index, grouped by slice. Each question renders its
/// rationale, the terms it exercises (linked), the full copy-paste-runnable
/// SPARQL query, and its expected result — either an enumerated row table or a
/// pinned row count.
fn md_competency_index(model: &DocsModel) -> String {
    let from = Page::CompetencyIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_competency_questions"));
    line(
        &mut out,
        &format!(
            "**{}** competency question(s) — declarative SPARQL questions the ontology must \
             answer, each pinning an expected result over the asserted merged ontology (or its \
             RDFS/native-reasoned closure, when the question opts in).",
            model.competencies.len(),
        ),
    );
    if model.competencies.is_empty() {
        line(&mut out, model.ui("body_no_competency_questions"));
        return out;
    }

    let mut by_slice: BTreeMap<&str, Vec<&crate::model::DocCompetency>> = BTreeMap::new();
    for cq in &model.competencies {
        by_slice
            .entry(cq.owner_slice.as_str())
            .or_default()
            .push(cq);
    }

    for (slice_iri, cqs) in by_slice {
        heading(&mut out, 2, &slice_name(model, slice_iri));
        for cq in cqs {
            heading(&mut out, 3, local_name(&cq.iri));

            if let Some(rationale) = &cq.rationale {
                let text = md_escape(&one_line(rationale));
                match &cq.query_file {
                    Some(qf) => push_line(&mut out, &format!("- {text} (`{}`)", code_escape(qf))),
                    None => push_line(&mut out, &format!("- {text}")),
                }
            }

            if !cq.exercises.is_empty() {
                let links: Vec<String> = cq
                    .exercises
                    .iter()
                    .map(|t| term_link(model, &from, t))
                    .collect();
                push_line(
                    &mut out,
                    &format!(
                        "- **{}:** {}",
                        model.ui("body_terms_used"),
                        links.join(", ")
                    ),
                );
            }

            match (cq.exact_rows, cq.expected_row_count) {
                (Some(true), _) => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** exactly {} row(s) (closed set)",
                        model.ui("body_expected_rows"),
                        cq.expected_rows.len()
                    ),
                ),
                (Some(false), _) => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** at least {} row(s) (subset)",
                        model.ui("body_expected_rows"),
                        cq.expected_rows.len()
                    ),
                ),
                (None, Some(n)) => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** exactly {n} row(s)",
                        model.ui("body_expected_rows")
                    ),
                ),
                (None, None) if !cq.expected_rows.is_empty() => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** {} row(s)",
                        model.ui("body_expected_rows"),
                        cq.expected_rows.len()
                    ),
                ),
                (None, None) => {}
            }
            blank(&mut out);

            push_line(&mut out, &format!("**{}:**", model.ui("body_query")));
            blank(&mut out);
            match &cq.query_text {
                Some(text) => fenced(&mut out, "sparql", text),
                None => line(&mut out, model.ui("body_no_query_text")),
            }

            if !cq.expected_rows.is_empty() {
                let mut vars: Vec<&str> = cq
                    .expected_rows
                    .iter()
                    .flat_map(|row| row.cells.iter().filter_map(|c| c.var.as_deref()))
                    .collect();
                vars.sort();
                vars.dedup();
                if !vars.is_empty() {
                    push_line(&mut out, &format!("| {} |", vars.join(" | ")));
                    push_line(
                        &mut out,
                        &format!(
                            "| {} |",
                            vars.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")
                        ),
                    );
                    for row in &cq.expected_rows {
                        let cells: Vec<String> = vars
                            .iter()
                            .map(|v| {
                                let Some(cell) =
                                    row.cells.iter().find(|c| c.var.as_deref() == Some(*v))
                                else {
                                    return String::new();
                                };
                                if let Some(iri) = &cell.value_iri {
                                    term_link(model, &from, iri)
                                } else if let Some(lit) = &cell.value_literal {
                                    format!("`{}`", code_escape(lit))
                                } else {
                                    String::new()
                                }
                            })
                            .collect();
                        push_line(&mut out, &format!("| {} |", cells.join(" | ")));
                    }
                    blank(&mut out);
                }
            }
        }
    }
    out
}

/// The notation-grammars index — every first-class `lang:Grammar` rendering
/// (GMN / GTS / Turtle) authored as plain W3C EBNF text under
/// `slices/grounding/lang/grammars/*.ebnf`, listed with its title and
/// license, each linking to the full source on its own [`Page::Grammar`] page.
fn md_notation_index(model: &DocsModel) -> String {
    let from = Page::NotationIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_notation_grammars"));
    line(
        &mut out,
        &format!(
            "**{}** notation grammar(s) — first-class W3C EBNF renderings of the sign systems \
             this project's own serialization notations use (the GMN compact record notation, \
             the GTS textual surface, and the RDF 1.1 Turtle grammar the native codec \
             interprets). Each is a RENDERING of the corresponding `lang:Grammar` object, never \
             a second parser: the grammar object itself carries the normative claims.",
            model.grammars.len()
        ),
    );
    if model.grammars.is_empty() {
        line(&mut out, model.ui("body_no_notation_grammars"));
        return out;
    }
    push_line(&mut out, "| Grammar | License |");
    push_line(&mut out, "| --- | --- |");
    for grammar in &model.grammars {
        let href = rel(&from, &Page::Grammar(grammar.slug.clone()).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | `{}` |",
                md_escape(&grammar.title),
                href,
                code_escape(&grammar.license),
            ),
        );
    }
    blank(&mut out);
    out
}

/// A single notation-grammar detail page: title, slug, license, and the full
/// W3C EBNF source in a fenced code block.
fn md_grammar(model: &DocsModel, slug: &str) -> String {
    let Some(grammar) = model.grammars.iter().find(|g| g.slug == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_grammar_not_found"));
        return out;
    };
    let mut out = String::new();
    heading(&mut out, 1, &grammar.title);
    line(
        &mut out,
        &format!(
            "`{}` · notation grammar · License: `{}`",
            code_escape(&grammar.slug),
            code_escape(&grammar.license)
        ),
    );
    heading(&mut out, 2, model.ui("body_grammar_source"));
    fenced(&mut out, "ebnf", &grammar.source);
    out
}

/// The build-pipeline DAG page: the deterministic SVG plus a stage table (impl,
/// capabilities, resources, consumes count) and the plan's goal + success mode.
/// A pure render of `model.pipeline` (the source-lane build graph).
fn md_pipeline_dag(model: &DocsModel) -> String {
    let from = Page::PipelineDag.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_build_pipeline"));
    let Some(pipeline) = &model.pipeline else {
        line(&mut out, model.ui("body_no_pipeline"));
        return out;
    };
    line(
        &mut out,
        &format!(
            "The dogfooded GMEOW build DAG, authored as data in \
             `slices/core/pipeline/module.ttl` and read back by the `gmeow-pipeline` \
             executor: **{}** `gmeow:PipelineStage` node(s) wired by **{}** dataflow \
             edge(s). Exactly one stage holds `gmeow:sinkCapability` — the single \
             serialization exit (the gts narrow waist), highlighted in the diagram.",
            pipeline.stages.len(),
            pipeline.edges.len(),
        ),
    );

    // The plan facets (a `gmeow:Pipeline` IS a `logic:Plan`).
    if pipeline.goal.is_some() || pipeline.success_mode.is_some() {
        heading(&mut out, 2, model.ui("body_goal"));
        if let Some(goal) = &pipeline.goal {
            push_line(
                &mut out,
                &format!(
                    "- **{}:** {}",
                    model.ui("body_goal"),
                    curie_link(model, &from, goal)
                ),
            );
        }
        if let Some(mode) = &pipeline.success_mode {
            push_line(
                &mut out,
                &format!(
                    "- **{}:** `{}`",
                    model.ui("body_pipeline_success_mode"),
                    code_escape(mode)
                ),
            );
        }
        blank(&mut out);
    }

    // The deterministic DAG SVG (emitted in `render_site_lang_exec`).
    heading(&mut out, 2, model.ui("body_pipeline_diagram"));
    push_line(
        &mut out,
        &format!(
            "![Build pipeline DAG]({}diagrams/pipeline.svg)",
            root_href(&from)
        ),
    );
    blank(&mut out);

    // The stage table.
    heading(&mut out, 2, model.ui("body_pipeline_stages"));
    push_line(
        &mut out,
        &format!(
            "| Stage | {} | {} | {} | {} |",
            model.ui("body_pipeline_implementation"),
            model.ui("body_pipeline_capabilities"),
            model.ui("body_pipeline_consumes"),
            model.ui("body_box_role"),
        ),
    );
    push_line(&mut out, "| --- | --- | --- | --- | --- |");
    for stage in &pipeline.stages {
        let name = curie_link(model, &from, &to_curie(&stage.iri));
        let impl_cell = match &stage.stage_impl {
            Some(module) => format!("`crates/pipeline/src/stages/{}.rs`", code_escape(module)),
            None => "—".to_string(),
        };
        let caps = pipeline_curie_cell(&stage.capabilities, &stage.resources);
        let box_role = stage
            .box_role
            .as_deref()
            .map(|r| format!("`{}`", code_escape(r)))
            .unwrap_or_else(|| "—".to_string());
        push_line(
            &mut out,
            &format!(
                "| {} | {} | {} | {} | {} |",
                name,
                impl_cell,
                caps,
                stage.consumes.len(),
                box_role,
            ),
        );
    }
    blank(&mut out);
    out
}

/// Render a stage's capability + resource CURIEs into one compact table cell
/// (each a code span), or an em-dash when the stage is a plain transform leaf.
fn pipeline_curie_cell(capabilities: &[String], resources: &[String]) -> String {
    let all: Vec<String> = capabilities
        .iter()
        .chain(resources.iter())
        .map(|c| format!("`{}`", code_escape(c)))
        .collect();
    if all.is_empty() {
        "—".to_string()
    } else {
        all.join(" ")
    }
}

/// Append the enriched build-pipeline stage section to a term page when the term
/// IS a `gmeow:PipelineStage` (its IRI matches a stage in `model.pipeline`). Renders
/// the Rust module binding, the consumes / consumed-by dataflow tables (from
/// `model.pipeline.edges`), the flowing named graphs (reified `gmeow:flowEntity`
/// where authored — honest absence otherwise), and the capabilities/resources.
///
/// No per-stage gate-verdict chip is rendered: `gmeow_errors::grade::GateVerdict`
/// is finding-scoped, and no genuine substrate attributes findings to one of the
/// 36 pipeline STAGE IRIs, so a per-stage verdict would be fabricated. Per the
/// proof-carrying doctrine, the honest static facts are rendered and the chip is
/// omitted (an honest computed-absence, not a scope gap).
fn append_stage_section(out: &mut String, model: &DocsModel, term: &DocTerm, from: &str) {
    let Some(pipeline) = &model.pipeline else {
        return;
    };
    let Some(stage) = pipeline.stages.iter().find(|s| s.iri == term.iri) else {
        return;
    };

    heading(out, 2, model.ui("body_pipeline_stage"));
    line(
        out,
        &format!(
            "This term is a stage of the [build pipeline]({}index.md) — a typed unit of \
             build work the `gmeow-pipeline` executor runs single-pass over the in-memory \
             dataset.",
            rel(from, &Page::PipelineDag.dir()),
        ),
    );

    if let Some(module) = &stage.stage_impl {
        push_line(
            out,
            &format!(
                "- **{}:** `crates/pipeline/src/stages/{}.rs`",
                model.ui("body_pipeline_implementation"),
                code_escape(module)
            ),
        );
    }
    if !stage.capabilities.is_empty() || !stage.resources.is_empty() {
        push_line(
            out,
            &format!(
                "- **{}:** {}",
                model.ui("body_pipeline_capabilities"),
                pipeline_curie_cell(&stage.capabilities, &stage.resources)
            ),
        );
    }
    if let Some(role) = &stage.box_role {
        push_line(
            out,
            &format!(
                "- **{}:** `{}`",
                model.ui("body_box_role"),
                code_escape(role)
            ),
        );
    }
    blank(out);

    // Consumes: the upstream producer stages this stage reads (each a term page).
    if !stage.consumes.is_empty() {
        heading(out, 3, model.ui("body_pipeline_consumes"));
        for producer in &stage.consumes {
            push_line(
                out,
                &format!("- {}", curie_link(model, from, &to_curie(producer))),
            );
        }
        blank(out);
    }

    // Attaches: the named graphs / blob-rep lanes this stage contributes to the carrier
    // as its delta (gmeow:attachesGraph / gmeow:attachesBlobRep) — the run-verified
    // declaration of what the stage produced, so `gmeow docs-on <stage>` self-explains.
    if !stage.attaches_graphs.is_empty() || !stage.attaches_blob_reps.is_empty() {
        heading(out, 3, model.ui("body_pipeline_attaches"));
        for graph in &stage.attaches_graphs {
            push_line(out, &format!("- `{}`", code_escape(graph)));
        }
        for rep in &stage.attaches_blob_reps {
            push_line(
                out,
                &format!(
                    "- {} `{}`",
                    model.ui("body_pipeline_attaches_blob"),
                    code_escape(rep)
                ),
            );
        }
        blank(out);
    }

    // Consumed by: the downstream stages that read THIS stage's product (the edge
    // reverse — `edges.from == this stage`).
    let mut consumed_by: Vec<&str> = pipeline
        .edges
        .iter()
        .filter(|e| e.from == stage.iri)
        .map(|e| e.to.as_str())
        .collect();
    consumed_by.sort_unstable();
    consumed_by.dedup();
    if !consumed_by.is_empty() {
        heading(out, 3, model.ui("body_pipeline_consumed_by"));
        for consumer in consumed_by {
            push_line(
                out,
                &format!("- {}", curie_link(model, from, &to_curie(consumer))),
            );
        }
        blank(out);
    }

    // Flowing graphs: the reified `gmeow:flowEntity` named graphs on any edge
    // touching this stage (sorted/deduped). Absent unless a `gmeow:BuildDataFlow`
    // authors them — honest computed-absence, so the heading is emitted only when
    // at least one flowing graph exists.
    let mut flowing: Vec<&str> = pipeline
        .edges
        .iter()
        .filter(|e| e.from == stage.iri || e.to == stage.iri)
        .flat_map(|e| e.flow_entities.iter().map(String::as_str))
        .collect();
    flowing.sort_unstable();
    flowing.dedup();
    if !flowing.is_empty() {
        heading(out, 3, model.ui("body_pipeline_flowing_graphs"));
        for graph in flowing {
            push_line(out, &format!("- `{}`", code_escape(graph)));
        }
        blank(out);
    }
}

/// The coarse-grain provenance chain for a durable page: the producing-stage path
/// walked BACKWARD over `gmeow:dataflowConsumes` from `start_local` (the stage
/// whose local name is `start_local`, default `stage-docs-render`), following the
/// lexicographically-smallest consumed producer at each step until a source-reading
/// stage (one that consumes nothing in-DAG) is reached. Cycle-safe (visited set).
/// Returns the stage local names in consumer→producer order, or empty when the
/// start stage is absent.
pub(crate) fn provenance_chain(
    pipeline: &crate::model::DocPipeline,
    start_local: &str,
) -> Vec<String> {
    use std::collections::BTreeSet;
    let by_iri: BTreeMap<&str, &crate::model::DocStage> = pipeline
        .stages
        .iter()
        .map(|s| (s.iri.as_str(), s))
        .collect();
    let Some(mut current) = pipeline
        .stages
        .iter()
        .find(|s| local_name(&s.iri) == start_local)
    else {
        return Vec::new();
    };
    let mut chain = vec![local_name(&current.iri).to_string()];
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    visited.insert(current.iri.as_str());
    // `next_iri` is cloned to an owned String so the borrow of `current.consumes`
    // ends before `current` is reassigned in the body (the condition temporary must
    // not outlive the reassignment).
    while let Some(next_iri) = current
        .consumes
        .iter()
        .filter(|p| !visited.contains(p.as_str()))
        .min()
        .cloned()
    {
        let Some(next) = by_iri.get(next_iri.as_str()) else {
            break;
        };
        chain.push(local_name(&next.iri).to_string());
        visited.insert(next.iri.as_str());
        current = *next;
    }
    chain
}

/// Append the per-page provenance footer: the producing-stage chain, the
/// build-grain projection of the single `gmeow:docGroundedBy` provenance relation.
/// A no-op when the model carries no pipeline (a bare unit-test model) — honest
/// absence, so the source-model goldens without a pipeline are unaffected.
fn append_provenance_footer(out: &mut String, model: &DocsModel) {
    let Some(pipeline) = &model.pipeline else {
        return;
    };
    let chain = provenance_chain(pipeline, "stage-docs-render");
    if chain.is_empty() {
        return;
    }
    let rendered = chain
        .iter()
        .map(|s| format!("`{}`", code_escape(s)))
        .collect::<Vec<_>>()
        .join(" ← ");
    blank(out);
    push_line(out, "---");
    blank(out);
    push_line(
        out,
        &format!(
            "**{}:** this page ← {}",
            model.ui("body_provenance"),
            rendered
        ),
    );
    push_line(
        out,
        "Rendered from `gmeow.gts` by the dogfooded build pipeline.",
    );
    blank(out);
}

fn md_concern_index(model: &DocsModel) -> String {
    let from = Page::ConcernIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_concerns"));
    line(
        &mut out,
        &format!(
            "**{}** cross-cutting documentation concern(s). Concerns group vocabulary terms by \
             the design question they answer, across slice boundaries.",
            model.concerns.len()
        ),
    );

    heading(&mut out, 2, model.ui("body_by_term_count"));
    push_line(
        &mut out,
        &format!(
            "![Concerns by term count]({}diagrams/concerns.svg)",
            root_href(&from)
        ),
    );
    blank(&mut out);

    push_line(&mut out, "| Concern | Terms | Slices |");
    push_line(&mut out, "| --- | --- | --- |");
    for concern in &model.concerns {
        let href = rel(&from, &Page::Concern(concern_slug(concern)).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} | {} |",
                md_escape(&concern_display(concern)),
                href,
                concern.terms.len(),
                concern.slices.len(),
            ),
        );
    }
    blank(&mut out);
    out
}

fn md_concern(model: &DocsModel, slug: &str) -> String {
    let Some(concern) = model.concerns.iter().find(|c| concern_slug(c) == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_concern_not_found"));
        return out;
    };
    let from = Page::Concern(slug.to_string()).dir();

    let mut out = String::new();
    heading(&mut out, 1, &concern_display(concern));
    line(&mut out, &format!("`{}`", code_escape(&concern.curie)));

    if let Some(def) = &concern.definition {
        heading(&mut out, 2, model.ui("body_definition"));
        line(&mut out, &md_escape(def));
    }

    if !concern.terms.is_empty() {
        heading(&mut out, 2, &format!("Terms ({})", concern.terms.len()));
        for curie in &concern.terms {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !concern.slices.is_empty() {
        heading(&mut out, 2, &format!("Slices ({})", concern.slices.len()));
        for slice_iri in &concern.slices {
            push_line(
                &mut out,
                &format!("- {}", slice_link(model, &from, slice_iri)),
            );
        }
        blank(&mut out);
    }
    out
}

fn md_external_index(model: &DocsModel) -> String {
    let from = Page::ExternalIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_external_ontologies"));
    line(
        &mut out,
        &format!(
            "**{}** external term(s) referenced across **{}** namespace(s). These are the \
             non-GMEOW IRIs GMEOW terms link to (via mappings) or inherit from / constrain (via \
             domain, range, super-class edges).",
            model.external_terms.len(),
            namespace_count(model),
        ),
    );

    // Group by namespace (deterministic: external_terms is IRI-sorted).
    let mut by_ns: BTreeMap<String, Vec<&crate::model::DocExternalTerm>> = BTreeMap::new();
    for term in &model.external_terms {
        by_ns.entry(term.namespace.clone()).or_default().push(term);
    }
    for (namespace, terms) in by_ns {
        heading(&mut out, 2, &namespace);
        push_line(&mut out, "| External term | Referenced by | Via |");
        push_line(&mut out, "| --- | --- | --- |");
        for term in terms {
            let referencers = term
                .referenced_by
                .iter()
                .map(|c| curie_link(model, &from, c))
                .collect::<Vec<_>>()
                .join(", ");
            push_line(
                &mut out,
                &format!(
                    "| [{}]({}) | {} | {} |",
                    md_escape(&term.iri),
                    md_escape(&term.iri),
                    referencers,
                    md_escape(&term.via_predicate.join(", ")),
                ),
            );
        }
        blank(&mut out);
    }
    out
}

fn md_integrity_index(model: &DocsModel) -> String {
    let from = Page::IntegrityIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_integrity_constraints"));
    line(
        &mut out,
        "Per-slice SPARQL verification queries. Each query asserts an integrity constraint the \
         slice's data must satisfy; a non-empty result is a violation.",
    );

    let mut any = false;
    for slice in &model.slices {
        let mut queries: Vec<&crate::model::DocArtifact> = slice
            .artifacts
            .iter()
            .filter(|a| a.role == purrdf::slice::ArtifactRole::VerifyQuery)
            .collect();
        queries.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
        if queries.is_empty() {
            continue;
        }
        any = true;
        heading(&mut out, 2, &slice_display(slice));
        let href = rel(&from, &Page::Slice(slice_slug(slice)).dir());
        line(&mut out, &format!("[Slice page]({href}index.md)"));
        for query in queries {
            heading(&mut out, 3, &query.logical_path);
            // The query text is not carried on DocArtifact (by-reference); note the
            // path and digest so the constraint is locatable in the slice tree.
            push_line(
                &mut out,
                &format!(
                    "- Path: `{}`  ·  digest `{}`",
                    code_escape(&query.logical_path),
                    code_escape(&short_digest(&query.raw_digest)),
                ),
            );
            blank(&mut out);
        }
    }
    if !any {
        line(&mut out, model.ui("body_no_verify_queries"));
    }
    out
}

/// The "What GMEOW enforces" page: the constraint catalog — every
/// `gmeow:ValidationRule` the toolchain enforces — grouped by finding category
/// and, within each category, sorted by rule code. Each rule entry carries an
/// explicit `<a id="{slug}">` anchor whose id is
/// `gmeow_validate::rule_catalog::slugify(code)` — the identical slug the
/// validator stamps into a finding's `helpUri` fragment — so a finding's help
/// link deep-links straight to its rule. Deterministic: categories, rules, and
/// applies-to terms are all sorted.
fn md_constraint_catalog(model: &DocsModel) -> String {
    let from = Page::ConstraintCatalog.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_enforced_constraints"));
    line(
        &mut out,
        &format!(
            "The **{}** validation rules the GMEOW toolchain enforces, grouped by finding \
             category. Each rule is a `gmeow:ValidationRule` individual from the constraint \
             catalog; a finding's `helpUri` deep-links to its entry here. **Binding** rules fail \
             the gate; **advisory** rules report without failing. The distinct **{}** section \
             below documents the non-gating *recommendations* (`avoidWhen` / `useWhen` / \
             `howToUse`) that advisory `advice.*` findings resolve to.",
            model.constraint_rules.len(),
            model.ui("body_usage_advice"),
        ),
    );

    if model.constraint_rules.is_empty() {
        line(&mut out, model.ui("body_no_enforced_constraints"));
        return out;
    }

    // Group by category IRI (rules are already sorted by code); the category map
    // is a BTreeMap so category headings emit in sorted IRI order. The `advice.`
    // family rule is EXCLUDED here — it heads the distinct Advice section below and
    // carries the single `#advice-` anchor, which must appear exactly once.
    let mut by_category: std::collections::BTreeMap<&str, Vec<&crate::model::ConstraintRule>> =
        std::collections::BTreeMap::new();
    for rule in &model.constraint_rules {
        if rule.code == gmeow_validate::codes::ADVICE_FAMILY {
            continue;
        }
        by_category
            .entry(rule.category.as_str())
            .or_default()
            .push(rule);
    }

    for (category, rules) in &by_category {
        heading(&mut out, 2, &finding_category_display(category));
        for rule in rules {
            // Explicit anchor so the finding helpUri fragment (`#{slug}`) resolves
            // regardless of the Markdown heading-id derivation. The slug is
            // `[a-z0-9-]` only (the validator's transform), so it is inert inside the
            // HTML `id` attribute — never md-escape it, or the id would gain
            // backslashes and stop matching the helpUri fragment.
            push_line(&mut out, &format!("<a id=\"{}\"></a>", rule.slug));
            blank(&mut out);
            heading(&mut out, 3, &rule.code);

            let severity = if rule.severity.is_empty() {
                "—".to_string()
            } else {
                rule.severity.clone()
            };
            push_line(
                &mut out,
                &format!(
                    "- {}: **{}**",
                    model.ui("body_label_severity"),
                    md_escape(&severity)
                ),
            );
            push_line(
                &mut out,
                &format!(
                    "- {}: `{}`",
                    model.ui("body_label_rule_code"),
                    code_escape(&rule.code)
                ),
            );
            if !rule.help_uri.is_empty() {
                // The display text is md-escaped; the link *target* is the raw URL
                // (escaping a target corrupts the href).
                push_line(
                    &mut out,
                    &format!(
                        "- {}: [{}]({})",
                        model.ui("body_label_help_link"),
                        md_escape(&rule.help_uri),
                        rule.help_uri,
                    ),
                );
            }
            if let Some(formalizes) = &rule.formalizes {
                push_line(
                    &mut out,
                    &format!(
                        "- {}: {}",
                        model.ui("body_formalized_by"),
                        curie_link(model, &from, &to_curie(formalizes)),
                    ),
                );
            }
            blank(&mut out);

            if let Some(definition) = &rule.definition {
                line(&mut out, &md_escape(definition));
            } else if let Some(label) = &rule.label {
                line(&mut out, &md_escape(label));
            }

            if !rule.applies_to_terms.is_empty() {
                push_line(&mut out, &format!("**{}**", model.ui("body_applies_to")));
                blank(&mut out);
                // applies_to_terms is already sorted by the model reader.
                for term in &rule.applies_to_terms {
                    push_line(
                        &mut out,
                        &format!("- {}", curie_link(model, &from, &to_curie(term))),
                    );
                }
                blank(&mut out);
            }
        }
    }

    // ── The Advice section: the recommendation tier, a distinct top-level
    // section headed by the `advice.` family rule's `#advice-` anchor. ────────────
    md_advice_section(&mut out, model, &from);
    out
}

/// Render the distinct "Advice" section: the non-gating recommendation tier. Headed
/// by the `advice.` family rule's `#advice-` anchor (the single static target every
/// advisory `advice.*` finding code resolves to), then one sub-entry per
/// [`crate::model::AdviceEntry`] carrying the term's verbatim avoid/use/how-to prose.
fn md_advice_section(out: &mut String, model: &DocsModel, from: &str) {
    // The `#advice-` anchor is the advice family rule's own slug — the guaranteed
    // resolution target of every advice.* helpUri fragment.
    let anchor = model
        .constraint_rules
        .iter()
        .find(|r| r.code == gmeow_validate::codes::ADVICE_FAMILY)
        .map(|r| r.slug.clone())
        .unwrap_or_else(|| {
            gmeow_validate::rule_catalog::slugify(gmeow_validate::codes::ADVICE_FAMILY)
        });
    heading(out, 2, model.ui("body_usage_advice"));
    push_line(out, &format!("<a id=\"{anchor}\"></a>"));
    blank(out);
    line(
        out,
        "Non-gating **recommendations** harvested verbatim from each term's usage prose \
         (`gmeow:avoidWhen` / `gmeow:useWhen` / `gmeow:howToUse`) and made machine-active as \
         realized advice carriers. Unlike the compliance rules above, advice NEVER fails the \
         gate — an advisory `advice.*` finding resolves to this section.",
    );

    if model.advice_entries.is_empty() {
        // Honest empty state: no realized advice carrier exists yet.
        line(out, model.ui("body_no_enforced_constraints"));
        return;
    }

    for entry in &model.advice_entries {
        // Per-term navigation sub-anchor (advice-<term-slug>); the guaranteed static
        // resolution target remains the section-head `#advice-` above.
        push_line(out, &format!("<a id=\"{}\"></a>", entry.slug));
        blank(out);
        let title = entry.label.clone().unwrap_or_else(|| to_curie(&entry.term));
        heading(out, 3, &md_escape(&title));
        push_line(
            out,
            &format!("- {}", curie_link(model, from, &to_curie(&entry.term))),
        );
        blank(out);
        if let Some(definition) = &entry.definition {
            line(out, &md_escape(definition));
        }
        // The three deontic-modality legs; each rendered only when present.
        for (label, values) in [
            (model.ui("body_advice_avoid_when"), &entry.avoid_when),
            (model.ui("body_advice_use_when"), &entry.use_when),
            (model.ui("body_advice_how_to_use"), &entry.how_to_use),
        ] {
            for value in values {
                push_line(out, &format!("- **{label}:** {}", md_escape(value)));
            }
        }
        blank(out);
    }
}

/// A human-readable display name for a `logic:FindingCategory` IRI (e.g.
/// `…/logic/FindingPolicyWarning` → `Finding: Policy Warning`). Falls back to the
/// raw IRI's local name when it is not a recognized `Finding…` class.
fn finding_category_display(iri: &str) -> String {
    let local = iri.rsplit(['/', '#']).next().unwrap_or(iri);
    let stem = local.strip_prefix("Finding").unwrap_or(local);
    // Split the CamelCase stem into spaced words.
    let mut words = String::new();
    for (i, ch) in stem.char_indices() {
        if i > 0 && ch.is_ascii_uppercase() {
            words.push(' ');
        }
        words.push(ch);
    }
    if stem == local {
        // Not a `Finding…` class — surface the local name verbatim.
        words
    } else {
        format!("Finding: {words}")
    }
}

// ── Guides: recipes / learning paths / four boxes ──────────────────────────────

fn md_recipe_index(model: &DocsModel) -> String {
    let from = Page::RecipeIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_recipes"));
    line(
        &mut out,
        &format!(
            "**{}** task-oriented adoption recipes — short, goal-named guides that show how to \
             model one recurring task in GMEOW, each backed by canonical example files and the \
             vocabulary terms it exercises.",
            model.recipes.len()
        ),
    );
    if model.recipes.is_empty() {
        line(&mut out, model.ui("body_no_recipes"));
        return out;
    }
    push_line(&mut out, "| Recipe | Goal |");
    push_line(&mut out, "| --- | --- |");
    // model.recipes is already sorted by slug.
    for recipe in &model.recipes {
        let href = rel(&from, &Page::Recipe(recipe.slug.clone()).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} |",
                md_escape(&recipe.title),
                href,
                md_escape(&one_line(&recipe.goal)),
            ),
        );
    }
    blank(&mut out);
    out
}

fn md_recipe(model: &DocsModel, slug: &str) -> String {
    let Some(recipe) = model.recipes.iter().find(|r| r.slug == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_recipe_not_found"));
        return out;
    };
    let from = Page::Recipe(slug.to_string()).dir();
    let mut out = String::new();
    heading(&mut out, 1, &recipe.title);
    line(
        &mut out,
        &format!("`{}` · recipe", code_escape(&recipe.slug)),
    );

    heading(&mut out, 2, model.ui("body_goal"));
    line(&mut out, &md_escape(&recipe.goal));

    // ── Quickstart (composed over every member term, so a promoted recipe is
    // copy-paste-runnable as a whole, not just per-term) ────────────────────────
    if !recipe.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_quickstart"));
        let skeleton = synthesize_composed_quickstart(model, &recipe.term_curies);
        fenced(&mut out, "turtle", skeleton.trim_end());
        blank(&mut out);
    }

    if !recipe.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_terms_used"));
        for curie in &recipe.term_curies {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !recipe.example_paths.is_empty() {
        heading(&mut out, 2, model.ui("body_example_files"));
        for path in &recipe.example_paths {
            push_line(&mut out, &format!("- `{}`", code_escape(path)));
        }
        blank(&mut out);
    }

    if !recipe.follow_pages.is_empty() {
        heading(&mut out, 2, model.ui("body_read_next"));
        for page in &recipe.follow_pages {
            push_line(&mut out, &format!("- `{}`", code_escape(page)));
        }
        blank(&mut out);
    }

    // Learning paths that fold this recipe in.
    let mut hosting: Vec<&crate::model::DocLearningPath> = model
        .learning_paths
        .iter()
        .filter(|p| p.recipe_slugs.iter().any(|s| s == slug))
        .collect();
    hosting.sort_by(|a, b| a.slug.cmp(&b.slug));
    if !hosting.is_empty() {
        heading(&mut out, 2, model.ui("body_part_of"));
        for path in hosting {
            let href = rel(&from, &Page::LearningPath(path.slug.clone()).dir());
            push_line(
                &mut out,
                &format!("- [{}]({}index.md)", md_escape(&path.title), href),
            );
        }
        blank(&mut out);
    }
    out
}

fn md_learning_path_index(model: &DocsModel) -> String {
    let from = Page::LearningPathIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_learning_paths"));
    line(
        &mut out,
        &format!(
            "**{}** curated adoption journeys — ordered itineraries that sequence recipes, \
             example files, and vocabulary terms so a developer learns to model a whole area end \
             to end.",
            model.learning_paths.len()
        ),
    );
    if model.learning_paths.is_empty() {
        line(&mut out, model.ui("body_no_learning_paths"));
        return out;
    }
    push_line(&mut out, "| Learning path | Audience | Goal |");
    push_line(&mut out, "| --- | --- | --- |");
    // model.learning_paths is already sorted by slug.
    for path in &model.learning_paths {
        let href = rel(&from, &Page::LearningPath(path.slug.clone()).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} | {} |",
                md_escape(&path.title),
                href,
                md_escape(&one_line(&path.audience)),
                md_escape(&one_line(&path.goal)),
            ),
        );
    }
    blank(&mut out);
    out
}

fn md_learning_path(model: &DocsModel, slug: &str) -> String {
    let Some(path) = model.learning_paths.iter().find(|p| p.slug == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_learning_path_not_found"));
        return out;
    };
    let from = Page::LearningPath(slug.to_string()).dir();
    let mut out = String::new();
    heading(&mut out, 1, &path.title);
    line(
        &mut out,
        &format!("`{}` · learning path", code_escape(&path.slug)),
    );

    push_line(&mut out, "| Field | Value |");
    push_line(&mut out, "| --- | --- |");
    push_line(
        &mut out,
        &format!("| Audience | {} |", md_escape(&one_line(&path.audience))),
    );
    blank(&mut out);

    heading(&mut out, 2, model.ui("body_goal"));
    line(&mut out, &md_escape(&path.goal));

    // ── Quickstart (composed over every member term, so a promoted learning
    // path is copy-paste-runnable as a whole, not just per-term) ───────────────
    if !path.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_quickstart"));
        let skeleton = synthesize_composed_quickstart(model, &path.term_curies);
        fenced(&mut out, "turtle", skeleton.trim_end());
        blank(&mut out);
    }

    if !path.recipe_slugs.is_empty() {
        heading(&mut out, 2, model.ui("body_recipes"));
        for recipe_slug in &path.recipe_slugs {
            let title = model
                .recipes
                .iter()
                .find(|r| &r.slug == recipe_slug)
                .map(|r| r.title.clone())
                .unwrap_or_else(|| recipe_slug.clone());
            // Link only when the recipe page exists in the model.
            if model.recipes.iter().any(|r| &r.slug == recipe_slug) {
                let href = rel(&from, &Page::Recipe(recipe_slug.clone()).dir());
                push_line(
                    &mut out,
                    &format!("- [{}]({}index.md)", md_escape(&title), href),
                );
            } else {
                push_line(&mut out, &format!("- {}", md_escape(&title)));
            }
        }
        blank(&mut out);
    }

    if !path.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_terms_used"));
        for curie in &path.term_curies {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !path.example_paths.is_empty() {
        heading(&mut out, 2, model.ui("body_example_files"));
        for p in &path.example_paths {
            push_line(&mut out, &format!("- `{}`", code_escape(p)));
        }
        blank(&mut out);
    }

    if !path.adoption_targets.is_empty() {
        heading(&mut out, 2, model.ui("body_projects_toward"));
        let cells: Vec<String> = path
            .adoption_targets
            .iter()
            .map(|t| format!("`{}`", code_escape(t)))
            .collect();
        line(&mut out, &cells.join(", "));
    }
    out
}

fn md_four_boxes(model: &DocsModel) -> String {
    let mut out = String::new();
    // The curated prose is authored Markdown; emit it verbatim (its SPDX comment
    // header is an HTML comment and renders inertly). Fall back to a stub when
    // absent so the page is never empty.
    match &model.four_boxes {
        Some(prose) => {
            out.push_str(prose);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        None => {
            heading(&mut out, 1, model.ui("body_what_is_this"));
            line(
                &mut out,
                "The four-boxes doctrine prose is not available in this build.",
            );
        }
    }
    out
}

// ── HTML layer ────────────────────────────────────────────────────────────────

/// Render a page to a complete, self-contained HTML document (English chrome).
///
/// Equivalent to [`to_html_lang`] with the English carrier language.
pub fn to_html(model: &DocsModel, page: &Page) -> String {
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
pub fn to_html_lang(model: &DocsModel, page: &Page, lang: &str) -> String {
    to_html_lang_exec(model, page, lang, &ExecutableDocsData::default())
}

/// Render a page to a complete HTML document with the executable-docs data: appends
/// the reasoner/export sections to the body, a SPARQL nav entry, and the playground
/// controller script — all only when `exec` supplies data (byte-identical to
/// [`to_html_lang`] otherwise).
pub fn to_html_lang_exec(
    model: &DocsModel,
    page: &Page,
    lang: &str,
    exec: &ExecutableDocsData,
) -> String {
    // Standalone render builds the map here; a full site render threads one shared map
    // through [`to_html_lang_exec_with_map`] (see [`to_markdown_exec`]).
    let page_map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");
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
) -> String {
    let body_html = rewrite_internal_links(&markdown_to_html(&to_markdown_exec_with_map(
        model, page, exec, page_map,
    )));
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
    let body_scripts = if (matches!(page, Page::SparqlPlayground) && exec.has_playground())
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
    tmpl.render(context! {
        site_title => model.title,
        site_version => model.version,
        page_title => page.title(model),
        css_href => format!("{root}{CSS_PATH}"),
        root_href => root,
        nav => nav,
        body => body_html,
        page_lang => page_lang,
        body_scripts => body_scripts,
    })
    .expect("shell template renders")
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

/// A nav menu entry (label + resolved href), used as a pre-sorted minijinja
/// loop input.
#[derive(serde::Serialize)]
struct NavItem {
    label: String,
    href: String,
}

fn nav_item(root: &str, dir: &str, label: &str) -> NavItem {
    NavItem {
        label: label.to_string(),
        href: format!("{root}{}", join(dir, "index.html")),
    }
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

/// A short, stable IRI discriminator: the first 12 hex chars of the full IRI's
/// BLAKE3 digest — a deterministic, order-independent tiebreak for the rare case
/// where two distinct terms share BOTH a base slug and a category.
fn short_iri_digest(iri: &str) -> String {
    blake3::hash(iri.as_bytes()).to_hex()[..12].to_owned()
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
#[derive(serde::Serialize)]
struct SearchRecord {
    /// The record kind (`term`, `slice`, `concern`, `mappingSet`).
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    advice: Vec<String>,
    /// Crosswalk facet — `tag:object` tokens (terms only); omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    alignments: Vec<String>,
    /// Documentation-coverage facet — machine keys of the dimensions a term is
    /// MISSING (terms only), so a client can filter under-documented terms; omitted
    /// from JSON when the term is fully covered.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    missing_coverage: Vec<&'static str>,
}

/// Build the deterministic `search-index.json`: one record per term, slice,
/// concern, and mapping set, sorted by URL. A pure function of the model.
pub fn search_index_json(model: &DocsModel) -> String {
    let page_map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");
    search_index_json_with_map(model, &page_map)
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

    records.sort_by(|a, b| {
        a.url
            .cmp(&b.url)
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });
    // serde_json with pretty printing is deterministic for a Vec.
    serde_json::to_string_pretty(&records).expect("search records serialize")
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
pub fn llms_txt(model: &DocsModel) -> String {
    let page_map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");
    llms_txt_with_map(model, &page_map)
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
pub fn llms_full_txt(model: &DocsModel) -> String {
    let page_map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");
    llms_full_txt_with_map(model, &page_map)
}

/// [`llms_full_txt`] with the shared [`SourceToPageMap`] threaded in (built once per site
/// render). Byte-identical to `llms_full_txt`.
pub(crate) fn llms_full_txt_with_map(model: &DocsModel, doc_map: &SourceToPageMap) -> String {
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
                );
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

    out
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
    serde_json::to_vec(&standard).expect("a pure-data Card of String/Vec/Option fields serializes")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DocTermStability;

    #[test]
    fn rel_computes_relative_dir_paths() {
        assert_eq!(rel("", "slices"), "slices/");
        assert_eq!(rel("slices", ""), "../");
        assert_eq!(rel("terms/cat", "slices/zoo"), "../../slices/zoo/");
        assert_eq!(rel("terms/cat", "terms/cat"), "");
        assert_eq!(rel("classes", "terms/cat"), "../terms/cat/");
    }

    #[test]
    fn root_href_counts_depth() {
        assert_eq!(root_href(""), "");
        assert_eq!(root_href("slices"), "../");
        assert_eq!(root_href("terms/cat"), "../../");
    }

    fn term(iri: &str, category: DocTermCategory) -> DocTerm {
        DocTerm {
            iri: iri.to_string(),
            category,
            ..Default::default()
        }
    }

    #[test]
    fn stage_page_self_explains_its_attached_graphs_and_blob_reps() {
        // docs-on <stage> surfaces the stage's declared carrier contribution: the
        // attached graph/documentation + the attached blob-rep lanes (Step 4 self-explain).
        use crate::model::{DocPipeline, DocStage};
        let stage_iri = "https://blackcatinformatics.ca/gmeow/stage-docs-render";
        let model = DocsModel {
            pipeline: Some(DocPipeline {
                stages: vec![DocStage {
                    iri: stage_iri.to_string(),
                    attaches_graphs: vec![
                        "https://blackcatinformatics.ca/gmeow/graph/documentation".to_string(),
                    ],
                    attaches_blob_reps: vec!["diagnostics:nodes".to_string()],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let doc_term = term(stage_iri, DocTermCategory::Individual);
        let mut out = String::new();
        append_stage_section(&mut out, &model, &doc_term, "terms/x");
        assert!(
            out.contains(model.ui("body_pipeline_attaches")),
            "the stage page must carry an Attaches heading: {out}"
        );
        assert!(
            out.contains("https://blackcatinformatics.ca/gmeow/graph/documentation"),
            "the stage page must surface the attached graph/documentation: {out}"
        );
        assert!(
            out.contains("diagnostics:nodes"),
            "the stage page must surface the attached blob-rep lane: {out}"
        );
    }

    #[test]
    fn resolve_term_slugs_disambiguates_only_colliders_injectively() {
        let base = "https://blackcatinformatics.ca/gmeow/";
        let terms = vec![
            // A unique base — absent from the map, keeps its base slug.
            term(&format!("{base}Solo"), DocTermCategory::Class),
            // A class/property case-collision on `acceptancestatus`.
            term(&format!("{base}AcceptanceStatus"), DocTermCategory::Class),
            term(
                &format!("{base}acceptanceStatus"),
                DocTermCategory::Property,
            ),
            // A base+category collision (two Individuals slugging to `foo`).
            term(&format!("{base}Foo"), DocTermCategory::Individual),
            term(&format!("{base}foo"), DocTermCategory::Individual),
        ];

        let map = resolve_term_slugs(&terms);

        // The unique-base term is NOT in the map (falls back to base).
        assert!(!map.contains_key(&format!("{base}Solo")));
        // Case-collision resolved by category.
        assert_eq!(
            map[&format!("{base}AcceptanceStatus")],
            "acceptancestatus-class"
        );
        assert_eq!(
            map[&format!("{base}acceptanceStatus")],
            "acceptancestatus-property"
        );
        // Base+category collision: the IRI-lexically-first keeps `foo-individual`,
        // the other gets the digest tiebreak.
        assert_eq!(map[&format!("{base}Foo")], "foo-individual");
        let other = &map[&format!("{base}foo")];
        assert!(
            other.starts_with("foo-individual-") && other.len() > "foo-individual-".len(),
            "digest-disambiguated slug expected, got {other}"
        );

        // Deterministic: identical input → identical map.
        assert_eq!(resolve_term_slugs(&terms), map);

        // Injective over the whole surface: term_slug is distinct for every term.
        let mut resolved = terms.clone();
        for t in &mut resolved {
            if let Some(s) = map.get(&t.iri) {
                t.slug = s.clone();
            }
        }
        let slugs: std::collections::BTreeSet<String> = resolved.iter().map(term_slug).collect();
        assert_eq!(slugs.len(), resolved.len(), "slugs must be injective");
    }

    #[test]
    fn slugify_is_filesystem_safe() {
        assert_eq!(slugify("HasOwner"), "hasowner");
        assert_eq!(slugify("Cat 9 Lives!"), "cat-9-lives");
        assert_eq!(slugify("--weird--"), "weird");
        assert_eq!(slugify(""), "unnamed");
    }

    #[test]
    fn md_escape_neutralizes_table_and_inline_metachars() {
        assert_eq!(md_escape("a|b"), "a\\|b");
        assert_eq!(md_escape("<x>"), "\\<x\\>");
        assert_eq!(md_escape("line\nbreak"), "line break");
    }

    /// Decode a Python double-quoted literal body the way `json.loads` would
    /// see it after Python's own string-literal decoding — i.e. undo exactly
    /// the two escapes [`python_str_escape`] introduces (`\\` and `\"`, plus
    /// the `\n`/`\r` it uses to stand in for a literal newline/CR that a
    /// non-raw literal cannot otherwise carry) — so the test can assert the
    /// round trip without invoking a Python interpreter.
    fn decode_python_double_quoted(escaped: &str) -> String {
        let mut out = String::with_capacity(escaped.len());
        let mut chars = escaped.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('\\') => out.push('\\'),
                    Some('"') => out.push('"'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    other => panic!("unexpected escape \\{other:?} in {escaped:?}"),
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn python_str_escape_round_trips_triple_quote_and_metachars() {
        // A payload containing `'''` would prematurely terminate the OLD
        // `r'''{payload}'''` raw literal; a payload containing `\` or `"`
        // exercises the two escapes a non-raw double-quoted literal needs.
        // A literal newline is what a pretty-printed JSON-LD payload actually
        // contains, and is exactly what a non-raw, non-triple-quoted literal
        // cannot carry unescaped.
        let payload = "{\"a\": \"it's '''not''' a \\\"quote\\\"\\\\end\",\n  \"b\": 1}";
        let literal = python_str_escape(payload);

        // No unescaped `"` may appear in the body (each `"` must be preceded
        // by exactly one `\`), and no raw newline may appear at all — both
        // would break a non-raw double-quoted Python literal.
        assert!(
            !literal.contains('\n'),
            "escaped body must not carry a literal newline: {literal:?}"
        );
        let mut prev = '\0';
        for ch in literal.chars() {
            if ch == '"' {
                assert_eq!(prev, '\\', "unescaped quote in {literal:?}");
            }
            prev = ch;
        }

        // Decoding what Python would decode reproduces the original payload
        // exactly, so `json.loads("{literal}")` gets back the exact JSON text.
        assert_eq!(decode_python_double_quoted(&literal), payload);
    }

    #[test]
    fn rust_raw_fence_width_widens_past_embedded_hash_quote_runs() {
        // No `"#`-like run at all: the familiar single-`#` fence suffices.
        assert_eq!(rust_raw_fence_width("plain turtle, no quotes"), 1);
        assert_eq!(rust_raw_fence_width(r#"a "quoted" string"#), 1);

        // A literal string value immediately followed by a `#`-fragment IRI
        // puts a `"#` run in the content — this demands a wider fence than
        // the naive one-`#` fence the old code always used (`r#"..."#`),
        // which is exactly the twin bug the reviewer flagged in
        // `python_syntax_tab`'s raw-triple-quote interpolation.
        let turtle = "ex:x ex:note \"ends right here\"##weird .";
        let width = rust_raw_fence_width(turtle);
        assert_eq!(
            width, 3,
            "content has a 2-`#` run after `\"`, needs a 3-`#` fence"
        );

        let literal = rust_raw_string_literal(turtle);
        let fence = "#".repeat(width);
        assert_eq!(literal, format!("r{fence}\"{turtle}\"{fence}"));
        // The fence must not appear as a `"`-followed-by-fence-or-more run
        // anywhere inside the raw content, or the literal would close early.
        let close = format!("\"{}", "#".repeat(width));
        assert!(
            !turtle.contains(&close),
            "chosen fence {width} still matches inside content: {turtle:?}"
        );
    }

    #[test]
    fn align_tag_handles_trailing_separators() {
        assert_eq!(
            align_tag("http://www.w3.org/2004/02/skos/core#closeMatch"),
            "closeMatch"
        );
        assert_eq!(
            align_tag("http://www.w3.org/2002/07/owl#equivalentClass"),
            "equivalentClass"
        );
        // trailing separator must not yield an empty tag
        assert_eq!(align_tag("http://example.org/vocab#"), "vocab");
        assert_eq!(align_tag("http://example.org/vocab/"), "vocab");
        // no separator at all -> whole predicate
        assert_eq!(align_tag("bareword"), "bareword");
    }

    /// A minimal two-term model with one French translation, used to assert the
    /// language-parametrized renderer picks the translation and falls back to
    /// English elsewhere.
    fn tiny_model() -> DocsModel {
        let foo = DocTerm {
            iri: format!("{GMEOW_NS}Foo"),
            curie: "gmeow:Foo".to_string(),
            label: Some("Foo".to_string()),
            definition: Some("A foo.".to_string()),
            category: DocTermCategory::Class,
            owner_slice: format!("{GMEOW_NS}slices/demo"),
            parents: Vec::new(),
            domain: Vec::new(),
            range: Vec::new(),
            scope_notes: Vec::new(),
            examples: Vec::new(),
            use_when: Vec::new(),
            avoid_when: Vec::new(),
            how_to_use: Vec::new(),
            use_for_consumer: Vec::new(),
            avoid_for_consumer: Vec::new(),
            ..Default::default()
        };
        let bar = DocTerm {
            iri: format!("{GMEOW_NS}Bar"),
            curie: "gmeow:Bar".to_string(),
            label: Some("Bar".to_string()),
            definition: Some("A bar.".to_string()),
            category: DocTermCategory::Class,
            owner_slice: format!("{GMEOW_NS}slices/demo"),
            parents: Vec::new(),
            domain: Vec::new(),
            range: Vec::new(),
            scope_notes: Vec::new(),
            examples: Vec::new(),
            use_when: Vec::new(),
            avoid_when: Vec::new(),
            how_to_use: Vec::new(),
            use_for_consumer: Vec::new(),
            avoid_for_consumer: Vec::new(),
            ..Default::default()
        };

        let translations = crate::i18n::Translations::from_entries(
            [
                (
                    (
                        format!("{GMEOW_NS}Foo"),
                        RDFS_LABEL.to_string(),
                        "fr".to_string(),
                    ),
                    "Fou".to_string(),
                ),
                (
                    (
                        format!("{GMEOW_NS}Foo"),
                        SKOS_DEFINITION.to_string(),
                        "fr".to_string(),
                    ),
                    "Un fou.".to_string(),
                ),
            ],
            ["fr".to_string()],
        );

        DocsModel {
            title: "Demo".to_string(),
            version: "test".to_string(),
            slices: Vec::new(),
            terms: vec![bar, foo],
            dependency_edges: Vec::new(),
            mapping_sets: Vec::new(),
            linkages: Vec::new(),
            examples: Vec::new(),
            fixtures: Vec::new(),
            shapes: Vec::new(),
            competencies: Vec::new(),
            grammars: Vec::new(),
            loss_targets: Vec::new(),
            worked_instances: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            seams: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            constraint_rules: Vec::new(),
            advice_entries: Vec::new(),
            four_boxes: None,
            concept_doi: None,
            pipeline: None,
            available_languages: vec!["english".to_string(), "fr".to_string()],
            translations,
            ui_catalog: crate::i18n::UiCatalog::default(),
            reasoning: None,
            diagnostics: None,
            term_loss: None,
            schema_fragments: None,
            lang: String::new(),
        }
    }

    // ── seam-registry render ↔ drift-gate parser contract ────────────────────
    //
    // `gmeow_validate::authoring_integrity::detect_seam_registry_drift` reads this
    // page BACK, per seam, to prove it never drifts from the authored `gmeow:Seam`
    // registry. That gate parses the exact table shape `md_seam_registry` writes —
    // bolded name cell, `;`-joined `from → to` legs whose slice identity is the slug
    // in the rendered link's href, backticked (and possibly linked) carrying-term
    // CURIEs, backticked owning-doc filenames. These tests pin that contract from
    // the RENDERER's side, so a change to this function's markup that the gate
    // cannot read fails here rather than turning the gate into a false positive (or,
    // worse, a false negative) in production.

    /// A two-slice, two-seam model whose seam registry exercises both rendered
    /// forms: a resolvable slice (link) and a term with its own term page (link).
    fn seam_model() -> DocsModel {
        use crate::model::{DocSeam, DocSeamDirection, DocSlice};
        fn slice(local: &str, title: &str) -> DocSlice {
            DocSlice {
                iri: format!("{GMEOW_NS}slices/{local}"),
                label: Some(local.to_string()),
                title: Some(title.to_string()),
                tier: None,
                identifier: None,
                creators: Vec::new(),
                consumers: Vec::new(),
                profiles: Vec::new(),
                depends_on: Vec::new(),
                artifacts: Vec::new(),
                documents: Vec::new(),
                has_thesis_sentence: false,
                realized_state_complete: false,
            }
        }
        let mut model = tiny_model();
        model.slices = vec![
            slice("lang", "Language grounding"),
            slice("logic", "Logic grounding"),
            slice("math", "Mathematics grounding"),
        ];
        model.seams = vec![
            DocSeam {
                iri: format!("{GMEOW_NS}seam/compilation"),
                label: Some("Compilation seam".to_string()),
                definition: Some("The math → logic lowering seam.".to_string()),
                directions: vec![DocSeamDirection {
                    from: format!("{GMEOW_NS}slices/math"),
                    to: format!("{GMEOW_NS}slices/logic"),
                }],
                carrying_terms: vec![
                    "https://blackcatinformatics.ca/math/compilesToLogicTerm".to_string(),
                ],
                owning_docs: vec!["MATHEMATICS-EXPRESSIONS.md".to_string()],
            },
            DocSeam {
                iri: format!("{GMEOW_NS}seam/denotation"),
                label: Some("Denotation seam".to_string()),
                definition: Some("The lang → logic meaning seam.".to_string()),
                directions: vec![
                    DocSeamDirection {
                        from: format!("{GMEOW_NS}slices/lang"),
                        to: format!("{GMEOW_NS}slices/logic"),
                    },
                    DocSeamDirection {
                        from: format!("{GMEOW_NS}slices/math"),
                        to: format!("{GMEOW_NS}slices/logic"),
                    },
                ],
                carrying_terms: vec![
                    "https://blackcatinformatics.ca/lang/denotationKind".to_string(),
                    "https://blackcatinformatics.ca/lang/denotationTarget".to_string(),
                ],
                owning_docs: vec!["LANG-MEANING.md".to_string()],
            },
        ];
        model
    }

    /// The `gmeow_validate` seam records the [`seam_model`] seams project to — the
    /// authored side of the comparison, built by hand so this test needs no
    /// repository (and no `generated/`) at all.
    fn seam_model_records() -> Vec<gmeow_validate::slice_peerage::SeamRecord> {
        use gmeow_validate::slice_peerage::SeamRecord;
        seam_model()
            .seams
            .iter()
            .map(|seam| SeamRecord {
                iri: seam.iri.clone(),
                name: seam.label.clone().expect("the fixture labels every seam"),
                labels: vec![(
                    seam.label.clone().expect("the fixture labels every seam"),
                    Some("x-gmeow-english".to_string()),
                )],
                carrying_terms: seam.carrying_terms.iter().map(|t| to_curie(t)).collect(),
                carrying_term_iris: seam.carrying_terms.iter().cloned().collect(),
                directions: seam
                    .directions
                    .iter()
                    .map(|d| (d.from.clone(), d.to.clone()))
                    .collect(),
                owning_docs: seam.owning_docs.iter().cloned().collect(),
            })
            .collect()
    }

    #[test]
    fn seam_registry_render_is_readable_by_the_drift_gate() {
        let page = md_seam_registry(&seam_model());
        // Non-vacuity: the render really produced the table and both seams.
        assert!(
            page.contains("| Seam | Direction | Carrying terms | Owning doc |"),
            "{page}"
        );
        assert!(page.contains("**Denotation seam**"), "{page}");
        assert!(page.contains("**Compilation seam**"), "{page}");
        let findings = gmeow_validate::authoring_integrity::detect_seam_registry_drift(
            &seam_model_records(),
            &page,
        );
        assert!(
            findings.is_empty(),
            "the drift gate must read this render back with zero drift; markup and \
             parser have diverged:\n{}\n--- page ---\n{page}",
            findings
                .iter()
                .map(|f| f.message.clone())
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    #[test]
    fn seam_registry_render_parity_fails_when_a_seam_loses_a_direction_leg() {
        // Teeth for the parity test above: the gate is genuinely reading the
        // rendered legs, not passing whatever it is handed.
        let mut model = seam_model();
        model.seams[1].directions.remove(1);
        let findings = gmeow_validate::authoring_integrity::detect_seam_registry_drift(
            &seam_model_records(),
            &md_seam_registry(&model),
        );
        assert!(
            findings.iter().any(
                |f| f.message.contains("Denotation seam") && f.message.contains("math → logic")
            ),
            "a leg dropped from the render must be caught: {findings:?}"
        );
    }

    /// NON-VACUITY, over the REAL repository: the seams actually authored in the
    /// grounding manifests render into a page the drift gate reads back with zero
    /// drift. The two tests above prove the markup/parser contract on a hand-built
    /// two-seam fixture; this one proves it over the live registry, so a newly
    /// registered seam whose carrying terms or direction legs the renderer cannot
    /// project (or the gate cannot parse) fails HERE, at authoring time, rather than
    /// in a `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs` run nobody has done yet. It needs the
    /// `slices/` tree but no `generated/` tree.
    #[test]
    fn the_real_seam_registry_renders_into_a_drift_free_page() {
        use crate::model::{DocSeam, DocSeamDirection};

        let slices_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../slices")
            .canonicalize()
            .expect("the real slices/ tree");
        let records = gmeow_validate::authoring_integrity::seam_registry_of_slices(&slices_dir)
            .expect("read the authored gmeow:Seam registry");
        assert!(
            records.len() >= 6,
            "the authored registry must be non-vacuous; got {}",
            records.len()
        );

        let mut model = seam_model();
        model.seams = records
            .iter()
            .map(|r| {
                let mut directions: Vec<DocSeamDirection> = r
                    .directions
                    .iter()
                    .map(|(from, to)| DocSeamDirection {
                        from: from.clone(),
                        to: to.clone(),
                    })
                    .collect();
                directions.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
                directions.dedup();
                DocSeam {
                    iri: r.iri.clone(),
                    label: Some(r.name.clone()),
                    definition: None,
                    directions,
                    carrying_terms: r.carrying_term_iris.iter().cloned().collect(),
                    owning_docs: r.owning_docs.iter().cloned().collect(),
                }
            })
            .collect();

        let page = md_seam_registry(&model);
        assert!(
            page.contains("| Seam | Direction | Carrying terms | Owning doc |"),
            "{page}"
        );
        let findings =
            gmeow_validate::authoring_integrity::detect_seam_registry_drift(&records, &page);
        assert!(
            findings.is_empty(),
            "the authored seam registry must render into a page the drift gate reads back \
             cleanly:\n{}\n--- page ---\n{page}",
            findings
                .iter()
                .map(|f| f.message.clone())
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    /// The "What GMEOW enforces" page renders the advice
    /// recommendation tier as a DISTINCT section from the compliance rules, headed by
    /// the single `#advice-` anchor and carrying each realized term's verbatim
    /// avoid/use/how-to prose. Self-contained (a hand-built model), so it proves the
    /// production render function independently of a regenerated catalog `.nq`.
    #[test]
    fn constraint_catalog_renders_distinct_advice_section() {
        use crate::model::{AdviceEntry, ConstraintRule};
        let mut model = tiny_model();
        // The advice family rule — its slug is the `#advice-` section anchor.
        model.constraint_rules = vec![ConstraintRule {
            code: gmeow_validate::codes::ADVICE_FAMILY.to_string(),
            slug: gmeow_validate::rule_catalog::slugify(gmeow_validate::codes::ADVICE_FAMILY),
            category: "https://blackcatinformatics.ca/logic/FindingPolicyWarning".to_string(),
            severity: "advisory".to_string(),
            help_uri: gmeow_validate::rule_catalog::help_uri_for(
                gmeow_validate::codes::ADVICE_FAMILY,
            ),
            label: None,
            definition: None,
            applies_to_terms: Vec::new(),
            formalizes: None,
        }];
        model.advice_entries = vec![
            AdviceEntry {
                term: format!("{GMEOW_NS}Entity"),
                slug: "advice-Entity".to_string(),
                label: Some("Entity".to_string()),
                definition: Some("The universal endurant".to_string()),
                avoid_when: vec!["Avoid bare Entity when a sortal applies".to_string()],
                use_when: vec!["Use for category-neutral resources".to_string()],
                how_to_use: vec!["Reserve the unqualified type".to_string()],
                documented_by_rule: Some(format!("{GMEOW_NS}rule/family/advice")),
            },
            AdviceEntry {
                term: format!("{GMEOW_NS}Event"),
                slug: "advice-Event".to_string(),
                label: Some("Event".to_string()),
                definition: None,
                avoid_when: vec!["Avoid typing an endurant as an Event".to_string()],
                use_when: vec!["Use for occurrences with participants".to_string()],
                how_to_use: Vec::new(),
                documented_by_rule: Some(format!("{GMEOW_NS}rule/family/advice")),
            },
        ];

        let md = md_constraint_catalog(&model);

        // A distinct Advice section heading and the single `#advice-` anchor (once).
        assert!(
            md.contains("## Usage Advice"),
            "missing the distinct Advice section heading:\n{md}"
        );
        assert_eq!(
            md.matches("id=\"advice-\"").count(),
            1,
            "the #advice- section anchor must appear exactly once:\n{md}"
        );
        // Both realized terms' per-term sub-anchors and all three deontic legs.
        assert!(md.contains("id=\"advice-Entity\""));
        assert!(md.contains("id=\"advice-Event\""));
        assert!(md.contains("**Avoid when:**"));
        assert!(md.contains("**Use when:**"));
        assert!(md.contains("**How to use:**"));
        // Verbatim prose (period-free substrings; md_escape escapes the trailing dot).
        // (md_escape backslash-escapes `-`/`.`, so assert on separator-free spans.)
        assert!(md.contains("Avoid bare Entity when a sortal applies"));
        assert!(md.contains("neutral resources"));
        assert!(md.contains("Reserve the unqualified type"));
        assert!(md.contains("Avoid typing an endurant as an Event"));
    }

    #[test]
    fn render_site_lang_uses_translation_with_english_fallback() {
        let model = tiny_model();

        // English page for Foo keeps the carrier values.
        let en = render_site_lang(&model, "english");
        let foo_en = String::from_utf8(en.files["terms/foo/index.md"].clone()).unwrap();
        assert!(foo_en.contains("Foo"), "english label present");
        // Definitions are md-escaped (`.` → `\.`), so match a bare substring.
        assert!(foo_en.contains("A foo"), "english definition present");
        assert!(!foo_en.contains("Fou"));

        // French page for Foo uses the translation.
        let fr = render_site_lang(&model, "fr");
        let foo_fr = String::from_utf8(fr.files["terms/foo/index.md"].clone()).unwrap();
        assert!(foo_fr.contains("Fou"), "french label used");
        assert!(foo_fr.contains("Un fou"), "french definition used");

        // Bar has no translation → English fallback even in the fr tree.
        let bar_fr = String::from_utf8(fr.files["terms/bar/index.md"].clone()).unwrap();
        assert!(bar_fr.contains("Bar"));
        assert!(bar_fr.contains("A bar"));

        // The page graph (file set) is identical across languages.
        let en_keys: Vec<&String> = en.files.keys().collect();
        let fr_keys: Vec<&String> = fr.files.keys().collect();
        assert_eq!(en_keys, fr_keys, "no dangling/extra links per language");
    }

    #[test]
    fn per_term_card_json_and_full_md_are_emitted() {
        use crate::model::{
            DiagnosticsDigest, DocDiagFinding, DocFixture, DocFixtureKind, TermLossDigest,
            TermLossRow,
        };

        let mut model = tiny_model();
        let foo_iri = format!("{GMEOW_NS}Foo");

        // A Do (well-formed) and a Don't (counter-example) fixture referencing Foo.
        model.fixtures.push(DocFixture {
            slice: format!("{GMEOW_NS}slices/demo"),
            logical_path: "tests/conformance-fixtures/foo-ok.ttl".to_string(),
            title: "Well-formed Foo".to_string(),
            text: "ex:a a gmeow:Foo .".to_string(),
            kind: DocFixtureKind::Wellformed,
            terms_referenced: vec!["gmeow:Foo".to_string()],
            expected_outcome: Some("conforms".to_string()),
            violation_code: None,
            rationale: None,
            catalog_slug: None,
        });
        model.fixtures.push(DocFixture {
            slice: format!("{GMEOW_NS}slices/demo"),
            logical_path: "tests/counter-examples/foo-bad.ttl".to_string(),
            title: "Foo missing something".to_string(),
            text: "ex:b a gmeow:Foo .".to_string(),
            kind: DocFixtureKind::CounterExample,
            terms_referenced: vec!["gmeow:Foo".to_string()],
            expected_outcome: Some("violates".to_string()),
            violation_code: Some("shacl.MinCountConstraintComponent".to_string()),
            rationale: None,
            catalog_slug: None,
        });

        // A per-term diagnostic and a per-term projection-loss row for Foo.
        let mut diag_by_term = BTreeMap::new();
        diag_by_term.insert(
            foo_iri.clone(),
            vec![DocDiagFinding {
                code: "gmeow-range-missing".to_string(),
                severity: "error".to_string(),
                category: "structural".to_string(),
                message: "Foo is missing a range".to_string(),
                slice_iri: None,
                help_uri: None,
            }],
        );
        model.diagnostics = Some(DiagnosticsDigest {
            by_term: diag_by_term,
            by_slice: BTreeMap::new(),
            total: 1,
        });
        let mut loss_by_term = BTreeMap::new();
        loss_by_term.insert(
            foo_iri.clone(),
            vec![TermLossRow {
                target: "property-path:https://example/fooShape".to_string(),
                preservation_kind: "SoundUnderApproximation".to_string(),
                complexity_class: "PTIME".to_string(),
                lossy_drops: Vec::new(),
            }],
        );
        model.term_loss = Some(TermLossDigest {
            by_term: loss_by_term,
            total_property_path_rows: 1,
        });

        // Reasoned entailment for Foo (English-only executable data).
        let mut term_entailments = BTreeMap::new();
        term_entailments.insert(
            foo_iri.clone(),
            vec![crate::exec::Entailment {
                rule: "subClassOf-transitivity".to_string(),
                conclusion: "gmeow:Foo rdfs:subClassOf owl:Thing".to_string(),
                premises: vec!["gmeow:Foo rdfs:subClassOf gmeow:Bar".to_string()],
            }],
        );
        let exec = ExecutableDocsData {
            term_entailments,
            ..Default::default()
        };

        let site = render_site_lang_exec(&model, "english", &exec);
        let foo = model
            .terms
            .iter()
            .find(|t| t.curie == "gmeow:Foo")
            .expect("foo term");
        let slug = term_slug(foo);
        let json_key = format!("terms/{slug}/card.json");
        let full_key = format!("terms/{slug}/card-full.md");

        // Both machine surfaces ride alongside `card.md`.
        assert!(site.files.contains_key(&json_key), "card.json emitted");
        assert!(site.files.contains_key(&full_key), "card-full.md emitted");
        assert!(
            site.files.contains_key(&format!("terms/{slug}/card.md")),
            "card.md still emitted"
        );

        // card.json parses and EQUALS the standard-tier Card serialized through the
        // SAME `serde_json` path the MCP `doc_card format=json detail=standard` uses.
        let bytes = &site.files[&json_key];
        let parsed: serde_json::Value =
            serde_json::from_slice(bytes).expect("card.json parses as JSON");
        assert_eq!(parsed["category"], "Class");
        assert_eq!(parsed["iri"], foo_iri);
        // The standard tier carries NO rich-panel keys.
        assert!(parsed.get("entailments").is_none(), "standard omits panels");
        let facets = precompute_alignment_facets(&model);
        let expected =
            doc_term_card(foo, &facets, &model).projected(crate::card::CardDetail::Standard);
        let expected_bytes = serde_json::to_vec(&expected).expect("serialize standard card");
        assert_eq!(
            bytes, &expected_bytes,
            "packed card.json equals the standard Card via the same serializer"
        );

        // card-full.md carries the H1 title and EVERY rich panel (the data is present
        // for Foo), rendered by the ONE canonical renderer at the Full tier.
        let full_md = String::from_utf8(site.files[&full_key].clone()).unwrap();
        assert!(
            full_md.starts_with("# gmeow:Foo"),
            "full card H1: {full_md}"
        );
        assert!(full_md.contains("## Entailments"), "{full_md}");
        assert!(full_md.contains("## Do"), "{full_md}");
        assert!(full_md.contains("## Don't"), "{full_md}");
        assert!(full_md.contains("## Diagnostics"), "{full_md}");
        assert!(
            full_md.contains("## Degrades under projection"),
            "{full_md}"
        );
        // The full body is a strict superset of the standard body (single renderer).
        let standard_body = crate::card::render_card_body(
            &doc_term_card(foo, &facets, &model),
            crate::card::CardDetail::Standard,
        );
        assert!(
            full_md.contains(standard_body.trim_end()),
            "full card contains the whole standard body"
        );

        // Bar has NO fixtures / diagnostics / loss / entailments, so its full card is
        // an honest projection: identical to its standard card (no fabricated panels).
        let bar = model
            .terms
            .iter()
            .find(|t| t.curie == "gmeow:Bar")
            .expect("bar term");
        let bar_full = String::from_utf8(
            site.files[&format!("terms/{}/card-full.md", term_slug(bar))].clone(),
        )
        .unwrap();
        assert!(!bar_full.contains("## Entailments"), "{bar_full}");
        assert!(!bar_full.contains("## Do"), "{bar_full}");
    }

    #[test]
    fn playground_asset_emitted_only_with_executable_data() {
        let model = tiny_model();

        // A model-only render (empty executable data) ships NO playground asset — the
        // base site is complete without the executable surfaces.
        let base = render_site_lang(&model, "english");
        assert!(
            !base.files.contains_key(PLAYGROUND_TRIG_PATH),
            "the model-only render must not emit the playground asset"
        );

        // With a playground asset supplied, it is emitted once, verbatim, under the
        // language-neutral path.
        let exec = ExecutableDocsData {
            playground_trig: b"@prefix ex: <https://e/> .\nex:a ex:b ex:c .\n".to_vec(),
            ..Default::default()
        };
        let live = render_site_lang_exec(&model, "english", &exec);
        assert_eq!(
            live.files.get(PLAYGROUND_TRIG_PATH).map(Vec::as_slice),
            Some(exec.playground_trig.as_slice()),
            "the playground asset must be emitted verbatim when supplied"
        );
    }

    #[test]
    fn bundle_assets_emitted_only_with_bundle_data() {
        let model = tiny_model();

        // Model-only render ships none of the browser-bundle assets.
        let base = render_site_lang(&model, "english");
        for path in [
            CORE_BUNDLE_NQ_PATH,
            FULL_BUNDLE_GTS_PATH,
            BUNDLE_MANIFEST_PATH,
        ] {
            assert!(
                !base.files.contains_key(path),
                "the model-only render must not emit {path}"
            );
        }

        // With both bundle bytes supplied, the core N-Quads + full gts + integrity
        // manifest are emitted verbatim, with the manifest carrying each asset's
        // blake3 content address and byte length.
        let core = b"<https://e/s> <https://e/p> <https://e/o> <https://e/g> .\n".to_vec();
        let full = b"\0asm-not-really-but-opaque-bytes".to_vec();
        let exec = ExecutableDocsData {
            playground_trig: b"@prefix ex: <https://e/> .\nex:a ex:b ex:c .\n".to_vec(),
            core_bundle_nquads: core.clone(),
            full_bundle_gts: full.clone(),
            ..Default::default()
        };
        let live = render_site_lang_exec(&model, "english", &exec);
        assert_eq!(
            live.files.get(CORE_BUNDLE_NQ_PATH).map(Vec::as_slice),
            Some(core.as_slice()),
            "the core bundle N-Quads must be emitted verbatim"
        );
        assert_eq!(
            live.files.get(FULL_BUNDLE_GTS_PATH).map(Vec::as_slice),
            Some(full.as_slice()),
            "the full gts bundle must be emitted verbatim"
        );
        let manifest = String::from_utf8(
            live.files
                .get(BUNDLE_MANIFEST_PATH)
                .expect("integrity manifest emitted")
                .clone(),
        )
        .expect("manifest is utf-8");
        assert!(
            manifest.contains(&format!("blake3:{}", blake3::hash(&core).to_hex())),
            "manifest carries the core asset's blake3 content address:\n{manifest}"
        );
        assert!(
            manifest.contains(&format!("\"bytes\": {}", core.len())),
            "manifest carries the core asset's byte length:\n{manifest}"
        );
    }

    #[test]
    fn conjecture_playground_ships_page_asset_and_manifest_entry() {
        let model = tiny_model();
        let core = b"<https://e/s> <https://e/p> <https://e/o> <https://e/g> .\n".to_vec();
        let full = b"\0asm-not-really-but-opaque-bytes".to_vec();
        let conjectures = b"@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:demo a logic:Conjecture .\n"
            .to_vec();
        let exec = ExecutableDocsData {
            playground_trig: b"@prefix ex: <https://e/> .\nex:a ex:b ex:c .\n".to_vec(),
            core_bundle_nquads: core,
            full_bundle_gts: full,
            conjectures_ttl: conjectures.clone(),
            ..Default::default()
        };
        assert!(exec.has_conjectures(), "the exec must be conjecture-backed");
        let live = render_site_lang_exec(&model, "english", &exec);

        // The demo library asset is emitted verbatim.
        assert_eq!(
            live.files.get(CONJECTURES_PATH).map(Vec::as_slice),
            Some(conjectures.as_slice()),
            "the conjecture demo library must be emitted verbatim"
        );
        // Its integrity entry rides the bundle manifest.
        let manifest = String::from_utf8(
            live.files
                .get(BUNDLE_MANIFEST_PATH)
                .expect("integrity manifest emitted")
                .clone(),
        )
        .expect("manifest is utf-8");
        assert!(
            manifest.contains(CONJECTURES_PATH)
                && manifest.contains(&format!("blake3:{}", blake3::hash(&conjectures).to_hex())),
            "manifest carries the conjecture library's content address:\n{manifest}"
        );
        // The playground page is emitted with the interactive form + both symmetric legs.
        let page_md = String::from_utf8(
            live.files
                .get(&Page::ConjecturePlayground.md_path())
                .expect("conjecture playground page emitted")
                .clone(),
        )
        .expect("page is utf-8");
        assert!(
            page_md.contains("gmeow-conjecture-form")
                && page_md.contains("proof")
                && page_md.contains("counterproof"),
            "the page presents the interactive form and both symmetric legs:\n{page_md}"
        );
    }

    #[test]
    fn conjecture_assets_absent_without_conjecture_data() {
        let model = tiny_model();
        // A bundle-only exec (no conjecture library) must NOT emit the demo asset, the
        // playground page, or a conjectures entry in the manifest.
        let exec = ExecutableDocsData {
            core_bundle_nquads: b"<https://e/s> <https://e/p> <https://e/o> <https://e/g> .\n"
                .to_vec(),
            full_bundle_gts: b"\0opaque".to_vec(),
            ..Default::default()
        };
        assert!(!exec.has_conjectures());
        let live = render_site_lang_exec(&model, "english", &exec);
        assert!(!live.files.contains_key(CONJECTURES_PATH));
        assert!(
            !live
                .files
                .contains_key(&Page::ConjecturePlayground.md_path())
        );
        let manifest =
            String::from_utf8(live.files.get(BUNDLE_MANIFEST_PATH).unwrap().clone()).unwrap();
        assert!(
            !manifest.contains(CONJECTURES_PATH),
            "a bundle-only manifest must not carry the conjectures entry:\n{manifest}"
        );
    }

    #[test]
    fn playground_explains_a_chase_invented_witness() {
        let model = tiny_model();
        let exec = ExecutableDocsData {
            playground_trig: b"@prefix ex: <https://e/> .\nex:a ex:b ex:c .\n".to_vec(),
            ..Default::default()
        };
        let page = md_playground(&model, &exec);

        // The affordance heading, the copy-pasteable query, and a prefilled `?q=` link.
        assert!(
            page.contains("Explain a chase-invented witness"),
            "witness explain heading present: {page}"
        );
        assert!(
            page.contains("gmeow:InventedWitness"),
            "the decomposition query text is emitted: {page}"
        );
        assert!(
            page.contains("gmeow:existentialOrdinal"),
            "the query decomposes the existential ordinal: {page}"
        );
        assert!(
            page.contains("sparql/index.html?q="),
            "a prefilled `?q=` playground link is emitted: {page}"
        );
        // The pin-a-null guidance (FILTER / DESCRIBE by exact Skolem IRI).
        assert!(
            page.contains("DESCRIBE <skolem-iri>") && page.contains("FILTER(?witness ="),
            "guidance to pin a specific null is present: {page}"
        );
    }

    #[test]
    fn executable_surfaces_render_with_exec() {
        let mut model = tiny_model();
        let slice_iri = format!("{GMEOW_NS}slices/demo");
        // tiny_model has no slice; add the one its terms are owned by so the slice
        // page (and its executable sections) render.
        model.slices.push(crate::model::DocSlice {
            iri: slice_iri.clone(),
            label: Some("Demo".to_string()),
            title: None,
            tier: None,
            identifier: None,
            creators: Vec::new(),
            consumers: Vec::new(),
            profiles: Vec::new(),
            depends_on: Vec::new(),
            artifacts: Vec::new(),
            documents: Vec::new(),
            has_thesis_sentence: false,
            realized_state_complete: false,
        });
        // Add a worked example so the "try it" surface has something to render.
        model.examples.push(crate::model::DocExample {
            slice: slice_iri.clone(),
            logical_path: "examples/demo.ttl".to_string(),
            title: "Demo example".to_string(),
            text: "ex:a a gmeow:Foo .".to_string(),
            terms_referenced: vec!["gmeow:Foo".to_string()],
        });

        let mut example_inferences = std::collections::BTreeMap::new();
        example_inferences.insert(
            crate::exec::example_key(&slice_iri, "examples/demo.ttl"),
            crate::exec::InferenceDiff {
                asserted: vec!["ex:a rdf:type gmeow:Foo".to_string()],
                inferred: vec!["ex:a rdf:type owl:Thing".to_string()],
            },
        );
        let exec = ExecutableDocsData {
            example_inferences,
            cross_example: vec!["ex:shared gmeow:derived ex:x".to_string()],
            playground_trig: b"@prefix ex: <https://e/> . ex:a ex:b ex:c .\n".to_vec(),
            ..Default::default()
        };

        let site = render_site_lang_exec(&model, "english", &exec);

        // Playground page + its assets (incl. the vendored purrdf engine) are present.
        assert!(site.files.contains_key("sparql/index.html"));
        assert!(site.files.contains_key(DOCS_JS_PATH));
        assert!(
            site.files
                .contains_key("assets/purrdf/gmeow_rdf_wasm_bg.wasm"),
            "the vendored wasm engine is emitted so the playground loads offline"
        );
        assert!(
            site.files
                .contains_key("assets/validate/gmeow_validate_wasm_bg.wasm"),
            "the vendored validator wasm engine is emitted alongside purrdf so the site \
             validates authored RDF client-side"
        );
        let sparql = String::from_utf8(site.files["sparql/index.html"].clone()).unwrap();
        assert!(
            sparql.contains("id=\"gmeow-sparql\""),
            "the query form renders"
        );
        assert!(
            sparql.contains(DOCS_JS_PATH),
            "the playground page loads the controller module"
        );
        assert!(sparql.contains("SPARQL"), "the SPARQL nav entry is present");
        assert!(
            sparql.contains("Cross-example inferences"),
            "the cross-example bucket is surfaced (no silent drop)"
        );

        // No static per-format export files — export runs through the playground.
        assert!(
            !site.files.keys().any(|k| k.starts_with("export/")),
            "export is client-side via the playground; no static export files"
        );
        let slug = slice_slug(model.slices.iter().find(|s| s.iri == slice_iri).unwrap());
        let slice_md =
            String::from_utf8(site.files[&format!("slices/{slug}/index.md")].clone()).unwrap();
        assert!(
            slice_md.contains("## Export"),
            "the slice page has an Export section"
        );
        assert!(
            slice_md.contains("sparql/index.html?q="),
            "the slice export links into the playground"
        );
        assert!(
            slice_md.contains("Try it"),
            "the slice page shows the reasoner try-it inferences"
        );
        assert!(
            slice_md.contains("owl:Thing"),
            "the inferred triple appears in the try-it block"
        );

        // Non-English trees carry NO executable surfaces (bundle-size gate).
        let fr = render_site_lang_exec(&model, "fr", &exec);
        assert!(
            !fr.files.contains_key("sparql/index.html")
                && !fr.files.contains_key(PLAYGROUND_TRIG_PATH),
            "the executable surfaces live only in the English carrier tree"
        );
    }

    #[test]
    fn okf_doc_reference_matches_the_bundle_scheme() {
        // Class / property / individual terms reference their `gmeow-okf/` document
        // by the SAME {category-dir}/{local-name}.md scheme the OKF projection emits;
        // datatypes / other categories have no per-concept OKF document.
        let mut class = DocTerm {
            iri: format!("{GMEOW_NS}Foo"),
            curie: "gmeow:Foo".to_string(),
            category: DocTermCategory::Class,
            ..Default::default()
        };
        assert_eq!(
            okf_doc_reference(&class).as_deref(),
            Some("gmeow-okf/classes/Foo.md")
        );
        class.category = DocTermCategory::Property;
        assert_eq!(
            okf_doc_reference(&class).as_deref(),
            Some("gmeow-okf/properties/Foo.md")
        );
        class.category = DocTermCategory::Individual;
        assert_eq!(
            okf_doc_reference(&class).as_deref(),
            Some("gmeow-okf/individuals/Foo.md")
        );
        class.category = DocTermCategory::Datatype;
        assert_eq!(okf_doc_reference(&class), None);
    }

    #[test]
    fn term_page_renders_usage_advice_and_alignments() {
        let mut model = tiny_model();
        // Enrich Foo with every advisory field + one consumer profile, and add a
        // documented consumer term so the consumer link resolves internally.
        let foo = model
            .terms
            .iter_mut()
            .find(|t| t.curie == "gmeow:Foo")
            .expect("Foo present");
        foo.scope_notes = vec!["Scope of the foo.".to_string()];
        foo.examples = vec!["ex:x a gmeow:Foo .".to_string()];
        foo.use_when = vec!["Use when foo-ing.".to_string()];
        foo.avoid_when = vec!["Avoid when bar-ing.".to_string()];
        foo.how_to_use = vec!["Reference via gmeow:hasFoo.".to_string()];
        foo.use_for_consumer = vec!["gmeow:Bar".to_string(), "ext:Other".to_string()];

        // One alignment cross-walk on Foo.
        model.linkages.push(crate::model::DocLinkage {
            mapping_set: None,
            subject: format!("{GMEOW_NS}Foo"),
            subject_curie: "gmeow:Foo".to_string(),
            predicate: "http://www.w3.org/2004/02/skos/core#closeMatch".to_string(),
            object: "http://www.wikidata.org/entity/Q42".to_string(),
            justification: None,
            confidence: None,
            owner_slice: format!("{GMEOW_NS}slices/demo"),
        });

        let md = to_markdown(&model, &Page::Term("foo".to_string()));

        // Usage Advice section + every field label, in order.
        assert!(md.contains("## Usage Advice"), "advice heading present");
        for label in [
            "Scope",
            "Example",
            "Use when",
            "Avoid when",
            "How to use",
            "Use for consumers",
        ] {
            assert!(
                md.contains(&format!("**{label}:**")),
                "missing advice label {label}"
            );
        }
        // Documented consumer resolves to an internal link; undocumented stays a CURIE.
        assert!(md.contains("[`gmeow:Bar`]"), "documented consumer linked");
        assert!(
            md.contains("`ext:Other`"),
            "undocumented consumer shown as code"
        );

        // Alignments section uses the short predicate tag and links the object.
        assert!(md.contains("## Alignments"), "alignments heading present");
        assert!(md.contains("`closeMatch`"), "predicate short tag");
        // The external object IRI is md-escaped (`.` → `\.`); match a dot-free tail.
        assert!(md.contains("entity/Q42"), "alignment object linked");
        // The approximate (closeMatch) crosswalk carries an inline lossy caveat and
        // the section cross-links the preservation loss ledger.
        assert!(
            md.contains("approximate match (close)"),
            "inline lossy-projection caveat present"
        );
        assert!(
            md.contains("preservation loss ledger"),
            "loss-ledger cross-link present for an approximate alignment"
        );
        // Any crosswalk also discloses that its EDOAL/FnO lowering is lossy.
        assert!(
            edoal_fno_lowering_is_lossy(),
            "the EDOAL/FnO lowerings are declared lossy in the projection ledger"
        );
        assert!(
            md.contains("lowered to EDOAL"),
            "per-term EDOAL/FnO lowering caveat present on an aligned term"
        );

        // Bar carries no advice/alignments → neither section appears on its page.
        let bar_md = to_markdown(&model, &Page::Term("bar".to_string()));
        assert!(
            !bar_md.contains("## Usage Advice"),
            "empty advice suppressed"
        );
        assert!(
            !bar_md.contains("## Alignments"),
            "empty alignments suppressed"
        );
    }

    /// The always-present Stability badge must render every `DocTermStability`
    /// variant. The `deprecated` arm is otherwise never exercised by the term
    /// goldens (no production term is `owl:deprecated` — this project deletes
    /// rather than deprecates), so this is the only coverage of that render
    /// path. The derivation logic itself is unit-tested separately in
    /// `model::tests::stability_resolves_by_precedence`.
    #[test]
    fn stability_badge_renders_every_state() {
        let mut model = tiny_model();
        model.terms.iter_mut().for_each(|t| {
            t.stability = match t.curie.as_str() {
                "gmeow:Foo" => DocTermStability::Deprecated,
                _ => DocTermStability::Experimental,
            };
        });

        let foo_md = to_markdown(&model, &Page::Term("foo".to_string()));
        assert!(foo_md.contains("## Stability"), "stability heading present");
        assert!(
            foo_md.contains("- **Status:** deprecated"),
            "deprecated badge renders: {foo_md}"
        );

        let bar_md = to_markdown(&model, &Page::Term("bar".to_string()));
        assert!(
            bar_md.contains("- **Status:** experimental"),
            "experimental badge renders: {bar_md}"
        );

        // The default (no override, core-tier) resolves to `stable` and still
        // renders unconditionally — assert via the variant label directly.
        assert_eq!(DocTermStability::Stable.label(), "stable");
    }

    #[test]
    fn finding_category_display_humanizes_finding_classes() {
        assert_eq!(
            finding_category_display("https://blackcatinformatics.ca/logic/FindingPolicyWarning"),
            "Finding: Policy Warning"
        );
        assert_eq!(
            finding_category_display(
                "https://blackcatinformatics.ca/logic/FindingDataShapeViolation"
            ),
            "Finding: Data Shape Violation"
        );
        // A non-Finding IRI degrades to its local name verbatim.
        assert_eq!(
            finding_category_display("https://example.org/vocab/SomethingElse"),
            "Something Else"
        );
    }

    #[test]
    fn constraint_catalog_anchors_rule_by_helpuri_slug() {
        use crate::model::ConstraintRule;

        let mut model = tiny_model();
        // `box-roles.invalid` → slug `box-roles-invalid` (the validator's transform:
        // `.`/`/` → `-`), which is the fragment of the rule's helpUri.
        let rule = ConstraintRule {
            code: "box-roles.invalid".to_string(),
            slug: gmeow_validate::rule_catalog::slugify("box-roles.invalid"),
            category: "https://blackcatinformatics.ca/logic/FindingPolicyWarning".to_string(),
            severity: "binding".to_string(),
            help_uri:
                "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#box-roles-invalid"
                    .to_string(),
            label: Some("box-roles.invalid".to_string()),
            definition: Some("Every box declares exactly one valid box role.".to_string()),
            applies_to_terms: vec![format!("{GMEOW_NS}Foo")],
            formalizes: None,
        };
        // The rendered anchor id MUST equal the helpUri fragment so a finding's
        // help link resolves to its rule entry.
        let fragment = rule.help_uri.rsplit('#').next().unwrap().to_string();
        assert_eq!(rule.slug, fragment, "anchor slug == helpUri fragment");
        model.constraint_rules = vec![rule];

        let md = to_markdown(&model, &Page::ConstraintCatalog);
        assert!(
            md.contains(&format!("<a id=\"{fragment}\"></a>")),
            "explicit anchor emitted: {md}"
        );
        assert!(md.contains("Finding: Policy Warning"), "category heading");
        assert!(md.contains("**binding**"), "severity rendered");
        // The definition is md-escaped (`.` → `\.`), so match a bare substring.
        assert!(
            md.contains("Every box declares exactly one valid box role"),
            "definition rendered: {md}"
        );
        // The applies-to term links internally to the documented gmeow:Foo term.
        assert!(
            md.contains("[`gmeow:Foo`]"),
            "applies-to term is a curie link: {md}"
        );
    }

    #[test]
    fn constraint_catalog_empty_renders_empty_state() {
        let model = tiny_model(); // constraint_rules empty
        let md = to_markdown(&model, &Page::ConstraintCatalog);
        assert!(
            md.contains("No validation rules are declared in the constraint catalog."),
            "empty-state line renders: {md}"
        );
    }
}
