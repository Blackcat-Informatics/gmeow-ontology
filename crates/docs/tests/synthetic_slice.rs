// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Executable end-to-end demonstration of the first-class Markdown document
//! pipeline over ONE hand-built "rich" synthetic slice.
//!
//! The slice carries a top-level `docs.md` guide (H1, H2, a table, a blockquote, a
//! fenced code block, and a relative cross-document link INTO a child design doc)
//! and a `design/ARCHITECTURE.md` child (H1 "Architecture", an "## Overview" H2, a
//! table, fenced code, and a relative back-link to the guide). The synthetic slice
//! is materialized as a minimal on-disk slice directory and discovered through the
//! REAL loader ([`DocsModel::from_slice_dir`] → `SliceCatalog::discover` →
//! `DocSlice::from_record` → `DocMarkdownDocument::collect`), so every assertion
//! pins a genuine production-surface fact, not a mock.
//!
//! Each `#[test]` pins one falsifiable behaviour of the full pipeline: the model
//! ordering/titles, the inline slice-page graft, the child page, cross-document
//! link rewriting (site + mdbook), anchor invariance, mdbook fidelity, search + LLM
//! coverage, cache-relevant digests, and the hard-fail guards (invalid UTF-8, page
//! collision, dangling link, H6 overflow, page-scoped duplicate headings).

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

use gmeow_docs::ExecutableDocsData;
use gmeow_docs::mdbook::render_book;
use gmeow_docs::model::{DocMarkdownDocument, DocSlice, DocsError, DocsModel};
use gmeow_docs::render::{
    Page, Site, llms_full_txt, llms_txt, render_site, search_index_json, to_markdown,
};
use gmeow_docs::source_map::{DocLinkResolution, LinkResolution, SourceToPageMap};

// ── The synthetic slice content ────────────────────────────────────────────────

/// The synthetic slice IRI; its local name `synth-rich` slugifies to the
/// `slices/synth-rich/…` page space.
const SLICE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/synth-rich";
const SLICE_SLUG: &str = "synth-rich";

/// The generated child page (trailing slash) for `design/ARCHITECTURE.md` — the
/// production scheme keeps the source stem case (`ARCHITECTURE`, not a lowercased
/// fold), so the page dir is derived exactly from the normalized source path.
const CHILD_PAGE: &str = "slices/synth-rich/documents/design/ARCHITECTURE/";

const MANIFEST_TTL: &str = r#"@prefix gmeow:   <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:    <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:    <http://www.w3.org/2004/02/skos/core#> .
@prefix dcterms: <http://purl.org/dc/terms/> .

<https://blackcatinformatics.ca/gmeow/slices/synth-rich>
    a gmeow:Slice ;
    rdfs:label "synth-rich"@x-gmeow-english ;
    skos:definition "A synthetic rich slice exercising the first-class Markdown document pipeline end to end."@x-gmeow-english ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/synth-rich> ;
    dcterms:title "Synthetic Rich Slice"@x-gmeow-english ;
    dcterms:creator "Blackcat Informatics® Inc." ;
    gmeow:sliceTier gmeow:tierCore .
"#;

/// A minimal `module.ttl` carrying one class and one property. The site's fixed
/// nav always links the Classes and Properties category pages, which are only
/// emitted when the model owns a term of that category — so a term-free synthetic
/// slice would dangle those two nav links. One class + one property makes both
/// category pages exist, so the whole synthetic site lints clean exactly like the
/// real `make check` docs gate.
const MODULE_TTL: &str = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .

gmeow:SynthWidget
    a owl:Class ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/synth-rich> ;
    rdfs:label "Synth Widget"@x-gmeow-english ;
    skos:definition "A synthetic class so the Classes category page exists."@x-gmeow-english .

gmeow:synthLink
    a owl:ObjectProperty ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/synth-rich> ;
    rdfs:label "synth link"@x-gmeow-english ;
    skos:definition "A synthetic property so the Properties category page exists."@x-gmeow-english .
"#;

/// The guide (`docs.md`): H1, thesis sentence, an H2, a cross-document link into
/// the design child WITH an anchor, a blockquote, a GFM table, and a fenced code
/// block.
const GUIDE_MD: &str = r#"# Synthetic Guide

The synthetic guide thesis sentence introduces the rich slice for readers.

## Usage

