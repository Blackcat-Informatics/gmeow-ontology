// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `SourceToPageMap` — the single link-rewrite authority for the first-class
//! Markdown document model.
//!
//! Every `text/markdown` source in the slice catalog is a
//! [`DocMarkdownDocument`](crate::model::DocMarkdownDocument). This module is the
//! ONE place that decides where each document's generated page lives and what its
//! in-page anchors are, and the ONE place that resolves a relative markdown
//! link/anchor to a target location or reports it dangling. It is a PURE function
//! of the [`DocsModel`] ([`SourceToPageMap::build`]); the renderers and the RDF
//! projection rebuild it from the model rather than caching a second copy, so the
//! page graph can never silently diverge.
//!
//! # Page-path scheme
//!
//! - The top-level `docs.md` maps to its slice page: `slices/{slice-slug}/`.
//! - Every other markdown maps to `slices/{slice-slug}/documents/{path-without-.md}/`.
//!
//! # Anchors
//!
//! Heading anchors are keyed by `(page, heading)` with PAGE-SCOPED slugs: two
//! documents that both contain `## Overview` land on different pages, so both keep
//! the slug `overview` without colliding. Within one page, a repeated heading text
//! is disambiguated by an incrementing `-N` suffix (the GitHub convention), so the
//! eventual heading-demotion graft has stable, unique anchors.

use std::collections::{BTreeMap, HashSet};

use crate::model::{DocsError, DocsModel};

/// A resolved location within the generated documentation site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetLocation {
    /// The generated page path (trailing slash), e.g.
    /// `slices/core-foo/documents/design/architecture/`, or the slice page
    /// `slices/core-foo/` for a top-level `docs.md`.
    pub page: String,
    /// The page-scoped anchor slug, when the resolution targeted a heading/fragment.
    pub anchor: Option<String>,
}

/// The outcome of resolving a link/anchor: a concrete [`TargetLocation`], or a
/// dangling reference the renderer must surface (never silently drop).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkResolution {
    /// The link resolved to a known page (and, when requested, a known anchor).
    Resolved(TargetLocation),
    /// The target document or anchor is not in the model — a dangling internal link.
    Dangling {
        /// The best available description of what was being resolved (a
        /// `slice-iri :: source-path`, or the raw link text for a relative link).
        target: String,
        /// The requested anchor slug, when the link carried a fragment.
        anchor: Option<String>,
    },
}

/// The classification of an authored markdown link, resolved through the single
/// [`SourceToPageMap`] authority. This is the ONE decision surface every document
/// renderer (the site graft/child pages, and the print/PDF inliner) consults to
/// decide how to re-emit a `[text](target)` destination — so no renderer invents a
/// second notion of "internal vs. off-corpus vs. external".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocLinkResolution {
    /// The target is not an internal source-document reference (an explicit scheme
    /// `http:`/`mailto:`, a protocol-relative `//…`, a site-absolute `/…`, or an
    /// empty destination): the caller keeps the URL exactly as authored.
    External,
    /// The target resolves to a page IN the corpus (a within-slice document and,
    /// when a fragment was given, a known heading anchor). The caller links to that
    /// generated page/anchor.
    Corpus(TargetLocation),
    /// The target is a relative reference that leaves this slice's document corpus —
    /// another slice, a repo `docs/` file, or a non-markdown asset. The caller
    /// absolutizes it to the published site (a declared cross-link loss), never a
    /// live in-corpus link.
    OffCorpus,
    /// A relative within-slice markdown/anchor reference that resolves to NO document
    /// or heading — a genuine dangling internal link. The caller HARD-FAILS (a broken
    /// link in authored slice markdown is fixed at the source, never rendered).
    Dangling {
        /// The best available description of what failed to resolve.
        target: String,
        /// The requested anchor slug, when the link carried a fragment.
        anchor: Option<String>,
    },
}

/// One document's listing entry for a slice's child-document index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentEntry {
    /// The owning slice IRI.
    pub slice_iri: String,
    /// The document title (first H1 or humanized filename).
    pub title: String,
    /// The normalized logical source path (the sort key).
    pub source_path: String,
    /// The raw content digest of the source bytes.
    pub raw_digest: String,
    /// The generated page path (trailing slash).
    pub page: String,
}

