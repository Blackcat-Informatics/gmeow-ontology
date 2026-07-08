// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from `slices/core/imagination/tests/test_imagination.py`.
//!
//! One exact-set-equality invariant over `manifest.ttl` — a file the module-scoped
//! structural-cell harness never loads (its store is `module.ttl` + `examples/` only)
//! and which an `ASK` could not pin to an EXACT set anyway. It was the residue left in
//! Python; it now lives natively here.
//!
//! * `slice_depends_on_is_exactly_kernel` — the imagination `manifest.ttl` declares
//!   `gmeow:sliceDependsOn` EXACTLY `{kernel}` (kernel alone; mentation / logic /
//!   deception / epistemics are consumed by reference, never declared).

mod conformance_support;
use conformance_support::*;

use std::collections::BTreeSet;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_manifest_depends_only_on_kernel`: the imagination `manifest.ttl`
/// declares `gmeow:sliceDependsOn` EXACTLY `{kernel}`.
#[test]
fn slice_depends_on_is_exactly_kernel() {
    let manifest = repo_root().join("slices/core/imagination/manifest.ttl");
    let m = GraphStore::parse_ttl_file(&manifest);

    let deps = m.objects(&gm("slices/imagination"), &gm("sliceDependsOn"));
    let expected: BTreeSet<String> = [gm("slices/kernel")].into_iter().collect();
    assert_eq!(
        deps, expected,
        "imagination sliceDependsOn must be exactly {{kernel}}, got {deps:?}"
    );
}
