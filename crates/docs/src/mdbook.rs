// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic mdbook **source-tree** renderer.
//!
//! [`render_book`] emits the mdbook `src/` tree (a `book.toml`, a `SUMMARY.md`,
//! and one `src/<page-dir>/index.md` chapter per page) as a [`Site`]. It reuses
//! the single Markdown authority in [`crate::render`]: every chapter body is
//! exactly `to_markdown_exec(model, page, exec)` passed through the SINGLE
//! link-rewrite helper [`rewrite_book_links`] — there is no second body
//! renderer, and the razor is mechanized by the coherence property test.
//!
//! The page set is [`crate::render::book_pages`], which excludes the SPARQL
//! playground and every other exec-only interactive surface. Because the book
//! sets `create-missing = false`, a Markdown link to a page the book does not
//! emit would make `mdbook build` FAIL. [`rewrite_book_links`] therefore
//! resolves every site-relative link: links that land on a real chapter stay
//! relative (with a `.html` → `.md` fix-up), and links to any DROPPED surface
//! (the playground, the prompt-ready `card.md`, the diagram SVGs, …) are
//! rewritten to their absolute [`PUBLISHED_SITE_BASE`] URL and recorded, so the
//! set of externalized targets is observable.
//!
//! Determinism is structural: [`Site::files`] is a [`BTreeMap`], the page walk
//! is the fixed [`crate::render::book_pages`] order, and the rewrite
//! is a pure function of the body + page directory + chapter set.

use std::collections::{BTreeMap, BTreeSet};

use crate::render::{
    Page, Site, book_pages, interactive_asset_files, slice_slug, term_slug,
    to_markdown_exec_with_map,
};
use gmeow_docs_model::exec::ExecutableDocsData;
use gmeow_docs_model::model::{DocTerm, DocTermCategory, DocsModel};
use gmeow_docs_model::source_map::SourceToPageMap;

/// The book-root path of the `additional-js` boot shim. mdbook injects `additional-js`
/// files as plain `<script>` tags (no `type="module"`), so this ONE classic script
/// dynamically `import()`s the ES-module controller (resolved against the shim's own URL,
/// so it is correct at any chapter depth). It is emitted at the book root (mdbook copies
/// `additional-js` from there), NOT under `src/`.
pub const MDBOOK_BOOT_JS_PATH: &str = "mdbook-boot.js";

/// The `src/`-relative chapter path of the packed interactive host page (the bundle
/// explorer: browser SPARQL/describe + live reasoning + GMN transcode). Emitted into the
/// book only when the render is bundle-backed.
const MDBOOK_EXPLORER_CHAPTER: &str = "explorer";

/// The book chapter dir hosting the conjecture playground (the WASM-interactive docs W4 deliverable: browser
/// symmetric proof / counterproof over the curated demo library). Emitted into the book
/// only when the render is conjecture-backed (`has_conjectures()`).
const MDBOOK_CONJECTURE_CHAPTER: &str = "conjectures";

/// The absolute base URL of the published documentation site. The model carries
/// no canonical site/base URL field, so this single constant defines it. Every
/// dropped-surface link (the SPARQL playground, prompt cards, diagram SVGs) is
/// rewritten to `PUBLISHED_SITE_BASE` + its site-relative path, so an mdbook
/// build never dangles into a chapter that does not exist. It mirrors the GMEOW
/// vocabulary namespace host with a `docs/` prefix.
pub const PUBLISHED_SITE_BASE: &str = "https://blackcatinformatics.ca/gmeow/docs/";

/// The result of rewriting one chapter body's site-relative links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRewrite {
    /// The chapter body with every site-relative link resolved (chapter links
    /// kept relative + normalized to `.md`; dropped-surface links absolutized).
    pub body: String,
    /// The canonical site-relative targets (with any `?query`/`#fragment`) that
    /// were externalized because the book does not emit them as a chapter. This
    /// is the honesty input to the interactivity / cross-link declared-loss.
    pub external: BTreeSet<String>,
}

