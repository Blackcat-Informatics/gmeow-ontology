// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use gmeow_build_inputs::{
    CargoResolutionInputs, CfgContext, CompilerInputs, InputInventory, ProductionSelection, SCHEMA,
    UnitSelection,
};
use std::path::Path;

fn write(root: &Path, name: &str, source: &str) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}
fn selection() -> ProductionSelection {
    ProductionSelection {
        schema: SCHEMA,
        units: vec![UnitSelection {
            package: "path+file://<workspace>/crate#example@1".into(),
            manifest: Some("crate/Cargo.toml".into()),
            source: Some("crate/src/lib.rs".into()),
            target: "example".into(),
            kinds: vec!["lib".into()],
            cfg: CfgContext::from_rustc(
                "unix\ntarget_arch=\"x86_64\"\ntarget_os=\"linux\"",
                &[],
                true,
            )
            .unwrap(),
            dependencies: vec![],
            dependency_names: vec![],
            controller: false,
        }],
        roots: vec![0],
        policy_files: vec![],
    }
}
fn fixture(source: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['crate']\n",
    );
    write(root.path(), "Cargo.lock", "version=4\npackage=[]\n");
    write(
        root.path(),
        "crate/Cargo.toml",
        "[package]\nname='example'\nversion='1.0.0'\n",
    );
    write(root.path(), "crate/src/lib.rs", source);
    root
}
fn inventory(root: &Path) -> InputInventory {
    InputInventory::collect(root, &selection()).unwrap()
}

fn resolution(
    root: &Path,
    selected: &ProductionSelection,
) -> gmeow_build_inputs::CargoResolutionEvidence {
    CargoResolutionInputs::capture(root, ["crate/Cargo.toml".into()].into_iter().collect())
        .unwrap()
        .bind(selected)
        .unwrap()
}

#[test]
fn development_resolution_changes_require_readmission_without_identity_churn() {
    let root = fixture("pub fn production() {}\n#[cfg(test)] mod tests;\n");
    write(root.path(), "crate/src/tests.rs", "#[test] fn first() {}\n");
    let selected = selection();
    let original = inventory(root.path());
    let evidence = resolution(root.path(), &selected);
    write(
        root.path(),
        "crate/src/tests.rs",
        "#[test] fn changed() {}\n#[test] fn added() {}\n",
    );
    evidence.verify_current(root.path(), &selected).unwrap();
    write(
        root.path(),
        "crate/Cargo.toml",
        "[package]\nname='example'\nversion='1.0.0'\n[dev-dependencies]\nhelper='1'\n",
    );
    write(
        root.path(),
        "Cargo.lock",
        "version=4\n[[package]]\nname='helper'\nversion='1.0.0'\n",
    );
    assert_eq!(original, inventory(root.path()));
    assert!(evidence.verify_current(root.path(), &selected).is_err());
    let fresh = resolution(root.path(), &selected);
    fresh.verify_current(root.path(), &selected).unwrap();
    assert_ne!(evidence, fresh);
    assert_eq!(
        original.digest().unwrap(),
        inventory(root.path()).digest().unwrap()
    );
}

