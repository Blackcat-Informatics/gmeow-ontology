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

use minijinja::{context, Environment};
use pulldown_cmark::{html as cmark_html, Options, Parser};

use crate::model::{DocSlice, DocTerm, DocTermCategory, DocsModel};

/// The GMEOW vocabulary namespace (mirrors `model.rs`).
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

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

/// Render the full static-site tree from the model.
///
/// Emits, for every page, both `<dir>/index.md` and `<dir>/index.html`, plus the
/// CSS asset at `assets/gmeow.css`. The output is byte-identical across calls.
pub fn render_site(model: &DocsModel) -> Site {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    for page in pages(model) {
        files.insert(page.md_path(), to_markdown(model, &page).into_bytes());
        files.insert(page.html_path(), to_html(model, &page).into_bytes());
    }
    files.insert(CSS_PATH.to_string(), CSS.as_bytes().to_vec());

    Site { files }
}

/// The full, deterministically ordered page set for the model.
fn pages(model: &DocsModel) -> Vec<Page> {
    let mut pages = vec![
        Page::Landing,
        Page::GettingStarted,
        Page::About,
        Page::Changelog,
        Page::SliceIndex,
    ];
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

    out
}

// ── HTML layer ────────────────────────────────────────────────────────────────

/// Render a page to a complete, self-contained HTML document: the page's
/// Markdown body converted to HTML and injected into the minijinja shell.
pub fn to_html(model: &DocsModel, page: &Page) -> String {
    let body_html = rewrite_internal_links(&markdown_to_html(&to_markdown(model, page)));
    let root = root_href(&page.dir());

    // Nav items are a fixed, pre-sorted Vec (never a map) for determinism.
    let nav = vec![
        nav_item(&root, &Page::Landing.dir(), "Home"),
        nav_item(&root, &Page::SliceIndex.dir(), "Slices"),
        nav_item(
            &root,
            &Page::Category(DocTermCategory::Class).dir(),
            "Classes",
        ),
        nav_item(
            &root,
            &Page::Category(DocTermCategory::Property).dir(),
            "Properties",
        ),
        nav_item(&root, &Page::GettingStarted.dir(), "Getting started"),
        nav_item(&root, &Page::About.dir(), "About"),
    ];

    let mut env = Environment::new();
    env.add_template("shell", SHELL)
        .expect("embedded shell template is valid");
    let tmpl = env
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
}