/// Render the mdbook `src/` tree for `model` as a deterministic [`Site`].
///
/// Emits `book.toml`, `SUMMARY.md`, and `src/<page-dir>/index.md` for every page
/// in [`book_pages`]. Chapter bodies are the single Markdown authority
/// ([`to_markdown_exec`]) transformed only by [`rewrite_book_links`]. The output
/// is byte-identical across calls.
pub fn render_book(model: &DocsModel, exec: &ExecutableDocsData) -> Site {
    let pages = book_pages(model);
    let chapters: BTreeSet<String> = pages.iter().map(Page::dir).collect();
    // The single link-rewrite authority, rebuilt from the model (a pure function of
    // its already-validated document set — the same total build the site render and
    // the RDF projection perform). It is the ONE authority that decides whether a
    // resolved cross-document link names a real corpus page, and it is what the
    // `SliceDocument` child-chapter nesting in `summary_md` reads.
    let page_map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");
    // The SINGLE collision-resolution authority (see [`dir_winners`]): both the
    // chapter body writer below and `summary_md`'s table of contents consult
    // this SAME map, so a colliding chapter path is only ever body-written and
    // ToC-listed for its one winning page.
    let winners = dir_winners(&pages);

    // The book packs the interactive engines when the render is exec-backed, so once
    // built it carries the SAME live SPARQL / reasoning / GMN transcode the site does
    // (its `Interactivity`/`LiveSparql`/`LiveReasoning` capabilities in
    // `gmeow_docs_model::formats` are not a bare claim — the assets are shipped here).
    let interactive = exec.has_bundle() || exec.has_playground();

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert(
        "book.toml".to_string(),
        book_toml(model, interactive).into_bytes(),
    );

    let mut summary = summary_md(model, &pages, &winners, &page_map);

    for (i, page) in pages.iter().enumerate() {
        if winners.get(&page.dir()) != Some(&i) {
            // A losing duplicate of a colliding chapter path — the winner (same
            // dir, chosen by `dir_winners`) already wrote (or will write) this
            // chapter's body, and `summary_md` only ever links the winner.
            continue;
        }
        let body = to_markdown_exec_with_map(model, page, exec, &page_map);
        let rewritten = rewrite_book_links(&body, &page.dir(), &chapters, &page_map);
        let path = chapter_src_path(&page.dir());
        files.insert(path, rewritten.body.into_bytes());
    }

    if interactive {
        pack_interactive_book(&mut files, &mut summary, model, exec, &chapters, &page_map);
    }

    // mdbook resolves the table of contents at `<src>/SUMMARY.md` (src defaults to
    // `src`), so the summary rides inside the source tree, not at the book root.
    files.insert("src/SUMMARY.md".to_string(), summary.into_bytes());

    Site { files }
}

/// The `additional-js` boot shim: a classic script that dynamically imports the
/// ES-module docs controller, resolving its URL against the shim's own `src` so it is
/// correct at any chapter depth. Emitted at [`MDBOOK_BOOT_JS_PATH`].
fn mdbook_boot_js() -> String {
    format!(
        "// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>\n\
         // SPDX-License-Identifier: AGPL-3.0-only\n\
         \n\
         // mdbook injects additional-js as a plain <script> (no type=\"module\"), so this classic\n\
         // shim dynamic-imports the ES-module docs controller. The URL is resolved against THIS\n\
         // script's own src, so it is correct regardless of the chapter's depth in the book.\n\
         //\n\
         // The shim runs on EVERY chapter, but the controller binds only to the DOM hooks a\n\
         // chapter actually carries — the same derivation the static site's per-page injection\n\
         // gate uses — so the two shells activate the identical control set.\n\
         (function () {{\n\
         \x20 var self = document.currentScript;\n\
         \x20 var base = (self && self.src) || window.location.href;\n\
         \x20 import(new URL(\"{path}\", base)).catch(function (e) {{\n\
         \x20   console.error(\"gmeow docs controller failed to load\", e);\n\
         \x20 }});\n\
         }})();\n",
        path = crate::render::DOCS_CONTROLLER_PATH,
    )
}

