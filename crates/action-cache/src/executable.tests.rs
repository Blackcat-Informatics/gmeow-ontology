// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn inventory(root: &Path) -> gmeow_build_inputs::InputInventory {
    use gmeow_build_inputs::{
        CfgContext, InputInventory, ProductionSelection, SCHEMA, UnitSelection,
    };
    InputInventory::collect(
        root,
        &ProductionSelection {
            schema: SCHEMA,
            units: vec![UnitSelection {
                package: "path+file://<workspace>/crate#fixture@1".into(),
                manifest: Some("Cargo.toml".into()),
                source: Some("input.rs".into()),
                target: "fixture".into(),
                kinds: vec!["lib".into()],
                cfg: CfgContext::from_rustc("unix", &[], true).unwrap(),
                dependencies: vec![],
                dependency_names: vec![],
                controller: false,
            }],
            roots: vec![0],
            policy_files: vec![],
        },
    )
    .unwrap()
}

fn fixture(root: &Path) {
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='1.0.0'\n",
    )
    .unwrap();
    std::fs::write(root.join("Cargo.lock"), "version=4\npackage=[]\n").unwrap();
    std::fs::write(root.join("input.rs"), "pub fn fixture() {}\n").unwrap();
}

/// Create a minimal recipe whose identity fields can be varied independently.
fn recipe(root: &Path) -> ExecutableRecipe {
    fixture(root);
    let source_inventory = inventory(root);
    ExecutableRecipe {
        schema: 2,
        profile: "pipeline".into(),
        source_digest: source_inventory.digest().unwrap(),
        source_inventory,
        rustc: "compiler".into(),
        cargo: "cargo".into(),
        compiler_environment: BTreeMap::new(),
        units: Vec::new(),
        roots: Vec::new(),
    }
}

/// Require the receipt to authenticate both the recipe and exact executable bytes.
#[test]
fn executable_and_recipe_substitution_are_rejected() {
    let scratch = tempfile::tempdir().expect("scratch");
    let binary = scratch.path().join("producer");
    std::fs::write(&binary, b"linked executable").expect("write");
    let recipe = recipe(scratch.path());
    let receipt = ExecutableReceipt {
        schema: 2,
        resolution: gmeow_build_inputs::CargoResolutionInputs::capture(
            scratch.path(),
            ["Cargo.toml".into()].into_iter().collect(),
        )
        .unwrap()
        .bind(&recipe.source_inventory.selection)
        .unwrap(),
        recipe,
        executable_sha256: sha256_file(&binary).expect("digest"),
    };
    let digest = receipt.recipe.digest().expect("recipe");
    receipt.verify(&binary, &digest).expect("authentic");
    let mut changed = receipt.clone();
    changed.recipe.profile = "test".into();
    assert!(changed.verify(&binary, &digest).is_err());
    std::fs::write(&binary, b"substituted executable").expect("replace");
    assert!(receipt.verify(&binary, &digest).is_err());
}

/// A producer transferred to a clean execution host keeps the build controller's
/// resolution evidence, but runtime admission must not require that host to recreate
/// the controller's Cargo environment. Exact graph binding and source freshness remain
/// mandatory.
#[test]
fn runtime_source_admission_is_portable_across_cargo_environments() {
    let scratch = tempfile::tempdir().expect("scratch");
    let binary = scratch.path().join("producer");
    std::fs::write(&binary, b"linked executable").expect("write");
    let recipe = recipe(scratch.path());
    let receipt = ExecutableReceipt {
        schema: 2,
        resolution: gmeow_build_inputs::CargoResolutionInputs::capture(
            scratch.path(),
            ["Cargo.toml".into()].into_iter().collect(),
        )
        .unwrap()
        .bind(&recipe.source_inventory.selection)
        .unwrap(),
        recipe,
        executable_sha256: sha256_file(&binary).expect("digest"),
    };
    let mut transferred = serde_json::to_value(receipt).expect("serialize receipt");
    transferred["resolution"]["inputs"]["environment"]["RUSTUP_TOOLCHAIN"] =
        serde_json::Value::String("0".repeat(64));
    let transferred: ExecutableReceipt =
        serde_json::from_value(transferred).expect("deserialize transferred receipt");

    transferred
        .verify_current_sources(scratch.path())
        .expect("current selected sources remain portable");
    assert!(
        transferred.verify_current_inputs(scratch.path()).is_err(),
        "the build-controller verification must still reject stale resolution inputs"
    );

    std::fs::write(scratch.path().join("input.rs"), "pub fn changed() {}\n")
        .expect("change selected source");
    assert!(
        transferred.verify_current_sources(scratch.path()).is_err(),
        "portable admission must still reject changed selected sources"
    );
}

/// Keep source-only edits out of action policy while retaining compiler and flag changes.
#[test]
fn compilation_policy_and_executable_source_have_separate_identities() {
    let scratch = tempfile::tempdir().unwrap();
    let original = recipe(scratch.path());
    let mut cli_edit = original.clone();
    cli_edit
        .source_inventory
        .files
        .get_mut("input.rs")
        .unwrap()
        .sha256 = "1".repeat(64);
    cli_edit.source_digest = cli_edit.source_inventory.digest().unwrap();
    assert_ne!(original.digest().unwrap(), cli_edit.digest().unwrap());
    assert_eq!(
        original.compilation_digest().unwrap(),
        cli_edit.compilation_digest().unwrap()
    );
    let changes: [fn(&mut ExecutableRecipe); 3] = [
        |recipe: &mut ExecutableRecipe| recipe.rustc.push_str("changed compiler"),
        |recipe: &mut ExecutableRecipe| recipe.profile = "test".into(),
        |recipe: &mut ExecutableRecipe| {
            recipe
                .compiler_environment
                .insert("CFLAGS".into(), "-O2".into());
        },
    ];
    for change in changes {
        let mut changed = original.clone();
        change(&mut changed);
        assert_ne!(
            original.compilation_digest().unwrap(),
            changed.compilation_digest().unwrap()
        );
    }
}

/// Bind source content and inventory membership while allowing checkout relocation.
#[test]
fn source_inventory_detects_new_and_changed_files_but_not_location() {
    let left = tempfile::tempdir().expect("left");
    let right = tempfile::tempdir().expect("right");
    for root in [left.path(), right.path()] {
        fixture(root);
    }
    let baseline = inventory(left.path()).digest().unwrap();
    assert_eq!(baseline, inventory(right.path()).digest().unwrap());
    std::fs::write(left.path().join("new.rs"), "pub fn new() {}\n").unwrap();
    std::fs::write(
        left.path().join("input.rs"),
        "mod new; pub fn fixture() {}\n",
    )
    .unwrap();
    assert_ne!(baseline, inventory(left.path()).digest().unwrap());
    std::fs::write(right.path().join("input.rs"), "pub fn changed() {}\n").unwrap();
    assert_ne!(baseline, inventory(right.path()).digest().unwrap());
}