#[test]
fn retained_dev_versions_cannot_authenticate_stale_normal_or_build_edges() {
    for kind in ["lib", "custom-build"] {
        let root = fixture("pub fn production() {}\n");
        let mut selected = selection();
        selected.units[0].kinds = vec![kind.into()];
        selected.units[0].dependencies = vec![1];
        let mut external = selected.units[0].clone();
        external.package = "registry+https://example.test/index#external@1.0.0".into();
        external.manifest = None;
        external.source = None;
        external.dependencies.clear();
        selected.units.push(external);
        let packages = "[[package]]\nname='external'\nversion='1.0.0'\nsource='registry+https://example.test/index'\nchecksum='one'\n[[package]]\nname='external'\nversion='2.0.0'\nsource='registry+https://example.test/index'\nchecksum='two'\n";
        write(
            root.path(),
            "Cargo.lock",
            &format!(
                "version=4\n[[package]]\nname='example'\nversion='1.0.0'\ndependencies=['external 1.0.0']\n{packages}"
            ),
        );
        let original = InputInventory::collect(root.path(), &selected).unwrap();
        let evidence = resolution(root.path(), &selected);
        // The old package AND parent edge remain because a dev dependency still
        // uses them. Membership is insufficient; only fresh Cargo selection is.
        write(
            root.path(),
            "Cargo.lock",
            &format!(
                "version=4\n[[package]]\nname='example'\nversion='1.0.0'\ndependencies=['external 1.0.0','external 2.0.0']\n{packages}"
            ),
        );
        assert_eq!(
            original,
            InputInventory::collect(root.path(), &selected).unwrap()
        );
        assert!(evidence.verify_current(root.path(), &selected).is_err());
        selected.units[1].package = "registry+https://example.test/index#external@2.0.0".into();
        assert!(evidence.verify_selection(&selected).is_err());
        let fresh = resolution(root.path(), &selected);
        fresh.verify_current(root.path(), &selected).unwrap();
        assert_ne!(
            original.digest().unwrap(),
            InputInventory::collect(root.path(), &selected)
                .unwrap()
                .digest()
                .unwrap()
        );
    }
}

#[test]
fn resolution_configuration_is_relocatable_and_detects_changed_ancestor_policy() {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    for directory in [&left, &right] {
        write(
            directory.path(),
            ".cargo/config.toml",
            "[registries.example]\nindex='https://example.test/index'\n",
        );
        write(
            directory.path(),
            "workspace/Cargo.toml",
            "[package]\nname='example'\nversion='1.0.0'\n",
        );
        write(
            directory.path(),
            "workspace/Cargo.lock",
            "version=4\npackage=[]\n",
        );
    }
    let manifests = ["Cargo.toml".into()].into_iter().collect();
    let before = CargoResolutionInputs::capture(&left.path().join("workspace"), manifests).unwrap();
    let other = CargoResolutionInputs::capture(
        &right.path().join("workspace"),
        ["Cargo.toml".into()].into_iter().collect(),
    )
    .unwrap();
    assert_eq!(before, other);
    before
        .verify_current(&right.path().join("workspace"))
        .unwrap();
    write(
        right.path(),
        ".cargo/config.toml",
        "[registries.example]\nindex='https://changed.test/index'\n",
    );
    assert!(
        before
            .verify_current(&right.path().join("workspace"))
            .is_err()
    );
    write(
        right.path(),
        ".cargo/config.toml",
        "[patch.crates-io]\nexternal={path='../unowned'}\n",
    );
    assert!(
        CargoResolutionInputs::capture(
            &right.path().join("workspace"),
            ["Cargo.toml".into()].into_iter().collect()
        )
        .is_err()
    );
}

#[test]
fn resolution_glob_membership_and_all_metadata_manifests_are_observed() {
    let root = fixture("pub fn production() {}\n");
    write(
        root.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['crate','extra/*']\n",
    );
    write(
        root.path(),
        "extra/first/Cargo.toml",
        "[package]\nname='first'\nversion='1.0.0'\n",
    );
    let manifests = ["crate/Cargo.toml".into(), "extra/first/Cargo.toml".into()]
        .into_iter()
        .collect();
    let before = CargoResolutionInputs::capture(root.path(), manifests).unwrap();
    write(
        root.path(),
        "extra/first/Cargo.toml",
        "[package]\nname='first'\nversion='1.0.0'\n[dev-dependencies]\nhelper='1'\n",
    );
    assert!(before.verify_current(root.path()).is_err());
    let before = CargoResolutionInputs::capture(
        root.path(),
        ["crate/Cargo.toml".into(), "extra/first/Cargo.toml".into()]
            .into_iter()
            .collect(),
    )
    .unwrap();
    write(
        root.path(),
        "extra/second/Cargo.toml",
        "[package]\nname='second'\nversion='1.0.0'\n",
    );
    assert!(before.verify_current(root.path()).is_err());
}