See [see arch](design/ARCHITECTURE.md#overview) for the architecture overview.

> Guidance lives in this blockquote so the graft must preserve it verbatim.

| Field | Value |
| --- | --- |
| Owner | synth team |
| Status | built |

```rust
fn synth_guide_snippet() -> u32 { 7 }
```
"#;

/// The design child (`design/ARCHITECTURE.md`): H1 "Architecture", an "## Overview"
/// H2 (the cross-doc anchor target), a relative back-link to the guide, a table,
/// and fenced code.
const DESIGN_MD: &str = r#"# Architecture

Prose about the synthetic architecture and its motivating constraints.

## Overview

Return to [the guide](../docs.md) for the surrounding context.

| Component | Role |
| --- | --- |
| core | anchors the whole model |

```turtle
ex:a a gmeow:Foo .
```
"#;

// ── Fixture materialization ──────────────────────────────────────────────────

/// A fresh, unique temp directory for one test's synthetic slice.
fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gmeow-synth-slice-{}-{}-{}",
        tag,
        std::process::id(),
        // A monotonic-ish disambiguator so parallel tests never share a dir.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::remove_dir_all(&dir).ok();
    dir
}

/// Write a synthetic slice directory with the given `docs.md` and optional design
/// child, and return its path.
fn write_slice(dir: &Path, guide: &str, design: Option<&[u8]>) {
    std::fs::create_dir_all(dir).expect("mkdir slice");
    std::fs::write(dir.join("manifest.ttl"), MANIFEST_TTL).expect("write manifest");
    std::fs::write(dir.join("module.ttl"), MODULE_TTL).expect("write module");
    std::fs::write(dir.join("docs.md"), guide).expect("write docs.md");
    if let Some(bytes) = design {
        let design_dir = dir.join("design");
        std::fs::create_dir_all(&design_dir).expect("mkdir design");
        std::fs::write(design_dir.join("ARCHITECTURE.md"), bytes).expect("write design");
    }
}

/// Build the canonical rich model (guide + design child) from a freshly
/// materialized synthetic slice, then delete the temp dir (the loader reads all
/// bytes into the in-memory model, so nothing on disk is consulted afterwards).
fn rich_model(tag: &str) -> DocsModel {
    let dir = fresh_dir(tag);
    write_slice(&dir, GUIDE_MD, Some(DESIGN_MD.as_bytes()));
    let model = DocsModel::from_slice_dir(&dir).expect("discover synthetic slice");
    std::fs::remove_dir_all(&dir).ok();
    model
}

/// The rendered site page body (`.md`) at a site-relative page dir.
fn site_md<'a>(site: &'a Site, page_dir: &str) -> &'a str {
    let key = format!("{page_dir}index.md");
    std::str::from_utf8(
        site.files
            .get(&key)
            .unwrap_or_else(|| panic!("site missing {key}")),
    )
    .expect("utf8 site page")
}

/// Capture the message of a panic raised by `f` (with the default noisy hook
/// suppressed for the duration), asserting a panic actually occurred.
fn capture_panic(f: impl FnOnce()) -> String {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(prev);
    let payload = result.expect_err("expected a panic but the call returned");
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        panic!("panic payload was not a string");
    }
}

// ── 1. Model + ordering ─────────────────────────────────────────────────────────

/// The discovered slice carries BOTH markdown sources, sorted by normalized logical
/// path, with correct titles (H1 for both) and strict-UTF-8 text — the contract of
/// `DocMarkdownDocument::collect`.
#[test]
fn model_carries_both_documents_sorted_with_titles() {
    let model = rich_model("model");
    assert_eq!(model.slices.len(), 1, "one synthetic slice");
    let docs = &model.slices[0].documents;
    assert_eq!(docs.len(), 2, "the guide + the design child");

    // Sorted by normalized logical path: `design/ARCHITECTURE.md` < `docs.md`.
    assert_eq!(docs[0].source_path, "design/ARCHITECTURE.md");
    assert_eq!(docs[1].source_path, "docs.md");

    // Titles: the first H1 of each source.
    assert_eq!(docs[0].title, "Architecture");
    assert_eq!(docs[1].title, "Synthetic Guide");

    // Strict-UTF-8 text carried verbatim (the full source, not a summary).
    assert!(docs[0].source_text.contains("motivating constraints"));
    assert!(docs[1].source_text.contains("thesis sentence"));

    // The slug is self-described on each document (the `slices/{slug}/…` space).
    assert_eq!(docs[0].slice_slug, SLICE_SLUG);
    // Content-address is populated (non-empty, distinct per source).
    assert!(!docs[0].raw_digest.is_empty());
    assert_ne!(docs[0].raw_digest, docs[1].raw_digest);
}

