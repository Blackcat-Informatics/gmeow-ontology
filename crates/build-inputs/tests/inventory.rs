// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use gmeow_build_inputs::{
    CargoResolutionInputs, CfgContext, CompilerInputs, InputInventory, ProductionSelection, SCHEMA,
    UnitSelection,
};
use std::path::Path;

#[path = "support/native_json.rs"]
mod native_json;

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
            edition: "2024".into(),
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

#[test]
fn source_policy_follows_module_ownership_instead_of_test_like_names() {
    let root = fixture(
        r#"
        #[cfg(all(test, not(target_arch = "wasm32")))]
        #[path = "arbitrary.rs"] mod checks;
        #[path = "production.tests.rs"] mod production;
        #[cfg(test)] #[path = "shared.rs"] mod test_shared;
        #[path = "shared.rs"] mod production_shared;
        #[cfg(any(test, feature = "harness"))]
        #[path = "feature.tests.rs"] mod feature_production;
    "#,
    );
    write(root.path(), "crate/src/arbitrary.rs", "mod nested;\n");
    write(
        root.path(),
        "crate/src/arbitrary/nested.rs",
        "fn test_helper() {}\n",
    );
    for name in ["production.tests.rs", "shared.rs", "feature.tests.rs"] {
        write(
            root.path(),
            &format!("crate/src/{name}"),
            "pub fn production() {}\n",
        );
    }
    let excluded =
        gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"]).unwrap();
    assert_eq!(
        excluded.into_iter().collect::<Vec<_>>(),
        ["crate/src/arbitrary.rs", "crate/src/arbitrary/nested.rs",]
    );
}

#[test]
fn source_policy_never_excludes_an_ambiguous_cfg_attr_production_branch() {
    let root = fixture("#[cfg_attr(feature = \"harness\", cfg(test))] mod shared;\n");
    write(
        root.path(),
        "crate/src/shared.rs",
        "pub fn production() {}\n",
    );
    let error =
        gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ambiguous production module cfg_attr")
    );
}

#[test]
fn source_policy_cfg_requires_every_production_branch_to_be_excluded() {
    use gmeow_build_inputs::cfg_excludes_production;
    assert!(cfg_excludes_production("all(test, not(target_arch = \"wasm32\"))").unwrap());
    assert!(!cfg_excludes_production("any(test, feature = \"harness\")").unwrap());
    assert!(!cfg_excludes_production("not(test)").unwrap());
    assert!(cfg_excludes_production("custom_unknown_axis").is_err());
}

