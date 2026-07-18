// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Golden + invariant tests for the mdbook source-tree renderer.
//!
//! The goldens lock the book scaffolding (`SUMMARY.md`, `book.toml`) and one
//! term chapter whose body carries both intra-book cross-links and a link to a
//! DROPPED site-only surface (the SPARQL playground + prompt card), pinning the
//! shared link-rewrite helper's A5 fidelity. Two property tests mechanize the
//! razor: A4 asserts every chapter body is EXACTLY the single Markdown authority
//! transformed by the link-rewrite helper, and A11 asserts a zero-term slice
//! still renders a valid chapter.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_docs::mdbook::{render_book, rewrite_book_links};
use gmeow_docs::render::{Page, book_pages, slice_slug, term_slug, to_markdown_exec};
use gmeow_docs::{DocSlice, ExecutableDocsData};

mod common;

/// An `exec` that supplies a (non-empty) playground asset, so the term/slice
/// export sections — which link the dropped SPARQL playground + prompt card —
/// are rendered. The bytes are opaque to the renderer (it only checks
/// non-emptiness via `has_playground`), so a fixed sentinel is sufficient.
fn exec_with_playground() -> ExecutableDocsData {
    ExecutableDocsData {
        playground_trig: b"@prefix ex: <http://example/> .".to_vec(),
        ..Default::default()
    }
}

/// The `src/`-relative chapter path of a page (mirrors the private helper).
fn chapter_src_path(dir: &str) -> String {
    if dir.is_empty() {
        "src/index.md".to_string()
    } else {
        format!("src/{dir}/index.md")
    }
}

/// The first term (by model order) with at least one parent that resolves to
/// another term in the model — so its chapter body carries an intra-book
/// relative cross-link, deterministically.
fn term_with_crosslinks(model: &gmeow_docs::DocsModel) -> String {
    model
        .terms
        .iter()
        .find(|t| {
            t.parents
                .iter()
                .any(|p| model.terms.iter().any(|x| &x.iri == p))
        })
        .map(term_slug)
        .expect("a term with a resolvable parent cross-link exists")
}

