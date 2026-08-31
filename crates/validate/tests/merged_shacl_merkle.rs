// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the S6a semantic Merkle merged-SHACL source key wired
//! into the production validation pipeline (RFC §12).
//!
//! The merged-SHACL `source_key` is now `merged_shacl_source_key(slices_dir)` —
//! the Merkle PRODUCT root over the whole slice composition. These tests prove
//! the headline acceptance against a small synthetic slices tree:
//!   1. path-independence — renaming/moving a slice to a different group does not
//!      change the key,
//!   2. semantic invariance — a comment-only module edit does not change the key,
//!   3. semantic sensitivity (module) — a canonical-RDF module change DOES change
//!      the key,
//!   4. semantic sensitivity (manifest) — a `gmeow:sliceDependsOn` change DOES
//!      change the key.
//!
//! All fixtures are hermetic (`tempfile`); no repository state is read.

use std::path::Path;

use gmeow_validate::validate_all::merged_shacl_source_key;
use tempfile::TempDir;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// Write one slice directory under `parent/<dirname>/` with an explicit manifest
/// and module body, so each fixture controls path, comments, and canonical RDF
/// independently. `dep_iris` are emitted as `gmeow:sliceDependsOn` targets.
fn write_slice(
    parent: &Path,
    dirname: &str,
    slice_iri: &str,
    term: &str,
    module_comment: &str,
    extra_module_triple: &str,
    dep_iris: &[&str],
) {
    let dir = parent.join(dirname);
    std::fs::create_dir_all(&dir).unwrap();

    let depends_on: String = dep_iris
        .iter()
        .map(|d| format!("    gmeow:sliceDependsOn <{d}> ;\n"))
        .collect();
    let manifest = format!(
        r#"@prefix gmeow: <{GMEOW}> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dcterms: <http://purl.org/dc/terms/> .

<{slice_iri}> a gmeow:Slice ;
    rdfs:label "slice label"@x-gmeow-english ;
    dcterms:title "Slice title"@x-gmeow-english ;
    dcterms:creator "Test Author" ;
    gmeow:sliceTier gmeow:tierCore ;
{depends_on}    gmeow:sliceConsumer "test"@x-gmeow-english .
"#
    );
    std::fs::write(dir.join("manifest.ttl"), manifest).unwrap();

    let module = format!(
        r#"{module_comment}@prefix gmeow: <{GMEOW}> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

<{term}> a owl:Class ;
    rdfs:isDefinedBy <{slice_iri}> ;
    rdfs:label "term"@x-gmeow-english .
{extra_module_triple}"#
    );
    std::fs::write(dir.join("module.ttl"), module).unwrap();
}

/// Build a baseline two-slice tree under `<tmp>/slices/core/{alpha,beta}` and
/// return the slices dir as a string.
fn baseline_tree(tmp: &TempDir) -> String {
    let core = tmp.path().join("slices").join("core");
    write_slice(
        &core,
        "alpha",
        &format!("{GMEOW}slice/alpha"),
        &format!("{GMEOW}Alpha"),
        "# alpha comment\n",
        "",
        &[],
    );
    write_slice(
        &core,
        "beta",
        &format!("{GMEOW}slice/beta"),
        &format!("{GMEOW}Beta"),
        "# beta comment\n",
        "",
        &[],
    );
    tmp.path().join("slices").to_string_lossy().into_owned()
}

#[test]
fn merged_shacl_key_is_path_independent() {
    // Baseline: slices/core/{alpha,beta}.
    let t1 = TempDir::new().unwrap();
    let dir1 = baseline_tree(&t1);

    // Same two slices (byte-identical), but moved to a DIFFERENT group path and a
    // renamed directory: slices/zztop/{moved_alpha,moved_beta}.
    let t2 = TempDir::new().unwrap();
    let other = t2.path().join("slices").join("zztop");
    write_slice(
        &other,
        "moved_alpha",
        &format!("{GMEOW}slice/alpha"),
        &format!("{GMEOW}Alpha"),
        "# alpha comment\n",
        "",
        &[],
    );
    write_slice(
        &other,
        "moved_beta",
        &format!("{GMEOW}slice/beta"),
        &format!("{GMEOW}Beta"),
        "# beta comment\n",
        "",
        &[],
    );
    let dir2 = t2.path().join("slices").to_string_lossy().into_owned();

    let k1 = merged_shacl_source_key(&dir1).unwrap();
    let k2 = merged_shacl_source_key(&dir2).unwrap();
    assert_eq!(
        k1, k2,
        "merged-SHACL Merkle key changed when slices were renamed/moved to another group"
    );
}

