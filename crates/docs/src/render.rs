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

use minijinja::{context, Environment};
use pulldown_cmark::{html as cmark_html, Options, Parser};

use crate::i18n::{self, ENGLISH};
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

/// The embedded minijinja HTML shell (doctype + head + nav + body + footer).
const SHELL: &str = include_str!("../templates/shell.html");

/// The embedded, self-contained CSS theme, emitted to `assets/gmeow.css`.
const CSS: &str = include_str!("../assets/gmeow.css");

/// The site-relative path the CSS asset is emitted to.
const CSS_PATH: &str = "assets/gmeow.css";

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
    /// The concerns index (`concerns/index`).
    ConcernIndex,
    /// A single concern page (`concerns/<slug>/index`).
    Concern(String),
    /// The external-ontologies index (`external-ontologies/index`).
    ExternalIndex,
    /// The integrity-constraints (verify queries) index
    /// (`integrity-constraints/index`).
    IntegrityIndex,
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
}

impl Page {
    /// The site-relative directory this page lives in (no trailing slash, empty
    /// for the root landing page).
    pub fn dir(&self) -> String {
        match self {
            Page::Landing => String::new(),
            Page::GettingStarted => "getting-started".to_string(),
            Page::About => "about".to_string(),
            Page::Changelog => "changelog".to_string(),
            Page::Category(category) => category_dir(*category).to_string(),
            Page::Term(slug) => format!("terms/{slug}"),
            Page::SliceIndex => "slices".to_string(),
            Page::Slice(slug) => format!("slices/{slug}"),
            Page::LinkageIndex => "linkages".to_string(),
            Page::ExampleIndex => "examples".to_string(),
            Page::ConcernIndex => "concerns".to_string(),
            Page::Concern(slug) => format!("concerns/{slug}"),
            Page::ExternalIndex => "external-ontologies".to_string(),
            Page::IntegrityIndex => "integrity-constraints".to_string(),
            Page::RecipeIndex => "recipes".to_string(),
            Page::Recipe(slug) => format!("recipes/{slug}"),
            Page::LearningPathIndex => "learning-paths".to_string(),
            Page::LearningPath(slug) => format!("learning-paths/{slug}"),
            Page::FourBoxes => "four-boxes".to_string(),
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
    fn title(&self, model: &DocsModel) -> String {
        match self {
            Page::Landing => model.title.clone(),
            Page::GettingStarted => "Getting started".to_string(),
            Page::About => "About".to_string(),
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
            Page::ConcernIndex => "Concerns".to_string(),
            Page::Concern(slug) => model
                .concerns
                .iter()
                .find(|c| concern_slug(c) == *slug)
                .map(concern_display)
                .unwrap_or_else(|| slug.clone()),
            Page::ExternalIndex => "External ontologies".to_string(),
            Page::IntegrityIndex => "Integrity constraints".to_string(),
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
        files.insert(page.md_path(), to_markdown(model, &page).into_bytes());
        files.insert(
            page.html_path(),
            to_html_lang(model, &page, lang).into_bytes(),
        );
    }
    files.insert(CSS_PATH.to_string(), CSS.as_bytes().to_vec());

    // Deterministic SVG diagrams (pure functions of the model).
    files.insert(
        "diagrams/slices.svg".to_string(),
        svg::slice_dependency_svg(model).into_bytes(),
    );
    files.insert(
        "diagrams/concerns.svg".to_string(),
        svg::concern_overview_svg(model).into_bytes(),
    );
    for slice in &model.slices {
        files.insert(
            format!("diagrams/slices/{}.svg", slice_slug(slice)),
            svg::slice_local_svg(model, &slice.iri).into_bytes(),
        );
    }

    // Static indexes (deterministic, pure functions of the model).
    files.insert(
        "search-index.json".to_string(),
        search_index_json(model).into_bytes(),
    );
    files.insert(
        "llms-docs.txt".to_string(),
        llms_docs_txt(model).into_bytes(),
    );

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

/// The full, deterministically ordered page set for the model.
fn pages(model: &DocsModel) -> Vec<Page> {
    let mut pages = vec![
        Page::Landing,
        Page::GettingStarted,
        Page::About,
        Page::Changelog,
        Page::SliceIndex,
        Page::LinkageIndex,
        Page::ExampleIndex,
        Page::ConcernIndex,
        Page::ExternalIndex,
        Page::IntegrityIndex,
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
    match page {
        Page::Landing => md_landing(model),
        Page::GettingStarted => md_getting_started(model),
        Page::About => md_about(model),
        Page::Changelog => md_changelog(model),
        Page::Category(category) => md_category(model, *category),
        Page::Term(slug) => md_term(model, slug),
        Page::SliceIndex => md_slice_index(model),
        Page::Slice(slug) => md_slice(model, slug),
        Page::LinkageIndex => md_linkage_index(model),
        Page::ExampleIndex => md_example_index(model),
        Page::ConcernIndex => md_concern_index(model),
        Page::Concern(slug) => md_concern(model, slug),
        Page::ExternalIndex => md_external_index(model),
        Page::IntegrityIndex => md_integrity_index(model),
        Page::RecipeIndex => md_recipe_index(model),
        Page::Recipe(slug) => md_recipe(model, slug),
        Page::LearningPathIndex => md_learning_path_index(model),
        Page::LearningPath(slug) => md_learning_path(model, slug),
        Page::FourBoxes => md_four_boxes(model),
    }
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

    heading(&mut out, 2, "Vocabulary by category");
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

    heading(&mut out, 2, "Browse");
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

fn md_getting_started(_model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, "Getting started");
    line(
        &mut out,
        "The GMEOW ontology is organized into self-contained *slices*. Each slice owns a \
         vocabulary module, optional SHACL shapes, mappings, queries, and tests, and declares \
         its manifest identity and dependencies.",
    );
    heading(&mut out, 2, "Where to go next");
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
    heading(&mut out, 1, "About");
    line(
        &mut out,
        &format!(
            "**{}** is generated directly from the slice catalog by the Rust `gmeow-docs` \
             renderer. Every page is a deterministic projection of the typed documentation \
             model — there is no hand-authored HTML.",
            md_escape(&model.title)
        ),
    );
    heading(&mut out, 2, "At a glance");
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

fn md_changelog(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, "Changelog");
    line(
        &mut out,
        "This documentation surface is regenerated from the slice catalog on every build, so it \
         always reflects the current state of the ontology.",
    );
    heading(
        &mut out,
        2,
        &format!("Documentation model v{}", md_escape(&model.version)),
    );
    push_line(
        &mut out,
        &format!(
            "- {} terms and {} slices documented.",
            model.terms.len(),
            model.slices.len()
        ),
    );
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
        line(&mut out, "Term not found.");
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

    if let Some(def) = &term.definition {
        heading(&mut out, 2, "Definition");
        line(&mut out, &md_escape(def));
    }

    if !term.parents.is_empty() {
        let label = match term.category {
            DocTermCategory::Property => "Super-properties",
            _ => "Super-classes",
        };
        heading(&mut out, 2, label);
        for parent in &term.parents {
            push_line(&mut out, &format!("- {}", term_link(model, &from, parent)));
        }
        blank(&mut out);
    }

    if !term.domain.is_empty() {
        heading(&mut out, 2, "Domain");
        for d in &term.domain {
            push_line(&mut out, &format!("- {}", term_link(model, &from, d)));
        }
        blank(&mut out);
    }

    if !term.range.is_empty() {
        heading(&mut out, 2, "Range");
        for r in &term.range {
            push_line(&mut out, &format!("- {}", term_link(model, &from, r)));
        }
        blank(&mut out);
    }

    out
}

fn md_slice_index(model: &DocsModel) -> String {
    let from = Page::SliceIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, "Slices");
    line(
        &mut out,
        &format!("{} compilation unit(s).", model.slices.len()),
    );

    heading(&mut out, 2, "Dependency graph");
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
        line(&mut out, "Slice not found.");
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
        heading(&mut out, 2, "Artifacts");
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
    heading(&mut out, 1, "Linkages");
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
        heading(&mut out, 2, "Other equivalences");
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
    heading(&mut out, 1, "Examples");
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

fn md_concern_index(model: &DocsModel) -> String {
    let from = Page::ConcernIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, "Concerns");
    line(
        &mut out,
        &format!(
            "**{}** cross-cutting documentation concern(s). Concerns group vocabulary terms by \
             the design question they answer, across slice boundaries.",
            model.concerns.len()
        ),
    );

    heading(&mut out, 2, "By term count");
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
        line(&mut out, "Concern not found.");
        return out;
    };
    let from = Page::Concern(slug.to_string()).dir();

    let mut out = String::new();
    heading(&mut out, 1, &concern_display(concern));
    line(&mut out, &format!("`{}`", code_escape(&concern.curie)));

    if let Some(def) = &concern.definition {
        heading(&mut out, 2, "Definition");
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
    heading(&mut out, 1, "External ontologies");
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
    heading(&mut out, 1, "Integrity constraints");
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
            .filter(|a| a.role == gmeow_slice::ArtifactRole::VerifyQuery)
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
        line(
            &mut out,
            "No verification queries are declared in any slice.",
        );
    }
    out
}

// ── Guides: recipes / learning paths / four boxes (#853 T3b) ──────────────────

fn md_recipe_index(model: &DocsModel) -> String {
    let from = Page::RecipeIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, "Recipes");
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
        line(&mut out, "No recipes are declared in the guides slice.");
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
        line(&mut out, "Recipe not found.");
        return out;
    };
    let from = Page::Recipe(slug.to_string()).dir();
    let mut out = String::new();
    heading(&mut out, 1, &recipe.title);
    line(
        &mut out,
        &format!("`{}` · recipe", code_escape(&recipe.slug)),
    );

    heading(&mut out, 2, "Goal");
    line(&mut out, &md_escape(&recipe.goal));

    if !recipe.term_curies.is_empty() {
        heading(&mut out, 2, "Terms used");
        for curie in &recipe.term_curies {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !recipe.example_paths.is_empty() {
        heading(&mut out, 2, "Example files");
        for path in &recipe.example_paths {
            push_line(&mut out, &format!("- `{}`", code_escape(path)));
        }
        blank(&mut out);
    }

    if !recipe.follow_pages.is_empty() {
        heading(&mut out, 2, "Read next");
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
        heading(&mut out, 2, "Part of");
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
    heading(&mut out, 1, "Learning paths");
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
        line(
            &mut out,
            "No learning paths are declared in the guides slice.",
        );
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
        line(&mut out, "Learning path not found.");
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

    heading(&mut out, 2, "Goal");
    line(&mut out, &md_escape(&path.goal));

    if !path.recipe_slugs.is_empty() {
        heading(&mut out, 2, "Recipes");
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
        heading(&mut out, 2, "Terms used");
        for curie in &path.term_curies {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !path.example_paths.is_empty() {
        heading(&mut out, 2, "Example files");
        for p in &path.example_paths {
            push_line(&mut out, &format!("- `{}`", code_escape(p)));
        }
        blank(&mut out);
    }

    if !path.adoption_targets.is_empty() {
        heading(&mut out, 2, "Projects toward");
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
            heading(&mut out, 1, "What is this?");
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
    let body_html = rewrite_internal_links(&markdown_to_html(&to_markdown(model, page)));
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
    let nav = vec![
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

    let page_lang = if lang == ENGLISH { "en" } else { lang };

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

    out
}

// ── Slugging ──────────────────────────────────────────────────────────────────

/// A filesystem-safe slug from a term's local name: the IRI tail after the last
/// `/` or `#`, lowercased and reduced to `[a-z0-9-]`.
pub fn term_slug(term: &DocTerm) -> String {
    slugify(local_name(&term.iri))
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
fn slice_link(model: &DocsModel, from: &str, iri: &str) -> String {
    if let Some(slice) = model.slices.iter().find(|s| s.iri == iri) {
        let href = rel(from, &Page::Slice(slice_slug(slice)).dir());
        return format!("[{}]({}index.md)", md_escape(&slice_display(slice)), href);
    }
    format!("`{}`", code_escape(iri))
}

/// A link from a term CURIE to its term page, or a plain `code` CURIE when the
/// term is not documented.
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
    use gmeow_slice::SliceTier;
    match &slice.tier {
        Some(SliceTier::Core) => "Core".to_string(),
        Some(SliceTier::Extension) => "Extension".to_string(),
        Some(SliceTier::Domain) => "Domain".to_string(),
        Some(SliceTier::Unknown(iri)) => local_name(iri).to_string(),
        None => "—".to_string(),
    }
}

/// A stable, human role name for an artifact role.
fn role_name(role: &gmeow_slice::ArtifactRole) -> String {
    use gmeow_slice::ArtifactRole;
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

// ── Static indexes (search-index.json, llms-docs.txt) ──────────────────────────

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
}

/// Build the deterministic `search-index.json`: one record per term, slice,
/// concern, and mapping set, sorted by URL. A pure function of the model.
pub fn search_index_json(model: &DocsModel) -> String {
    let mut records: Vec<SearchRecord> = Vec::new();

    for term in &model.terms {
        records.push(SearchRecord {
            kind: "term",
            id: term.curie.clone(),
            label: term.label.clone().unwrap_or_else(|| term.curie.clone()),
            definition: term.definition.clone(),
            url: format!("{}/index.html", Page::Term(term_slug(term)).dir()),
        });
    }
    for slice in &model.slices {
        records.push(SearchRecord {
            kind: "slice",
            id: slice.iri.clone(),
            label: slice_display(slice),
            definition: None,
            url: format!("{}/index.html", Page::Slice(slice_slug(slice)).dir()),
        });
    }
    for concern in &model.concerns {
        records.push(SearchRecord {
            kind: "concern",
            id: concern.curie.clone(),
            label: concern_display(concern),
            definition: concern.definition.clone(),
            url: format!("{}/index.html", Page::Concern(concern_slug(concern)).dir()),
        });
    }
    for set in &model.mapping_sets {
        records.push(SearchRecord {
            kind: "mappingSet",
            id: set.curie.clone(),
            label: set_display(set),
            definition: set.comment.clone(),
            url: format!("{}/index.html", Page::LinkageIndex.dir()),
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

/// Build the deterministic `llms-docs.txt`: an LLM-friendly plaintext dump —
/// a header (title + version + counts) then one sorted line per term:
/// `curie — label: definition (category, owner slice)`.
pub fn llms_docs_txt(model: &DocsModel) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n", model.title));
    out.push_str(&format!("# version: {}\n", model.version));
    out.push_str(&format!(
        "# terms: {}  slices: {}  concerns: {}  linkages: {}\n",
        model.terms.len(),
        model.slices.len(),
        model.concerns.len(),
        model.linkages.len(),
    ));
    out.push('\n');

    // Terms are already IRI-sorted; emit by (curie, iri) for a stable, readable
    // ordering keyed on the human identifier.
    let mut terms: Vec<&DocTerm> = model.terms.iter().collect();
    terms.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    for term in terms {
        let label = term.label.as_deref().unwrap_or("");
        let def = term.definition.as_deref().map(one_line).unwrap_or_default();
        let slice = local_name(&term.owner_slice);
        out.push_str(&format!(
            "{} — {}: {} ({}, {})\n",
            term.curie,
            label,
            def,
            category_singular(term.category),
            slice,
        ));
    }
    out
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
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            four_boxes: None,
            available_languages: vec!["english".to_string(), "fr".to_string()],
            translations,
            ui_catalog: crate::i18n::UiCatalog::default(),
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
}