/// Pack the interactive engines + the bundle-explorer host chapter into the book.
///
/// The shared [`interactive_asset_files`] set (the controller + vendored wasm engines +
/// bundle/playground data — the byte-identical assets the site's witness lanes prove) is
/// copied under `src/`, so mdbook copies them to the built book preserving the `assets/`
/// prefix the controller resolves against. The boot shim rides at the book root (where
/// mdbook copies `additional-js` from). When the bundle is present the explorer chapter
/// (browser SPARQL/describe + live reasoning + GMN transcode) is emitted and appended to
/// the table of contents, giving the book a concrete interactive host page.
fn pack_interactive_book(
    files: &mut BTreeMap<String, Vec<u8>>,
    summary: &mut String,
    model: &DocsModel,
    exec: &ExecutableDocsData,
    chapters: &BTreeSet<String>,
    page_map: &SourceToPageMap,
) {
    for (path, bytes) in interactive_asset_files(exec) {
        // The controller resolves `assets/…`; mdbook copies `src/assets/…` → `assets/…`.
        files.insert(format!("src/{path}"), bytes);
    }
    files.insert(
        MDBOOK_BOOT_JS_PATH.to_string(),
        mdbook_boot_js().into_bytes(),
    );

    if exec.has_bundle() {
        let page = Page::BundleExplorer;
        let dir = page.dir();
        debug_assert_eq!(dir, MDBOOK_EXPLORER_CHAPTER);
        let body = to_markdown_exec_with_map(model, &page, exec, page_map);
        let rewritten = rewrite_book_links(&body, &dir, chapters, page_map);
        files.insert(chapter_src_path(&dir), rewritten.body.into_bytes());
        // A top-level table-of-contents entry so the chapter is reachable (mdbook's
        // `create-missing = false` accepts it because the chapter file exists).
        summary.push_str(&format!("\n- [{}]({}/index.md)\n", page.title(model), dir));
    }

    // The conjecture playground host chapter (browser symmetric proof / counterproof over
    // the curated demo library + the live wasm conjecture engine).
    if exec.has_conjectures() {
        let page = Page::ConjecturePlayground;
        let dir = page.dir();
        debug_assert_eq!(dir, MDBOOK_CONJECTURE_CHAPTER);
        let body = to_markdown_exec_with_map(model, &page, exec, page_map);
        let rewritten = rewrite_book_links(&body, &dir, chapters, page_map);
        files.insert(chapter_src_path(&dir), rewritten.body.into_bytes());
        summary.push_str(&format!("\n- [{}]({}/index.md)\n", page.title(model), dir));
    }
}

/// Compute the SINGLE deterministic winner for each colliding chapter
/// directory — the FIRST page in `pages` order for a given [`Page::dir`].
///
/// Distinct term IRIs can slugify to the same chapter path (see the
/// module-level note on [`summary_md`]); mdbook rejects a `SUMMARY.md` that
/// lists the same chapter file twice, and a chapter path can only carry ONE
/// body. Both the body writer in [`render_book`] and the table of contents in
/// [`summary_md`] resolve a collision by consulting THIS map, so the ToC link
/// label always names the exact page whose body is shipped at that path —
/// there is no second, independently-dedup'd notion of "the winner".
///
/// The FIRST-wins fold is the LEGACY term-slug behavior and applies ONLY to
/// term pages: two distinct term IRIs that lossily slugify to one path differ
/// only in that slug, so collapsing them is not a silent capability loss.
///
/// The child-document namespace is NOT allowed a silent winner. Child-document
/// page paths (`Page::SliceDocument`) are minted through
/// [`SourceToPageMap::build`], which HARD-FAILS on any two distinct documents
/// mapping to one generated page path, so every child-document chapter dir is
/// unique by construction. A collision that involves a child-document page
/// therefore signals a broken invariant, never a benign slug fold — it is a hard
/// failure naming BOTH colliding pages, so the new namespace can never quietly
/// drop a document behind a first-wins pick.
fn dir_winners(pages: &[Page]) -> BTreeMap<String, usize> {
    use std::collections::btree_map::Entry;
    let mut winners: BTreeMap<String, usize> = BTreeMap::new();
    for (i, page) in pages.iter().enumerate() {
        match winners.entry(page.dir()) {
            Entry::Vacant(slot) => {
                slot.insert(i);
            }
            Entry::Occupied(slot) => {
                let first = &pages[*slot.get()];
                if matches!(page, Page::SliceDocument { .. })
                    || matches!(first, Page::SliceDocument { .. })
                {
                    panic!(
                        "chapter dir {:?} collides between {first:?} and {page:?} — child-document \
                         page paths are minted through SourceToPageMap and MUST be unique; a \
                         collision here means the page-path scheme was violated",
                        page.dir()
                    );
                }
                // Legacy term-slug fold: keep the first, the shared collision winner.
            }
        }
    }
    winners
}

