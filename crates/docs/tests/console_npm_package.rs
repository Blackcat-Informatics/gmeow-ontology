// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The npm package the standalone console ships carries THIS crate's version.
//!
//! The console's assets live under `crates/docs/assets/console/`, so `gmeow-docs` is the
//! crate whose version the published `@blackcatinformatics/gmeow-console` must equal. The
//! manifest bytes are embedded with `include_str!`, so this fails at COMPILE time if the
//! manifest is deleted and at test time if it drifts.
//!
//! The cross-cutting half of the contract (scoping, export-set equality, the release
//! lanes, the CDN drift gate) lives in
//! `crates/gmeow-dev-cli/tests/npm_packaging_contract.rs`.

use std::collections::BTreeSet;

use gmeow_docs::ExecutableDocsData;
use gmeow_docs::console::console_files;

mod common;

/// A package manifest, PARSED as the JSON it is.
///
/// A string scrape (`find("\"version\"")` and count quotes) answers a different question
/// than the one npm asks: it matches the key wherever it appears — inside a dependency
/// name, a script body, a nested object — and it cannot tell `"private": true` from
/// `"private": "true"` or from the word appearing in a description. Parsing is the only
/// read that agrees with the registry's own.
fn manifest(bytes: &str) -> serde_json::Value {
    serde_json::from_str(bytes).expect("the package manifest is valid JSON")
}

/// The shipped console package's version equals this crate's version.
#[test]
fn console_package_version_equals_the_crate_version() {
    let manifest = manifest(include_str!("../assets/console/package.json"));
    assert_eq!(
        manifest["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "crates/docs/assets/console/package.json version drifted from the crate version"
    );
}

/// Paths the published README names that belong to the DEPLOYED SITE, not to the package.
///
/// Two sentences need them: "it does not ship the standalone site shell", and the one
/// explaining why the worker's single import specifier resolves in both trees. Both are
/// about the contrast, so each entry is checked in BOTH directions — present in the
/// assembled site tree, absent from the package — and a path that stopped being either is a
/// failure rather than a permanent exemption.
const SITE_PATHS_THE_README_NAMES: &[&str] = &[
    "index.html",
    "manifest.webmanifest",
    "sw.mjs",
    "console/pkg/mcp-transport.mjs",
    "assets/mcp-transport.mjs",
];

/// The file set the published tarball carries: the declared `files` plus what npm always
/// includes.
fn published_files(manifest: &serde_json::Value) -> BTreeSet<String> {
    let mut files: BTreeSet<String> = manifest["files"]
        .as_array()
        .expect("the package declares a files array")
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .expect("every files entry is a string")
                .to_string()
        })
        .collect();
    // npm includes these whatever `files` says, which is exactly why the README's own
    // accuracy is a packaging concern: it is published whether or not anyone listed it.
    files.insert("package.json".to_string());
    files.insert("README.md".to_string());
    files
}

/// An `exec` that makes the console producer emit its full tree.
fn interactive_exec() -> ExecutableDocsData {
    ExecutableDocsData {
        full_bundle_gts: b"gts-bundle-sentinel-bytes".to_vec(),
        conjectures_ttl: b"@prefix ex: <http://example/> .".to_vec(),
        ..Default::default()
    }
}

/// Every path the PUBLISHED README names exists in the PUBLISHED tarball.
///
/// npm ships `README.md` verbatim whatever the `files` list says, and the document it shipped
/// was the deployed site's: it documented a service worker, a PWA manifest, four icons, a
/// byte table of `assets/…` rows and a dev-only Playwright lane, and the installed tree
/// carried none of them. A reader following that document reached for eleven files that were
/// not there.
///
/// The rule is the byte table's rule, applied to the prose: a path a shipped document names
/// is a path its own distribution answers for — unless it is named as belonging to another
/// one, and then it must really be there and really not be here.
#[test]
fn every_path_the_published_readme_names_exists_in_the_package() {
    let manifest = manifest(include_str!("../assets/console/package.json"));
    let published = published_files(&manifest);
    let readme = include_str!("../assets/console/README.md");

    let problems =
        common::unresolved_readme_paths(readme, &published, "", SITE_PATHS_THE_README_NAMES);
    assert!(
        problems.is_empty(),
        "the published console README does not describe the package npm publishes:\n{}",
        problems.join("\n")
    );

    // …and the contrast the README draws is real: each path it hands to the site is a path
    // the assembled site tree carries.
    let site = console_files(&interactive_exec());
    for path in SITE_PATHS_THE_README_NAMES {
        let carried = site.contains_key(*path) || site.contains_key(&format!("console/{path}"));
        assert!(
            carried,
            "the README tells a reader that {path} lives in the deployed tree, which does \
             not carry it"
        );
    }
}

/// The console's dev-only Playwright smoke manifest declares itself PRIVATE, which is what
/// keeps it out of the published set every packaging gate quantifies over.
#[test]
fn the_smoke_manifest_declares_itself_private() {
    let manifest = manifest(include_str!("../assets/console/smoke/package.json"));
    assert_eq!(
        manifest["private"].as_bool(),
        Some(true),
        "the console smoke manifest must declare the JSON boolean `private: true` — it is \
         what excludes Playwright, and the smoke lane itself, from everything this \
         repository publishes"
    );
}
