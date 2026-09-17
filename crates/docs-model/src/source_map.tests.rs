// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