/// One heading anchor within a page, in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingAnchor {
    /// The ATX heading level (1–6).
    pub level: u8,
    /// The heading text (trimmed, closing `#` run removed).
    pub text: String,
    /// The page-scoped, disambiguated anchor slug.
    pub slug: String,
}

/// Per-document resolution state.
#[derive(Debug, Clone)]
struct DocEntry {
    /// The generated page path (trailing slash).
    page: String,
    /// The injective documentation-node slug (the `{slug}` in the RDF
    /// `documentation/document/{slug}` subject), unique across the whole model.
    node_slug: String,
    /// Whether this document IS the slice page (the top-level `docs.md`).
    is_slice_page: bool,
}

/// Per-page anchor index.
#[derive(Debug, Clone, Default)]
struct PageAnchors {
    /// The headings in source order.
    headings: Vec<HeadingAnchor>,
    /// The heading slug set for O(1) fragment membership.
    slugs: HashSet<String>,
}

/// The single link-rewrite authority: a pure function of the [`DocsModel`]'s
/// document set. Keyed by `(slice-iri, normalized-source-path)`.
#[derive(Debug, Clone, Default)]
pub struct SourceToPageMap {
    /// Every document, keyed by `(slice-iri, source-path)`.
    by_source: BTreeMap<(String, String), DocEntry>,
    /// Per-page anchor index, keyed by page path.
    pages: BTreeMap<String, PageAnchors>,
    /// Per-slice child-document listing (excludes the top-level `docs.md`), sorted
    /// by source path.
    children: BTreeMap<String, Vec<DocumentEntry>>,
    /// page path → the owning document's injective node slug. The inverse of the
    /// per-document `node_slug`, so a resolver that only has a target page (the
    /// print inliner minting a collision-free Typst label) can recover the injective
    /// identity without a scan.
    page_node_slug: BTreeMap<String, String>,
}

impl SourceToPageMap {
    /// Build the map from the model's document set — the pure function the whole
    /// surface derives from.
    ///
    /// Hard-fails ([`DocsError::MarkdownPageCollision`]) when two distinct documents
    /// map to the same generated page path (e.g. two slices whose slugs collide,
    /// both carrying a `docs.md`). Never silently drops the second.
    pub fn build(model: &DocsModel) -> Result<Self, DocsError> {
        let mut by_source: BTreeMap<(String, String), DocEntry> = BTreeMap::new();
        let mut pages: BTreeMap<String, PageAnchors> = BTreeMap::new();
        let mut children: BTreeMap<String, Vec<DocumentEntry>> = BTreeMap::new();
        let mut page_node_slug: BTreeMap<String, String> = BTreeMap::new();
        // page path → the (slice-iri, source-path) that first claimed it, for the
        // page-collision hard-fail.
        let mut page_owner: BTreeMap<String, (String, String)> = BTreeMap::new();
        // Minted documentation-node slugs, kept injective across the whole model.
        let mut used_node_slugs: HashSet<String> = HashSet::new();

        // `model.slices` is IRI-sorted and each slice's `documents` is path-sorted,
        // so this walk is deterministic.
        for slice in &model.slices {
            for doc in &slice.documents {
                let page = page_path(&doc.slice_slug, &doc.source_path);

                // Page-path collision (3c): two DISTINCT documents onto one page.
                if let Some((owner_iri, owner_path)) = page_owner.get(&page) {
                    return Err(DocsError::MarkdownPageCollision {
                        page,
                        first: format!("{owner_iri} :: {owner_path}"),
                        second: format!("{} :: {}", doc.slice_iri, doc.source_path),
                    });
                }
                page_owner.insert(
                    page.clone(),
                    (doc.slice_iri.clone(), doc.source_path.clone()),
                );

                // Page-scoped heading anchors.
                let headings = heading_anchors(&doc.source_text);
                let slugs: HashSet<String> = headings.iter().map(|h| h.slug.clone()).collect();
                pages.insert(page.clone(), PageAnchors { headings, slugs });

                // Injective documentation-node slug. `page` is already unique (the
                // collision check above), so its slug is unique save for the lossy
                // fold in `slugify`; a residual clash appends an incrementing suffix,
                // exactly the term-slug disambiguation discipline — never a silent
                // conflation.
                let base = slugify(&page);
                let mut node_slug = base.clone();
                let mut n = 2;
                while used_node_slugs.contains(&node_slug) {
                    node_slug = format!("{base}-{n}");
                    n += 1;
                }
                used_node_slugs.insert(node_slug.clone());
                page_node_slug.insert(page.clone(), node_slug.clone());

                let is_slice_page = doc.source_path == SLICE_PAGE_SOURCE;
                by_source.insert(
                    (doc.slice_iri.clone(), doc.source_path.clone()),
                    DocEntry {
                        page: page.clone(),
                        node_slug,
                        is_slice_page,
                    },
                );

                // A child document is every non-slice-page markdown (gets its own
                // child page). The top-level `docs.md` IS the slice page, so it is
                // not itself a child.
                if !is_slice_page {
                    children
                        .entry(doc.slice_iri.clone())
                        .or_default()
                        .push(DocumentEntry {
                            slice_iri: doc.slice_iri.clone(),
                            title: doc.title.clone(),
                            source_path: doc.source_path.clone(),
                            raw_digest: doc.raw_digest.clone(),
                            page: page.clone(),
                        });
                }
            }
        }

        // Children are pushed in the already path-sorted document order, but sort
        // explicitly so the contract ("sorted by logical path") holds regardless.
        for entries in children.values_mut() {
            entries.sort_by(|a, b| a.source_path.cmp(&b.source_path));
        }

        Ok(Self {
            by_source,
            pages,
            children,
            page_node_slug,
        })
    }

