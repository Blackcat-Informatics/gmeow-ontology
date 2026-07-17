// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Whole-corpus authoring-integrity gates over the committed repository. Each
//! detector must find ZERO violations on the real corpus (the gate), and the
//! corpus it scans must be genuinely non-empty (non-vacuity — a silently empty
//! scan would make "zero findings" meaningless). The detectors' *detection* logic
//! (that they fire on a bad input) is proven by the synthetic-negative unit tests
//! in `authoring_integrity`'s `#[cfg(test)]` module; here we prove the shipped
//! corpus is clean.

use std::path::{Path, PathBuf};

use gmeow_validate::authoring_integrity;

/// The repository root — the `gmeow-validate` crate lives at `crates/validate`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the validate crate should live under crates/")
        .to_path_buf()
}

#[test]
fn shape_iri_ownership_is_collision_free() {
    let root = repo_root();
    // Non-vacuity: the shape corpus must be genuinely populated.
    let shape_files =
        purrdf::shapes::shape_union::shape_files(&root).expect("enumerate the merged shape corpus");
    assert!(
        !shape_files.is_empty(),
        "merged shape corpus is empty — the collision sweep would be vacuous"
    );

    let findings =
        authoring_integrity::shape_iri_collision_findings(&root).expect("shape collision sweep");
    assert!(
        findings.is_empty(),
        "shape-IRI ownership collisions in the committed corpus:\n{}",
        findings
            .iter()
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn core_rights_module_has_no_norms_extension_leak() {
    let root = repo_root();
    // Non-vacuity: the core rights module must exist and parse to a non-empty graph.
    let path = root.join("slices/core/rights/module.ttl");
    assert!(path.is_file(), "core rights module missing at {path:?}");

    let findings = authoring_integrity::graft_isolation_findings(&root).expect("graft isolation");
    assert!(
        findings.is_empty(),
        "core rights module references norms-extension IRIs:\n{}",
        findings
            .iter()
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn slice_discipline_is_clean_across_the_committed_manifests() {
    let root = repo_root();
    let slices_dir = root.join("slices");
    assert!(slices_dir.is_dir(), "slices/ directory missing");

    let findings =
        authoring_integrity::slice_discipline_findings(&slices_dir).expect("slice discipline");
    assert!(
        findings.is_empty(),
        "slice-discipline defects (duplicate IRI or missing tier) in the committed manifests:\n{}",
        findings
            .iter()
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
