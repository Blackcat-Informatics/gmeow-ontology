// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Input hashing binds owner manifests without canonicalizing unrelated RDF;
/// selected semantic consumption still refuses the same invalid artifacts.
#[test]
fn physical_grounding_inputs_bind_owner_changes_and_preserve_semantic_refusal() {
    let directory = tempfile::tempdir().expect("synthetic grounding inputs");
    let root = directory.path();
    let slice = root.join("slices/grounding/lang");
    std::fs::create_dir_all(slice.join("mappings")).unwrap();
    std::fs::create_dir_all(slice.join("examples")).unwrap();
    let manifest = slice.join("manifest.ttl");
    let mapping = slice.join("mappings/grounding.ttl");
    let example = slice.join("examples/unselected.ttl");
    std::fs::write(slice.join("module.ttl"), "invalid module RDF").unwrap();
    std::fs::write(&mapping, "invalid mapping RDF").unwrap();
    std::fs::write(&example, "invalid unselected RDF").unwrap();
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
    assert!(selected.contains(&slice.join("module.ttl")));
    assert!(selected.contains(&example));
    assert!(
        discover(root).is_err(),
        "semantic admission remains mandatory"
    );

    let valid = "<https://example.org/s> <https://example.org/p> <https://example.org/o> .\n";
    std::fs::write(slice.join("module.ttl"), valid).unwrap();
    std::fs::write(&mapping, valid).unwrap();
    assert!(
        discover(root).is_err(),
        "an unselected artifact still participates in semantic admission"
    );
    std::fs::write(&example, valid).unwrap();
    let sources = discover(root).expect("valid grounding owner is admitted");
    assert_eq!(
        sources.len(),
        1,
        "only Mapping-role artifacts are grounding authority"
    );
    assert_eq!(
        sources[0].identity().owner,
        "https://blackcatinformatics.ca/gmeow/slices/lang"
    );
    assert_eq!(
        sources[0].identity().path,
        "slices/grounding/lang/mappings/grounding.ttl"
    );

    write_owner("https://example.org/another-owner");
    let unselected = input_files(root).expect("other owners retain their manifest dependency");
    assert_eq!(unselected, selected);
    assert!(
        discover(root).unwrap().is_empty(),
        "other owners cannot supply grounding mappings"
    );
    write_owner("https://blackcatinformatics.ca/gmeow/slices/logic");
    assert_eq!(input_files(root).unwrap(), selected);
    let refusal = discover(root).expect_err("off-owner mapping must be refused");
    assert!(
        refusal
            .message()
            .contains("off-owner selected catalog artifact")
            && refusal
                .message()
                .contains("slices/grounding/lang/mappings/grounding.ttl")
            && refusal
                .message()
                .contains("https://blackcatinformatics.ca/gmeow/slices/logic"),
        "{refusal}"
    );

    std::fs::write(&manifest, "invalid manifest RDF").unwrap();
    assert!(
        input_files(root).is_err(),
        "required manifests cannot be skipped"
    );
}