    /// The total resolver: map a target document (by owning slice IRI + normalized
    /// source path) and optional heading anchor to its generated location, or report
    /// it dangling when no such document/anchor exists.
    ///
    /// The requested anchor is normalized through the SAME slug function the map
    /// mints headings with, so `#Overview` and `#overview` both resolve to the
    /// `overview` anchor.
    pub fn resolve(
        &self,
        slice_iri: &str,
        source_path: &str,
        anchor: Option<&str>,
    ) -> LinkResolution {
        let key = (slice_iri.to_string(), normalize_logical_path(source_path));
        let Some(entry) = self.by_source.get(&key) else {
            return LinkResolution::Dangling {
                target: format!("{slice_iri} :: {source_path}"),
                anchor: anchor.map(fragment_slug),
            };
        };
        match anchor {
            None => LinkResolution::Resolved(TargetLocation {
                page: entry.page.clone(),
                anchor: None,
            }),
            Some(raw) => {
                let slug = fragment_slug(raw);
                let page_anchors = self.pages.get(&entry.page);
                if page_anchors.is_some_and(|p| p.slugs.contains(&slug)) {
                    LinkResolution::Resolved(TargetLocation {
                        page: entry.page.clone(),
                        anchor: Some(slug),
                    })
                } else {
                    LinkResolution::Dangling {
                        target: format!("{slice_iri} :: {source_path}"),
                        anchor: Some(slug),
                    }
                }
            }
        }
    }

    /// Resolve a raw relative markdown link (`../design/FOO.md#overview`, `./BAR.md`,
    /// `#anchor`) as written INSIDE the document `(from_slice, from_path)`. Splits
    /// off any `#fragment`, joins the relative path against the linking document's
    /// directory, then defers to [`resolve`](Self::resolve).
    ///
    /// A link with an explicit scheme (`http:`, `https:`, `mailto:`, …) or a
    /// site-absolute `/…` path is not an internal source-document link; it is
    /// reported dangling here (the caller filters external links out before asking).
    pub fn resolve_link(&self, from_slice: &str, from_path: &str, link: &str) -> LinkResolution {
        let (path_part, fragment) = split_fragment(link);

        // A pure fragment (`#anchor`) targets a heading in the SAME document.
        if path_part.is_empty() {
            return self.resolve(from_slice, from_path, fragment);
        }

        if is_external_link(path_part) {
            return LinkResolution::Dangling {
                target: link.to_string(),
                anchor: fragment.map(fragment_slug),
            };
        }

        let target_path = join_relative(from_path, path_part);
        self.resolve(from_slice, &target_path, fragment)
    }

