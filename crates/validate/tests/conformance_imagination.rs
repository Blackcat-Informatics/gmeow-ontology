// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from `slices/core/imagination/tests/test_imagination.py`.
//!
//! One exact-set-equality invariant over `manifest.ttl` — a file the module-scoped
//! structural-cell harness never loads (its store is `module.ttl` + `examples/` only)
//! and which an `ASK` could not pin to an EXACT set anyway. It was the residue left in
//! Python; it now lives natively here.
//!
//! * `slice_depends_on_is_exactly_kernel_and_logic` — the imagination `manifest.ttl`
//!   declares `gmeow:sliceDependsOn` EXACTLY `{kernel, logic}` (mentation / deception /
//!   epistemics are named only in prose, so they stay undeclared). `logic` is declared
//!   because `module.ttl` uses it structurally — `a logic:AbstractIndividualType`,
//!   `rdfs:subClassOf logic:QualityValue`, and twelve `gmeow:graphBoxRole` assertions —
//!   and "consumed by reference, never declared" is exactly the undeclared-dependency
//!   defect `slice-ownership.undeclared-dependency` gates. The set stays EXACT so the
//!   pin still catches dependency creep.

mod conformance_support;
use conformance_support::*;

use std::collections::BTreeSet;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_manifest_depends_only_on_kernel`: the imagination `manifest.ttl`
/// declares `gmeow:sliceDependsOn` EXACTLY `{kernel, logic}` — the two slices whose
/// terms `module.ttl` actually uses, and no others.
#[test]
fn slice_depends_on_is_exactly_kernel_and_logic() {
    let manifest = repo_root().join("slices/core/imagination/manifest.ttl");
    let m = GraphStore::parse_ttl_file(&manifest);

    let deps = m.objects(&gm("slices/imagination"), &gm("sliceDependsOn"));
    let expected: BTreeSet<String> = [gm("slices/kernel"), gm("slices/logic")]
        .into_iter()
        .collect();
    assert_eq!(
        deps, expected,
        "imagination sliceDependsOn must be exactly {{kernel, logic}}, got {deps:?}"
    );
}