// ── 2. Inline slice-page graft ──────────────────────────────────────────────────

/// The slice page grafts the guide's prose with headings demoted ONE level (its H1
/// becomes an H2 under the generated slice H1), preserving the table, blockquote,
/// and fenced code verbatim.
#[test]
fn slice_page_grafts_guide_with_demoted_headings() {
    let model = rich_model("graft");
    let md = to_markdown(&model, &Page::Slice(SLICE_SLUG.to_string()));

    // Generated slice H1 is the manifest title; the guide's own H1/H2 are demoted.
    assert!(md.contains("# Synthetic Rich Slice"), "generated slice H1");
    assert!(md.contains("## Synthetic Guide"), "guide H1 demoted to H2");
    assert!(md.contains("### Usage"), "guide H2 demoted to H3");
    // The demotion is genuine: no bare `# Synthetic Guide` line survives.
    assert!(
        !md.lines().any(|l| l.trim() == "# Synthetic Guide"),
        "guide H1 must not survive undemoted"
    );

    // Table, blockquote, and fenced code are grafted verbatim.
    assert!(md.contains("| Owner | synth team |"), "table row grafted");
    assert!(
        md.contains("> Guidance lives in this blockquote"),
        "blockquote grafted"
    );
    assert!(
        md.contains("fn synth_guide_snippet() -> u32 { 7 }"),
        "fenced code grafted verbatim"
    );

    // The HTML projection carries the same code text (the shell wraps the body).
    let site = render_site(&model);
    let html = std::str::from_utf8(
        site.files
            .get("slices/synth-rich/index.html")
            .expect("slice html page"),
    )
    .expect("utf8");
    assert!(html.contains("synth_guide_snippet"), "graft reaches HTML");
}

// ── 3. Child page ────────────────────────────────────────────────────────────────

/// The design child renders to a COMPLETE child page (both `.md` and `.html`) at the
/// scheme-derived path, carrying its prose, table, and code — its authored H1 kept
/// (NOT demoted, since it is the child page's own H1).
#[test]
fn design_child_renders_to_its_own_page() {
    let model = rich_model("child");
    let site = render_site(&model);

    for ext in ["md", "html"] {
        assert!(
            site.files.contains_key(&format!("{CHILD_PAGE}index.{ext}")),
            "child {ext} page emitted at {CHILD_PAGE}"
        );
    }

    let md = site_md(&site, CHILD_PAGE);
    assert!(md.contains("# Architecture"), "child H1 kept");
    assert!(md.contains("## Overview"), "child H2 kept");
    assert!(
        md.contains("| core | anchors the whole model |"),
        "child table"
    );
    assert!(md.contains("ex:a a gmeow:Foo ."), "child fenced code");
}

// ── 4. Cross-document link rewriting (site + mdbook) ────────────────────────────

