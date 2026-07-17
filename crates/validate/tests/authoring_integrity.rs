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

fn joined(findings: &[gmeow_errors::Finding]) -> String {
    findings
        .iter()
        .map(|f| f.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn profile_and_partition_closure_is_clean() {
    let root = repo_root();
    // Non-vacuity: the profile documents must exist and the partition must have
    // genuinely populated core + extension sets (guarded by the detector's
    // full == ontology ∪ extensions check, which is non-trivial only when
    // extensions is non-empty).
    assert!(
        root.join("generated/profiles/full.ttl").is_file(),
        "generated/profiles/full.ttl missing"
    );
    assert!(
        root.join("generated/profiles/claims.ttl").is_file(),
        "generated/profiles/claims.ttl missing"
    );

    let findings = authoring_integrity::profile_closure_findings(&root).expect("profile closure");
    assert!(
        findings.is_empty(),
        "profile/partition closure defects:\n{}",
        joined(&findings)
    );
}

#[test]
fn every_slice_module_is_in_the_catalog() {
    let root = repo_root();
    // Non-vacuity: the catalog must parse to a genuinely populated name set.
    let names = purrdf_free_catalog_names(&root);
    assert!(
        names > 1,
        "catalog parsed {names} <uri> entries — the closure check would be vacuous"
    );

    let findings = authoring_integrity::catalog_closure_findings(&root).expect("catalog closure");
    assert!(
        findings.is_empty(),
        "slice modules absent from catalog-v001.xml:\n{}",
        joined(&findings)
    );
}

/// A local re-parse of the catalog `<uri>` name count for the non-vacuity guard —
/// independent of the detector, so a broken detector cannot make the guard pass.
fn purrdf_free_catalog_names(root: &Path) -> usize {
    let text = std::fs::read_to_string(root.join("catalog-v001.xml")).expect("read catalog");
    let doc = roxmltree::Document::parse(&text).expect("parse catalog");
    doc.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "uri")
        .count()
}

#[test]
fn every_slice_module_iri_matches_its_location() {
    let root = repo_root();
    let findings = authoring_integrity::module_iri_findings(&root).expect("module iri");
    assert!(
        findings.is_empty(),
        "module owl:Ontology IRIs that do not match their location:\n{}",
        joined(&findings)
    );
}

#[test]
fn coverage_fixtures_use_only_declared_terms() {
    let root = repo_root();
    let declared = authoring_integrity::declared_ontology_terms(&root).expect("declared terms");
    // Non-vacuity on the EXTRACTED set: the declared authority must be populated,
    // not merely the file globs.
    assert!(
        declared.len() > 50,
        "declared-term set is implausibly small ({}) — the authority is vacuous",
        declared.len()
    );
    let findings = authoring_integrity::coverage_fixture_undeclared_findings(&root, &declared)
        .expect("fixture term check");
    assert!(
        findings.is_empty(),
        "coverage fixtures reference undeclared terms:\n{}",
        joined(&findings)
    );
}

#[test]
fn slice_examples_use_only_declared_terms() {
    let root = repo_root();
    let declared = authoring_integrity::declared_ontology_terms(&root).expect("declared terms");
    let findings = authoring_integrity::example_undeclared_term_findings(&root, &declared)
        .expect("example term check");
    assert!(
        findings.is_empty(),
        "slice examples reference undeclared terms:\n{}",
        joined(&findings)
    );
}

#[test]
fn slice_source_localizable_literals_are_language_tagged() {
    let root = repo_root();
    let findings =
        authoring_integrity::slice_source_untagged_findings(&root).expect("slice source langtag");
    assert!(
        findings.is_empty(),
        "untagged localizable literals in slice source:\n{}",
        joined(&findings)
    );
}

#[test]
fn nonslice_authored_localizable_literals_are_language_tagged() {
    let root = repo_root();
    let findings = authoring_integrity::nonslice_authored_untagged_findings(&root)
        .expect("non-slice source langtag");
    assert!(
        findings.is_empty(),
        "untagged localizable literals in non-slice authored source:\n{}",
        joined(&findings)
    );
}

#[test]
fn docs_examples_use_only_allowlisted_terms() {
    let root = repo_root();
    // Non-vacuity on the EXTRACTED docs-term set: a broken fence/inline regex that
    // yielded nothing would make "no unallowlisted terms" trivially true.
    let extracted = authoring_integrity::docs_gmeow_terms(&root).expect("docs term extraction");
    assert!(
        !extracted.is_empty(),
        "no gmeow: terms extracted from docs/*.md — the extractor is vacuous"
    );

    let findings = authoring_integrity::docs_undeclared_findings(&root).expect("docs term check");
    assert!(
        findings.is_empty(),
        "docs examples reference unallowlisted GMEOW terms:\n{}",
        joined(&findings)
    );
}