#[test]
fn external_test_changes_do_not_change_producer_or_action_identity() {
    let root = fixture("pub fn production() {}\n#[cfg(test)] mod tests;\n");
    write(
        root.path(),
        "crate/src/tests.rs",
        "#[test] fn original() { assert!(true); }\n",
    );
    let initial = inventory(root.path());
    write(
        root.path(),
        "crate/src/tests.rs",
        "mod more; #[test] fn changed() { assert_eq!(1,1); }\n",
    );
    write(
        root.path(),
        "crate/src/tests/more.rs",
        "#[test] fn added() {}\n",
    );
    assert_eq!(initial, inventory(root.path()));
    initial.verify_current(root.path()).unwrap();
    let selected = selection().scoped_to_manifest("crate/Cargo.toml").unwrap();
    assert_eq!(
        initial.digest().unwrap(),
        InputInventory::collect(root.path(), &selected)
            .unwrap()
            .digest()
            .unwrap()
    );
}
#[test]
fn production_module_named_tests_is_not_excluded() {
    let root = fixture("mod tests;\n");
    write(
        root.path(),
        "crate/src/tests.rs",
        "pub fn actual_production() {}\n",
    );
    let before = inventory(root.path());
    assert!(before.files.contains_key("crate/src/tests.rs"));
    write(
        root.path(),
        "crate/src/tests.rs",
        "pub fn actual_production() { let _ = 1; }\n",
    );
    assert_ne!(before, inventory(root.path()));
}
#[test]
fn complete_files_and_module_membership_are_portable_and_binding() {
    let left = fixture("pub fn actual() {}\n");
    let right = fixture("pub fn actual() {}\n");
    let before = inventory(left.path());
    assert_eq!(before, inventory(right.path()));
    write(
        left.path(),
        "crate/src/lib.rs",
        "mod added; pub fn actual() {}\n",
    );
    assert!(InputInventory::collect(left.path(), &selection()).is_err());
    write(left.path(), "crate/src/added.rs", "pub fn added() {}\n");
    assert_ne!(before, inventory(left.path()));
    write(
        right.path(),
        "crate/src/lib.rs",
        "// Every selected byte remains owned.\npub fn actual() {}\n",
    );
    assert_ne!(before, inventory(right.path()));
    assert!(before.verify_current(right.path()).is_err());
}
#[test]
fn explicit_paths_nested_modules_and_assets_resolve_from_their_rust_owners() {
    let root = fixture(
        "#[path=\"alternate.rs\"] mod declared; mod inside { mod child; } const DATA: &str=include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"),\"/asset.txt\"));\n",
    );
    write(
        root.path(),
        "crate/src/alternate.rs",
        "#[path=\"sibling.rs\"] mod sibling;\n",
    );
    write(root.path(), "crate/src/sibling.rs", "pub fn sibling() {}\n");
    write(
        root.path(),
        "crate/src/inside/child.rs",
        "pub fn child() {}\n",
    );
    write(root.path(), "crate/asset.txt", "bytes\n");
    let before = inventory(root.path());
    for file in [
        "crate/src/alternate.rs",
        "crate/src/sibling.rs",
        "crate/src/inside/child.rs",
        "crate/asset.txt",
    ] {
        assert!(before.files.contains_key(file));
    }
    write(root.path(), "crate/asset.txt", "new bytes\n");
    assert_ne!(before, inventory(root.path()));
}
#[test]
fn development_cargo_changes_do_not_mask_production_manifest_changes() {
    let root = fixture("pub fn production() {}\n");
    let before = inventory(root.path());
    write(
        root.path(),
        "crate/Cargo.toml",
        "[package]\nname='example'\nversion='1.0.0'\n[dev-dependencies]\nassertion='9'\n[profile.test]\nopt-level=1\n",
    );
    write(
        root.path(),
        "Cargo.lock",
        "version=4\n[[package]]\nname='assertion'\nversion='9.0.0'\nsource='registry+https://example.test/index'\nchecksum='unused'\n",
    );
    assert_eq!(before, inventory(root.path()));
    write(
        root.path(),
        "crate/Cargo.toml",
        "[package]\nname='example'\nversion='1.0.0'\n[dependencies]\nproduction='9'\n",
    );
    assert_ne!(before, inventory(root.path()));
}
#[test]
fn inline_test_code_unknown_cfg_and_unowned_source_macros_fail_closed() {
    for source in [
        "#[cfg(test)] mod tests { #[test] fn hidden() {} }",
        "#[cfg(all(test,unix))] fn helper() {}",
        "#[cfg(unknown_build_setting)] mod unknown;",
        "unknown_source_macro!();",
        "macro_rules! indirect { ($format:ident) => { $format!(\"hidden-input\") }; } const S: &str = indirect!(include_str);",
        "const S: &str=env!(\"UNDECLARED_AMBIENT\");",
        "macro_rules! produce { () => { mod generated; } } produce!();",
        "const S: &str=include_str!(env!(\"UNDECLARED_PATH\"));",
    ] {
        let root = fixture(source);
        assert!(
            InputInventory::collect(root.path(), &selection()).is_err(),
            "{source}"
        );
    }
}
#[test]
fn embedding_excluded_tests_is_not_accepted_as_production() {
    let root = fixture("#[cfg(test)] mod tests; const S:&str=include_str!(\"tests.rs\");");
    write(
        root.path(),
        "crate/src/tests.rs",
        "#[test] fn control() {}\n",
    );
    assert!(
        InputInventory::collect(root.path(), &selection())
            .unwrap_err()
            .to_string()
            .contains("embeds excluded")
    );
}
#[test]
fn module_ambiguity_cycles_and_workspace_escape_are_errors() {
    let root = fixture("mod ambiguous;");
    write(root.path(), "crate/src/ambiguous.rs", "");
    write(root.path(), "crate/src/ambiguous/mod.rs", "");
    assert!(InputInventory::collect(root.path(), &selection()).is_err());
    write(
        root.path(),
        "crate/src/lib.rs",
        "#[path=\"lib.rs\"] mod cycle;",
    );
    assert!(InputInventory::collect(root.path(), &selection()).is_err());
    write(
        root.path(),
        "crate/src/lib.rs",
        "const DATA:&str=include_str!(\"../../../outside\");",
    );
    assert!(InputInventory::collect(root.path(), &selection()).is_err());
}
#[cfg(unix)]
#[test]
fn a_symlink_is_rejected_even_when_followed_by_parent_navigation() {
    let root = fixture("#[path=\"linked/../other.rs\"] mod unsafe_path;");
    write(root.path(), "crate/real/marker", "");
    write(root.path(), "crate/src/other.rs", "");
    std::os::unix::fs::symlink("../real", root.path().join("crate/src/linked")).unwrap();
    assert!(
        InputInventory::collect(root.path(), &selection())
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );
}
#[test]
fn actual_compiler_reads_must_equal_declared_workspace_inputs() {
    let root = fixture("pub fn production() {}\n");
    let mut declared = inventory(root.path());
    std::fs::create_dir_all(root.path().join("watched/intermediate")).unwrap();
    std::fs::create_dir(root.path().join("outside")).unwrap();
    declared
        .memberships
        .insert("watched/selected".into(), Vec::new());
    declared
        .memberships
        .insert("watched".into(), vec!["watched/selected".into()]);
    let resolution = resolution(root.path(), &declared.selection);
    let dep = root.path().join("synthetic.d");
    write(
        root.path(),
        "synthetic.d",
        "object: crate/src/lib.rs Cargo.lock watched/intermediate\n",
    );
    let mut observed = CompilerInputs::default();
    observed.extend_dep_info(&dep, root.path()).unwrap();
    observed
        .verify(root.path(), &declared, &resolution, &[], &[])
        .unwrap();
    write(
        root.path(),
        "synthetic.d",
        "object: crate/src/lib.rs undeclared.rs second-undeclared.rs outside\n",
    );
    let mut observed = CompilerInputs::default();
    observed.extend_dep_info(&dep, root.path()).unwrap();
    let error = observed
        .verify(root.path(), &declared, &resolution, &[], &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("undeclared.rs"));
    assert!(error.contains("second-undeclared.rs"));
    assert!(error.contains("outside"));
    assert!(
        CompilerInputs::default()
            .verify(root.path(), &declared, &resolution, &[], &[])
            .is_err()
    );
}