/// The set of dropped-surface targets the book externalizes, across every
/// chapter — the union of each chapter's [`LinkRewrite::external`]. Exposed so
/// the declared-loss ledger can be computed from the SAME rewrite authority the
/// book emits (no second, drifting definition of "what the book drops").
pub fn book_external_links(model: &DocsModel, exec: &ExecutableDocsData) -> BTreeSet<String> {
    let pages = book_pages(model);
    let chapters: BTreeSet<String> = pages.iter().map(Page::dir).collect();
    let page_map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");
    let mut all: BTreeSet<String> = BTreeSet::new();
    for page in &pages {
        let body = to_markdown_exec_with_map(model, page, exec, &page_map);
        all.extend(rewrite_book_links(&body, &page.dir(), &chapters, &page_map).external);
    }
    all
}

/// The `src/`-relative path of a page's chapter file (`""` → `src/index.md`).
fn chapter_src_path(dir: &str) -> String {
    if dir.is_empty() {
        "src/index.md".to_string()
    } else {
        format!("src/{dir}/index.md")
    }
}

/// The `SUMMARY.md` path (relative to `src/`) of a page's chapter.
fn summary_target(dir: &str) -> String {
    if dir.is_empty() {
        "index.md".to_string()
    } else {
        format!("{dir}/index.md")
    }
}

// ── book.toml ──────────────────────────────────────────────────────────────

/// The book configuration. Structure is fixed; only the title is interpolated
/// (from `model.title`). `create-missing = false` makes a dangling chapter link
/// a hard build failure — which is why [`rewrite_book_links`] externalizes every
/// link to a page the book does not emit.
///
/// When `interactive`, the book packs the vendored wasm engines + the shared controller
/// (see [`render_book`]) and wires them through the `additional-js` bootstrap
/// [`MDBOOK_BOOT_JS_PATH`] — a classic script that dynamically `import()`s the ES-module
/// controller (mdbook's `additional-js` injects plain `<script>` tags, so the module is
/// loaded through this one-line boot shim rather than directly).
fn book_toml(model: &DocsModel, interactive: bool) -> String {
    let additional_js = if interactive {
        format!("additional-js = [\"{MDBOOK_BOOT_JS_PATH}\"]\n")
    } else {
        String::new()
    };
    format!(
        "[book]\n\
         title = \"{}\"\n\
         language = \"en\"\n\
         multilingual = false\n\
         \n\
         [build]\n\
         create-missing = false\n\
         \n\
         [output.html]\n\
         default-theme = \"light\"\n\
         preferred-dark-theme = \"navy\"\n\
         no-section-label = false\n\
         {}",
        toml_escape(&model.title),
        additional_js,
    )
}

/// Escape a string for a double-quoted TOML value (backslash + quote + control).
fn toml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

// ── SUMMARY.md ─────────────────────────────────────────────────────────────