    /// Classify an authored markdown link `[text](target)` as written INSIDE the
    /// document `(from_slice, from_path)` into a [`DocLinkResolution`] — the single
    /// decision the site and print renderers both consume. Mirrors the resolver's
    /// path handling: an explicit scheme / site-absolute / empty destination is
    /// [`External`](DocLinkResolution::External); a relative non-markdown asset or a
    /// reference that climbs above the slice root is
    /// [`OffCorpus`](DocLinkResolution::OffCorpus); a within-slice markdown/anchor
    /// reference resolves through [`resolve_link`](Self::resolve_link) to
    /// [`Corpus`](DocLinkResolution::Corpus) or (when nothing matches)
    /// [`Dangling`](DocLinkResolution::Dangling).
    pub fn classify_doc_link(
        &self,
        from_slice: &str,
        from_path: &str,
        target: &str,
    ) -> DocLinkResolution {
        if target.is_empty() {
            return DocLinkResolution::External;
        }
        let (path_part, _fragment) = split_fragment(target);
        // A non-fragment path is classified before resolution; a pure `#fragment`
        // (empty path) is a same-document anchor and falls straight to the resolver.
        if !path_part.is_empty() {
            if is_external_link(path_part) {
                return DocLinkResolution::External;
            }
            // Only relative markdown→markdown references can name a corpus document.
            if !path_part.ends_with(".md") {
                return DocLinkResolution::OffCorpus;
            }
            // A `..`-climb above the slice root leaves this slice-scoped corpus.
            if escapes_slice_root(from_path, path_part) {
                return DocLinkResolution::OffCorpus;
            }
        }
        match self.resolve_link(from_slice, from_path, target) {
            LinkResolution::Resolved(loc) => DocLinkResolution::Corpus(loc),
            LinkResolution::Dangling { target, anchor } => {
                DocLinkResolution::Dangling { target, anchor }
            }
        }
    }

    /// A slice's child documents (every non-slice-page markdown), sorted by logical
    /// path, each carrying its title, source path, raw digest, and generated page —
    /// the provenance the slice-page renderer's document index shows. Empty for a
    /// slice with no child markdown.
    pub fn slice_children(&self, slice_iri: &str) -> Vec<DocumentEntry> {
        self.children.get(slice_iri).cloned().unwrap_or_default()
    }

    /// The generated page path for a document, or `None` when it is not in the model.
    pub fn page_of(&self, slice_iri: &str, source_path: &str) -> Option<&str> {
        let key = (slice_iri.to_string(), normalize_logical_path(source_path));
        self.by_source.get(&key).map(|e| e.page.as_str())
    }

    /// The injective `documentation/document/{slug}` node slug for a document — the
    /// stable, collision-free identity the RDF projection mints its subject from.
    pub fn node_slug(&self, slice_iri: &str, source_path: &str) -> Option<&str> {
        let key = (slice_iri.to_string(), normalize_logical_path(source_path));
        self.by_source.get(&key).map(|e| e.node_slug.as_str())
    }

    /// The ordered heading anchors of a page (empty when the page carries none) —
    /// the stable, page-scoped anchor set the heading-demotion graft rewrites onto.
    pub fn heading_anchors(&self, page: &str) -> &[HeadingAnchor] {
        self.pages
            .get(page)
            .map(|p| p.headings.as_slice())
            .unwrap_or(&[])
    }

    /// Whether a document is the slice page (the top-level `docs.md`).
    pub fn is_slice_page(&self, slice_iri: &str, source_path: &str) -> bool {
        let key = (slice_iri.to_string(), normalize_logical_path(source_path));
        self.by_source.get(&key).is_some_and(|e| e.is_slice_page)
    }

    /// Whether `page` (a trailing-slash generated page path, e.g. `slices/zoo/` or
    /// `slices/zoo/documents/design/A/`) is a page the map minted for some document.
    /// This is the single authority the mdbook / site link rewriters consult to
    /// decide whether a resolved cross-document target names a real corpus page (so a
    /// within-book link that resolves here MUST be an emitted chapter — a resolved
    /// document page that is somehow not emitted is a hard-fail dangling link, never a
    /// silent externalization).
    pub fn is_known_page(&self, page: &str) -> bool {
        self.pages.contains_key(page)
    }

    /// The injective node slug of the document whose generated page is `page`, or
    /// `None` when no document maps there. The inverse of
    /// [`node_slug`](Self::node_slug); the print inliner uses it to mint a
    /// collision-free Typst label for a link that resolves only to a target page.
    pub fn node_slug_of_page(&self, page: &str) -> Option<&str> {
        self.page_node_slug.get(page).map(String::as_str)
    }
}