#[test]
fn selected_registry_identity_and_feature_resolution_are_binding() {
    let root = fixture("pub fn production() {}\n");
    let mut selected = selection();
    let mut external = selected.units[0].clone();
    external.package = "registry+https://example.test/index#external@1.0.0".into();
    external.manifest = None;
    external.source = None;
    external.target = "external".into();
    selected.units.push(external);
    selected.units[0].dependencies.push(1);
    write(
        root.path(),
        "Cargo.lock",
        "version=4\n[[package]]\nname='external'\nversion='1.0.0'\nsource='registry+https://example.test/index'\nchecksum='first'\n",
    );
    let original = InputInventory::collect(root.path(), &selected).unwrap();
    write(
        root.path(),
        "Cargo.lock",
        "version=4\n[[package]]\nname='external'\nversion='1.0.0'\nsource='registry+https://example.test/index'\nchecksum='second'\n",
    );
    assert_ne!(
        original,
        InputInventory::collect(root.path(), &selected).unwrap()
    );
    selected.units[0].cfg.values.insert(
        "feature".into(),
        ["selected-feature".into()].into_iter().collect(),
    );
    assert_ne!(original.selection, selected);
    write(root.path(), "Cargo.lock", "version=4\npackage=[]\n");
    assert!(InputInventory::collect(root.path(), &selected).is_err());
}