/// Build `SUMMARY.md` from the [`book_pages`] ordering as a nested list that
/// mirrors the category grouping: the landing page is the prefix chapter, the
/// fixed singleton pages are top-level chapters, and terms nest under their
/// category index, slices under the slice index, concerns under the concern
/// index, recipes / learning-paths under their indexes, and the logic
/// compiler-product pages under the logic index.
fn summary_md(
    model: &DocsModel,
    pages: &[Page],
    winners: &BTreeMap<String, usize>,
    map: &SourceToPageMap,
) -> String {
    // Bucket the child pages, preserving the book_pages order within each bucket.
    let mut slices: Vec<&Page> = Vec::new();
    let mut concerns: Vec<&Page> = Vec::new();
    let mut recipes: Vec<&Page> = Vec::new();
    let mut learning_paths: Vec<&Page> = Vec::new();
    let mut logic_children: Vec<&Page> = Vec::new();
    // Terms bucketed by category (the category-index ordering is fixed below).
    let mut terms_by_category: BTreeMap<DocTermCategory, Vec<&Page>> = BTreeMap::new();

    // Term slug → term lookup, built ONCE up front so the page loop below is
    // O(1) per `Page::Term` rather than re-slugging every term (O(N×M)) for
    // each page. `or_insert` keeps the FIRST term in `model.terms` order for a
    // given slug — the same collision winner `.find()` would have returned —
    // so a colliding slug's category resolution is unchanged.
    let mut term_by_slug: BTreeMap<String, &DocTerm> = BTreeMap::new();
    for term in &model.terms {
        term_by_slug.entry(term_slug(term)).or_insert(term);
    }

    // Distinct term IRIs can slugify to the same chapter path. mdbook rejects a
    // SUMMARY.md that lists the same chapter file twice, so the table of contents
    // must list each chapter path exactly once — and it must name the SAME winner
    // [`render_book`]'s body writer chose, via the shared [`dir_winners`] map,
    // never an independently-dedup'd first/last pick of its own.
    for (i, page) in pages.iter().enumerate() {
        if winners.get(&page.dir()) != Some(&i) {
            continue;
        }
        match page {
            Page::Slice(_) => slices.push(page),
            Page::Concern(_) => concerns.push(page),
            Page::Recipe(_) => recipes.push(page),
            Page::LearningPath(_) => learning_paths.push(page),
            Page::LogicCanonicalIr
            | Page::LogicLossLedger
            | Page::LogicDerivationGraph
            | Page::LogicDiagnostics => logic_children.push(page),
            Page::Term(slug) => {
                if let Some(term) = term_by_slug.get(slug) {
                    terms_by_category
                        .entry(term.category)
                        .or_default()
                        .push(page);
                }
            }
            _ => {}
        }
    }

    let mut out = String::new();
    out.push_str("# Summary\n\n");
    // Landing is the prefix chapter (before the numbered list).
    out.push_str(&format!(
        "[{}]({})\n\n",
        link_text(&Page::Landing.title(model)),
        summary_target(&Page::Landing.dir())
    ));

    for page in pages {
        // The prefix chapter and every nested child are emitted under their
        // parent, not at the top level.
        match page {
            Page::Landing
            | Page::Slice(_)
            | Page::SliceDocument { .. }
            | Page::Concern(_)
            | Page::Recipe(_)
            | Page::LearningPath(_)
            | Page::LogicCanonicalIr
            | Page::LogicLossLedger
            | Page::LogicDerivationGraph
            | Page::LogicDiagnostics
            | Page::Term(_) => continue,
            _ => {}
        }

        summary_entry(&mut out, model, page, 0);

        // Emit this parent's nested children (if it is a parent).
        let children: &[&Page] = match page {
            Page::SliceIndex => &slices,
            Page::ConcernIndex => &concerns,
            Page::RecipeIndex => &recipes,
            Page::LearningPathIndex => &learning_paths,
            Page::Logic => &logic_children,
            Page::Category(category) => terms_by_category
                .get(category)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            _ => &[],
        };
        for child in children {
            summary_entry(&mut out, model, child, 1);
            // A slice's child documents (`Page::SliceDocument`) nest ONE level deeper
            // under their slice, in the SAME path-sorted order the book emits their
            // chapters (`SourceToPageMap::slice_children`). This is the single place
            // the child-document table of contents is derived, so the ToC nesting can
            // never disagree with the emitted chapter set.
            if let Page::Slice(slug) = *child {
                for doc_page in slice_document_pages(model, map, slug) {
                    summary_entry(&mut out, model, &doc_page, 2);
                }
            }
        }
    }

    out
}