/// The source path that maps to the slice page rather than a child page.
pub(crate) const SLICE_PAGE_SOURCE: &str = "docs.md";

/// The generated page path (trailing slash) for a document given its slice slug
/// and normalized source path — the ONE scheme both the map and the renderers
/// derive from. The top-level `docs.md` maps to its slice page `slices/{slug}/`;
/// every other markdown maps to `slices/{slug}/documents/{stem}/`. Public so the
/// static-site renderer's [`crate::render::Page::SliceDocument`] arm computes a
/// child page's path through this single authority rather than re-deriving the
/// scheme.
pub fn page_for(slice_slug: &str, source_path: &str) -> String {
    page_path(slice_slug, source_path)
}

/// The generated page path for a document. The top-level `docs.md` is the slice
/// page; every other markdown gets `slices/{slice-slug}/documents/{stem}/`.
fn page_path(slice_slug: &str, source_path: &str) -> String {
    if source_path == SLICE_PAGE_SOURCE {
        return format!("slices/{slice_slug}/");
    }
    let stem = source_path.strip_suffix(".md").unwrap_or(source_path);
    format!("slices/{slice_slug}/documents/{stem}/")
}

/// Normalize a logical source path: forward slashes, no leading `./`, no leading
/// `/`. Mirrors `crate::model`'s normalization so the map and the model agree.
fn normalize_logical_path(path: &str) -> String {
    let mut p = path.replace('\\', "/");
    while let Some(stripped) = p.strip_prefix("./") {
        p = stripped.to_string();
    }
    p.trim_start_matches('/').to_string()
}

/// Split a link into `(path, Some(fragment))` on the first `#`; `(path, None)` when
/// there is no fragment. The fragment is the raw text after `#` (empty stays
/// `Some("")` collapsed to the same document).
fn split_fragment(link: &str) -> (&str, Option<&str>) {
    match link.split_once('#') {
        Some((path, frag)) => (path, Some(frag)),
        None => (link, None),
    }
}

/// True when a link carries an explicit URI scheme or is site-absolute — i.e. it is
/// NOT a relative source-document reference.
fn is_external_link(path: &str) -> bool {
    if path.starts_with('/') {
        return true;
    }
    // A scheme is `[A-Za-z][A-Za-z0-9+.-]*:` before any `/`. Detect a `:` that
    // precedes the first `/` (so `foo:bar` is a scheme, `a/b:c` is not).
    match (path.find(':'), path.find('/')) {
        (Some(colon), Some(slash)) => colon < slash,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Whether a relative link `..`-climbs above the slice root when joined against the
/// linking document's directory — i.e. it targets something OUTSIDE this slice's
/// document corpus (another slice, a repo-level doc). The resolver silently clamps
/// such an escape; this reports it so [`SourceToPageMap::classify_doc_link`] can
/// classify the reference as off-corpus rather than mis-resolving it.
fn escapes_slice_root(from_path: &str, link_path: &str) -> bool {
    let mut depth = match from_path.rsplit_once('/') {
        Some((dir, _)) => dir.split('/').filter(|s| !s.is_empty()).count(),
        None => 0,
    };
    for seg in link_path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            _ => depth += 1,
        }
    }
    false
}

/// Join a relative link path against the directory of `from_path`, resolving `.`
/// and `..` segments over the slash-separated path. The result is a normalized
/// slice-relative path (leading `..` that escape the slice root are dropped, since
/// there is no document above the slice root to reach).
fn join_relative(from_path: &str, link: &str) -> String {
    // Start from the linking document's DIRECTORY segments.
    let mut segments: Vec<&str> = {
        let dir = match from_path.rsplit_once('/') {
            Some((dir, _file)) => dir,
            None => "",
        };
        dir.split('/').filter(|s| !s.is_empty()).collect()
    };
    for seg in link.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// The page-scoped GitHub-style slug of a heading (or a requested fragment):
/// lowercase, drop characters that are not alphanumeric / space / `-`, then map
/// spaces to `-` and collapse `-` runs. Empty input → `section`.
fn heading_slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_dash = false;
        } else if (ch == ' ' || ch == '-' || ch == '\t' || ch == '_')
            && !prev_dash
            && !out.is_empty()
        {
            out.push('-');
            prev_dash = true;
        }
        // All other punctuation is dropped (GitHub convention).
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "section".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Normalize a requested link fragment to a heading slug for membership testing —
/// the same fold headings are slugged with, so `#Overview` matches heading
/// `## Overview`.
fn fragment_slug(fragment: &str) -> String {
    heading_slug(fragment)
}