#[test]
fn book_summary_golden() {
    let site = common::cached_book();
    let summary = String::from_utf8(
        site.files
            .get("src/SUMMARY.md")
            .expect("src/SUMMARY.md is emitted")
            .clone(),
    )
    .expect("src/SUMMARY.md is UTF-8");
    // The full tree is large; lock the header skeleton + the first chapters so
    // structure/nesting is pinned without a multi-thousand-line golden.
    let head: String = summary.lines().take(40).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn book_summary_lists_each_chapter_at_most_once() {
    // mdbook rejects a SUMMARY.md that lists the same chapter file twice. Distinct
    // term IRIs can slugify to one chapter path (the site archive collapses them),
    // so the table of contents MUST dedup by chapter target. The real model carries
    // such slug collisions, so this asserts `mdbook build` cannot choke on the
    // committed book without needing the mdbook binary in the gate.
    let model = common::cached_model();
    let site = common::cached_book();

    // The set of chapter dirs with more than one page in the underlying page set —
    // i.e. the real slug collisions. Only these need the stronger label == shipped
    // -body-title check below (a legitimately unique page's body may open with
    // curated prose rather than a bare `# ` heading, e.g. `four-boxes`, so that
    // check would be a false positive if applied site-wide).
    let pages = book_pages(&model);
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();
    for page in &pages {
        *dir_counts.entry(page.dir()).or_default() += 1;
    }
    let colliding_dirs: BTreeSet<String> = dir_counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(dir, _)| dir)
        .collect();
    assert!(
        !colliding_dirs.is_empty(),
        "the real model must carry at least one slug collision for this test to exercise the \
         collision-consistency invariant — if this fires, the fixture no longer collides and the \
         test below is vacuous"
    );

    let summary = String::from_utf8(
        site.files
            .get("src/SUMMARY.md")
            .expect("src/SUMMARY.md is emitted")
            .clone(),
    )
    .expect("src/SUMMARY.md is UTF-8");
    // Every list entry is `[text](target)`; collect the link targets (+ labels) and
    // assert each target appears at most once.
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for line in summary.lines() {
        let Some(open) = line.find('[') else {
            continue;
        };
        let Some(label_close) = line[open..].find(']') else {
            continue;
        };
        let label = &line[open + 1..open + label_close];
        let rest = &line[open + label_close..];
        let Some(target_open) = rest.find('(') else {
            continue;
        };
        let rest = &rest[target_open + 1..];
        let Some(close) = rest.find(')') else {
            continue;
        };
        let target = &rest[..close];
        assert!(
            seen.insert(target.to_string(), label.to_string()).is_none(),
            "SUMMARY.md lists the chapter target {target:?} more than once — mdbook build would fail"
        );
    }
    // Every chapter target the summary names must exist as an emitted src/ file.
    for target in seen.keys() {
        let key = format!("src/{target}");
        assert!(
            site.files.contains_key(&key),
            "SUMMARY.md names {target:?} but no {key} chapter was emitted"
        );
    }

    // The collision-consistency invariant this test guards: for every ACTUAL slug
    // collision, the ToC link LABEL must name the exact same term whose body is
    // shipped at that path. `render_book` and `summary_md` resolve a collision
    // through the SAME shared authority (`dir_winners` in `mdbook.rs`); if either
    // producer ever reverted to an independently-dedup'd pick of its own, a
    // collision could make the ToC label and the shipped chapter body disagree —
    // this loop catches that against the real model's ~160 live collisions.
    let mut checked_collisions = 0usize;
    for (target, label) in &seen {
        let dir = target.trim_end_matches("/index.md");
        if !colliding_dirs.contains(dir) {
            continue;
        }
        checked_collisions += 1;
        let key = format!("src/{target}");
        let body = site.files.get(&key).expect("checked to exist above");
        let body = std::str::from_utf8(body).expect("chapter is UTF-8");
        let title = body
            .lines()
            .next()
            .and_then(|h1| h1.strip_prefix("# "))
            .unwrap_or_else(|| panic!("colliding chapter {key} does not open with an H1"));
        // The label is the page's `title()` (a CURIE for terms, e.g. `gmeow:Foo`);
        // the body's H1 is the term's label (falling back to its CURIE). Compare
        // case/space-insensitively on the local-name so a term CURIE label
        // (`gmeow:AcceptanceStatus`) is recognized as naming the same term as its
        // rendered title ("Acceptance Status").
        let normalize = |s: &str| {
            s.rsplit(':')
                .next()
                .unwrap_or(s)
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        };
        let (norm_label, norm_title) = (normalize(label), normalize(title));
        assert!(
            norm_label == norm_title
                || norm_label.contains(&norm_title)
                || norm_title.contains(&norm_label),
            "SUMMARY.md labels colliding chapter {target:?} as {label:?} but the shipped chapter \
             body's title is {title:?} — the ToC and the shipped page name different terms"
        );
    }
    assert_eq!(
        checked_collisions,
        colliding_dirs.len(),
        "every colliding dir must be named exactly once in SUMMARY.md"
    );
}

#[test]
fn book_toml_golden() {
    let site = common::cached_book();
    let toml = String::from_utf8(
        site.files
            .get("book.toml")
            .expect("book.toml is emitted")
            .clone(),
    )
    .expect("book.toml is UTF-8");
    insta::assert_snapshot!(toml);
}

