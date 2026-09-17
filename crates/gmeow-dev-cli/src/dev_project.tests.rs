// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn snippets_readme_describes_the_offline_corpus() {
    // The README must name the corpus, its per-term card layout, its offline
    // agent-ingestible role, and how to (re)generate it.
    assert!(SNIPPETS_README.starts_with("# GMEOW documentation snippets"));
    assert!(SNIPPETS_README.contains("offline, agent-ingestible projection"));
    assert!(SNIPPETS_README.contains("one prompt-ready Markdown card per vocabulary term"));
    assert!(SNIPPETS_README.contains("terms/<slug>.md"));
    assert!(SNIPPETS_README.contains("gmeow-dev sync --mode update --outputs docs"));
}

#[test]
fn source_snippets_flattens_cards_and_emits_the_readme() {
    // A minimal site tree: two term cards plus unrelated files that must be
    // dropped by the snippets projection.
    let mut site: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    site.insert("terms/foo/card.md".to_string(), b"# gmeow:Foo".to_vec());
    site.insert("terms/bar/card.md".to_string(), b"# gmeow:Bar".to_vec());
    site.insert("terms/foo/index.html".to_string(), b"<html/>".to_vec());
    site.insert("index.html".to_string(), b"<html/>".to_vec());

    let out = source_snippets(&site).expect("cards present → snippets projection succeeds");

    // The cards are flattened to `terms/<slug>.md`; nothing else leaks through.
    assert_eq!(
        out.get("terms/foo.md").map(Vec::as_slice),
        Some(&b"# gmeow:Foo"[..])
    );
    assert_eq!(
        out.get("terms/bar.md").map(Vec::as_slice),
        Some(&b"# gmeow:Bar"[..])
    );
    assert!(!out.contains_key("terms/foo/index.html"));
    assert!(!out.contains_key("index.html"));

    // The corpus README is emitted at the tree root with the corpus paragraph.
    let readme = out
        .get("README.md")
        .expect("snippets export writes a README");
    let readme = std::str::from_utf8(readme).expect("README is UTF-8");
    assert!(readme.contains("offline, agent-ingestible projection"));
    assert!(readme.contains("terms/<slug>.md"));
}

#[test]
fn source_snippets_hard_fails_without_cards() {
    // No `terms/*/card.md` in the tree → a hard error, never a silent empty tree.
    let mut site: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    site.insert("index.html".to_string(), b"<html/>".to_vec());
    assert!(source_snippets(&site).is_err());
}

#[test]
fn explicit_unknown_docs_language_hard_fails() {
    assert!(pick_source_lang(Some("not-a-language"), &Default::default()).is_err());
}

// ── the release-row producers, over the REAL catalog table ─────────────────────

/// A non-empty tree carrying every declared sub-asset prefix, so a fixture built from
/// it exercises the real pricing loop rather than a stub.
fn interactive_tree(owner: &str) -> BTreeMap<String, Vec<u8>> {
    let mut tree = BTreeMap::from([("index.html".to_string(), b"<html/>".to_vec())]);
    for pricing in gmeow_pipeline::stages::distribution_catalog::sub_asset_pricing()
        .into_iter()
        .filter(|pricing| pricing.owner == owner)
    {
        let prefix = pricing.tree_path_prefix;
        // A directory prefix ends in `/`; a file prefix IS the key.
        let key = if prefix.ends_with('/') {
            format!("{prefix}engine.wasm")
        } else {
            prefix
        };
        tree.insert(key, format!("bytes-for-{}", pricing.slug).into_bytes());
    }
    tree
}

/// Every declared distribution gets a tree, so the happy path is genuinely complete.
fn full_rendered<'a>(
    trees: &'a BTreeMap<&'static str, BTreeMap<String, Vec<u8>>>,
) -> RenderedTrees<'a> {
    trees.iter().map(|(slug, tree)| (*slug, tree)).collect()
}

fn every_slug_rendered() -> BTreeMap<&'static str, BTreeMap<String, Vec<u8>>> {
    gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS
        .iter()
        .map(|row| (row.slug, interactive_tree(row.slug)))
        .collect()
}

/// The happy path: one release row per DECLARED distribution, each carrying the
/// catalog's own `rel_path` and media type — no restated table on this side of the
/// crate seam.
#[test]
fn content_addressing_emits_one_row_per_declared_distribution() {
    let trees = every_slug_rendered();
    let entries =
        content_address_distributions(&full_rendered(&trees)).expect("full render prices");
    let declared = gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS;
    assert_eq!(entries.len(), declared.len());
    for (entry, row) in entries.iter().zip(declared.iter()) {
        assert_eq!(entry.slug, row.slug);
        assert_eq!(entry.rel_path, row.rel_path);
        assert_eq!(entry.media_type, row.media_type);
        assert!(entry.blake3.starts_with("blake3:"));
    }
    assert!(
        entries.iter().any(|e| e.slug == "console"),
        "the console must get a release row: {entries:?}"
    );
}

/// An EMPTY console tree is refused, by slug, BEFORE it is content-addressed — an
/// empty tree hashes fine, so the digest can never be what notices.
#[test]
fn an_empty_console_tree_hard_fails_naming_the_slug() {
    let mut trees = every_slug_rendered();
    trees.insert("console", BTreeMap::new());
    let err = content_address_distributions(&full_rendered(&trees))
        .expect_err("an empty declared distribution must hard-fail")
        .to_string();
    assert!(
        err.contains("console"),
        "the refusal must name the empty distribution: {err}"
    );
    assert!(
        err.contains("EMPTY"),
        "the refusal must say what went wrong: {err}"
    );
}

