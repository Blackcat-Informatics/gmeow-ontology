// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn schemas_stage_emits_all_authenticated_artifacts() {
    let stage = SchemasStage::new();
    assert_eq!(stage.id(), "stage-export-schemas");
    let root = repo_root();
    let first = crate::fixture::stage_artifacts(&root, 1, "stage-export-schemas")
        .expect("authenticated developer-schema projection");
    for path in SCHEMA_PATHS {
        assert!(first.contains_key(path), "missing {path}");
        assert!(!first[path].is_empty(), "{path} is empty");
    }
}

/// A value-vocabulary enum (e.g. `gmeow:TermStability`'s members) must reach
/// the emitted LinkML YAML — proving the enrichment
/// ([`crate::stages::schema_compile::enriched_compiled_schema`]) reached the
/// developer-surface emitters, not just the JSON-Schema leaf.
#[test]
fn value_vocab_enum_reaches_linkml_output() {
    let root = repo_root();
    let artifacts = crate::fixture::stage_artifacts(&root, 1, "stage-export-schemas")
        .expect("authenticated developer-schema projection");
    let linkml_yaml =
        String::from_utf8(artifacts[LINKML_PATH].clone()).expect("linkml yaml is utf8");
    // The `gmeow:TermStability` value vocabulary's seed members are
    // `gmeow:stabilityStable` / `gmeow:stabilityExperimental` /
    // `gmeow:stabilityDeprecated` (slices/core/versions/module.ttl) — assert the
    // enum class name and a real member CURIE both reached the LinkML output.
    assert!(
        linkml_yaml.contains("TermStability") && linkml_yaml.contains("stabilityStable"),
        "expected the TermStability value vocabulary to reach the LinkML output:\n{linkml_yaml}"
    );
}