#[test]
fn book_term_chapter_with_dropped_link_golden() {
    // A term chapter that HAS cross-links AND (via the playground exec) links the
    // dropped SPARQL playground + prompt card — pins the A5 rewrite fidelity.
    let model = common::cached_model();
    let exec = exec_with_playground();
    let site = render_book(&model, &exec);
    let slug = term_with_crosslinks(&model);
    let body = String::from_utf8(
        site.files
            .get(&chapter_src_path(&Page::Term(slug.clone()).dir()))
            .expect("the term chapter is emitted")
            .clone(),
    )
    .expect("chapter is UTF-8");

    // Hard invariants (independent of the golden text): the dropped playground
    // link is externalized to the published site, never left as a relative
    // `sparql/index.html` (which would fail `mdbook build`).
    assert!(
        body.contains("https://blackcatinformatics.ca/gmeow/docs/sparql/index.html"),
        "the dropped SPARQL playground link must be externalized"
    );
    assert!(
        !body.contains("](../../sparql/index.html"),
        "no relative link to the dropped playground chapter may survive"
    );

    // Lock the header slice (title + fields) so the chapter body is pinned.
    let head: String = body.lines().take(20).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn book_bodies_are_rewrite_of_single_authority() {
    // A4 — render-once coherence: the ONLY difference between a book chapter and
    // the site body is the deterministic link rewrite. This mechanizes the razor.
    let model = common::cached_model();
    let exec = exec_with_playground();
    let site = render_book(&model, &exec);
    let pages = book_pages(&model);
    let chapters: BTreeSet<String> = pages.iter().map(Page::dir).collect();

    for page in &pages {
        let expected = rewrite_book_links(
            &to_markdown_exec(&model, page, &exec),
            &page.dir(),
            &chapters,
        )
        .body;
        let actual = String::from_utf8(
            site.files
                .get(&chapter_src_path(&page.dir()))
                .unwrap_or_else(|| panic!("chapter for {page:?} is emitted"))
                .clone(),
        )
        .expect("chapter is UTF-8");
        assert_eq!(
            actual, expected,
            "book chapter for {page:?} must be exactly rewrite(to_markdown_exec(...))"
        );
    }
}

#[test]
fn book_no_relative_link_to_dropped_page() {
    // With `create-missing = false`, a relative link to a non-chapter would fail
    // `mdbook build`. Every `[text](target)` link to a dropped surface (the SPARQL
    // playground or a `card.md`) must be ABSOLUTE (externalized), never relative.
    let model = common::cached_model();
    let site = render_book(&model, &exec_with_playground());
    for (path, bytes) in &site.files {
        if !path.ends_with("index.md") || path == "src/SUMMARY.md" {
            continue;
        }
        let body = std::str::from_utf8(bytes).expect("chapter is UTF-8");
        for target in link_targets(body) {
            let dropped = target.contains("sparql/index.html")
                || target.trim_end_matches(')').ends_with("card.md");
            if dropped {
                assert!(
                    target.starts_with("http://") || target.starts_with("https://"),
                    "chapter {path} links a dropped surface relatively: {target}"
                );
            }
        }
    }
}

/// Every Markdown link target `[..](target)` on a body (naive; sufficient for the
/// renderer's percent-encoded, paren-free targets).
fn link_targets(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find("](") {
        let after = &rest[i + 2..];
        if let Some(j) = after.find(')') {
            out.push(after[..j].to_string());
            rest = &after[j + 1..];
        } else {
            break;
        }
    }
    out
}

#[test]
fn book_zero_term_slice_renders_valid_chapter() {
    // A11 — degenerate edge: a slice with zero terms must still yield a valid,
    // non-empty chapter (no panic, real H1).
    let mut model = common::cached_model();
    let empty = DocSlice {
        iri: "https://blackcatinformatics.ca/gmeow/EmptyDemoSlice".to_string(),
        label: Some("Empty Demo Slice".to_string()),
        title: None,
        tier: None,
        identifier: None,
        creators: Vec::new(),
        consumers: Vec::new(),
        profiles: Vec::new(),
        depends_on: Vec::new(),
        artifacts: Vec::new(),
        has_thesis_sentence: false,
        realized_state_complete: false,
    };
    let slug = slice_slug(&empty);
    model.slices.push(empty);
    let site = render_book(&model, &ExecutableDocsData::default());
    let path = chapter_src_path(&Page::Slice(slug).dir());
    let body = std::str::from_utf8(
        site.files
            .get(&path)
            .expect("the zero-term slice chapter is emitted"),
    )
    .expect("chapter is UTF-8");
    assert!(
        body.starts_with("# "),
        "the zero-term slice chapter must open with an H1"
    );
}

#[test]
fn cached_book_matches_live_render() {
    // The cached default book must be byte-identical to a fresh render (no blind
    // re-bless). Under the primer (the gate/production config) `cached_book()` reads
    // from disk, so this is a real disk-round-trip-vs-live comparison; on a cold
    // plain `cargo test` it renders live and caches, proving the NotFound arm falls
    // through without panicking.
    let model = common::cached_model();
    let live = render_book(&model, &ExecutableDocsData::default());
    assert_eq!(common::cached_book(), live);
}