/// The guide's `design/ARCHITECTURE.md#overview` link resolves to the child page +
/// `#overview` anchor in BOTH the site render and the mdbook render; the design
/// doc's back-link resolves to the guide/slice page; and the rendered synthetic
/// output carries NO dangling internal link.
#[test]
fn cross_document_links_rewrite_in_site_and_mdbook() {
    let model = rich_model("xlink");

    // The single authority classifies the guide → child anchored link as Corpus.
    let map = SourceToPageMap::build(&model).expect("build map");
    match map.classify_doc_link(SLICE_IRI, "docs.md", "design/ARCHITECTURE.md#overview") {
        DocLinkResolution::Corpus(loc) => {
            assert_eq!(loc.page, CHILD_PAGE);
            assert_eq!(loc.anchor.as_deref(), Some("overview"));
        }
        other => panic!("guide→child link must be corpus, got {other:?}"),
    }
    // The design doc's back-link resolves to the slice page (the guide).
    match map.classify_doc_link(SLICE_IRI, "design/ARCHITECTURE.md", "../docs.md") {
        DocLinkResolution::Corpus(loc) => {
            assert_eq!(loc.page, "slices/synth-rich/");
            assert_eq!(loc.anchor, None);
        }
        other => panic!("back-link must be corpus, got {other:?}"),
    }

    // Site render: the grafted guide links to the child page + anchor, relatively.
    let site = render_site(&model);
    let slice_md = site_md(&site, "slices/synth-rich/");
    assert!(
        slice_md.contains("documents/design/ARCHITECTURE/index.md#overview"),
        "site graft rewrites the cross-doc link to the child page + anchor"
    );
    // The raw authored `.md` target never survives unrewritten.
    assert!(
        !slice_md.contains("design/ARCHITECTURE.md#overview"),
        "authored source link must be rewritten, not passed through"
    );

    // mdbook render: the slice chapter body links to the emitted child chapter
    // (relative, `.md`, with the same anchor).
    let book = render_book(&model, &ExecutableDocsData::default());
    let slice_chapter = std::str::from_utf8(
        book.files
            .get("src/slices/synth-rich/index.md")
            .expect("slice chapter"),
    )
    .expect("utf8");
    assert!(
        slice_chapter.contains("documents/design/ARCHITECTURE/index.md#overview"),
        "mdbook slice chapter links to the child chapter + anchor"
    );

    // No dangling internal link anywhere in the rendered synthetic output: the docs
    // lint (the same gate `make check` enforces) is error-free.
    let report = gmeow_docs::lint(&model, &site);
    assert_eq!(
        report.error_count(),
        0,
        "synthetic site must carry no dangling links / broken anchors; got {:?}",
        report.legacy_errors()
    );
}

// ── 5. Anchor invariance (E11) ──────────────────────────────────────────────────

/// `resolve_link` / `classify_doc_link` produce the SAME target fragment for the
/// cross-doc anchor, and that fragment slug agrees across the site and mdbook
/// renders (the single authority is what both consume, so they cannot disagree).
#[test]
fn cross_doc_anchor_fragment_is_invariant_across_targets() {
    let model = rich_model("anchor");
    let map = SourceToPageMap::build(&model).expect("build map");

    // The map's own two entry points agree on the fragment.
    let resolved = map.resolve_link(SLICE_IRI, "docs.md", "design/ARCHITECTURE.md#overview");
    let anchor_from_resolve = match resolved {
        LinkResolution::Resolved(loc) => loc.anchor.expect("anchor present"),
        other => panic!("expected resolved, got {other:?}"),
    };
    let anchor_from_classify =
        match map.classify_doc_link(SLICE_IRI, "docs.md", "design/ARCHITECTURE.md#overview") {
            DocLinkResolution::Corpus(loc) => loc.anchor.expect("anchor present"),
            other => panic!("expected corpus, got {other:?}"),
        };
    assert_eq!(anchor_from_resolve, "overview");
    assert_eq!(anchor_from_resolve, anchor_from_classify);

    // The SAME fragment slug is what both the site and the mdbook cross-links carry.
    let fragment = format!("index.md#{anchor_from_resolve}");
    let site = render_site(&model);
    let book = render_book(&model, &ExecutableDocsData::default());
    let site_slice = site_md(&site, "slices/synth-rich/");
    let book_slice = std::str::from_utf8(
        book.files
            .get("src/slices/synth-rich/index.md")
            .expect("slice chapter"),
    )
    .expect("utf8");
    assert!(
        site_slice.contains(&fragment),
        "site cross-link carries the map fragment `{fragment}`"
    );
    assert!(
        book_slice.contains(&fragment),
        "mdbook cross-link carries the SAME fragment `{fragment}`"
    );
}

// ── 6. mdbook fidelity ──────────────────────────────────────────────────────────