#[test]
fn development_cargo_config_and_unused_features_are_not_production_policy() {
    let root = fixture("pub fn production() {}\n");
    let mut selected = selection();
    selected.policy_files.push(".cargo/config.toml".into());
    write(
        root.path(),
        ".cargo/config.toml",
        "[build]\nrustflags=['-Dwarnings']\n",
    );
    let original = InputInventory::collect(root.path(), &selected).unwrap();
    write(
        root.path(),
        ".cargo/config.toml",
        "[build]\nrustflags=['-Dwarnings']\n[profile.test]\nopt-level=1\n[alias]\nunit='test --lib'\n",
    );
    write(
        root.path(),
        "crate/Cargo.toml",
        "[package]\nname='example'\nversion='1.0.0'\n[features]\ntest-helper=[]\n[target.'cfg(unix)'.dev-dependencies]\nassertion='9'\n",
    );
    assert_eq!(
        original,
        InputInventory::collect(root.path(), &selected).unwrap()
    );
    write(
        root.path(),
        ".cargo/config.toml",
        "[build]\nrustflags=['-Dwarnings','-Ctarget-cpu=x86-64-v3']\n",
    );
    assert_ne!(
        original,
        InputInventory::collect(root.path(), &selected).unwrap()
    );
}

#[test]
fn generated_query_owner_tracks_only_query_contents_and_selected_membership() {
    let root = fixture("pub fn production() {}\n");
    write(
        root.path(),
        "crates/logic/Cargo.toml",
        "[package]\nname='logic'\nversion='1.0.0'\n",
    );
    write(root.path(), "crates/logic/build.rs", "fn main() {}\n");
    write(
        root.path(),
        "crates/logic/src/lib.rs",
        "include!(concat!(env!(\"OUT_DIR\"),\"/verify_queries.rs\"));\n",
    );
    write(root.path(), "queries/verify/root.rq", "ASK {}\n");
    write(
        root.path(),
        "slices/group/name/queries/verify/slice.rq",
        "ASK {}\n",
    );
    let mut selected = selection();
    selected.units[0].manifest = Some("crates/logic/Cargo.toml".into());
    selected.units[0].source = Some("crates/logic/src/lib.rs".into());
    let original = InputInventory::collect(root.path(), &selected).unwrap();
    assert!(original.files.contains_key("queries/verify/root.rq"));
    assert!(
        original
            .files
            .contains_key("slices/group/name/queries/verify/slice.rq")
    );
    write(
        root.path(),
        "slices/group/name/docs/nested/note.md",
        "unrelated presentation\n",
    );
    assert_eq!(
        original,
        InputInventory::collect(root.path(), &selected).unwrap()
    );
    write(
        root.path(),
        "slices/group/new/queries/verify/new.rq",
        "ASK { ?s ?p ?o }\n",
    );
    assert_ne!(
        original,
        InputInventory::collect(root.path(), &selected).unwrap()
    );
    write(
        root.path(),
        "slices/group/new/queries/verify/root.rq",
        "ASK {}\n",
    );
    assert!(
        InputInventory::collect(root.path(), &selected).is_err(),
        "query stems have one owner"
    );
}

