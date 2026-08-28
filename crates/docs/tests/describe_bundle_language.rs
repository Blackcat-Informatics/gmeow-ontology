// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration regression: `gmeow describe --lang <carrier>` must resolve every
//! framework carrier tag against the SHIPPED bundle.
//!
//! The generated `gmeow:bcp47Tag` projection rides the NAMED
//! `graph/lang-projection-corpus`, not the default graph. A `describe` tag map
//! built from the default-graph projection alone therefore never saw the carrier
//! BCP-47 tags, so `--lang fr` / `--lang zh` hard-failed with "unknown language
//! tag" even though fr/zh are the framework's own translation-target carriers.
//! The fix builds the tag map from a FLATTENED (all-graphs) projection and unions
//! the known carrier public tags into the requestable set. A synthetic-fixture
//! unit test cannot catch this — the bug only manifests with the real bundle's
//! named-graph placement — so this test drives the committed `gmeow.gts`.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must be resolvable")
}

fn bundle_bytes() -> Vec<u8> {
    gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
        .expect("authenticated shipped bundle; tests never produce it")
}

#[test]
fn describe_resolves_carrier_tags_against_shipped_bundle() {
    let bytes = bundle_bytes();
    // The two non-English carriers were the regression: fr/zh hard-failed because
    // the default-graph tag map omitted the corpus-graph bcp47 projection. They
    // must now resolve to a rendered card (exit 0) — falling back to the English
    // carrier where the term carries no content in that language, never a
    // hard-fail (they are shippable translation targets). One representative
    // non-English carrier (`fr`) suffices to prove a carrier renders end-to-end
    // over the shipped bundle; a single whole-bundle fold keeps the test within
    // the per-test time budget. That en/fr/zh are all requestable is asserted by
    // `describe_rejects_unknown_tag_but_lists_the_carriers` (which folds once and
    // reads the available set), and the internal `x-gmeow-*` path by the
    // describe.rs unit tests.
    let (text, status) = gmeow_docs::describe(
        "gmeow:Language",
        &bytes,
        Some("fr"),
        gmeow_docs::card::CardFormat::Prose,
        &std::collections::BTreeSet::new(),
    );
    assert_eq!(
        status,
        gmeow_docs::DescribeStatus::Ok,
        "--lang fr must resolve, got: {text}"
    );
    assert!(
        text.contains("gmeow:Language"),
        "--lang fr must render the term card, got: {text}"
    );
}

#[test]
fn describe_rejects_unknown_tag_but_lists_the_carriers() {
    let bytes = bundle_bytes();
    let (text, status) = gmeow_docs::describe(
        "gmeow:Language",
        &bytes,
        Some("zz-nonsense"),
        gmeow_docs::card::CardFormat::Prose,
        &std::collections::BTreeSet::new(),
    );
    assert_eq!(
        status,
        gmeow_docs::DescribeStatus::UnknownLanguage,
        "a truly-unknown tag must hard-fail, got: {text}"
    );
    // The known carriers are always requestable, so the diagnostic lists them.
    // Parse the "Available languages: a, b, c" tail into a token set.
    let listed: std::collections::BTreeSet<&str> = text
        .split("Available languages: ")
        .nth(1)
        .unwrap_or_else(|| panic!("diagnostic must carry an available-languages list: {text}"))
        .trim()
        .split(", ")
        .map(str::trim)
        .collect();
    for carrier in ["en", "fr", "zh"] {
        assert!(
            listed.contains(carrier),
            "the available-languages diagnostic must list the {carrier} carrier, got: {text}"
        );
    }
}