/// `render_book` emits the design child as its own chapter and nests it in
/// `SUMMARY.md` under the slice (one level deeper than the slice entry).
#[test]
fn mdbook_emits_child_chapter_nested_under_slice() {
    let model = rich_model("mdbook");
    let book = render_book(&model, &ExecutableDocsData::default());

    // The child chapter file exists and carries the design prose/table/code.
    let child_chapter = std::str::from_utf8(
        book.files
            .get("src/slices/synth-rich/documents/design/ARCHITECTURE/index.md")
            .expect("child chapter emitted"),
    )
    .expect("utf8");
    assert!(child_chapter.contains("# Architecture"));
    assert!(child_chapter.contains("| core | anchors the whole model |"));
    assert!(child_chapter.contains("ex:a a gmeow:Foo ."));

    // SUMMARY.md nests the child under the slice: the child entry appears AFTER the
    // slice entry and is indented deeper.
    let summary = std::str::from_utf8(
        book.files
            .get("src/SUMMARY.md")
            .expect("SUMMARY.md present"),
    )
    .expect("utf8");
    let slice_line = summary
        .lines()
        .find(|l| l.contains("slices/synth-rich/index.md"))
        .expect("slice summary entry");
    let child_line = summary
        .lines()
        .find(|l| l.contains("slices/synth-rich/documents/design/ARCHITECTURE/index.md"))
        .expect("child summary entry");
    let indent = |l: &str| l.len() - l.trim_start().len();
    assert!(
        indent(child_line) > indent(slice_line),
        "child chapter must nest DEEPER than its slice in SUMMARY.md \
         (slice indent {}, child indent {})",
        indent(slice_line),
        indent(child_line)
    );
}

// ── 8. Search + LLM coverage ────────────────────────────────────────────────────

/// `search_index_json` carries a record for the design child (title + full prose
/// indexed); `llms_txt` lists its page; `llms_full_txt` inlines its full prose.
#[test]
fn search_and_llms_cover_the_child_document() {
    let model = rich_model("search");

    let search = search_index_json(&model);
    // A `document` record for the design child: title, page url, and prose body.
    assert!(
        search.contains("\"kind\": \"document\""),
        "document search record"
    );
    assert!(
        search.contains("\"label\": \"Architecture\""),
        "child title indexed"
    );
    assert!(
        search.contains("slices/synth-rich/documents/design/ARCHITECTURE/index.html"),
        "child page url indexed"
    );
    assert!(
        search.contains("motivating constraints"),
        "child full prose indexed as the search definition"
    );

    // llms.txt lists the child page under Documents.
    let llms = llms_txt(&model);
    assert!(llms.contains("## Documents"), "llms Documents section");
    assert!(
        llms.contains("slices/synth-rich/documents/design/ARCHITECTURE/index.html"),
        "llms.txt links the child page"
    );

    // llms-full.txt inlines the FULL prose of the child (and the guide).
    let full = llms_full_txt(&model);
    assert!(
        full.contains("motivating constraints"),
        "llms-full inlines the child prose"
    );
    assert!(
        full.contains("thesis sentence"),
        "llms-full inlines the guide prose"
    );
}

// ── 9. Cache invalidation (digest sensitivity) ──────────────────────────────────

/// Editing the design doc's bytes changes its `DocMarkdownDocument.raw_digest` (the
/// content address the docs cache/source inventory keys on), so a changed design doc
/// materially changes the model.
#[test]
fn changed_design_doc_bytes_change_the_model_digest() {
    let dir = fresh_dir("digest");
    write_slice(&dir, GUIDE_MD, Some(DESIGN_MD.as_bytes()));
    let model_a = DocsModel::from_slice_dir(&dir).expect("discover A");

    // Rewrite ONLY the design child with different bytes.
    let mutated = DESIGN_MD.replace("motivating constraints", "revised motivating rationale");
    std::fs::write(dir.join("design").join("ARCHITECTURE.md"), &mutated).expect("rewrite design");
    let model_b = DocsModel::from_slice_dir(&dir).expect("discover B");
    std::fs::remove_dir_all(&dir).ok();

    let design_digest = |m: &DocsModel| -> String {
        m.slices[0]
            .documents
            .iter()
            .find(|d| d.source_path == "design/ARCHITECTURE.md")
            .expect("design doc")
            .raw_digest
            .clone()
    };
    assert_ne!(
        design_digest(&model_a),
        design_digest(&model_b),
        "a changed design-doc byte stream must change its raw_digest"
    );
    // And the model as a whole differs (the digest is load-bearing, not cosmetic).
    assert_ne!(model_a.slices, model_b.slices);
}

// ── 10a. Hard-fail: invalid UTF-8 names the source path ─────────────────────────