#[test]
fn cfg_attr_doc_assets_and_inline_module_path_attributes_have_real_owners() {
    let root = fixture(
        "#[cfg_attr(unix,doc=include_str!(\"../readme.md\"))] pub fn production() {}\n#[path=\"alternate\"] mod nested { mod child; }\n",
    );
    write(root.path(), "crate/readme.md", "embedded documentation\n");
    write(
        root.path(),
        "crate/src/alternate/child.rs",
        "pub fn nested() {}\n",
    );
    let original = inventory(root.path());
    assert!(original.files.contains_key("crate/readme.md"));
    assert!(original.files.contains_key("crate/src/alternate/child.rs"));
    write(
        root.path(),
        "crate/readme.md",
        "changed embedded documentation\n",
    );
    assert_ne!(original, inventory(root.path()));
}

#[test]
fn production_cannot_embed_a_child_of_an_excluded_test_module() {
    let root =
        fixture("#[cfg(test)] mod cases; const HELPER:&str=include_str!(\"cases/helper.rs\");\n");
    write(root.path(), "crate/src/cases.rs", "mod helper;\n");
    write(
        root.path(),
        "crate/src/cases/helper.rs",
        "pub fn only_for_tests() {}\n",
    );
    assert!(InputInventory::collect(root.path(), &selection()).is_err());
}

#[test]
fn selected_build_controller_bytes_are_owned_but_unselected_tools_are_not() {
    let root = fixture("pub fn production() {}\n");
    let mut selected = selection();
    let mut controller = selected.units[0].clone();
    controller.controller = true;
    controller.source = Some("crate/src/controller.rs".into());
    selected.units.push(controller);
    selected.roots.push(1);
    write(
        root.path(),
        "crate/src/controller.rs",
        "pub fn build() {}\n",
    );
    let original = InputInventory::collect(root.path(), &selected).unwrap();
    write(
        root.path(),
        "crate/src/unselected_tool.rs",
        "pub fn only_for_migration() {}\n",
    );
    assert_eq!(
        original,
        InputInventory::collect(root.path(), &selected).unwrap()
    );
    write(
        root.path(),
        "crate/src/controller.rs",
        "pub fn changed_build() {}\n",
    );
    assert_ne!(
        original,
        InputInventory::collect(root.path(), &selected).unwrap()
    );
}

#[test]
fn compiler_artifact_binding_ignores_unselected_dep_info_and_normalizes_paths() {
    use gmeow_build_inputs::CompilerArtifact;
    let root = fixture("pub fn production() {}\n");
    write(
        root.path(),
        "output/actual.d",
        "output/libactual.rlib: crate/src/../src/lib.rs\n",
    );
    write(
        root.path(),
        "output/unselected.d",
        "output/libother.rlib: arbitrary-unselected.rs\n",
    );
    let inputs = CompilerInputs::from_artifacts(
        root.path(),
        &[CompilerArtifact {
            files: vec![root.path().join("output/libactual.rlib")],
            source: root.path().join("crate/src/lib.rs"),
            build_script: false,
            package: selection().units[0].package.clone(),
        }],
    )
    .unwrap();
    let declared = inventory(root.path());
    let resolution = resolution(root.path(), &declared.selection);
    inputs
        .verify(root.path(), &declared, &resolution, &[], &[])
        .unwrap();
    assert_eq!(
        inputs.files,
        [root.path().join("crate/src/lib.rs")].into_iter().collect()
    );
}

