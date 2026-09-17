// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Allow canonical aliases of one checkout while rejecting absent paths and later root changes.
#[test]
fn explicit_root_is_portable_immutable_and_missing_roots_fail_closed() {
    let first = tempfile::tempdir().expect("first checkout");
    let second = tempfile::tempdir().expect("second checkout");
    let binding = OnceLock::new();
    assert!(bind_root(&binding, &first.path().join("absent")).is_err());
    assert!(
        binding.get().is_none(),
        "a missing selection cannot bind another root"
    );
    let selected = bind_root(&binding, first.path()).expect("bind relocated checkout");
    assert_eq!(
        selected,
        first.path().canonicalize().expect("absolute root")
    );
    assert_eq!(
        binding.get(),
        Some(&first.path().canonicalize().expect("root"))
    );
    bind_root(&binding, &first.path().join(".")).expect("same canonical root");
    assert!(bind_root(&binding, second.path()).is_err());
    assert_eq!(
        binding.get(),
        Some(&first.path().canonicalize().expect("root"))
    );
}

#[test]
fn slice_dir_is_the_spec_grandparent() {
    let spec = Path::new("/repo/slices/core/epistemics/tests/competency.ttl");
    assert_eq!(slice_dir(spec), Path::new("/repo/slices/core/epistemics"));
}

#[test]
fn module_and_migrated_shapes_resolve_to_their_authorities() {
    let slice = Path::new("/repo/slices/core/epistemics");
    assert_eq!(
        module_file(slice),
        Path::new("/repo/slices/core/epistemics/module.ttl")
    );
    assert_eq!(
        shapes_files(slice),
        vec![
            repo_root().join("generated/shapes/validation-shapes.ttl"),
            repo_root().join("generated/shapes/constraint-shapes.ttl"),
            repo_root().join("generated/shapes/procedural-constraints.ttl"),
        ]
    );
    assert_eq!(
        examples_dir(slice),
        Path::new("/repo/slices/core/epistemics/examples")
    );
}

#[test]
fn conformance_data_is_slice_local_except_for_the_grounding_kernel() {
    let ordinary = slices_root().join("core/epistemics");
    assert_eq!(
        conformance_module_files(&ordinary),
        vec![module_file(&ordinary)]
    );

    let grounding = slices_root().join("grounding");
    let expected = ["lang", "logic", "math"]
        .into_iter()
        .map(|name| grounding.join(name).join("module.ttl"))
        .collect::<Vec<_>>();
    for name in ["lang", "logic", "math"] {
        assert_eq!(conformance_module_files(&grounding.join(name)), expected);
    }
}

#[test]
fn partially_migrated_slice_adds_local_residue_after_generated_authorities() {
    let slice = repo_root().join("slices/grounding/lang");
    assert!(slice.join("shapes.ttl").is_file());
    assert_eq!(
        shapes_files(&slice),
        vec![
            repo_root().join("generated/shapes/validation-shapes.ttl"),
            repo_root().join("generated/shapes/constraint-shapes.ttl"),
            repo_root().join("generated/shapes/procedural-constraints.ttl"),
            slice.join("shapes.ttl"),
        ]
    );
}

#[test]
fn example_file_is_slice_relative_query_file_is_repo_relative() {
    let slice = Path::new("/repo/slices/core/epistemics");
    // exampleFile resolves against the slice, never the repo root.
    assert_eq!(
        example_file(slice, "tests/counter-examples/x.ttl"),
        Path::new("/repo/slices/core/epistemics/tests/counter-examples/x.ttl")
    );
    // The real repo root is used for cqQueryFile; just assert the suffix so
    // the test is independent of where the checkout lives.
    assert!(query_file("queries/competency/agents.rq").ends_with("queries/competency/agents.rq"));
}