/// The same guard is not console-specific: it holds for every declared slug.
#[test]
fn an_empty_tree_hard_fails_for_every_declared_distribution() {
    for row in gmeow_pipeline::stages::distribution_catalog::DISTRIBUTIONS {
        let mut trees = every_slug_rendered();
        trees.insert(row.slug, BTreeMap::new());
        let err = content_address_distributions(&full_rendered(&trees))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains(row.slug),
            "an empty {} tree must hard-fail naming the slug: {err}",
            row.slug
        );
    }
}

/// A declared distribution with no producer, and a producer with no declared row, are
/// both hard fails — the bijection is enforced in both directions at run time.
#[test]
fn the_producer_registry_and_the_catalog_must_be_a_bijection() {
    let mut trees = every_slug_rendered();
    trees.remove("console");
    let err = content_address_distributions(&full_rendered(&trees))
        .expect_err("a declared distribution with no producer must hard-fail")
        .to_string();
    assert!(err.contains("console") && err.contains("no tree"), "{err}");

    let mut trees = every_slug_rendered();
    trees.insert("not-a-distribution", interactive_tree("site"));
    let err = content_address_distributions(&full_rendered(&trees))
        .expect_err("a producer with no catalog row must hard-fail")
        .to_string();
    assert!(err.contains("not-a-distribution"), "{err}");
}

/// Sub-assets are priced from EVERY owner's own tree, onto the shared subject, with
/// the cross-owner byte-identity invariant enforced.
#[test]
fn sub_assets_are_priced_from_every_owners_tree() {
    use gmeow_pipeline::stages::distribution_catalog as catalog;
    let trees = every_slug_rendered();
    let entries = price_sub_assets(&full_rendered(&trees)).expect("full render prices");
    let owners = catalog::sub_asset_owner_slugs();
    let subs = catalog::declared_site_sub_asset_slugs();
    assert_eq!(
        entries.len(),
        owners.len() * subs.len(),
        "one row per (owner, sub-asset) pair: {entries:?}"
    );
    for owner in &owners {
        let row = catalog::distribution_row(owner).expect("owner is declared");
        for sub in &subs {
            assert!(
                entries
                    .iter()
                    .any(|e| e.slug == *sub && e.rel_path.starts_with(row.rel_path)),
                "sub-asset {sub:?} is unpriced in the {owner:?} tree: {entries:?}"
            );
        }
    }
    assert!(
        entries
            .iter()
            .any(|e| e.rel_path.starts_with("dist/gmeow-docs/console/")),
        "the console's copy of the shared engines must be priced: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|e| e.rel_path.starts_with("dist/gmeow-docs/mdbook/src/assets/")),
        "the mdbook's src/assets copy of the shared engines must be priced: {entries:?}"
    );
}

/// Exercise the actual three renderers, not a shape-compatible fixture: mdBook's
/// `src/assets/` layout must normalize to the same shared payload digest as the site's
/// and console's `assets/` layouts while retaining its real release path.
#[test]
fn real_interactive_render_layouts_price_the_same_shared_assets() {
    let model = gmeow_docs::DocsModel {
        title: "Synthetic interactive distributions".to_string(),
        version: "test".to_string(),
        ..Default::default()
    };
    let exec = gmeow_docs::ExecutableDocsData {
        playground_trig: b"synthetic playground".to_vec(),
        full_bundle_gts: b"synthetic bundle".to_vec(),
        conjectures_ttl: b"synthetic conjectures".to_vec(),
        ..Default::default()
    };
    let site = gmeow_docs::render_site_lang_exec(&model, gmeow_docs::i18n::ENGLISH, &exec).files;
    let mdbook = render_source_book(&model, &exec);
    let console = gmeow_docs::console_files(&exec);
    let rendered: RenderedTrees<'_> =
        BTreeMap::from([("site", &site), ("mdbook", &mdbook), ("console", &console)]);

    let entries = price_sub_assets(&rendered).expect("real interactive trees price");
    assert_eq!(
        entries.len(),
        gmeow_pipeline::stages::distribution_catalog::sub_asset_pricing().len()
    );
    assert!(entries.iter().any(|entry| {
        entry
            .rel_path
            .starts_with("dist/gmeow-docs/mdbook/src/assets/")
    }));
}

/// An owner whose tree is missing a declared sub-asset hard-fails naming both.
#[test]
fn a_missing_sub_asset_hard_fails_naming_the_owner_and_the_asset() {
    let mut trees = every_slug_rendered();
    trees.insert(
        "console",
        BTreeMap::from([("index.html".to_string(), b"<html/>".to_vec())]),
    );
    let err = price_sub_assets(&full_rendered(&trees))
        .expect_err("a console tree with no engines must hard-fail")
        .to_string();
    assert!(err.contains("console"), "{err}");
    assert!(err.contains("mandatory output"), "{err}");
}

/// Two owners whose copies of one shared sub-asset DIFFER is refused: the subject is
/// shared, so two digests for it is a contradiction rather than two release rows.
#[test]
fn divergent_copies_of_a_shared_sub_asset_are_refused() {
    let mut trees = every_slug_rendered();
    let mut console = interactive_tree("console");
    for pricing in gmeow_pipeline::stages::distribution_catalog::sub_asset_pricing()
        .into_iter()
        .filter(|pricing| pricing.owner == "console")
    {
        let prefix = pricing.tree_path_prefix;
        let key = if prefix.ends_with('/') {
            format!("{prefix}engine.wasm")
        } else {
            prefix
        };
        console.insert(key, b"DIFFERENT BYTES".to_vec());
    }
    trees.insert("console", console);
    let err = price_sub_assets(&full_rendered(&trees))
        .expect_err("two digests for one shared subject must be refused")
        .to_string();
    assert!(err.contains("contradiction"), "{err}");
}
