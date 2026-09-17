// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Positive detection: a native alignment cell planted under `dsl/mappings/` must
/// still trip the purity gate. This proves the re-keyed gate did NOT go vacuous after
/// the legacy reified alignment-cell type was deleted — it now recognizes the native
/// reified-annotation cell shape.
#[test]
fn mapping_purity_fires_on_misplaced_cell() {
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("dsl").join("mappings");
    std::fs::create_dir_all(&dir).expect("mkdir dsl/mappings");
    std::fs::write(
        dir.join("stray.ttl"),
        br#"
@prefix gmeow:  <https://blackcatinformatics.ca/gmeow/> .
@prefix skos:   <http://www.w3.org/2004/02/skos/core#> .
@prefix schema: <https://schema.org/> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .

gmeow:Foo skos:exactMatch schema:Thing {|
    gmeow:sssomFile     "gmeow-demo.sssom.tsv" ;
    gmeow:justification semapv:ManualMappingCuration ;
    gmeow:confidence    0.9
|} .
"#,
    )
    .expect("write stray cell");

    let diags = lint_dsl_mapping_purity(root.path()).expect("lint runs");
    assert_eq!(diags.len(), 1, "the misplaced native cell must be detected");
    assert_eq!(diags[0].code, "dsl-linkage-purity");
    assert!(
        diags[0].message.contains("native alignment cell"),
        "{}",
        diags[0].message
    );
}

/// A `MappingSet` publication header carries `gmeow:sssomFile` but is NOT a reified
/// match cell, so it legitimately lives under `dsl/mappings/` and must NOT trip the
/// gate (proves the gate is not a blunt `gmeow:sssomFile` counter).
#[test]
fn mapping_purity_ignores_mapping_set_header() {
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("dsl").join("mappings");
    std::fs::create_dir_all(&dir).expect("mkdir dsl/mappings");
    std::fs::write(
        dir.join("headers.ttl"),
        br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:setDemo a gmeow:MappingSet ;
    gmeow:sssomFile "gmeow-demo.sssom.tsv" ;
    gmeow:setId "https://blackcatinformatics.ca/gmeow/mappings/demo" ;
    gmeow:license "https://creativecommons.org/licenses/by/4.0/" .
"#,
    )
    .expect("write mapping-set header");

    let diags = lint_dsl_mapping_purity(root.path()).expect("lint runs");
    assert!(
        diags.is_empty(),
        "a MappingSet publication header is not a misplaced alignment cell: {diags:?}"
    );
}

/// A missing `dsl/mappings/` tree contributes no sources and is not an error.
#[test]
fn mapping_purity_clean_on_missing_tree() {
    let root = tempfile::tempdir().expect("tempdir");
    let diags = lint_dsl_mapping_purity(root.path()).expect("lint runs");
    assert!(diags.is_empty());
}
