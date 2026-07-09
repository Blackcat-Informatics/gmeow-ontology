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

use std::collections::BTreeMap;
use std::sync::OnceLock;

use minijinja::{Environment, context};
use pulldown_cmark::{Options, Parser, html as cmark_html};

use crate::badge;
use crate::exec::ExecutableDocsData;
use crate::i18n::{self, ENGLISH};
use crate::llms::{self, LlmsBullet, LlmsSection};
use crate::model::{DocConcern, DocSlice, DocTerm, DocTermCategory, DocsModel};
use crate::svg;

/// The GMEOW vocabulary namespace (mirrors `model.rs`).
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

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

/// The site-relative path of the docs controller module (SPARQL playground query
/// execution + result transcoding). A self-contained ES module.
const DOCS_JS_PATH: &str = "assets/gmeow-docs.js";

/// The embedded docs controller module, emitted to [`DOCS_JS_PATH`] when the
/// playground is present.
const DOCS_JS: &str = include_str!("../assets/gmeow-docs.js");

/// The vendored purrdf wasm engine (the offline SPARQL runtime), emitted under
/// `assets/purrdf/` when the playground is present. Pinned build inputs — see
/// `crates/docs/assets/purrdf/PROVENANCE.md`.
const PURRDF_ASSETS: &[(&str, &[u8])] = &[
    (
        "gmeow_rdf_wasm.js",
        include_bytes!("../assets/purrdf/gmeow_rdf_wasm.js"),
    ),
    (
        "gmeow_rdf_wasm_bg.wasm",
        include_bytes!("../assets/purrdf/gmeow_rdf_wasm_bg.wasm"),
    ),
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
    /// The offline SPARQL playground (`sparql/index`). Emitted only when the pipeline
    /// supplies a bundled query asset (never in a model-only render).
    SparqlPlayground,
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
            Page::SparqlPlayground => "sparql".to_string(),
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
            Page::SparqlPlayground => "SPARQL playground".to_string(),
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

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    for page in pages(model) {
        files.insert(
            page.md_path(),
            to_markdown_exec(model, &page, exec).into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec(model, &page, lang, exec).into_bytes(),
        );
    }
    files.insert(CSS_PATH.to_string(), CSS.as_bytes().to_vec());

    // The offline SPARQL playground: its bundled RDF asset (documentation graph + the
    // reasoned ontology closure, TriG), the vendored purrdf wasm engine, the controller
    // module, and the page itself. All language-independent, emitted only in the
    // English tree (see the gate above). Term/slice export runs through this same
    // engine + asset via `DESCRIBE`, so no static export files are needed.
    if exec.has_playground() {
        files.insert(
            PLAYGROUND_TRIG_PATH.to_string(),
            exec.playground_trig.clone(),
        );
        files.insert(DOCS_JS_PATH.to_string(), DOCS_JS.as_bytes().to_vec());
        for (name, bytes) in PURRDF_ASSETS {
            files.insert(format!("assets/purrdf/{name}"), bytes.to_vec());
        }
        let page = Page::SparqlPlayground;
        files.insert(
            page.md_path(),
            to_markdown_exec(model, &page, exec).into_bytes(),
        );
        files.insert(
            page.html_path(),
            to_html_lang_exec(model, &page, lang, exec).into_bytes(),
        );
    }

    // Deterministic SVG diagrams (pure functions of the model).
    files.insert(
        "diagrams/slices.svg".to_string(),
        svg::slice_dependency_svg(model).into_bytes(),
    );
    files.insert(
        "diagrams/concerns.svg".to_string(),
        svg::concern_overview_svg(model).into_bytes(),
    );
    // The per-slice documentation-coverage heatmap embedded on the health page.
    files.insert(
        "diagrams/coverage-heatmap.svg".to_string(),
        svg::coverage_heatmap_svg(model).into_bytes(),
    );
    for slice in &model.slices {
        files.insert(
            format!("diagrams/slices/{}.svg", slice_slug(slice)),
            svg::slice_local_svg(model, &slice.iri).into_bytes(),
        );
    }
    // Per-term neighbourhood diagrams — only for terms that actually have a
    // neighbourhood, gated on the same predicate as the page embed below so the
    // two never disagree (no dangling image paths).
    for term in &model.terms {
        if svg::term_has_neighbourhood(term) {
            files.insert(
                format!("diagrams/terms/{}.svg", term_slug(term)),
                svg::term_neighbourhood_svg(term).into_bytes(),
            );
        }
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
        search_index_json(model).into_bytes(),
    );
    // Standard llmstxt.org surfaces: a links-only index and a complete
    // inlined form, both at the site root, superseding the ad-hoc `llms-docs.txt`.
    files.insert("llms.txt".to_string(), llms_txt(model).into_bytes());
    files.insert(
        "llms-full.txt".to_string(),
        llms_full_txt(model).into_bytes(),
    );

    // Prompt-ready per-term cards: a compact, link-free Markdown card per
    // term at `terms/{slug}/card.md`, for context-window injection. The alignment
    // facets are precomputed once so emitting every card stays O(N), not O(N²).
    {
        let alignment_facets = precompute_alignment_facets(model);
        for term in &model.terms {
            files.insert(
                format!("terms/{}/card.md", term_slug(term)),
                term_card_md_inner(term, &alignment_facets).into_bytes(),
            );
        }
    }

    // Casefolded slash-namespace aliases (tiny redirect pages).
    for (alias_dir, target_dir) in term_aliases(model) {
        files.insert(
            join(&alias_dir, "index.html"),
            alias_redirect_html(&alias_dir, &target_dir).into_bytes(),
        );
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
    ];
    // The curated "four boxes" doctrine page only when its prose is present.
    if model.four_boxes.is_some() {
        pages.push(Page::FourBoxes);
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
    match page {
        Page::Term(slug) => {
            let mut md = md_term(model, slug);
            append_term_export_section(&mut md, model, slug, exec);
            md
        }
        Page::Slice(slug) => {
            let mut md = md_slice(model, slug);
            append_slice_executable_sections(&mut md, model, slug, exec);
            md
        }
        Page::SparqlPlayground => md_playground(model, exec),
        _ => to_markdown_base(model, page),
    }
}

fn to_markdown_base(model: &DocsModel, page: &Page) -> String {
    match page {
        Page::Landing => md_landing(model),
        Page::GettingStarted => md_getting_started(model),
        Page::About => md_about(model),
        Page::Health => md_health(model),
        Page::Changelog => md_changelog(model),
        Page::Category(category) => md_category(model, *category),
        Page::Term(slug) => md_term(model, slug),
        Page::SliceIndex => md_slice_index(model),
        Page::Slice(slug) => md_slice(model, slug),
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
        // Routed through `to_markdown_exec`; this arm keeps the match exhaustive.
        Page::SparqlPlayground => md_playground(model, &ExecutableDocsData::default()),
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
    let local = term
        .curie
        .split_once(':')
        .map(|(_, l)| l)
        .unwrap_or(&term.curie);
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

/// The documentation-health dashboard: per-dimension coverage of the vocabulary
/// surface and a completeness distribution. Reads the shared coverage source, so
/// its per-dimension *covered* counts are the exact complement of the
/// `docs/missing-*` lint counts.
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
    let coverages: Vec<crate::coverage::TermCoverage> = model
        .terms
        .iter()
        .map(|t| crate::coverage::term_coverage(t, &aligned))
        .collect();
    let total = coverages.len();

    // Per-dimension coverage — covered count = total − (the docs/missing-* count).
    heading(&mut out, 2, model.ui("body_coverage_by_dimension"));
    push_line(&mut out, "| Dimension | Covered | Total | % |");
    push_line(&mut out, "| --- | --- | --- | --- |");
    for (i, dim) in crate::coverage::DIMENSIONS.iter().enumerate() {
        let covered = coverages.iter().filter(|c| c.flags()[i]).count();
        let pct = (covered * 100).checked_div(total).unwrap_or(0);
        push_line(
            &mut out,
            &format!("| {} | {covered} | {total} | {pct}% |", dim.label),
        );
    }
    blank(&mut out);

    // Completeness distribution: how many terms carry exactly k of the dimensions.
    heading(&mut out, 2, model.ui("body_completeness_distribution"));
    push_line(&mut out, "| Dimensions present | Terms |");
    push_line(&mut out, "| --- | --- |");
    let dims_total = crate::coverage::TermCoverage::TOTAL;
    for k in (0..=dims_total).rev() {
        let count = coverages.iter().filter(|c| c.present_count() == k).count();
        push_line(&mut out, &format!("| {k} / {dims_total} | {count} |"));
    }
    blank(&mut out);

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

fn md_term(model: &DocsModel, slug: &str) -> String {
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
        let aligned = crate::coverage::alignment_subjects(model);
        let badges = crate::badge::term_badges(term, &aligned, model.reasoning.as_ref());
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

    // ── Related terms (bidirectional: skos:related / pairsWith / seeAlso) ────────
    if !term.related_terms.is_empty() {
        heading(&mut out, 2, model.ui("body_related_terms"));
        for related in &term.related_terms {
            push_line(&mut out, &format!("- {}", term_link(model, &from, related)));
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
        }
        let fixture_index_href = rel(&from, &Page::FixtureIndex.dir());
        push_line(
            &mut out,
            &format!("- See the [conformance fixtures index]({fixture_index_href}index.md)."),
        );
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

    // ── Formalized by (reverse logic:formalizes back-refs) ──────────────────────
    if !term.formalized_by.is_empty() {
        heading(&mut out, 2, model.ui("body_formalized_by"));
        for subject in &term.formalized_by {
            push_line(&mut out, &format!("- {}", term_link(model, &from, subject)));
        }
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

    // ── Reasoning status (present only when the native reasoner verdict is attached)
    // The textual, accessible counterpart of the reasoning badge: a class is
    // satisfiable unless the native DL reasoner proved it unsatisfiable; a
    // non-class term is not-evaluated (satisfiability is a class notion). Never
    // rendered for a source-only model, so no satisfiability claim is fabricated.
    if let Some(verdict) = &model.reasoning {
        heading(&mut out, 2, model.ui("body_reasoning_status"));
        let status = if term.category == DocTermCategory::Class {
            if verdict.unsatisfiable.contains(&term.iri) {
                model.ui("body_reasoning_unsatisfiable")
            } else {
                model.ui("body_reasoning_satisfiable")
            }
        } else {
            model.ui("body_reasoning_not_evaluated")
        };
        push_line(&mut out, &format!("- {status}"));
        blank(&mut out);
    }

    // ── Documentation coverage (always present) ──────────────────────────────────
    // The six richness dimensions this term carries, read from the shared coverage
    // source — exactly the predicates behind the `docs/missing-*` lint, so the page
    // and the gate can never disagree about what a term is missing.
    {
        let aligned = crate::coverage::alignment_subjects(model);
        let cov = crate::coverage::term_coverage(term, &aligned);
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
    // slice's identifier cites the slice when one is registered.
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
         the target format cannot carry; the exact targets (canonical RDF 1.2, Nemo) \
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

fn md_slice(model: &DocsModel, slug: &str) -> String {
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
             the gate; **advisory** rules report without failing.",
            model.constraint_rules.len()
        ),
    );

    if model.constraint_rules.is_empty() {
        line(&mut out, model.ui("body_no_enforced_constraints"));
        return out;
    }

    // Group by category IRI (rules are already sorted by code); the category map
    // is a BTreeMap so category headings emit in sorted IRI order.
    let mut by_category: std::collections::BTreeMap<&str, Vec<&crate::model::ConstraintRule>> =
        std::collections::BTreeMap::new();
    for rule in &model.constraint_rules {
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
    out
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
    let body_html = rewrite_internal_links(&markdown_to_html(&to_markdown_exec(model, page, exec)));
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
    let body_scripts = if matches!(page, Page::SparqlPlayground) && exec.has_playground() {
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
    html.replace("index.md\"", "index.html\"")
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

/// A filesystem-safe slug from a term's local name: the IRI tail after the last
/// `/` or `#`, lowercased and reduced to `[a-z0-9-]`.
pub fn term_slug(term: &DocTerm) -> String {
    slugify(local_name(&term.iri))
}

/// A filesystem-safe slug from a term IRI's local name — the standalone twin of
/// [`term_slug`] for callers holding an IRI (not a [`DocTerm`]), e.g. `docs-on`
/// resolving a query to its `terms/<slug>/` page under the ontology-docs blob.
pub fn slug_for_iri(iri: &str) -> String {
    slugify(local_name(iri))
}

/// A filesystem-safe slug from a slice IRI's last path segment.
pub fn slice_slug(slice: &DocSlice) -> String {
    slugify(local_name(&slice.iri))
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
/// The compact CURIE for an IRI: `gmeow:Local` / `logic:Local` for the two
/// GMEOW-family namespaces, otherwise the IRI unchanged. Used by the constraint
/// catalog to abbreviate a rule's applies-to terms and formalized axiom.
fn to_curie(iri: &str) -> String {
    const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
    const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
    if let Some(local) = iri.strip_prefix(GMEOW_NS) {
        format!("gmeow:{local}")
    } else if let Some(local) = iri.strip_prefix(LOGIC_NS) {
        format!("logic:{local}")
    } else {
        iri.to_string()
    }
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
fn slice_display(slice: &DocSlice) -> String {
    slice
        .title
        .clone()
        .or_else(|| slice.label.clone())
        .unwrap_or_else(|| local_name(&slice.iri).to_string())
}

/// The display name for a concern: its label, else its IRI local name.
fn concern_display(concern: &DocConcern) -> String {
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
fn term_advice_facet(term: &DocTerm) -> Vec<String> {
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
type AlignmentFacets<'a> = std::collections::HashMap<&'a str, Vec<String>>;

/// Precompute alignment facets for all terms in one pass: maps each subject IRI
/// to a sorted+deduped `tag:object` token list. Avoids the O(N×M) per-term
/// linear scan of `model.linkages` when rendering the search and llms surfaces.
fn precompute_alignment_facets(model: &DocsModel) -> AlignmentFacets<'_> {
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
    let mut records: Vec<SearchRecord> = Vec::new();
    let alignment_facets = precompute_alignment_facets(model);
    let aligned = crate::coverage::alignment_subjects(model);

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
            missing_coverage: crate::coverage::term_coverage(term, &aligned).missing_keys(),
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
    let prose = vec![format!(
        "Vocabulary {}. Namespace: {GMEOW_NS}. The OWL source is canonical; this index links into the published documentation.",
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

    // ── Reference: the standing index pages (always present). ──
    sections.push(LlmsSection {
        heading: "Reference".to_string(),
        bullets: vec![
            reference_bullet("Slice index", &Page::SliceIndex),
            reference_bullet("Linkages", &Page::LinkageIndex),
            reference_bullet("External ontologies", &Page::ExternalIndex),
            reference_bullet("Integrity constraints", &Page::IntegrityIndex),
            reference_bullet("Logic & reasoning", &Page::Logic),
        ],
    });

    llms::render_index(&model.title, &prose, &sections)
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
    let prose = vec![format!(
        "Vocabulary {}. Namespace: {GMEOW_NS}. Complete inlined form — every term, its definition, and its usage advice in full.",
        model.version
    )];
    let mut out = llms::llms_header(&model.title, &prose);

    let alignment_facets = precompute_alignment_facets(model);
    out.push_str("## Terms\n\n");
    for term in &model.terms {
        out.push_str(&term_full_block(term, &alignment_facets));
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

    out
}

/// The metadata + definition + advisory-field body of a term (NO heading). The
/// shared core of both the per-term card and the `llms-full.txt` inlined block.
/// Pure markdown text (no links), so it is safe to inline anywhere. Takes the
/// precomputed alignment facets so a caller emitting every term pays the linkage
/// scan ONCE (not O(N²)).
fn term_body(term: &DocTerm, alignment_facets: &AlignmentFacets) -> String {
    crate::card::render_card_body(&doc_term_card(term, alignment_facets))
}

/// Build the neutral [`crate::card::Card`] from a docs-site [`DocTerm`], resolving
/// every IRI-bearing field to its display (local-name) form. The shared
/// [`crate::card::render_card_body`] then renders it — the SAME renderer the
/// folded-snapshot MCP card uses, so the two never diverge (§19 one-path).
fn doc_term_card(term: &DocTerm, alignment_facets: &AlignmentFacets) -> crate::card::Card {
    let label = match &term.label {
        Some(l) if l != &term.curie => Some(l.clone()),
        _ => None,
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
    }
}

/// The full inlined block for one term in `llms-full.txt`: a `### {curie}{signature}`
/// heading followed by the shared [`term_body`].
fn term_full_block(term: &DocTerm, alignment_facets: &AlignmentFacets) -> String {
    format!(
        "### {}{}\n\n{}",
        term.curie,
        term_signature(term),
        term_body(term, alignment_facets)
    )
}

/// A prompt-ready, standalone Markdown card for one term: a `# {curie}{signature}`
/// title followed by the shared [`term_body`] (metadata + definition + every
/// advisory field). Compact, link-free, and self-contained for context-window
/// injection. Emitted at `terms/{slug}/card.md` and served live over MCP.
pub fn term_card_md(model: &DocsModel, term: &DocTerm) -> String {
    let alignment_facets = precompute_alignment_facets(model);
    term_card_md_inner(term, &alignment_facets)
}

/// [`term_card_md`] with the alignment facets supplied — lets `render_site_lang`
/// emit every card while paying the linkage scan once.
fn term_card_md_inner(term: &DocTerm, alignment_facets: &AlignmentFacets) -> String {
    format!(
        "# {}{}\n\n{}",
        term.curie,
        term_signature(term),
        term_body(term, alignment_facets)
    )
}

/// The local names of a list of IRIs as an owned `Vec` (for the advisory-field
/// helper that takes `&[String]`).
fn local_name_vec(iris: &[String]) -> Vec<String> {
    iris.iter().map(|i| local_name(i).to_string()).collect()
}

// ── Casefolded slash-namespace aliases ─────────────────────────────────────────

/// For every term whose canonical slug differs from a casefolded form of its
/// local name, return `(alias_dir, target_dir)` pairs for tiny redirect pages.
///
/// Deterministic: derived purely from sorted terms; aliases that collide with a
/// canonical slug or with each other are skipped (first-wins, sorted) so two
/// terms never fight over the same alias directory.
fn term_aliases(model: &DocsModel) -> Vec<(String, String)> {
    // All canonical slugs, so an alias never shadows a real term page.
    let canonical: std::collections::BTreeSet<String> = model.terms.iter().map(term_slug).collect();

    let mut seen_aliases: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut out: Vec<(String, String)> = Vec::new();

    // model.terms is IRI-sorted → first-wins is deterministic.
    for term in &model.terms {
        let canonical_slug = term_slug(term);
        let alias = local_name(&term.iri).to_ascii_lowercase();
        if alias.is_empty() || alias == canonical_slug {
            continue;
        }
        if canonical.contains(&alias) || !seen_aliases.insert(alias.clone()) {
            continue;
        }
        out.push((format!("terms/{alias}"), format!("terms/{canonical_slug}")));
    }
    out.sort();
    out
}

/// A tiny redirect HTML page (meta refresh + canonical link + JS fallback) from
/// an alias directory to the canonical term directory.
fn alias_redirect_html(alias_dir: &str, target_dir: &str) -> String {
    let href = rel(alias_dir, target_dir);
    let target = format!("{href}index.html");
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n\
         <meta http-equiv=\"refresh\" content=\"0; url={target}\" />\n\
         <link rel=\"canonical\" href=\"{target}\" />\n<title>Redirecting…</title>\n\
         </head>\n<body>\n<p>Redirecting to <a href=\"{target}\">{target}</a>.</p>\n\
         </body>\n</html>\n"
    )
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
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            constraint_rules: Vec::new(),
            four_boxes: None,
            concept_doi: None,
            available_languages: vec!["english".to_string(), "fr".to_string()],
            translations,
            ui_catalog: crate::i18n::UiCatalog::default(),
            reasoning: None,
            lang: String::new(),
        }
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