#[test]
fn generated_compiler_inputs_are_bound_outside_the_checkout_to_their_owner() {
    use gmeow_build_inputs::GeneratedInputs;
    let root = fixture("pub fn production() {}\n");
    let generated = tempfile::tempdir().unwrap();
    let mut declared = inventory(root.path());
    declared
        .generated
        .insert("generated.rs".into(), "crate/build.rs".into());
    let outputs = [GeneratedInputs {
        directory: generated.path().to_owned(),
        package: selection().units[0].package.clone(),
    }];
    let inputs = CompilerInputs {
        files: [
            root.path().join("crate/src/lib.rs"),
            generated.path().join("generated.rs"),
        ]
        .into_iter()
        .collect(),
        environments: Default::default(),
    };
    let resolution = resolution(root.path(), &declared.selection);
    inputs
        .verify(root.path(), &declared, &resolution, &[], &outputs)
        .unwrap();
    let wrong = [GeneratedInputs {
        directory: generated.path().to_owned(),
        package: "unselected package".into(),
    }];
    assert!(
        inputs
            .verify(root.path(), &declared, &resolution, &[], &wrong)
            .is_err()
    );
}

#[test]
fn native_semantic_contract_is_portable_and_omits_external_tests() {
    use gmeow_build_inputs::NativeSources;
    let root = tempfile::tempdir().unwrap();
    for name in ["logic", "logic-compile", "term-arena", "ns"] {
        write(
            root.path(),
            &format!("crates/{name}/Cargo.toml"),
            "[package]\n",
        );
        write(
            root.path(),
            &format!("crates/{name}/src/lib.rs"),
            "#[cfg(test)] mod tests; #[cfg(target_arch=\"wasm32\")] mod wasm; #[cfg(not(target_arch=\"wasm32\"))] mod native;\n",
        );
        write(
            root.path(),
            &format!("crates/{name}/src/tests.rs"),
            "#[test] fn synthetic() {}\n",
        );
        write(
            root.path(),
            &format!("crates/{name}/src/wasm.rs"),
            "pub fn wasm() {}\n",
        );
        write(
            root.path(),
            &format!("crates/{name}/src/native.rs"),
            "pub fn native() {}\n",
        );
    }
    let original = NativeSources::collect(root.path()).unwrap();
    assert_eq!(original.files.len(), 12);
    write(
        root.path(),
        "crates/logic/src/tests.rs",
        "#[test] fn more_test_work() { assert!(true); }\n",
    );
    assert_eq!(
        original.digest().unwrap(),
        NativeSources::collect(root.path())
            .unwrap()
            .digest()
            .unwrap()
    );
    write(
        root.path(),
        "crates/logic/src/wasm.rs",
        "pub fn new_wasm_semantics() {}\n",
    );
    assert_ne!(
        original.digest().unwrap(),
        NativeSources::collect(root.path())
            .unwrap()
            .digest()
            .unwrap()
    );
}