/// A filesystem-safe slug over an arbitrary string, lowercased and reduced to
/// `[a-z0-9-]`. Used to mint the injective documentation-node slug from a page path.
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

/// Extract the page's heading anchors in source order, assigning each a page-scoped
/// slug with duplicate-heading disambiguation (`overview`, `overview-1`, …), the
/// GitHub convention. Fenced code blocks (```` ``` ````/`~~~`) are skipped so a `#`
/// comment inside a code sample is never mistaken for a heading.
fn heading_anchors(source: &str) -> Vec<HeadingAnchor> {
    let mut out: Vec<HeadingAnchor> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut in_fence = false;
    let mut fence_marker = "";
    for line in source.lines() {
        let trimmed = line.trim_start();
        // Toggle fenced code blocks on ``` or ~~~ (at least three).
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
            continue;
        }
        if in_fence {
            continue;
        }
        let Some((level, text)) = atx_heading(trimmed) else {
            continue;
        };
        let base = heading_slug(&text);
        let count = seen.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        out.push(HeadingAnchor { level, text, slug });
    }
    out
}

/// The fence marker (` ``` ` or `~~~`) a line opens/closes with, when it is a code
/// fence (three or more of one marker char).
pub(crate) fn fence_open(trimmed: &str) -> Option<&'static str> {
    if trimmed.starts_with("```") {
        Some("```")
    } else if trimmed.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// Parse an ATX heading line into `(level, text)`: 1–6 leading `#`, then required
