// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Production dependencies are selected by Cargo and recorded in the same
/// source inventory that both builder and read-only verifier consume.
#[test]
fn scorer_dep_closure_is_fully_hashed() {
    let source = include_str!("lib.rs");
    assert!(source.contains("current_source_inventory("));
    assert!(source.contains("crates/slice-quality/Cargo.toml"));
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let closure = gmeow_build_inputs::declared_path_dependency_manifests(
        workspace,
        "crates/slice-quality/Cargo.toml",
    )
    .unwrap();
    for owner in ["logic", "docs-model", "errors", "action-cache"] {
        assert!(
            closure.contains(&format!("crates/{owner}/Cargo.toml")),
            "selected scorer dependency {owner} must be owned"
        );
    }
}

/// The CODE half of the freshness witness is load-bearing: editing a scorer source
/// file must move the fingerprint even though no scored `.ttl` changed.
#[test]
fn a_scorer_source_edit_moves_the_fingerprint() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    // A minimal tree: one scored source (the rubric module) and one scorer source.
    let rubric = root.join(RUBRIC_MODULE);
    std::fs::create_dir_all(rubric.parent().expect("rubric parent")).expect("mkdir rubric");
    std::fs::write(&rubric, b"# rubric\n").expect("write rubric");
    let scorer_src = root.join("crates").join("slice-quality").join("src");
    std::fs::create_dir_all(&scorer_src).expect("mkdir scorer src");
    let unit = scorer_src.join("lib.rs");
    std::fs::write(&unit, b"// v1\n").expect("write scorer");

    let before = scored_input_fingerprint_with_implementation(
        root,
        &gmeow_action_cache::bytes_digest(&std::fs::read(&unit).unwrap()),
    )
    .expect("fingerprint v1");
    std::fs::write(&unit, b"// v2: the axis now scores differently\n").expect("rewrite scorer");
    let after = scored_input_fingerprint_with_implementation(
        root,
        &gmeow_action_cache::bytes_digest(&std::fs::read(&unit).unwrap()),
    )
    .expect("fingerprint v2");
    assert_ne!(
        before, after,
        "a scorer source edit must move the freshness fingerprint: a corpus produced by \
             the old scorer does not describe what the new one would record"
    );
}