#[test]
fn local_macro_capabilities_follow_lexical_blocks_and_module_scopes() {
    let root =
        fixture("const VALUE: &str = { macro_rules! label { () => { \"bound\" }; } label!() };");
    inventory(root.path());
    for source in [
        "fn defines() { macro_rules! scoped { () => { 1 }; } let _ = scoped!(); } fn outside() { scoped!(); }",
        "mod inside { macro_rules! scoped { () => { 1 }; } } fn outside() { scoped!(); }",
        "fn outside() { scoped!(); } macro_rules! scoped { () => { 1 }; }",
        "macro_rules! ambient { () => { env!(\"UNDECLARED_AMBIENT\") }; } const V: &str = ambient!();",
    ] {
        write(root.path(), "crate/src/lib.rs", source);
        assert!(
            InputInventory::collect(root.path(), &selection()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn negated_keyword_expressions_are_not_macro_invocations() {
    let root = fixture(
        "macro_rules! ensure { ($condition:expr) => { if !($condition) { panic!(\"failed\"); } }; } fn check() { ensure!(true); }",
    );
    inventory(root.path());
    write(
        root.path(),
        "crate/src/lib.rs",
        "macro_rules! check { () => { if !(unknown_input!()) { panic!(\"failed\"); } }; }",
    );
    assert!(InputInventory::collect(root.path(), &selection()).is_err());
}

#[test]
fn standard_output_macros_retain_nested_input_ownership() {
    for name in ["print", "println", "eprint", "eprintln"] {
        let root = fixture(&format!(
            "fn output() {{ {name}!(\"{{}}\", include_str!(\"message.txt\")); }}",
        ));
        write(root.path(), "crate/src/message.txt", "first");
        let before = inventory(root.path());
        assert!(before.files.contains_key("crate/src/message.txt"));
        write(root.path(), "crate/src/message.txt", "second");
        assert_ne!(before, inventory(root.path()));
    }
}

#[test]
fn test_reexport_wiring_is_hashed_but_external_helper_bodies_are_not() {
    let root = fixture("#[cfg(test)] mod support; #[cfg(test)] pub(crate) use support::helper;");
    write(
        root.path(),
        "crate/src/support.rs",
        "pub(crate) fn helper() -> u8 { 1 }",
    );
    let before = inventory(root.path());
    write(
        root.path(),
        "crate/src/support.rs",
        "pub(crate) fn helper() -> u8 { 2 }",
    );
    assert_eq!(before, inventory(root.path()));
    write(
        root.path(),
        "crate/src/lib.rs",
        "#[cfg(test)] mod support; #[cfg(test)] use support::helper as renamed;",
    );
    assert_ne!(before, inventory(root.path()));
}

#[test]
fn compiler_environment_requires_the_exact_selected_input_owner() {
    use gmeow_build_inputs::CompilerArtifact;
    let root = fixture("pub fn production() {}\n");
    let artifact = CompilerArtifact {
        files: vec![root.path().join("output/actual.rlib")],
        source: root.path().join("crate/src/lib.rs"),
        build_script: false,
        package: selection().units[0].package.clone(),
    };
    for (environment, admitted) in [
        ("CARGO_PKG_VERSION=1.0.0", true),
        ("UNDECLARED_AMBIENT=hidden", false),
    ] {
        write(
            root.path(),
            "output/actual.d",
            &format!("output/actual.rlib: crate/src/lib.rs\n# env-dep:{environment}\n"),
        );
        let observed =
            CompilerInputs::from_artifacts(root.path(), std::slice::from_ref(&artifact)).unwrap();
        let declared = inventory(root.path());
        let resolution = resolution(root.path(), &declared.selection);
        assert_eq!(
            observed
                .verify(root.path(), &declared, &resolution, &[], &[])
                .is_ok(),
            admitted
        );
        assert!(
            CompilerInputs::default()
                .extend_dep_info(&root.path().join("output/actual.d"), root.path())
                .is_err(),
            "anonymous environment evidence must not lose package ownership"
        );
    }
}

#[test]
fn controller_profile_code_selection_is_owned_without_developer_optimization_churn() {
    let root = fixture("pub fn production() {}\n");
    let mut selected = selection();
    selected.units[0].controller = true;
    write(
        root.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['crate']\n[profile.dev]\nopt-level=1\ndebug-assertions=true\n",
    );
    let before = InputInventory::collect(root.path(), &selected).unwrap();
    write(
        root.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['crate']\n[profile.dev]\nopt-level=3\ndebug-assertions=true\n",
    );
    assert_eq!(
        before,
        InputInventory::collect(root.path(), &selected).unwrap()
    );
    write(
        root.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['crate']\n[profile.dev]\nopt-level=3\ndebug-assertions=false\n",
    );
    assert_ne!(
        before,
        InputInventory::collect(root.path(), &selected).unwrap()
    );
}