/// whitespace, then the heading text (trailing closing `#` run and whitespace
/// trimmed). `None` for a non-heading line.
fn atx_heading(trimmed: &str) -> Option<(u8, String)> {
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    // A heading requires whitespace (or end of line) after the hash run.
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some((hashes as u8, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DocMarkdownDocument, DocSlice};

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

    fn doc(slice_iri: &str, slice_slug: &str, path: &str, text: &str) -> DocMarkdownDocument {
        DocMarkdownDocument {
            slice_iri: slice_iri.to_string(),
            slice_slug: slice_slug.to_string(),
            source_path: path.to_string(),
            title: crate::model::markdown_title(text, path),
            source_text: text.to_string(),
            raw_digest: format!("digest-of-{path}"),
        }
    }

    fn model_with(docs: Vec<DocMarkdownDocument>) -> DocsModel {
        let slice_iri = format!("{GMEOW}slices/zoo");
        let mut model = DocsModel::empty_for_test();
        model.slices = vec![DocSlice::bare_for_test(&slice_iri, docs)];
        model
    }

    #[test]
    fn docs_md_maps_to_slice_page_others_to_child_pages() {
        let iri = format!("{GMEOW}slices/zoo");
        let model = model_with(vec![
            doc(&iri, "zoo", "docs.md", "# Zoo\n\nThesis.\n"),
            doc(
                &iri,
                "zoo",
                "design/ARCHITECTURE.md",
                "# Architecture\n\n## Overview\n\nx\n",
            ),
        ]);
        let map = SourceToPageMap::build(&model).expect("build");
        assert_eq!(map.page_of(&iri, "docs.md"), Some("slices/zoo/"));
        assert_eq!(
            map.page_of(&iri, "design/ARCHITECTURE.md"),
            Some("slices/zoo/documents/design/ARCHITECTURE/")
        );
        // The child index excludes docs.md.
        let children = map.slice_children(&iri);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].source_path, "design/ARCHITECTURE.md");
        assert_eq!(children[0].title, "Architecture");
    }

    #[test]
    fn page_scoped_anchors_do_not_collide_across_documents() {
        let iri = format!("{GMEOW}slices/zoo");
        let model = model_with(vec![
            doc(&iri, "zoo", "a.md", "# A\n\n## Overview\n"),
            doc(&iri, "zoo", "b.md", "# B\n\n## Overview\n"),
        ]);
        let map = SourceToPageMap::build(&model).expect("build");
        // Both documents keep the `overview` slug — page-scoped, no collision.
        match map.resolve(&iri, "a.md", Some("overview")) {
            LinkResolution::Resolved(loc) => assert_eq!(loc.anchor.as_deref(), Some("overview")),
            other => panic!("expected resolved, got {other:?}"),
        }
        match map.resolve(&iri, "b.md", Some("Overview")) {
            LinkResolution::Resolved(loc) => assert_eq!(loc.anchor.as_deref(), Some("overview")),
            other => panic!("expected resolved, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_headings_in_one_page_disambiguate() {
        let iri = format!("{GMEOW}slices/zoo");
        let model = model_with(vec![doc(
            &iri,
            "zoo",
            "a.md",
            "# A\n\n## Notes\n\n## Notes\n",
        )]);
        let map = SourceToPageMap::build(&model).expect("build");
        let anchors = map.heading_anchors("slices/zoo/documents/a/");
        let slugs: Vec<&str> = anchors.iter().map(|h| h.slug.as_str()).collect();
        assert_eq!(slugs, vec!["a", "notes", "notes-1"]);
    }

    #[test]
    fn relative_link_resolves_and_dangling_is_reported() {
        let iri = format!("{GMEOW}slices/zoo");
        let model = model_with(vec![
            doc(&iri, "zoo", "docs.md", "# Zoo\n"),
            doc(&iri, "zoo", "design/A.md", "# A\n\n## Deep Dive\n"),
        ]);
        let map = SourceToPageMap::build(&model).expect("build");
        // From docs.md, a relative link into the design doc + a valid anchor.
        match map.resolve_link(&iri, "docs.md", "design/A.md#deep-dive") {
            LinkResolution::Resolved(loc) => {
                assert_eq!(loc.page, "slices/zoo/documents/design/A/");
                assert_eq!(loc.anchor.as_deref(), Some("deep-dive"));
            }
            other => panic!("expected resolved, got {other:?}"),
        }
        // A `..` climb back out from the design doc to docs.md.
        assert!(matches!(
            map.resolve_link(&iri, "design/A.md", "../docs.md"),
            LinkResolution::Resolved(_)
        ));
        // A missing target is dangling.
        assert!(matches!(
            map.resolve_link(&iri, "docs.md", "design/NOPE.md"),
            LinkResolution::Dangling { .. }
        ));
        // A missing anchor on a real page is dangling.
        assert!(matches!(
            map.resolve_link(&iri, "docs.md", "design/A.md#ghost"),
            LinkResolution::Dangling { .. }
        ));
        // An external link is not an internal source reference.
        assert!(matches!(
            map.resolve_link(&iri, "docs.md", "https://example.org/x"),
            LinkResolution::Dangling { .. }
        ));
    }

    #[test]
    fn page_path_collision_hard_fails() {
        // Two slices whose slugs collide, both carrying a docs.md → one page path.
        let a = format!("{GMEOW}slices/a/zoo");
        let b = format!("{GMEOW}slices/b/zoo");
        let mut model = DocsModel::empty_for_test();
        model.slices = vec![
            DocSlice::bare_for_test(&a, vec![doc(&a, "zoo", "docs.md", "# A\n")]),
            DocSlice::bare_for_test(&b, vec![doc(&b, "zoo", "docs.md", "# B\n")]),
        ];
        let err = SourceToPageMap::build(&model).expect_err("page collision must hard-fail");
        assert!(matches!(err, DocsError::MarkdownPageCollision { .. }));
    }

    #[test]
    fn build_is_deterministic() {
        let iri = format!("{GMEOW}slices/zoo");
        let model = model_with(vec![
            doc(&iri, "zoo", "docs.md", "# Zoo\n\n## Overview\n"),
            doc(&iri, "zoo", "design/A.md", "# A\n"),
        ]);
        let a = SourceToPageMap::build(&model).expect("build");
        let b = SourceToPageMap::build(&model).expect("build");
        assert_eq!(a.slice_children(&iri), b.slice_children(&iri));
        assert_eq!(
            a.node_slug(&iri, "design/A.md"),
            b.node_slug(&iri, "design/A.md")
        );
    }
}
