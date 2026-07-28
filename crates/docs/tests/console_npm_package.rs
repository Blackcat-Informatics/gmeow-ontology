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

/// The `version` field of a package manifest, read without a JSON dependency: the first
/// quoted string after the `"version"` key.
fn manifest_version(manifest: &str) -> &str {
    let key = manifest
        .find("\"version\"")
        .expect("the package manifest declares a version");
    let tail = &manifest[key + "\"version\"".len()..];
    let open = tail.find('"').expect("the version value is a JSON string");
    let rest = &tail[open + 1..];
    let close = rest.find('"').expect("the version string is terminated");
    &rest[..close]
}

/// The shipped console package's version equals this crate's version.
#[test]
fn console_package_version_equals_the_crate_version() {
    let manifest = include_str!("../assets/console/package.json");
    assert_eq!(
        manifest_version(manifest),
        env!("CARGO_PKG_VERSION"),
        "crates/docs/assets/console/package.json version drifted from the crate version"
    );
}

/// The console's dev-only Playwright smoke manifest declares itself PRIVATE, which is what
/// keeps it out of the published set every packaging gate quantifies over.
#[test]
fn the_smoke_manifest_declares_itself_private() {
    let manifest = include_str!("../assets/console/smoke/package.json");
    assert!(
        manifest.contains("\"private\": true"),
        "the console smoke manifest must declare `private: true` — it is what excludes \
         Playwright, and the smoke lane itself, from everything this repository publishes"
    );
}
