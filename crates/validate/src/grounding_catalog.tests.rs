// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Input hashing binds owner manifests without canonicalizing unrelated RDF;
/// selected semantic consumption still refuses the same invalid artifacts.
#[test]
fn physical_grounding_inputs_bind_owner_changes_and_preserve_semantic_refusal() {
    let directory = tempfile::tempdir().expect("synthetic grounding inputs");
    let root = directory.path();
    let slice = root.join("slices/core/example");
    std::fs::create_dir_all(slice.join("mappings")).unwrap();
    let manifest = slice.join("manifest.ttl");
    let mapping = slice.join("mappings/grounding.ttl");
    std::fs::write(slice.join("module.ttl"), "invalid module RDF").unwrap();
    std::fs::write(&mapping, "invalid mapping RDF").unwrap();
    let write_owner = |owner: &str| {
        std::fs::write(
            &manifest,
            format!("<{owner}> a <https://blackcatinformatics.ca/gmeow/Slice> .\n"),
        )
        .unwrap();
    };

    write_owner("https://blackcatinformatics.ca/gmeow/slices/lang");
    let selected = input_files(root).expect("physical paths need no RDF artifact admission");
    assert!(selected.contains(&manifest));
    assert!(selected.contains(&mapping));
    assert!(!selected.contains(&slice.join("module.ttl")));
    assert!(
        discover(root).is_err(),
        "semantic admission remains mandatory"
    );

    write_owner("https://example.org/another-owner");
    let unselected = input_files(root).expect("other owners retain their manifest dependency");
    assert_eq!(unselected, vec![manifest.clone()]);
    write_owner("https://blackcatinformatics.ca/gmeow/slices/logic");
    assert_eq!(input_files(root).unwrap(), selected);

    std::fs::write(&manifest, "invalid manifest RDF").unwrap();
    assert!(
        input_files(root).is_err(),
        "required manifests cannot be skipped"
    );
}
