// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The npm package this crate ships carries THIS crate's version.
//!
//! `version()` crosses the wasm boundary as `env!("CARGO_PKG_VERSION")`, so a package
//! manifest whose `version` disagrees publishes a tarball that reports a different
//! version than the registry entry claims. The manifest bytes are embedded with
//! `include_str!`, so this fails at COMPILE time if the manifest is deleted and at test
//! time if it drifts.
//!
//! The cross-cutting half of the contract (every published package, the scoping, the
//! export-set equality, the release lanes) lives in
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

/// The shipped MCP reasoning segment package's version equals this crate's version.
#[test]
fn js_package_version_equals_the_crate_version() {
    let manifest = include_str!("../js/package.json");
    assert_eq!(
        manifest_version(manifest),
        env!("CARGO_PKG_VERSION"),
        "crates/mcp-wasm/js/package.json version drifted from the crate version"
    );
}