/// The `Page::SliceDocument` pages of the slice whose slug is `slug`, in the
/// map's path-sorted child order (the SAME order [`crate::render::pages`] emits
/// the child chapters, and the same set — every non-`docs.md` markdown source).
/// Empty when the slug names no slice or the slice has no child documents.
fn slice_document_pages(model: &DocsModel, map: &SourceToPageMap, slug: &str) -> Vec<Page> {
    let Some(slice) = model.slices.iter().find(|s| slice_slug(s) == slug) else {
        return Vec::new();
    };
    map.slice_children(&slice.iri)
        .into_iter()
        .map(|entry| Page::SliceDocument {
            slice: slice.iri.clone(),
            path: entry.source_path,
        })
        .collect()
}

/// Push one `SUMMARY.md` list entry at `depth` (2 spaces per level).
fn summary_entry(out: &mut String, model: &DocsModel, page: &Page, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(&format!(
        "- [{}]({})\n",
        link_text(&page.title(model)),
        summary_target(&page.dir())
    ));
}

/// Escape link text for a Markdown link label (`[`/`]` only — titles are CURIEs
/// / plain names, never containing a URL).
fn link_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

// ── Link rewrite (the single link-fidelity authority) ──────────────────────

/// Rewrite every site-relative link in a chapter body for the mdbook `src/`
/// tree. This is the SINGLE place link fidelity is defined.
///
/// For each `[text](target)` / `![alt](target)` whose `target` is site-relative
/// (not an absolute URL, `//`-scheme, `mailto:`, or bare `#fragment`):
///
/// * Resolve it against `page_dir` to a canonical site path.
/// * If that path is a chapter the book emits (`<dir>/index.{md,html}` with
///   `dir` in `chapters`), keep the link relative and normalize `.html` → `.md`
///   so it resolves inside the book.
/// * Otherwise the target is a DROPPED surface (SPARQL playground, prompt card,
///   diagram SVG, …): rewrite it to its absolute [`PUBLISHED_SITE_BASE`] URL and
///   record the canonical target (with any query/fragment) in
///   [`LinkRewrite::external`]. With `create-missing = false` this is what keeps
///   `mdbook build` from failing on a link to a page the book does not emit.
///
/// Links inside fenced code blocks are left verbatim.
///
/// `map` is the single [`SourceToPageMap`] link authority. For a resolved
/// cross-document target that names a real corpus page, the map confirms it is a
/// document chapter the book emits (so the link stays relative and resolves inside
/// the book); a target the map recognizes as a corpus page but that is somehow NOT
/// emitted as a chapter is a hard-fail dangling internal link, never a silent
/// externalization. A link the upstream document rewrite already absolutized to
/// [`PUBLISHED_SITE_BASE`] (an off-corpus document reference) is folded back into
/// the SAME [`LinkRewrite::external`] ledger, so there is one — and only one —
/// notion of "what the book points off-site to".
pub fn rewrite_book_links(
    body: &str,
    page_dir: &str,
    chapters: &BTreeSet<String>,
    map: &SourceToPageMap,
) -> LinkRewrite {
    let mut out = String::with_capacity(body.len());
    let mut external: BTreeSet<String> = BTreeSet::new();
    let mut in_fence = false;

    for line in body.split_inclusive('\n') {
        // Toggle fenced-code state on ``` / ~~~ fences; emit fenced lines verbatim.
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            out.push_str(line);
            continue;
        }
        if in_fence {
            out.push_str(line);
            continue;
        }
        rewrite_line(line, page_dir, chapters, map, &mut out, &mut external);
    }

    LinkRewrite {
        body: out,
        external,
    }
}