/// An invalid-UTF-8 markdown source hard-fails discovery with
/// `DocsError::MarkdownUtf8` naming the offending slice-relative path.
#[test]
fn invalid_utf8_markdown_hard_fails_with_source_path() {
    let dir = fresh_dir("utf8");
    // A lone 0xFF byte is not valid UTF-8.
    let bad: &[u8] = b"# Architecture\n\n\xff\xfe not utf8\n";
    write_slice(&dir, GUIDE_MD, Some(bad));
    let err = DocsModel::from_slice_dir(&dir).expect_err("invalid UTF-8 must hard-fail");
    std::fs::remove_dir_all(&dir).ok();

    match &err {
        DocsError::MarkdownUtf8 { source_path, .. } => {
            assert!(
                source_path.contains("ARCHITECTURE.md"),
                "error names the offending path, got {source_path:?}"
            );
        }
        other => panic!("expected MarkdownUtf8, got {other:?}"),
    }
    // The Display message names the path too.
    assert!(
        err.to_string().contains("ARCHITECTURE.md"),
        "Display message names the source path: {err}"
    );
}

// 10b (two sources normalizing to one logical path → `MarkdownPathCollision`)
// exercises the crate-private `DocMarkdownDocument::collect`, so it lives as a unit
// test in `crates/docs/src/model.rs` (`collect_hard_fails_on_normalized_path_collision`)
// where a hand-built `SliceRecord` with two `./`-vs-bare artifacts can reach it. Two
// distinct real files cannot produce a post-normalization collision, so it is not
// reproducible through the on-disk loader used here.

// ── 10c. Hard-fail: two documents map to one generated page path ────────────────

/// Two documents mapping to the SAME generated page path hard-fail
/// `SourceToPageMap::build` with `MarkdownPageCollision` naming BOTH identities.
#[test]
fn colliding_page_path_hard_fails_naming_both() {
    // Two slices whose IRI local names slugify to the SAME slug (`zoo`), each with a
    // top-level `docs.md` → both map to `slices/zoo/`.
    let iri_a = "https://blackcatinformatics.ca/gmeow/slices/a/zoo";
    let iri_b = "https://blackcatinformatics.ca/gmeow/slices/b/zoo";
    let doc = |iri: &str, body: &str| DocMarkdownDocument {
        slice_iri: iri.to_string(),
        slice_slug: "zoo".to_string(),
        source_path: "docs.md".to_string(),
        title: "Zoo".to_string(),
        source_text: body.to_string(),
        raw_digest: format!("digest-{iri}"),
    };
    let model = DocsModel {
        slices: vec![
            hand_slice(iri_a, vec![doc(iri_a, "# A\n")]),
            hand_slice(iri_b, vec![doc(iri_b, "# B\n")]),
        ],
        ..Default::default()
    };
    let err = SourceToPageMap::build(&model).expect_err("page collision must hard-fail");
    match &err {
        DocsError::MarkdownPageCollision {
            page,
            first,
            second,
        } => {
            assert_eq!(page, "slices/zoo/");
            assert!(
                first.contains(iri_a),
                "first identity names slice A: {first}"
            );
            assert!(
                second.contains(iri_b),
                "second identity names slice B: {second}"
            );
        }
        other => panic!("expected MarkdownPageCollision, got {other:?}"),
    }
    assert!(err.to_string().contains(iri_a) && err.to_string().contains(iri_b));
}

// ── 10d. Hard-fail: a dangling internal link ────────────────────────────────────

/// A guide linking a within-slice markdown that names no document hard-fails
/// rendering, and the panic message names the source path AND the offending link.
#[test]
fn dangling_internal_link_hard_fails_naming_path_and_link() {
    let dir = fresh_dir("dangling");
    let guide = "# Guide\n\nSee [x](design/NOPE.md) which does not exist.\n";
    // No design child on disk → the link dangles within the slice corpus.
    write_slice(&dir, guide, None);
    let model = DocsModel::from_slice_dir(&dir).expect("model builds; links resolve at render");
    std::fs::remove_dir_all(&dir).ok();

    let msg = capture_panic(|| {
        let _ = render_site(&model);
    });
    assert!(
        msg.contains("docs.md"),
        "dangling-link panic names the source path: {msg}"
    );
    assert!(
        msg.contains("design/NOPE.md"),
        "dangling-link panic names the offending link: {msg}"
    );
}

// ── 10e. Hard-fail: H6 overflow in the graft ────────────────────────────────────