#[test]
fn source_policy_registry_type_capability_never_admits_an_unowned_macro() {
    for (source, accepted) in [
        (
            "fn registry() { let _ = quote! { inventory::collect!(Contract); }; }",
            true,
        ),
        ("inventory::collect!(Contract);", true),
        ("other::collect!(Contract);", false),
        ("collect!(Contract);", false),
        ("inventory::collect!(include!(\"missing.rs\"));", false),
    ] {
        let root = fixture(source);
        let result = gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"]);
        assert_eq!(result.is_ok(), accepted, "{source}: {result:?}");
    }
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
    let rich_manifest = r#"[package]
name='example'
version='1.0.0'
[package.metadata.capture]
text="escape\n\"\\ é中😀"
integer=-9223372036854775808
boolean=true
finite=1.25
floats=[0.0,-0.0,1.0,-1.5,1e-6,1e-7,1e16,1e20,5e-324,1.7976931348623157e308]
offset=1979-05-27T07:32:00.999999999Z
local=1979-05-27T07:32:00
date=1979-05-27
time=07:32:00.001
array=["é中😀",1,true]
"#;
    write(root.path(), "crate/Cargo.toml", rich_manifest);
    write(
        root.path(),
        ".cargo/config.toml",
        "[build]\ntarget='x86_64-unknown-linux-gnu'\n[alias]\nread='check'\n[env]\nGMEOW_JSON_FIXTURE='é中😀'\n",
    );
    let mut selected = selection();
    selected.policy_files = vec![".cargo/config.toml".into()];
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
    let selection_path = root.path().join("selected.json");
    let plan_path = root.path().join("selected-plan.json");
    std::fs::write(&selection_path, purrdf_lex::json::record::to_vec(&selected)).unwrap();
    assert!(
        std::process::Command::new(env!("CARGO_BIN_EXE_extract-tests"))
            .arg("selected-plan")
            .arg(root.path())
            .arg(&selection_path)
            .arg(&plan_path)
            .status()
            .unwrap()
            .success()
    );
    let bytes = std::fs::read(plan_path).unwrap();
    native_json::assert_reference("selected-plan", &bytes, &gmeow_build_inputs::digest(&bytes));
    assert_eq!(
        original.manifests["crate/Cargo.toml"],
        native_json::original("cargo-manifest").0
    );
    assert_eq!(
        original.manifests[".cargo/config.toml"],
        native_json::original("cargo-configuration").0
    );
    assert_eq!(
        original.packages["registry+https://example.test/index#external@1.0.0"],
        native_json::original("cargo-package").0
    );

    for nonfinite in ["nan", "inf", "-inf"] {
        write(
            root.path(),
            "crate/Cargo.toml",
            &rich_manifest.replace("finite=1.25", &format!("finite={nonfinite}")),
        );
        let error = InputInventory::collect(root.path(), &selected)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("crate/Cargo.toml")
                && error.contains("non-finite TOML")
                && error.contains("field \"finite\""),
            "{error}"
        );
    }
    write(root.path(), "crate/Cargo.toml", rich_manifest);

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
fn explicit_git_package_identity_binds_the_unique_resolved_lock_revision() {
    let root = fixture("pub fn production() {}\n");
    let mut selected = selection();
    let mut external = selected.units[0].clone();
    let source = "git+https://example.test/repository?rev=locked";
    external.package = format!("{source}#actual-facade@3.0.1");
    external.manifest = None;
    external.source = None;
    external.target = "actual_facade".into();
    selected.units.push(external);
    selected.units[0].dependencies.push(1);
    let record = |revision: &str| {
        format!(
            "[[package]]\nname='actual-facade'\nversion='3.0.1'\nsource='{source}#{revision}'\n"
        )
    };
    write(
        root.path(),
        "Cargo.lock",
        &format!("version=4\n{}", record("first")),
    );
    let first = InputInventory::collect(root.path(), &selected).unwrap();
    write(
        root.path(),
        "Cargo.lock",
        &format!("version=4\n{}", record("second")),
    );
    let second = InputInventory::collect(root.path(), &selected).unwrap();
    assert_ne!(
        first.packages, second.packages,
        "the resolved Git object remains binding after metadata normalization"
    );
    write(
        root.path(),
        "Cargo.lock",
        &format!("version=4\n{}{}", record("first"), record("second")),
    );
    assert!(
        InputInventory::collect(root.path(), &selected).is_err(),
        "an ambiguous source is not authority to select either lock record"
    );
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
    let historical: NativeSources =
        purrdf_lex::json::record::from_slice(&native_json::original("native-document").1).unwrap();
    assert_eq!(historical, original);
    let native_document = purrdf_lex::json::record::to_vec(&original);
    native_json::assert_reference(
        "native-document",
        &native_document,
        &gmeow_build_inputs::digest(&native_document),
    );
    let native_input =
        purrdf_lex::json::record::to_vec(&("gmeow-native-semantic-sources-v1", &original.files));
    native_json::assert_reference("native-input", &native_input, &original.digest().unwrap());
    let source_path = root.path().join("source-list.json");
    let plan_path = root.path().join("native-plan.json");
    assert!(
        std::process::Command::new(env!("CARGO_BIN_EXE_extract-tests"))
            .arg("native-plan")
            .arg(root.path())
            .arg(&source_path)
            .arg(&plan_path)
            .status()
            .unwrap()
            .success()
    );
    for (name, path) in [
        ("native-source-list", &source_path),
        ("native-plan", &plan_path),
    ] {
        let bytes = std::fs::read(path).unwrap();
        native_json::assert_reference(name, &bytes, &gmeow_build_inputs::digest(&bytes));
    }
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

fn tracing_selection(root: &Path, alias: &str) -> ProductionSelection {
    write(
        root,
        "telemetry/Cargo.toml",
        "[package]\nname='tracing'\nversion='0.1.0'\n",
    );
    write(root, "telemetry/src/lib.rs", "pub fn telemetry() {}\n");
    let mut selected = selection();
    selected.units.push(UnitSelection {
        package: "path+file://<workspace>/telemetry#tracing@0.1.0".into(),
        manifest: Some("telemetry/Cargo.toml".into()),
        source: Some("telemetry/src/lib.rs".into()),
        target: "tracing".into(),
        edition: "2024".into(),
        ..selected.units[0].clone()
    });
    selected.units[0].dependencies.push(1);
    selected.units[0].dependency_names.push(alias.into());
    selected
}

const JSON_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn native_semantic_json_macro_binds_substrate_and_nested_source_bytes() {
    use gmeow_build_inputs::{NativeSources, native_substrate_source};
    let root = tempfile::tempdir().unwrap();
    let repository = "https://github.com/Blackcat-Informatics/purrdf";
    let manifest = format!(
        "[workspace]\n[workspace.dependencies]\npurrdf={{git='{repository}',rev='{JSON_REVISION}'}}\n{}\n",
        json_declaration("purrdf-lex")
    );
    write(root.path(), "Cargo.toml", &manifest);
    let lock = format!(
        "version=4\n[[package]]\nname='purrdf-lex'\nversion='3.0.0'\nsource='git+{repository}?rev={JSON_REVISION}#{JSON_REVISION}'\n"
    );
    write(root.path(), "Cargo.lock", &lock);
    for name in ["logic", "logic-compile", "term-arena", "ns"] {
        write(
            root.path(),
            &format!("crates/{name}/Cargo.toml"),
            "[package]\nedition='2024'\n[dependencies]\npurrdf-lex={workspace=true}\n",
        );
        write(root.path(), &format!("crates/{name}/src/lib.rs"), "");
    }
    let source = "::purrdf_lex::json_record!(impl ToJson for Row { include_str!(\"field.txt\") => value }); #[cfg(test)] mod tests;";
    write(root.path(), "crates/logic-compile/src/lib.rs", source);
    write(root.path(), "crates/logic-compile/src/field.txt", "first");
    write(
        root.path(),
        "crates/logic-compile/src/tests.rs",
        "fn first() {}",
    );
    write(root.path(), "crates/logic/build.rs", "fn main() {}\n");
    write(
        root.path(),
        "crates/logic/src/lib.rs",
        "include!(concat!(env!(\"OUT_DIR\"),\"/native_semantic_sources.rs\"));\n",
    );
    let mut selected = json_selection(root.path(), "purrdf-lex");
    selected.units[0].manifest = Some("crates/logic/Cargo.toml".into());
    selected.units[0].source = Some("crates/logic/src/lib.rs".into());
    let native_resolution = |selection: &ProductionSelection| {
        CargoResolutionInputs::capture(
            root.path(),
            ["crates/logic/Cargo.toml".into()].into_iter().collect(),
        )
        .unwrap()
        .bind(selection)
        .unwrap()
    };
    let compiler_reads = |inventory: &InputInventory, directory: &str| CompilerInputs {
        files: inventory
            .files
            .keys()
            .map(|name| root.path().join(name))
            .chain([root.path().join(directory)])
            .collect(),
        environments: Default::default(),
    };
    let absent = InputInventory::collect(root.path(), &selected).unwrap();
    assert_eq!(absent.memberships.get(""), Some(&Vec::new()));
    let absent_resolution = native_resolution(&selected);
    compiler_reads(&absent, "")
        .verify(root.path(), &absent, &absent_resolution, &[], &[])
        .unwrap();

    std::fs::create_dir(root.path().join(".cargo")).unwrap();
    let empty = InputInventory::collect(root.path(), &selected).unwrap();
    assert_eq!(empty.memberships.get(".cargo"), Some(&Vec::new()));
    assert!(!empty.memberships.contains_key(""));
    compiler_reads(&empty, ".cargo")
        .verify(root.path(), &empty, &absent_resolution, &[], &[])
        .unwrap();

    write(root.path(), ".cargo/config.toml", "");
    assert!(
        InputInventory::collect(root.path(), &selected).is_err(),
        "new configuration must invalidate policy membership"
    );
    assert!(
        absent_resolution
            .verify_current(root.path(), &selected)
            .is_err()
    );
    selected.policy_files = gmeow_build_inputs::cargo_policy_files(root.path()).unwrap();
    let configured = InputInventory::collect(root.path(), &selected).unwrap();
    assert_eq!(configured.memberships[".cargo"], [".cargo/config.toml"]);
    let configured_resolution = native_resolution(&selected);
    compiler_reads(&configured, ".cargo")
        .verify(root.path(), &configured, &configured_resolution, &[], &[])
        .unwrap();
    write(root.path(), ".cargo/unselected.txt", "not a selected input");
    std::fs::create_dir(root.path().join(".cargo/unselected-dir")).unwrap();
    std::fs::create_dir(root.path().join("outside")).unwrap();
    assert_eq!(
        configured,
        InputInventory::collect(root.path(), &selected).unwrap(),
        "unselected directory contents must not enter the inventory"
    );
    for unselected in [
        "",
        ".cargo/unselected.txt",
        ".cargo/unselected-dir",
        "outside",
    ] {
        let refusal = compiler_reads(&configured, unselected)
            .verify(root.path(), &configured, &configured_resolution, &[], &[])
            .unwrap_err()
            .to_string();
        assert!(
            refusal.contains("undeclared inputs"),
            "{unselected}: {refusal}"
        );
    }
    write(
        root.path(),
        ".cargo/config.toml",
        "# changed configuration bytes\n",
    );
    assert_eq!(
        configured,
        InputInventory::collect(root.path(), &selected).unwrap(),
        "nonsemantic config comments must not change executable identity"
    );
    assert!(
        configured_resolution
            .verify_current(root.path(), &selected)
            .is_err()
    );
    std::fs::remove_file(root.path().join(".cargo/config.toml")).unwrap();

    let first = NativeSources::collect(root.path()).unwrap();
    assert!(
        first
            .files
            .contains_key("crates/logic-compile/src/field.txt")
    );
    write(
        root.path(),
        "crates/logic-compile/src/tests.rs",
        "fn changed() {}",
    );
    assert_eq!(first, NativeSources::collect(root.path()).unwrap());
    write(root.path(), "crates/logic-compile/src/field.txt", "second");
    assert_ne!(
        first.digest().unwrap(),
        NativeSources::collect(root.path())
            .unwrap()
            .digest()
            .unwrap()
    );

    let changed = "abcdef0123456789abcdef0123456789abcdef01";
    let mismatched = manifest.replace(
        &json_declaration("purrdf-lex"),
        &json_declaration("purrdf-lex").replace(JSON_REVISION, changed),
    );
    write(root.path(), "Cargo.toml", &mismatched);
    write(
        root.path(),
        "Cargo.lock",
        &lock.replace(JSON_REVISION, changed),
    );
    assert!(
        NativeSources::collect(root.path())
            .unwrap_err()
            .to_string()
            .contains("must match native substrate")
    );
    write(
        root.path(),
        "Cargo.toml",
        &manifest.replace(JSON_REVISION, changed),
    );
    NativeSources::collect(root.path()).unwrap();
    assert_eq!(
        native_substrate_source(root.path()).unwrap(),
        format!("{repository}#{changed}")
    );

    for invalid in [
        source.replace("::purrdf_lex::", "purrdf_lex::"),
        source.replace("include_str!(\"field.txt\")", "unknown_input!()"),
        format!("extern crate self as purrdf_lex; {source}"),
    ] {
        write(root.path(), "crates/logic-compile/src/lib.rs", &invalid);
        assert!(NativeSources::collect(root.path()).is_err(), "{invalid}");
    }
    write(root.path(), "crates/logic-compile/src/lib.rs", source);
    write(root.path(), ".cargo/config.toml", "paths=['other']\n");
    assert!(NativeSources::collect(root.path()).is_err());
}

fn json_selection(root: &Path, alias: &str) -> ProductionSelection {
    let source = format!("git+https://github.com/Blackcat-Informatics/purrdf?rev={JSON_REVISION}");
    write(
        root,
        "Cargo.lock",
        &format!(
            "version=4\n[[package]]\nname='purrdf-lex'\nversion='3.0.0'\nsource='{source}#{JSON_REVISION}'\n"
        ),
    );
    let mut selected = selection();
    selected.units.push(UnitSelection {
        package: format!("{source}#purrdf-lex@3.0.0"),
        manifest: None,
        source: None,
        target: "purrdf_lex".into(),
        ..selected.units[0].clone()
    });
    selected.units[0].dependencies.push(1);
    selected.units[0]
        .dependency_names
        .push(alias.replace('-', "_"));
    selected
}

fn json_declaration(alias: &str) -> String {
    format!(
        "{alias}={{package='purrdf-lex',git='https://github.com/Blackcat-Informatics/purrdf',rev='{JSON_REVISION}',version='3'}}"
    )
}

const LOCAL_EXPORT: &str = r#"
#[macro_export]
macro_rules! wrap {
    ($type:ty) => { impl $type { pub fn proof() -> &'static str { "first proof" } } };
}
pub use crate::wrap;
"#;

fn local_export_fixture(alias: &str, caller: &str) -> (tempfile::TempDir, ProductionSelection) {
    let root = fixture(caller);
    write(
        root.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['crate','provider']\n",
    );
    write(
        root.path(),
        "crate/Cargo.toml",
        &format!(
            "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\n{alias}={{package='provider',path='../provider'}}\n"
        ),
    );
    write(
        root.path(),
        "provider/Cargo.toml",
        "[package]\nname='provider'\nversion='1.0.0'\nedition='2024'\n",
    );
    // The owner-relative call deliberately precedes the definition's module.
    write(
        root.path(),
        "provider/src/lib.rs",
        "mod caller; pub mod json;\n",
    );
    write(
        root.path(),
        "provider/src/caller.rs",
        "struct Own; crate::json::wrap!(Own); crate::wrap!(Root); fn quoted() { quote! { crate::json::wrap!(Other); crate::wrap!(QuotedRoot); } }\n",
    );
    write(root.path(), "provider/src/json.rs", LOCAL_EXPORT);
    write(root.path(), "crate/src/argument.txt", "nested argument");
    let mut selected = selection();
    selected.units.push(UnitSelection {
        package: "path+file://<workspace>/provider#provider@1".into(),
        manifest: Some("provider/Cargo.toml".into()),
        source: Some("provider/src/lib.rs".into()),
        target: "provider".into(),
        kinds: vec!["rlib".into()],
        ..selected.units[0].clone()
    });
    selected.units[0].dependencies.push(1);
    selected.units[0]
        .dependency_names
        .push(alias.replace('-', "_"));
    (root, selected)
}

#[test]
fn local_exported_macros_bind_selected_sources_aliases_and_nested_inputs() {
    for alias in ["provider", "evidence-owner"] {
        let binding = alias.replace('-', "_");
        let source = format!(
            "pub struct Wrap<const N: usize>; ::{binding}::json::wrap!(Wrap<{{ include_str!(\"argument.txt\").len() }}>); ::{binding}::wrap!(Root); fn quoted() {{ quote! {{ ::{binding}::json::wrap!(Other); ::{binding}::wrap!(QuotedRoot); }} }}"
        );
        let (root, mut selected) = local_export_fixture(alias, &source);
        if alias == "evidence-owner" {
            // The controller graph has its own selected ordinary libraries too.
            for unit in &mut selected.units {
                unit.controller = true;
            }
            write(
                root.path(),
                "Cargo.toml",
                &format!(
                    "[workspace]\nmembers=['crate','provider']\n[workspace.dependencies]\n{alias}={{package='provider',path='provider'}}\n"
                ),
            );
            write(
                root.path(),
                "crate/Cargo.toml",
                &format!(
                    "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\n{alias}={{workspace=true}}\n"
                ),
            );
        }
        let first = InputInventory::collect(root.path(), &selected).unwrap();
        for path in [
            "provider/src/lib.rs",
            "provider/src/json.rs",
            "provider/src/caller.rs",
            "crate/src/argument.txt",
            "provider/Cargo.toml",
            "crate/Cargo.toml",
        ] {
            assert!(first.files.contains_key(path), "{path}");
        }
        write(
            root.path(),
            "provider/src/json.rs",
            &LOCAL_EXPORT.replace("pub fn proof", "pub fn changed_proof"),
        );
        assert_ne!(
            first.digest().unwrap(),
            InputInventory::collect(root.path(), &selected)
                .unwrap()
                .digest()
                .unwrap()
        );
        write(root.path(), "provider/src/json.rs", LOCAL_EXPORT);
        write(root.path(), "crate/src/argument.txt", "changed argument");
        assert_ne!(
            first.digest().unwrap(),
            InputInventory::collect(root.path(), &selected)
                .unwrap()
                .digest()
                .unwrap()
        );
    }
    // Root exports require no module re-export, as with the diagnostics macros.
    let (root, selected) = local_export_fixture("provider", "::provider::wrap!(External);");
    write(root.path(), "provider/src/caller.rs", "crate::wrap!(Own);");
    let definition = LOCAL_EXPORT.replace("pub use crate::wrap;", "");
    write(root.path(), "provider/src/json.rs", &definition);
    assert!(InputInventory::collect(root.path(), &selected).is_ok());
    write(
        root.path(),
        "provider/src/json.rs",
        &definition.replace("#[macro_export]", ""),
    );
    assert!(InputInventory::collect(root.path(), &selected).is_err());
}

#[test]
fn local_exported_macros_refuse_unselected_fake_and_shadowed_owners() {
    for mutation in [
        "unselected",
        "wrong-index",
        "wrong-source",
        "wrong-kind",
        "wrong-package",
        "same-root-different-package",
        "wrong-path",
        "relative",
        "shadow-before",
        "shadow-after",
        "qualified-lexical",
        "quoted-relative",
        "opaque",
        "nested-unknown",
    ] {
        let mut source = "::provider::json::wrap!(Wrap);".to_owned();
        match mutation {
            "relative" => source = "provider::json::wrap!(Wrap);".into(),
            "shadow-before" => source = format!("extern crate self as provider; {source}"),
            "shadow-after" => source.push_str("extern crate self as provider;"),
            "qualified-lexical" => source = "macro_rules! wrap { () => {} } fake::wrap!();".into(),
            "quoted-relative" => {
                source = "macro_rules! wrap { () => {} } quote! { fake::wrap!(); }".into()
            }
            "opaque" => source = "::provider::json::wrap!(${concat(Wrap)});".into(),
            "nested-unknown" => source = "::provider::json::wrap!(unknown_input!());".into(),
            _ => {}
        }
        let (root, mut selected) = local_export_fixture("provider", &source);
        match mutation {
            "unselected" => {
                selected.units.truncate(1);
                selected.units[0].dependencies.clear();
                selected.units[0].dependency_names.clear();
            }
            "wrong-index" => selected.units[0].dependencies[0] = 0,
            "wrong-source" => selected.units[1].source = Some("provider/src/caller.rs".into()),
            "wrong-kind" => selected.units[1].kinds = vec!["proc-macro".into()],
            "wrong-package" => {
                selected.units[1].package = "path+file://<workspace>/provider#fake@1".into()
            }
            "same-root-different-package" => {
                let mut other = selected.units[1].clone();
                other.package = "path+file://<workspace>/provider#fake@1".into();
                selected.units.push(other);
            }
            "wrong-path" => {
                write(
                    root.path(),
                    "fake/Cargo.toml",
                    "[package]\nname='provider'\nversion='1.0.0'\nedition='2024'\n",
                );
                write(root.path(), "fake/src/lib.rs", "");
                write(
                    root.path(),
                    "crate/Cargo.toml",
                    "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\nprovider={path='../fake'}\n",
                );
            }
            _ => {}
        }
        assert!(
            InputInventory::collect(root.path(), &selected).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn local_exported_macros_require_unique_active_pure_definitions_and_direct_reexports() {
    for mutation in [
        "no-export",
        "no-reexport",
        "renamed-reexport",
        "private-module",
        "excluded-definition",
        "excluded-reexport",
        "duplicate-definition",
        "duplicate-reexport",
        "source-producing",
        "unsupported-export",
    ] {
        let (root, selected) = local_export_fixture("provider", "::provider::json::wrap!(Wrap);");
        let definition = match mutation {
            "no-export" => LOCAL_EXPORT.replace("#[macro_export]", ""),
            "no-reexport" => LOCAL_EXPORT.replace("pub use crate::wrap;", ""),
            "renamed-reexport" => {
                LOCAL_EXPORT.replace("pub use crate::wrap;", "pub use crate::wrap as renamed;")
            }
            "excluded-definition" => LOCAL_EXPORT.replace(
                "#[macro_export]",
                "#[cfg(feature=\"not-selected\")] #[macro_export]",
            ),
            "excluded-reexport" => LOCAL_EXPORT.replace(
                "pub use crate::wrap;",
                "#[cfg(feature=\"not-selected\")] pub use crate::wrap;",
            ),
            "duplicate-definition" => {
                format!("{LOCAL_EXPORT}\n#[macro_export] macro_rules! wrap {{ () => {{}} }}")
            }
            "duplicate-reexport" => format!("{LOCAL_EXPORT}\npub use crate::wrap;"),
            "source-producing" => LOCAL_EXPORT.replace("impl $type", "mod hidden; impl $type"),
            "unsupported-export" => {
                LOCAL_EXPORT.replace("#[macro_export]", "#[macro_export(local_inner_macros)]")
            }
            _ => LOCAL_EXPORT.into(),
        };
        write(root.path(), "provider/src/json.rs", &definition);
        if mutation == "private-module" {
            write(
                root.path(),
                "provider/src/lib.rs",
                "mod caller; mod json;\n",
            );
        }
        assert!(
            InputInventory::collect(root.path(), &selected).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn native_local_export_proof_captures_declared_owner_and_refuses_portable_ambiguity() {
    use gmeow_build_inputs::NativeSources;
    let (root, _) = local_export_fixture("provider", "");
    for name in ["logic", "logic-compile", "term-arena", "ns"] {
        write(
            root.path(),
            &format!("crates/{name}/Cargo.toml"),
            &format!(
                "[package]\nname='{name}'\nversion='1.0.0'\nedition='2024'\n[dependencies]\nprovider={{path='../../provider'}}\n"
            ),
        );
        write(root.path(), &format!("crates/{name}/src/lib.rs"), "");
    }
    write(
        root.path(),
        "crates/logic/src/lib.rs",
        "::provider::json::wrap!(Wrap);",
    );
    let first = NativeSources::collect(root.path()).unwrap();
    for path in [
        "provider/src/lib.rs",
        "provider/src/json.rs",
        "provider/Cargo.toml",
        "crates/logic/Cargo.toml",
        "Cargo.toml",
    ] {
        assert!(first.files.contains_key(path), "{path}");
    }
    write(
        root.path(),
        "provider/src/json.rs",
        &LOCAL_EXPORT.replace("pub fn proof", "pub fn changed_proof"),
    );
    assert_ne!(
        first.digest().unwrap(),
        NativeSources::collect(root.path())
            .unwrap()
            .digest()
            .unwrap()
    );
    write(root.path(), "provider/src/json.rs", LOCAL_EXPORT);
    write(
        root.path(),
        "provider/Cargo.toml",
        "[package]\nname='provider'\nversion='1.0.0'\nedition='2024'\n# changed selection bytes\n",
    );
    assert_ne!(
        first.digest().unwrap(),
        NativeSources::collect(root.path())
            .unwrap()
            .digest()
            .unwrap()
    );
    write(
        root.path(),
        "provider/src/json.rs",
        &LOCAL_EXPORT.replace(
            "#[macro_export]",
            "#[cfg(feature=\"uncertain\")] #[macro_export]",
        ),
    );
    assert!(NativeSources::collect(root.path()).is_err());
    write(root.path(), "provider/src/json.rs", LOCAL_EXPORT);
    write(root.path(), ".cargo/config.toml", "paths=['elsewhere']\n");
    assert!(NativeSources::collect(root.path()).is_err());
}

#[test]
fn pinned_json_macros_bind_alias_package_revision_and_nested_inputs() {
    for (alias, inherited, nested) in [
        ("purrdf-lex", false, false),
        ("lex-codec", true, false),
        ("codec", false, true),
    ] {
        let binding = alias.replace('-', "_");
        let call = format!(
            "::{binding}::json_record!(impl ToJson for Record {{ include_str!(\"label.txt\") => value }}); ::{binding}::json_string_enum!(Mode {{ One => \"one\" }});"
        );
        let source = if nested {
            format!("fn definitions() {{ quote! {{ {call} }}; }}")
        } else {
            call
        };
        let root = fixture(&format!(
            "{source}\n#[cfg(test)] #[path=\"checks.rs\"] mod checks;"
        ));
        let declaration = json_declaration(alias);
        write(
            root.path(),
            "Cargo.toml",
            &format!("[workspace]\nmembers=['crate']\n[workspace.dependencies]\n{declaration}\n"),
        );
        let dependency = if inherited {
            format!("{alias}={{workspace=true}}")
        } else {
            declaration
        };
        write(
            root.path(),
            "crate/Cargo.toml",
            &format!(
                "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\n{dependency}\n"
            ),
        );
        write(root.path(), "crate/src/label.txt", "first input");
        write(root.path(), "crate/src/checks.rs", "fn check() {}\n");
        let selected = json_selection(root.path(), alias);
        let first = InputInventory::collect(root.path(), &selected).unwrap();
        assert!(first.files.contains_key("crate/src/label.txt"));
        let excluded =
            gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"]).unwrap();
        assert!(excluded.contains("crate/src/checks.rs"));
        write(root.path(), "crate/src/label.txt", "second input");
        assert_ne!(
            first,
            InputInventory::collect(root.path(), &selected).unwrap()
        );
    }
}

#[test]
fn pinned_json_macro_revision_changes_require_matching_selection_and_identity() {
    let root = fixture("::purrdf_lex::json_string_enum!(Mode { One => \"one\" });");
    let manifest = format!(
        "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\n{}\n",
        json_declaration("purrdf-lex")
    );
    write(root.path(), "crate/Cargo.toml", &manifest);
    let mut selected = json_selection(root.path(), "purrdf-lex");
    let first = InputInventory::collect(root.path(), &selected).unwrap();
    let new_revision = "abcdef0123456789abcdef0123456789abcdef01";
    let lock = std::fs::read_to_string(root.path().join("Cargo.lock")).unwrap();
    write(
        root.path(),
        "Cargo.lock",
        &lock.replace(JSON_REVISION, new_revision),
    );
    assert!(InputInventory::collect(root.path(), &selected).is_err());
    write(
        root.path(),
        "crate/Cargo.toml",
        &manifest.replace(JSON_REVISION, new_revision),
    );
    selected.units[1].package = selected.units[1]
        .package
        .replace(JSON_REVISION, new_revision);
    let second = InputInventory::collect(root.path(), &selected).unwrap();
    assert_ne!(first.packages, second.packages);
    assert_ne!(first.selection, second.selection);
}

#[test]
fn pinned_json_macros_refuse_unproven_paths_shadowing_and_opaque_inputs() {
    for source in [
        "purrdf_lex::json_record!(impl ToJson for Record { 'x' => value });",
        "json_record!(impl ToJson for Record { 'x' => value });",
        "::other::purrdf_lex::json_record!(impl ToJson for Record {});",
        "mod purrdf_lex { pub use std::include_str as json_record; } purrdf_lex::json_record!(\"hidden.txt\");",
        "extern crate self as purrdf_lex; ::purrdf_lex::json_record!(impl ToJson for Record {});",
        "::purrdf_lex::json_record!(impl ToJson for Record {}); extern crate self as purrdf_lex;",
        "::purrdf_lex::json_record!(impl ToJson for Record { unknown_input!() => value });",
        "::purrdf_lex::json_record!(impl ToJson for Record { include_str!(\"missing.txt\") => value });",
        "::purrdf_lex::json_record!(impl ToJson for Record { {mod hidden; 'x'} => value });",
        "quote! { ::purrdf_lex::json_string_enum!(Mode { extern crate self as purrdf_lex; }); }",
        "::purrdf_lex::json_string_enum!(Mode { ${concat(One)} => \"one\" });",
    ] {
        let root = fixture(source);
        write(
            root.path(),
            "crate/Cargo.toml",
            &format!(
                "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\n{}\n",
                json_declaration("purrdf-lex")
            ),
        );
        let selected = json_selection(root.path(), "purrdf-lex");
        assert!(
            InputInventory::collect(root.path(), &selected).is_err(),
            "{source}"
        );
        let portable =
            gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"]);
        if source.contains("include_str!(\"missing.txt\")") {
            // This query owns module declarations, not embedded data existence.
            // The authenticated inventory above must still refuse missing bytes.
            assert!(portable.is_ok(), "portable module ownership {source}");
        } else {
            assert!(portable.is_err(), "portable {source}");
        }
    }
}

fn json_mutated_declaration(mutation: &str) -> String {
    let mut declaration = json_declaration("purrdf-lex");
    match mutation {
        "local" => declaration = "purrdf-lex={path='../fake'}".into(),
        "package" => declaration = declaration.replace("package='purrdf-lex'", "package='fake'"),
        "malformed-package" => {
            declaration = declaration.replace("package='purrdf-lex'", "package=17")
        }
        "malformed-version" => declaration = declaration.replace("version='3'", "version=17"),
        "repository" => {
            declaration = declaration.replace(
                "github.com/Blackcat-Informatics/purrdf",
                "example.invalid/purrdf",
            )
        }
        "floating" => declaration = declaration.replace(&format!(",rev='{JSON_REVISION}'"), ""),
        "branch" => {
            declaration = declaration.replace(&format!("rev='{JSON_REVISION}'"), "branch='main'")
        }
        "alias-collision" => declaration.push_str("\npurrdf_lex={package='fake',path='../fake'}"),
        "missing-workspace" => declaration = "purrdf-lex={workspace=true}".into(),
        _ => {}
    }
    declaration
}

fn json_mutated_policy(root: &Path, mutation: &str, selected: &mut ProductionSelection) {
    if mutation == "lock-object" {
        let lock = std::fs::read_to_string(root.join("Cargo.lock")).unwrap();
        write(
            root,
            "Cargo.lock",
            &lock.replace(
                &format!("#{JSON_REVISION}"),
                "#abcdef0123456789abcdef0123456789abcdef01",
            ),
        );
    }
    if mutation == "patch" {
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers=['crate']\n[patch.'https://github.com/Blackcat-Informatics/purrdf']\npurrdf-lex={path='fake'}\n",
        );
    }
    if mutation == "malformed-patch" {
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers=['crate']\n[patch.'https://github.com/Blackcat-Informatics/purrdf']\nfake={package=17,path='fake'}\n",
        );
    }
    if mutation == "malformed-replace" {
        write(
            root,
            "Cargo.toml",
            "replace=17\n[workspace]\nmembers=['crate']\n",
        );
    }
    if mutation == "source-replacement" {
        write(
            root,
            ".cargo/config.toml",
            "[source.crates-io]\nreplace-with='replacement'\n[source.replacement]\ndirectory='vendor'\n",
        );
        selected.policy_files.push(".cargo/config.toml".into());
    }
}

#[test]
fn pinned_json_macros_refuse_fake_floating_patched_and_conflicting_packages() {
    let source = "::purrdf_lex::json_record!(impl ToJson for Record {});";
    for mutation in [
        "local",
        "package",
        "repository",
        "floating",
        "branch",
        "lock-object",
        "patch",
        "target",
        "edition",
        "unselected",
        "alias-collision",
        "source-replacement",
        "malformed-package",
        "malformed-patch",
        "malformed-version",
        "malformed-replace",
        "missing-workspace",
        "selected-alias",
    ] {
        let root = fixture(source);
        let declaration = json_mutated_declaration(mutation);
        let extra = if mutation == "target" {
            "\n[target.'cfg(unix)'.dependencies]\npurrdf-lex={path='../fake'}\n"
        } else {
            ""
        };
        let edition = if mutation == "edition" {
            "2015"
        } else {
            "2024"
        };
        write(
            root.path(),
            "crate/Cargo.toml",
            &format!(
                "[package]\nname='example'\nversion='1.0.0'\nedition='{edition}'\n[dependencies]\n{declaration}\n{extra}"
            ),
        );
        let mut selected = json_selection(root.path(), "purrdf-lex");
        if mutation == "local" {
            selected.units[1].manifest = Some("crate/Cargo.toml".into());
        }
        if mutation == "edition" {
            selected.units[0].edition = "2015".into();
        }
        if mutation == "unselected" {
            selected.units[0].dependencies.clear();
            selected.units[0].dependency_names.clear();
        }
        if mutation == "selected-alias" {
            selected.units[0].dependency_names[0] = "other".into();
        }
        json_mutated_policy(root.path(), mutation, &mut selected);
        assert!(
            InputInventory::collect(root.path(), &selected).is_err(),
            "{mutation}"
        );
        if !["unselected", "selected-alias"].contains(&mutation) {
            assert!(
                gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"])
                    .is_err(),
                "portable {mutation}"
            );
        }
    }
}

#[test]
fn renamed_tracing_spans_follow_selected_package_identity() {
    for nested in [false, true] {
        let span = "::telemetry::info_span!(\"phase\", label = include_str!(\"label.txt\"))";
        let source = if nested {
            format!("fn phase() {{ quote! {{ let _ = {span}; }}; }}")
        } else {
            format!("fn phase() {{ let _ = {span}; }}")
        };
        let root = fixture(&source);
        write(root.path(), "crate/src/label.txt", "first");
        write(
            root.path(),
            "crate/Cargo.toml",
            "[package]\nname='example'\nversion='1.0.0'\n[dependencies]\ntelemetry={package='tracing',path='../telemetry'}\n",
        );
        let mut selected = tracing_selection(root.path(), "telemetry");
        let before = InputInventory::collect(root.path(), &selected).unwrap();
        assert!(before.files.contains_key("crate/src/label.txt"));
        write(root.path(), "crate/src/label.txt", "second");
        assert_ne!(
            before,
            InputInventory::collect(root.path(), &selected).unwrap()
        );
        selected.units[1].package = "path+file://<workspace>/telemetry#not-tracing@0.1.0".into();
        assert!(InputInventory::collect(root.path(), &selected).is_err());
        selected.units[1].package = "path+file://<workspace>/telemetry#tracing@0.1.0".into();
        selected.units[0].dependency_names[0] = "different_alias".into();
        assert!(InputInventory::collect(root.path(), &selected).is_err());
        selected.units[0].dependency_names[0] = "telemetry".into();
        for unowned in [
            source.replace("::telemetry::", "telemetry::"),
            source.replace("::telemetry::", "::wrong::telemetry::"),
            source.replace("include_str!(\"label.txt\")", "unknown_input!()"),
            source.replace("include_str!(\"label.txt\")", "{ mod hidden; 1 }"),
        ] {
            write(root.path(), "crate/src/lib.rs", &unowned);
            assert!(
                InputInventory::collect(root.path(), &selected).is_err(),
                "{unowned}"
            );
        }
    }
}

/// Diagnostic syntax retains nested file bytes while unknown capabilities fail closed.
#[test]
fn tracing_spans_require_the_selected_owner_and_retain_nested_inputs() {
    for (nested, statement) in [
        (false, "let _ ="),
        (true, "let _ ="),
        (false, "return"),
        (true, "return"),
    ] {
        let span = "::tracing::info_span!(\"phase\", label = include_str!(\"label.txt\"))";
        let source = if nested {
            format!("fn phase() {{ quote! {{ {statement} {span}; }}; }}")
        } else {
            format!("fn phase() {{ {statement} {span}; }}")
        };
        let root = fixture(&source);
        write(root.path(), "crate/src/label.txt", "first");
        assert!(InputInventory::collect(root.path(), &selection()).is_err());
        write(
            root.path(),
            "crate/Cargo.toml",
            "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\ntracing='0.1'\n",
        );
        let selected = tracing_selection(root.path(), "tracing");
        let before = InputInventory::collect(root.path(), &selected).unwrap();
        assert!(before.files.contains_key("crate/src/label.txt"));
        gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"]).unwrap();
        write(root.path(), "crate/src/label.txt", "second");
        assert_ne!(
            before,
            InputInventory::collect(root.path(), &selected).unwrap()
        );
        for unowned in [
            "fn phase() { tracing::info_span!(\"phase\"); }",
            "fn phase() { quote! { tracing::info_span!(\"phase\") }; }",
            "mod tracing { pub(crate) use std::include_str as info_span; } fn phase() { tracing::info_span!(\"hidden.txt\"); }",
            "fn phase() { quote! { mod tracing { pub(crate) use std::include_str as info_span; } tracing::info_span!(\"hidden.txt\"); } }",
            "fn phase() { ::wrong::tracing::info_span!(\"phase\"); }",
            "fn phase() { quote! { ::wrong::tracing::info_span!(\"phase\") }; }",
            "fn phase() { other::info_span!(\"phase\"); }",
            "fn phase() { info_span!(\"phase\"); }",
            "fn phase() { quote! { other::info_span!(\"phase\") }; }",
            "fn phase() { wrong::tracing::info_span!(\"phase\"); }",
            "fn phase() { quote! { wrong::tracing::info_span!(\"phase\") }; }",
            "fn phase() { ::tracing::info_span!(\"phase\", value = unknown_input!()); }",
            "fn phase() { quote! { ::tracing::info_span!(\"phase\", value = unknown_input!()) }; }",
            "fn phase() { ::tracing::info_span!(\"phase\", value = { mod hidden; 1 }); }",
            "fn phase() { quote! { ::tracing::info_span!(\"phase\", value = { mod hidden; 1 }) }; }",
        ] {
            write(root.path(), "crate/src/lib.rs", unowned);
            assert!(
                InputInventory::collect(root.path(), &selected).is_err(),
                "{unowned}"
            );
            assert!(
                gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"])
                    .is_err(),
                "{unowned}"
            );
        }
    }
}

#[test]
fn tracing_extern_crate_aliases_cannot_hide_embedded_inputs() {
    for edition in ["2018", "2021", "2024"] {
        for (alias, owner, raw, placement, quoted) in [
            ("tracing", "self", false, "before", false),
            ("telemetry", "self", false, "after", true),
            ("tracing", "other", false, "after", false),
            ("telemetry", "other", false, "include", false),
            ("tracing", "self", true, "include", true),
            ("telemetry", "other", true, "after", true),
            ("tracing", "self", false, "reused", false),
            ("tracing", "self", false, "macro", false),
            ("tracing", "self", false, "forwarded", false),
            ("tracing", "self", false, "split-head", false),
            ("telemetry", "self", false, "split-tail", false),
            ("tracing", "self", false, "split-both", false),
            ("tracing", "self", false, "abi-argument", false),
            ("tracing", "self", false, "synthesis", false),
            ("tracing", "self", false, "builtin-shadow", false),
            ("tracing", "self", false, "exported-shadow", false),
            ("tracing", "self", false, "nested-abi-argument", false),
        ] {
            let binding = if raw {
                format!("r#{alias}")
            } else {
                alias.into()
            };
            let declaration = format!("extern crate {owner} as {binding};");
            let call = format!("::{alias}::info_span!(\"hidden.txt\")");
            let body = if quoted {
                format!("quote! {{ {call}; }}")
            } else {
                call
            };
            let phase = format!("fn phase() {{ let _ = {body}; }}");
            let root = fixture("");
            let source = match placement {
                "before" => format!("{declaration}\n{phase}"),
                "after" => format!("{phase}\n{declaration}"),
                "include" => {
                    write(root.path(), "crate/src/alias.rs", &declaration);
                    format!("{phase}\ninclude!(\"alias.rs\");")
                }
                "reused" => {
                    write(root.path(), "crate/src/mod.rs", &declaration);
                    format!("#[path=\"mod.rs\"] mod local;\n{phase}\ninclude!(\"mod.rs\");")
                }
                "macro" => {
                    format!("macro_rules! bind {{ () => {{ {declaration} }}; }} bind!();\n{phase}")
                }
                "forwarded" => format!(
                    "macro_rules! bind {{ ($item:item) => {{ $item }}; }} bind! {{ {declaration} }}\n{phase}"
                ),
                "split-head" => format!(
                    "macro_rules! bind {{ ($k:tt) => {{ $k crate {owner} as {binding}; }}; }} bind!(extern);\n{phase}"
                ),
                "split-tail" => format!(
                    "macro_rules! bind {{ ($k:tt) => {{ extern $k {owner} as {binding}; }}; }} bind!(crate);\n{phase}"
                ),
                "split-both" => format!(
                    "macro_rules! bind {{ ($a:tt, $b:tt) => {{ $a $b {owner} as {binding}; }}; }} bind!(extern, crate);\n{phase}"
                ),
                "abi-argument" => format!(
                    "macro_rules! bind {{ ($k:tt $abi:literal) => {{ $k crate {owner} as {binding}; }}; }} bind!(extern \"C\");\n{phase}"
                ),
                "synthesis" => format!(
                    "macro_rules! bind {{ () => {{ ${{concat(ex, tern)}} ${{concat(cra, te)}} {owner} as {binding}; }}; }} bind!();\n{phase}"
                ),
                "builtin-shadow" => format!(
                    "macro_rules! format {{ ($k:tt) => {{ $k crate {owner} as {binding}; }}; }} format!(extern);\n{phase}"
                ),
                "exported-shadow" => format!(
                    "mod macros {{ #[macro_export] macro_rules! format {{ ($k:tt) => {{ $k crate {owner} as {binding}; }}; }} }} crate::format!(extern);\n{phase}"
                ),
                "nested-abi-argument" => format!(
                    "macro_rules! bind {{ ($k:tt $abi:literal) => {{ $k crate {owner} as {binding}; }}; }} macro_rules! wrapper {{ () => {{ bind!(extern \"C\"); }}; }} wrapper!();\n{phase}"
                ),
                _ => unreachable!(),
            };
            let feature = if placement == "synthesis" {
                "#![feature(macro_metavar_expr_concat)]\n"
            } else {
                ""
            };
            write(
                root.path(),
                "crate/src/lib.rs",
                &format!("{feature}pub use core::include_str as info_span;\n{source}"),
            );
            write(
                root.path(),
                "crate/src/hidden.txt",
                "unrecorded source input",
            );
            write(
                root.path(),
                "crate/Cargo.toml",
                &format!(
                    "[package]\nname='example'\nversion='1.0.0'\nedition='{edition}'\n[dependencies]\n{alias}={{package='tracing',version='0.1'}}\nother='0.1'\n",
                ),
            );
            let mut selected = tracing_selection(root.path(), alias);
            selected.units[0].edition = edition.into();
            write(
                root.path(),
                "other/Cargo.toml",
                "[package]\nname='other'\nversion='0.1.0'\n",
            );
            write(
                root.path(),
                "other/src/lib.rs",
                "pub use core::include_str as info_span;\n",
            );
            selected.units.push(UnitSelection {
                package: "path+file://<workspace>/other#other@0.1.0".into(),
                manifest: Some("other/Cargo.toml".into()),
                source: Some("other/src/lib.rs".into()),
                target: "other".into(),
                ..selected.units[1].clone()
            });
            selected.units[0].dependencies.push(2);
            selected.units[0].dependency_names.push("other".into());
            let Err(error) = InputInventory::collect(root.path(), &selected) else {
                panic!("root alias accepted: {edition} {placement}");
            };
            let message = if matches!(placement, "macro" | "split-tail" | "synthesis") {
                "source-producing declarative macro"
            } else if matches!(
                placement,
                "forwarded"
                    | "split-head"
                    | "split-both"
                    | "abi-argument"
                    | "builtin-shadow"
                    | "exported-shadow"
                    | "nested-abi-argument"
            ) {
                "explicit source owner"
            } else {
                "extern crate"
            };
            assert!(
                error.to_string().contains(message),
                "{edition} {placement}: {error}"
            );
            let error =
                gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"])
                    .expect_err("portable ownership must also reject the shadowing alias");
            assert!(
                error.to_string().contains(message),
                "{edition} {placement}: {error}"
            );
        }
    }
}

#[test]
fn tracing_extern_crate_scope_keeps_genuine_and_inactive_owners() {
    for alias in ["tracing", "telemetry"] {
        for location in [
            "genuine",
            "inactive",
            "inline",
            "external",
            "block",
            "forwarded",
            "abi-type",
            "bare-abi-type",
            "abi-block",
        ] {
            let call =
                format!("::{alias}::info_span!(\"phase\", label = include_str!(\"label.txt\"))");
            let phase = format!("fn phase() {{ let _ = {call}; }}");
            let root = fixture("");
            let source = match location {
                "genuine" => format!("extern crate {alias};\n{phase}"),
                "inactive" => format!("#[cfg(any())] extern crate self as {alias};\n{phase}"),
                "inline" => format!("mod local {{ extern crate self as {alias}; {phase} }}"),
                "external" => {
                    write(
                        root.path(),
                        "crate/src/local.rs",
                        &format!("extern crate self as {alias}; {phase}"),
                    );
                    "mod local;".into()
                }
                "block" => {
                    format!("fn phase() {{ extern crate self as {alias}; let _ = {call}; }}")
                }
                "forwarded" => format!(
                    "macro_rules! forward {{ ($value:expr) => {{ $value }}; }}\nfn phase() {{ let _ = ::{alias}::info_span!(\"phase\", label=forward!(include_str!(\"label.txt\"))); }}"
                ),
                "abi-type" => format!(
                    "macro_rules! callback {{ ($name:ident) => {{ type $name = unsafe extern \"C\" fn(); }}; }} callback!(Callback);\n{phase}"
                ),
                "bare-abi-type" => format!(
                    "macro_rules! callback {{ ($name:ident) => {{ type $name = extern fn(); }}; }} callback!(Callback);\n{phase}"
                ),
                "abi-block" => format!(
                    "macro_rules! foreign {{ ($name:ident) => {{ unsafe extern \"C\" {{ fn $name(); }} }}; }} foreign!(foreign_fn);\n{phase}"
                ),
                _ => unreachable!(),
            };
            write(
                root.path(),
                "crate/src/lib.rs",
                &format!("{source}\n#[cfg(test)] #[path=\"checks.rs\"] mod checks;"),
            );
            write(root.path(), "crate/src/label.txt", "owned embedded input");
            write(root.path(), "crate/src/checks.rs", "fn check() {}\n");
            write(
                root.path(),
                "crate/Cargo.toml",
                &format!(
                    "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\n{alias}={{package='tracing',version='0.1'}}\n",
                ),
            );
            let selected = tracing_selection(root.path(), alias);
            let inventory = InputInventory::collect(root.path(), &selected).unwrap();
            assert!(
                inventory.files.contains_key("crate/src/label.txt"),
                "{alias} {location}"
            );
            let excluded =
                gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"])
                    .unwrap();
            assert_eq!(
                excluded.into_iter().collect::<Vec<_>>(),
                ["crate/src/checks.rs"],
                "{alias} {location}"
            );
        }
    }
}

#[test]
fn portable_tracing_requires_registry_package_identity() {
    for nested in [false, true] {
        let span = "::tracing::info_span!(\"phase\", label = include_str!(\"label.txt\"))";
        let source = if nested {
            format!("fn phase() {{ quote! {{ let _ = {span}; }}; }}")
        } else {
            format!("fn phase() {{ let _ = {span}; }}")
        };
        for (dependency, expected) in [
            (
                "{package='other',path='../fake'}",
                "macro tracing::info_span! has no unique selected local export",
            ),
            (
                "{path='../fake'}",
                "macro tracing::info_span! has no unique selected local export",
            ),
            (
                "{version='0.1',path='../fake'}",
                "macro tracing::info_span! has no unique selected local export",
            ),
            (
                "{git='https://example.invalid/tracing'}",
                "exported macro owner must be an unambiguous local path declaration",
            ),
            (
                "{version='0.1',git='https://example.invalid/tracing'}",
                "exported macro owner must be an unambiguous local path declaration",
            ),
            (
                "{version='0.1',registry='other'}",
                "exported macro owner must be an unambiguous local path declaration",
            ),
            (
                "{version='0.1',package='other'}",
                "exported macro owner has no declared local path",
            ),
        ] {
            let root = fixture(&source);
            write(root.path(), "crate/src/label.txt", "embedded input");
            // A valid local library must still prove its macro export ownership.
            let package = if dependency.contains("package='other'") {
                "other"
            } else {
                "tracing"
            };
            write(
                root.path(),
                "fake/Cargo.toml",
                &format!("[package]\nname='{package}'\nversion='0.1.0'\nedition='2024'\n"),
            );
            write(root.path(), "fake/src/lib.rs", "pub fn unrelated() {}\n");
            write(
                root.path(),
                "crate/Cargo.toml",
                &format!(
                    "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\ntracing={dependency}\n",
                ),
            );
            let error =
                gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"])
                    .expect_err("a dependency name cannot grant registry macro ownership");
            assert!(
                error.to_string().contains(expected),
                "{dependency}: {error}"
            );
        }
    }
}

#[test]
fn portable_tracing_preserves_registry_aliases_and_workspace_declarations() {
    for (alias, dependency, inherited) in [
        ("tracing", "'0.1'", ""),
        ("tracing", "{version='0.1',registry='crates-io'}", ""),
        ("telemetry", "{package='tracing',version='0.1'}", ""),
        ("tracing", "{workspace=true}", "tracing='0.1'"),
        (
            "trace-events",
            "{workspace=true,features=['std']}",
            "trace-events={package='tracing',version='0.1'}",
        ),
    ] {
        let name = alias.replace('-', "_");
        let root = fixture(&format!(
            "fn phase() {{ let _ = ::{name}::info_span!(\"phase\", label = include_str!(\"label.txt\")); }}\n#[cfg(test)] #[path=\"checks.rs\"] mod checks;",
        ));
        write(root.path(), "crate/src/label.txt", "embedded input");
        write(root.path(), "crate/src/checks.rs", "fn check() {}\n");
        write(
            root.path(),
            "Cargo.toml",
            &format!("[workspace]\nmembers=['crate']\n[workspace.dependencies]\n{inherited}\n"),
        );
        write(
            root.path(),
            "crate/Cargo.toml",
            &format!(
                "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\n{alias}={dependency}\n",
            ),
        );
        let excluded =
            gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"]).unwrap();
        assert_eq!(
            excluded.into_iter().collect::<Vec<_>>(),
            ["crate/src/checks.rs"],
            "{alias}: {dependency}"
        );
    }
}

#[test]
fn portable_tracing_refuses_source_overrides_and_ambiguous_target_owners() {
    for (dependency, manifest_extra, workspace_extra, config, expected) in [
        (
            "{workspace=true}",
            "",
            "[workspace.dependencies]\ntracing={path='fake'}",
            "",
            "macro tracing::info_span! has no unique selected local export",
        ),
        (
            "{workspace=true,package='other'}",
            "",
            "[workspace.dependencies]\ntracing='0.1'",
            "",
            "unsupported inherited local macro declaration",
        ),
        (
            "'0.1'",
            "[target.'cfg(windows)'.dependencies]\ntracing={path='../fake'}",
            "",
            "",
            "exported macro owner must be an unambiguous local path declaration",
        ),
        (
            "'0.1'",
            "",
            "[patch.crates-io]\ntracing={path='../fake'}",
            "",
            "exported macro owner must be an unambiguous local path declaration",
        ),
        (
            "'0.1'",
            "",
            "[patch.crates-io]\nreplacement={package='tracing',git='https://example.invalid/tracing'}",
            "",
            "exported macro owner must be an unambiguous local path declaration",
        ),
        (
            "'0.1'",
            "",
            "[replace]\n\"tracing:0.1.0\"={path='../fake'}",
            "",
            "exported macro owner must be an unambiguous local path declaration",
        ),
        (
            "'0.1'",
            "",
            "",
            "paths=['../fake']",
            "exported macro owner must be an unambiguous local path declaration",
        ),
        (
            "'0.1'",
            "",
            "",
            "[source.crates-io]\nreplace-with='fake'\n[source.fake]\ndirectory='../fake'",
            "exported macro owner must be an unambiguous local path declaration",
        ),
        (
            "'0.1'",
            "",
            "",
            "[source.crates-io]\nregistry='https://example.invalid/index'",
            "exported macro owner must be an unambiguous local path declaration",
        ),
        (
            "'0.1'",
            "",
            "",
            "[registries.crates-io]\nindex='https://example.invalid/index'",
            "exported macro owner must be an unambiguous local path declaration",
        ),
        (
            "'0.1'",
            "",
            "",
            "[patch.crates-io]\ntracing={path='../fake'}",
            "exported macro owner must be an unambiguous local path declaration",
        ),
    ] {
        let root = fixture("fn phase() { let _ = ::tracing::info_span!(\"phase\"); }");
        write(
            root.path(),
            "fake/Cargo.toml",
            "[package]\nname='tracing'\nversion='0.1.0'\nedition='2024'\n",
        );
        write(root.path(), "fake/src/lib.rs", "pub fn unrelated() {}\n");
        write(
            root.path(),
            "Cargo.toml",
            &format!("[workspace]\nmembers=['crate']\n{workspace_extra}\n"),
        );
        write(
            root.path(),
            "crate/Cargo.toml",
            &format!(
                "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\ntracing={dependency}\n{manifest_extra}\n",
            ),
        );
        if !config.is_empty() {
            write(root.path(), ".cargo/config.toml", config);
        }
        let error = gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"])
            .unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "{dependency}, {manifest_extra}, {workspace_extra}, {config}: {error}"
        );
    }
    let root = fixture("fn phase() { let _ = ::tracing::info_span!(\"phase\"); }");
    write(
        root.path(),
        "crate/Cargo.toml",
        "[package]\nname='example'\nversion='1.0.0'\nedition='2024'\n[dependencies]\ntracing.workspace=true\n",
    );
    let error =
        gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("missing workspace dependency tracing"),
        "{error}"
    );
}

#[test]
fn diagnostic_macros_retain_nested_inputs_and_refuse_unowned_capabilities() {
    for name in ["define_diag_kind", "impl_diag_kind"] {
        let root = fixture(&format!(
            "gmeow_errors::{name}! {{ Payload; code = \"fixture.failed\"; grade = include_str!(\"grade.txt\"); }}",
        ));
        write(root.path(), "crate/src/grade.txt", "first");
        let before = inventory(root.path());
        assert!(before.files.contains_key("crate/src/grade.txt"));
        write(root.path(), "crate/src/grade.txt", "second");
        assert_ne!(before, inventory(root.path()));
        write(
            root.path(),
            "crate/src/lib.rs",
            &format!(
                "gmeow_errors::{name}! {{ Payload; code = \"fixture.failed\"; grade = unknown_input!(); }}",
            ),
        );
        assert!(InputInventory::collect(root.path(), &selection()).is_err());
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

#[test]
fn rust_2015_tracing_names_never_hide_embedded_source() {
    for alias in ["tracing", "telemetry"] {
        for nested in [false, true] {
            let span = format!("::{alias}::info_span!(\"hidden.txt\")");
            let call = if nested {
                format!("quote! {{ {span} }}")
            } else {
                span
            };
            let root = fixture(&format!(
                "mod {alias} {{ pub(crate) use std::include_str as info_span; }} fn phase() {{ let _ = {call}; }}",
            ));
            write(root.path(), "crate/src/hidden.txt", "embedded source");
            write(
                root.path(),
                "crate/Cargo.toml",
                &format!(
                    "[package]\nname='example'\nversion='1.0.0'\nedition='2015'\n[dependencies]\n{alias}={{package='tracing',path='../telemetry'}}\n",
                ),
            );
            let mut selected = tracing_selection(root.path(), alias);
            selected.units[0].edition = "2015".into();
            let error = InputInventory::collect(root.path(), &selected).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("external local macro path requires the external prelude"),
                "{error}"
            );
            assert!(
                gmeow_build_inputs::test_only_module_paths(root.path(), &["crate/src/lib.rs"])
                    .is_err()
            );
            for edition in ["2018", "2021", "2024"] {
                selected.units[0].edition = edition.into();
                InputInventory::collect(root.path(), &selected).unwrap();
            }
            selected.units[0].edition = "2099".into();
            assert!(
                selected
                    .validate()
                    .unwrap_err()
                    .to_string()
                    .contains("unsupported selected Rust edition")
            );
        }
    }
}

#[test]
fn portable_source_editions_resolve_declared_inherited_and_language_default() {
    let root = fixture("pub fn source() {}\n");
    assert_eq!(
        gmeow_build_inputs::source_edition(root.path(), "crate/src/lib.rs").unwrap(),
        "2015"
    );
    write(
        root.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['crate']\n[workspace.package]\nedition='2024'\n",
    );
    write(
        root.path(),
        "crate/Cargo.toml",
        "[package]\nname='example'\nversion='1.0.0'\nedition.workspace=true\n",
    );
    assert_eq!(
        gmeow_build_inputs::source_edition(root.path(), "crate/src/lib.rs").unwrap(),
        "2024"
    );
    write(
        root.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['crate']\n",
    );
    assert!(gmeow_build_inputs::source_edition(root.path(), "crate/src/lib.rs").is_err());
}