/// Rewrite the link targets on a single (non-fenced) line.
fn rewrite_line(
    line: &str,
    page_dir: &str,
    chapters: &BTreeSet<String>,
    map: &SourceToPageMap,
    out: &mut String,
    external: &mut BTreeSet<String>,
) {
    let mut cursor = 0usize;
    while let Some(rel_open) = line[cursor..].find("](") {
        let open = cursor + rel_open + 2; // index just past "]("
        // Targets in this codebase never contain ')' (queries are percent-encoded).
        let Some(rel_close) = line[open..].find(')') else {
            break;
        };
        let close = open + rel_close;
        let target = &line[open..close];
        out.push_str(&line[cursor..open]);
        out.push_str(&rewrite_target(target, page_dir, chapters, map, external));
        // Keep the closing ')' with the following span.
        cursor = close;
    }
    out.push_str(&line[cursor..]);
}

/// Rewrite a single link target (the text between `](` and `)`).
fn rewrite_target(
    target: &str,
    page_dir: &str,
    chapters: &BTreeSet<String>,
    map: &SourceToPageMap,
    external: &mut BTreeSet<String>,
) -> String {
    // A link the upstream document rewrite already absolutized to the published
    // site (an off-corpus cross-document reference) is recorded in the ONE external
    // ledger — no second, parallel notion of "what the book drops" — then left as
    // the absolute URL it already is (it can never dangle a chapter).
    if let Some(rest) = target.strip_prefix(PUBLISHED_SITE_BASE) {
        external.insert(rest.to_string());
        return target.to_string();
    }

    // Other absolute / scheme-qualified / anchor-only / protocol-relative: leave as is.
    if target.is_empty()
        || target.starts_with('#')
        || target.starts_with('/')
        || target.starts_with("mailto:")
        || target.contains("://")
        || target.starts_with("//")
    {
        return target.to_string();
    }

    // Split off the query/fragment suffix; resolve only the path part.
    let split = target.find(['?', '#']).unwrap_or(target.len());
    let (path_part, suffix) = target.split_at(split);

    let Some(canonical) = resolve_site_path(page_dir, path_part) else {
        // Escapes above the site root — not a site path we own; leave untouched.
        return target.to_string();
    };

    if let Some(dir) = chapter_dir_of(&canonical) {
        if chapters.contains(&dir) {
            // A real chapter: keep the link relative, normalize the extension.
            let md = if let Some(stem) = path_part.strip_suffix(".html") {
                format!("{stem}.md")
            } else {
                path_part.to_string()
            };
            return format!("{md}{suffix}");
        }
        // The single link authority: if the map recognizes this canonical path as a
        // real corpus document page, it MUST have been emitted as a chapter above.
        // A resolved cross-document link that lands on a known page which the book
        // did not emit is a dangling internal link — a hard failure, never a quiet
        // externalization that would hide a broken cross-reference.
        if map.is_known_page(&format!("{dir}/")) {
            panic!(
                "book chapter link {target:?} on page {page_dir:?} resolves to the known corpus \
                 document page {dir:?}/ but no chapter was emitted for it — a dangling internal \
                 cross-document link"
            );
        }
    }

    // A dropped surface: externalize to the published site and record it.
    external.insert(format!("{canonical}{suffix}"));
    format!("{PUBLISHED_SITE_BASE}{canonical}{suffix}")
}

/// Resolve a page-relative path against a page directory into a canonical
/// site-relative path (no `./` or `../`). Returns `None` if the path escapes
/// above the site root.
fn resolve_site_path(page_dir: &str, path: &str) -> Option<String> {
    let mut parts: Vec<&str> = if page_dir.is_empty() {
        Vec::new()
    } else {
        page_dir.split('/').collect()
    };
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// If a canonical site path is a page index (`<dir>/index.md` / `index.html` /
/// bare `index.md`), return its page directory (`""` for the root landing page).
fn chapter_dir_of(canonical: &str) -> Option<String> {
    for leaf in ["index.md", "index.html"] {
        if canonical == leaf {
            return Some(String::new());
        }
        if let Some(dir) = canonical.strip_suffix(&format!("/{leaf}")) {
            return Some(dir.to_string());
        }
    }
    None
}