/// A `docs.md` whose source carries an H6 heading hard-fails the slice-page graft
/// (H6→H7 is illegal), and the panic message names the source path.
#[test]
fn h6_source_heading_overflows_the_graft_naming_path() {
    let dir = fresh_dir("h6");
    let guide = "# Guide\n\n###### Deep Note\n\nBody.\n";
    write_slice(&dir, guide, None);
    let model = DocsModel::from_slice_dir(&dir).expect("model builds; demotion at render");
    std::fs::remove_dir_all(&dir).ok();

    let msg = capture_panic(|| {
        let _ = to_markdown(&model, &Page::Slice(SLICE_SLUG.to_string()));
    });
    assert!(
        msg.contains("docs.md"),
        "H6-overflow panic names the source path: {msg}"
    );
    assert!(
        msg.contains("H6→H7") || msg.contains("level-6"),
        "H6-overflow panic explains the illegal demotion: {msg}"
    );
}

// ── 10f. Page-scoped duplicate headings across two child docs ────────────────────

/// Two child docs each with `## Overview` get DISTINCT page-scoped anchors: both
/// resolve to their OWN page's `overview` (page-scoped, no cross-document collision).
#[test]
fn duplicate_headings_across_child_docs_are_page_scoped() {
    let iri = SLICE_IRI;
    let child = |path: &str, body: &str| DocMarkdownDocument {
        slice_iri: iri.to_string(),
        slice_slug: SLICE_SLUG.to_string(),
        source_path: path.to_string(),
        title: path.to_string(),
        source_text: body.to_string(),
        raw_digest: format!("digest-{path}"),
    };
    let model = DocsModel {
        slices: vec![hand_slice(
            iri,
            vec![
                child("a.md", "# A\n\n## Overview\n"),
                child("b.md", "# B\n\n## Overview\n"),
            ],
        )],
        ..Default::default()
    };
    let map = SourceToPageMap::build(&model).expect("build map");

    let resolve = |path: &str| match map.resolve(iri, path, Some("overview")) {
        LinkResolution::Resolved(loc) => (loc.page, loc.anchor),
        other => panic!("expected resolved for {path}, got {other:?}"),
    };
    let (page_a, anchor_a) = resolve("a.md");
    let (page_b, anchor_b) = resolve("b.md");
    // Same page-scoped slug, DISTINCT pages — no collision.
    assert_eq!(anchor_a.as_deref(), Some("overview"));
    assert_eq!(anchor_b.as_deref(), Some("overview"));
    assert_ne!(page_a, page_b, "each `## Overview` scopes to its own page");
    assert_eq!(page_a, "slices/synth-rich/documents/a/");
    assert_eq!(page_b, "slices/synth-rich/documents/b/");
}

// ── 11. Docs link lint over the clean synthetic output ──────────────────────────

/// The docs link lint (the `make check` gate) is error-free over the clean
/// synthetic site: no dangling links, no broken anchors.
#[test]
fn docs_link_lint_is_clean_on_synthetic_site() {
    let model = rich_model("lint");
    let site = render_site(&model);
    let report = gmeow_docs::lint(&model, &site);
    assert_eq!(
        report.error_count(),
        0,
        "clean synthetic site must lint error-free; got {:?}",
        report.legacy_errors()
    );
    // And it genuinely rendered the child page the lint walked (guard against a
    // vacuous pass over an empty site).
    assert!(
        site.files.contains_key(&format!("{CHILD_PAGE}index.html")),
        "the lint must have a non-trivial child page to walk"
    );
}

// ── Shared helper: a hand-built public DocSlice ─────────────────────────────────

/// A bare public [`DocSlice`] carrying only an IRI and a document set — the same
/// shape `docs-print`'s fixture builds, used by the tests that exercise the page map
/// / collision guards over a hand-built model.
fn hand_slice(iri: &str, documents: Vec<DocMarkdownDocument>) -> DocSlice {
    DocSlice {
        iri: iri.to_string(),
        label: None,
        title: None,
        tier: None,
        identifier: None,
        creators: Vec::new(),
        consumers: Vec::new(),
        profiles: Vec::new(),
        depends_on: Vec::new(),
        artifacts: Vec::new(),
        documents,
        has_thesis_sentence: false,
        realized_state_complete: false,
    }
}
