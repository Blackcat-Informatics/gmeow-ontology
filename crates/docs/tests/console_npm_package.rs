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