#[test]
fn merged_shacl_key_is_comment_invariant() {
    let t1 = TempDir::new().unwrap();
    let dir1 = baseline_tree(&t1);

    // Identical canonical RDF; only the leading comment on alpha's module differs.
    let t2 = TempDir::new().unwrap();
    let core2 = t2.path().join("slices").join("core");
    write_slice(
        &core2,
        "alpha",
        &format!("{GMEOW}slice/alpha"),
        &format!("{GMEOW}Alpha"),
        "# a COMPLETELY different comment\n# with an extra line\n",
        "",
        &[],
    );
    write_slice(
        &core2,
        "beta",
        &format!("{GMEOW}slice/beta"),
        &format!("{GMEOW}Beta"),
        "# beta comment\n",
        "",
        &[],
    );
    let dir2 = t2.path().join("slices").to_string_lossy().into_owned();

    let k1 = merged_shacl_source_key(&dir1).unwrap();
    let k2 = merged_shacl_source_key(&dir2).unwrap();
    assert_eq!(
        k1, k2,
        "merged-SHACL Merkle key changed on a comment-only module edit (canonical RDF identical)"
    );
}

#[test]
fn merged_shacl_key_changes_on_module_semantic_change() {
    let t1 = TempDir::new().unwrap();
    let dir1 = baseline_tree(&t1);

    // alpha's module gains a real (canonical) triple — a new owned term.
    let t2 = TempDir::new().unwrap();
    let core2 = t2.path().join("slices").join("core");
    let extra = format!(
        "<{GMEOW}AlphaExtra> a <http://www.w3.org/2002/07/owl#Class> ;\n    \
         rdfs:isDefinedBy <{GMEOW}slice/alpha> .\n"
    );
    write_slice(
        &core2,
        "alpha",
        &format!("{GMEOW}slice/alpha"),
        &format!("{GMEOW}Alpha"),
        "# alpha comment\n",
        &extra,
        &[],
    );
    write_slice(
        &core2,
        "beta",
        &format!("{GMEOW}slice/beta"),
        &format!("{GMEOW}Beta"),
        "# beta comment\n",
        "",
        &[],
    );
    let dir2 = t2.path().join("slices").to_string_lossy().into_owned();

    let k1 = merged_shacl_source_key(&dir1).unwrap();
    let k2 = merged_shacl_source_key(&dir2).unwrap();
    assert_ne!(
        k1, k2,
        "merged-SHACL Merkle key did NOT change on a canonical-RDF module change"
    );
}

#[test]
fn merged_shacl_key_changes_on_manifest_depends_on_change() {
    let t1 = TempDir::new().unwrap();
    let dir1 = baseline_tree(&t1);

    // alpha's manifest gains `gmeow:sliceDependsOn beta` (a semantic manifest
    // change). The module bytes/canonical-RDF are unchanged.
    let t2 = TempDir::new().unwrap();
    let core2 = t2.path().join("slices").join("core");
    write_slice(
        &core2,
        "alpha",
        &format!("{GMEOW}slice/alpha"),
        &format!("{GMEOW}Alpha"),
        "# alpha comment\n",
        "",
        &[&format!("{GMEOW}slice/beta")],
    );
    write_slice(
        &core2,
        "beta",
        &format!("{GMEOW}slice/beta"),
        &format!("{GMEOW}Beta"),
        "# beta comment\n",
        "",
        &[],
    );
    let dir2 = t2.path().join("slices").to_string_lossy().into_owned();

    let k1 = merged_shacl_source_key(&dir1).unwrap();
    let k2 = merged_shacl_source_key(&dir2).unwrap();
    assert_ne!(
        k1, k2,
        "merged-SHACL Merkle key did NOT change when a manifest's sliceDependsOn changed"
    );
}

#[test]
fn merged_shacl_key_is_deterministic_repeated() {
    let t = TempDir::new().unwrap();
    let dir = baseline_tree(&t);
    let k1 = merged_shacl_source_key(&dir).unwrap();
    let k2 = merged_shacl_source_key(&dir).unwrap();
    let k3 = merged_shacl_source_key(&dir).unwrap();
    assert_eq!(k1, k2, "merged-SHACL key not deterministic across repeats");
    assert_eq!(k2, k3, "merged-SHACL key not deterministic across repeats");
}
